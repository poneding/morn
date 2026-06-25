use crate::decode_thread::{DecodeThread, SeekMode};
use crate::play_clock::PlayClock;
use crate::timeline::Timeline;
use crate::wall_clock::WallClock;
use audio::AudioHandle;
use player_core::{Command, Playlist, StateMachine};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// 视频帧相对主时钟的容差(毫秒): PTS 落在 [master-TOL, master+TOL] 内即显示。
const PRESENT_TOL_MS: u64 = 15;
/// 单次 present 的丢帧上限: 巨量迟帧时达此上限即强制显示, 避免长时间无画面。
const MAX_DROP_PER_PRESENT: u32 = 8;
/// 音频钟停摆超过此时长且音频线程已报结束/缺失 → 切墙钟接管走时。
/// 留足余量: 正常播放回调间隔约 10ms, 不会误触发。
const AUDIO_STALL_HANDOVER_MS: u64 = 200;

#[path = "player_commands.rs"]
mod player_commands;
#[path = "player_open.rs"]
mod player_open;
#[path = "player_present.rs"]
mod player_present;
#[path = "player_seek.rs"]
mod player_seek;

use player_present::DebugProbe;
#[cfg(test)]
use player_seek::user_seek_mode;
use player_seek::SeekGate;

pub struct Player {
    machine: StateMachine,
    playlist: Playlist,
    volume: u8,
    volume_shared: Arc<AtomicU8>,
    rate_pct: u16,
    rate_shared: Arc<AtomicU32>,
    muted: bool,
    volume_before_mute: u8,
    duration_ms: u64,
    video: Option<DecodeThread>,
    audio_out: Option<AudioHandle>,
    audio_join: Option<JoinHandle<()>>,
    audio_stop: Arc<AtomicBool>,
    // u64::MAX 表示无待处理 seek; 否则为目标毫秒, 音频线程消费后复位。
    audio_seek: Arc<AtomicU64>,
    // 置位后音频回调清空环形缓冲里的陈旧样本(seek 后丢弃旧音频)。
    audio_flush: Arc<AtomicBool>,
    // 音频线程上报"音频已结束/缺失"(打开失败或解码到 EOF; seek 后复位)。
    // 引擎据此把主时钟无缝切到墙钟, 避免位置冻结(纯视频文件/音频先于视频结束)。
    audio_ended: Arc<AtomicBool>,
    // seek 闸门: 置位期间音频回调只出静音、不消费不计时(主时钟冻结)。
    audio_gate: Arc<AtomicBool>,
    seek_gate: Option<SeekGate>,
    subtitles: Option<subtitle::Subtitles>,
    sub_tracks: Vec<media::SubtitleTrack>,
    prefs: persist::Preferences,
    prefs_path: std::path::PathBuf,
    playback_ended: bool,
    // 统一播放时钟: 有音轨=音频主时钟(真实样本驱动), 无音轨=墙钟。取代旧的静态 fallback 位置。
    clock: PlayClock,
    // 当前显示帧(跨重绘保留)与已取出但未到点的未来帧(pending)。选帧逻辑现在在引擎里。
    current_frame: Option<media::VideoFrame>,
    current_frame_generation: u64,
    pending_frame: Option<media::VideoFrame>,
    present_drops: u32,
    dbg: DebugProbe,
    // 上次保存播放进度的时间戳，用于周期性保存
    last_save_instant: std::time::Instant,
}

impl Player {
    pub fn new() -> Self {
        Self {
            machine: StateMachine::new(),
            playlist: Playlist::new(),
            volume: 100,
            volume_shared: Arc::new(AtomicU8::new(100)),
            rate_pct: 100,
            rate_shared: Arc::new(AtomicU32::new(100)),
            muted: false,
            volume_before_mute: 100,
            duration_ms: 0,
            video: None,
            audio_out: None,
            audio_join: None,
            audio_stop: Arc::new(AtomicBool::new(false)),
            audio_seek: Arc::new(AtomicU64::new(u64::MAX)),
            audio_flush: Arc::new(AtomicBool::new(false)),
            audio_ended: Arc::new(AtomicBool::new(false)),
            audio_gate: Arc::new(AtomicBool::new(false)),
            seek_gate: None,
            subtitles: None,
            sub_tracks: Vec::new(),
            prefs: persist::Preferences::default(),
            prefs_path: std::path::PathBuf::new(),
            playback_ended: false,
            clock: PlayClock::Wall(WallClock::new()),
            current_frame: None,
            current_frame_generation: 0,
            pending_frame: None,
            present_drops: 0,
            dbg: DebugProbe::new(),
            last_save_instant: std::time::Instant::now(),
        }
    }

    /// 以指定路径加载偏好后构造 Player(续播位置/音量等从磁盘恢复)。
    pub fn with_prefs(prefs_path: std::path::PathBuf) -> Self {
        let prefs = persist::Preferences::load(&prefs_path).unwrap_or_default();
        let mut p = Self::new();
        p.volume = prefs.volume;
        p.volume_shared.store(prefs.volume, Ordering::Relaxed);
        p.prefs = prefs;
        p.prefs_path = prefs_path;
        // 恢复上次的播放列表与选中项(不自动开播, 仅恢复列表+选择)。
        if !p.prefs.last_playlist.is_empty() {
            let items: Vec<std::path::PathBuf> =
                p.prefs.last_playlist.iter().map(Into::into).collect();
            let idx = p.prefs.last_index;
            p.playlist.set_items(items, idx);
        }
        p
    }

    pub fn prefs(&self) -> &persist::Preferences {
        &self.prefs
    }

    /// 已解析的截图目录。支持把旧配置里的 `~` 展开为用户 Home。
    pub fn screenshot_dir(&self) -> std::path::PathBuf {
        persist::resolve_screenshot_dir(&self.prefs.screenshot_dir)
    }

    /// 启动时恢复上次选中的视频, seek 到记忆进度后保持暂停。
    /// 返回是否成功恢复(找到有效视频并打开)。
    pub fn restore_last_session_paused(&mut self) -> bool {
        let path = match self.playlist.current() {
            Some(p) => p.to_path_buf(),
            None => return false,
        };

        if !self.is_valid_video_file(&path) {
            return false;
        }

        self.open_media(&path);
        if self.video.is_none() {
            return false;
        }

        self.restore_playback_position(&path);
        self.pause_playback();
        true
    }

    fn is_valid_video_file(&self, path: &std::path::Path) -> bool {
        path.is_file() && is_video_ext(path)
    }

    fn restore_playback_position(&mut self, path: &std::path::Path) {
        if let Some(resume_ms) = self.prefs.resume_point(&path.to_string_lossy()) {
            if resume_ms > 0 && resume_ms < self.duration_ms {
                self.seek_to(resume_ms);
            }
        }
    }

    pub fn set_language(&mut self, v: &str) {
        self.prefs.language = v.to_string();
        self.persist_preferences();
    }
    pub fn set_seek_step(&mut self, secs: u64) {
        self.prefs.seek_step_secs = secs;
        self.persist_preferences();
    }
    pub fn set_theme(&mut self, v: &str) {
        self.prefs.theme = v.to_string();
        self.persist_preferences();
    }
    pub fn set_subtitle_font_size(&mut self, size: f32) {
        self.prefs.subtitle_font_size = size;
        self.persist_preferences();
    }
    pub fn set_playback_mode(&mut self, mode: persist::PlaybackMode) {
        self.prefs.playback_mode = mode;
        self.persist_preferences();
    }
    pub fn set_check_updates_on_startup(&mut self, enabled: bool) {
        self.prefs.check_updates_on_startup = enabled;
        if !enabled {
            // Beta checks are subordinate to startup checks; disabling the parent
            // preference clears the child so the settings UI cannot save a hidden
            // enabled state.
            self.prefs.check_beta_updates = false;
        }
        self.persist_preferences();
    }
    pub fn set_check_beta_updates(&mut self, enabled: bool) {
        self.prefs.check_beta_updates = enabled && self.prefs.check_updates_on_startup;
        self.persist_preferences();
    }
    pub fn set_screenshot_dir(&mut self, path: &str) {
        self.prefs.screenshot_dir = persist::resolve_screenshot_dir(path)
            .to_string_lossy()
            .into_owned();
        self.persist_preferences();
    }

    fn raw_position_ms(&self) -> u64 {
        self.clock.position_ms()
    }

    pub fn timeline(&self) -> Timeline {
        // UI consumers should never see a position beyond duration even if the
        // underlying clock briefly advances before end-of-playback handling runs.
        let position_ms = if self.duration_ms > 0 {
            self.raw_position_ms().min(self.duration_ms)
        } else {
            self.raw_position_ms()
        };
        Timeline {
            position_ms,
            duration_ms: self.duration_ms,
            state: self.machine.state(),
            volume: self.volume,
            rate_pct: self.rate_pct,
            muted: self.muted,
        }
    }

    /// 取视频解码线程句柄(供 UI 拉帧)。
    pub fn video(&self) -> Option<&DecodeThread> {
        self.video.as_ref()
    }

    pub fn current_video_dimensions(&self) -> Option<(u32, u32)> {
        self.video.as_ref().map(DecodeThread::dimensions)
    }

    /// 音频结束/缺失且音频钟已停摆时, 把主时钟无缝切到墙钟(从当前位置、当前倍速续走)。
    /// 覆盖两类场景: 纯视频文件(音频线程开流失败)与音频先于视频结束。
    fn maybe_handover_to_wall(&mut self) {
        if !self.audio_ended.load(Ordering::Relaxed) {
            return;
        }
        let PlayClock::Audio(mc) = &self.clock else {
            return;
        };
        if mc.stalled_for_ms() < AUDIO_STALL_HANDOVER_MS {
            return;
        }
        // Preserve current media time and rate when switching clocks so video-only
        // playback continues from the same timeline position.
        let now = std::time::Instant::now();
        let mut wc = WallClock::new();
        wc.reset_to_at(mc.position_ms(), now);
        wc.set_rate_at(self.rate_pct.max(1), now);
        // 非播放态或 seek 闸门挂起时, 新墙钟保持冻结(放行/恢复播放时再 resume)。
        if self.machine.state() != player_core::PlaybackState::Playing || self.seek_gate.is_some() {
            wc.pause_at(now);
        }
        self.clock = PlayClock::Wall(wc);
    }

    /// 当前显示帧的 RGBA (像素, 宽, 高), 供截图使用。
    pub fn current_frame_rgba(&self) -> Option<(&[u8], u32, u32)> {
        self.current_frame
            .as_ref()
            .map(|f| (f.rgba.as_slice(), f.width, f.height))
    }

    /// Monotonic generation of the cached visible frame. UI renderers use this
    /// to notice frames advanced while painting was suppressed.
    pub fn current_frame_generation(&self) -> u64 {
        self.current_frame_generation
    }

    /// 累计丢帧数(供调试 HUD, CP4 用)。
    pub fn present_drops(&self) -> u32 {
        self.present_drops
    }

    pub fn playlist_paths(&self) -> &[std::path::PathBuf] {
        self.playlist.as_slice()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.playlist.current_index()
    }

    /// 打开目录: 把目录内所有视频(排序后)作为播放列表, 从第一个开始播。
    pub fn open_folder(&mut self, dir: &Path) {
        let items = dir_videos(dir);
        if let Some(first) = items.first().cloned() {
            self.save_state();
            self.playlist.set_items(items, 0);
            self.open_media(&first);
            self.save_state();
        }
    }

    pub fn history(&self) -> &[String] {
        &self.prefs.history
    }

    pub fn current_subtitle(&self) -> Option<String> {
        let pos = self.timeline().position_ms;
        self.subtitles
            .as_ref()
            .and_then(|s| s.text_at(pos))
            .map(|t| t.to_string())
    }

    /// 手动加载字幕文件(拖入 .srt/.ass 时)。
    pub fn load_subtitle(&mut self, path: &Path) {
        if let Ok(s) = subtitle::load_file(path) {
            self.subtitles = Some(s);
        }
    }

    /// 当前文件内嵌的字幕轨道列表(供 UI 下拉)。
    pub fn subtitle_tracks(&self) -> &[media::SubtitleTrack] {
        &self.sub_tracks
    }

    pub fn tick(&mut self) {
        self.maybe_release_seek_gate();
        self.maybe_handover_to_wall();

        // 周期性保存播放进度（每10秒一次，仅在播放中保存）
        self.maybe_save_playback_progress();

        if !self.playback_reached_end() {
            return;
        }
        self.handle_playback_end();
    }

    fn playback_reached_end(&self) -> bool {
        self.machine.state() == player_core::PlaybackState::Playing
            && self.duration_ms > 0
            && self.raw_position_ms() >= self.duration_ms
    }

    fn handle_playback_end(&mut self) {
        // End handling is centralized here so loop modes, pause-at-end, and next
        // playlist item all share the same EOF detection.
        match end_playback_action(
            self.prefs.playback_mode,
            self.playlist.len(),
            self.playlist.current_index(),
        ) {
            EndPlaybackAction::PauseAtEnd => {
                self.clock.reset_to(self.duration_ms);
                if let Some(a) = &self.audio_out {
                    a.pause();
                }
                self.clock.pause();
                let _ = self.machine.apply(player_core::Transition::Pause);
                self.playback_ended = true;
            }
            EndPlaybackAction::RepeatCurrent => {
                self.seek_to(0);
                if let Some(a) = &self.audio_out {
                    a.resume();
                }
                // seek 闸门挂起时由放行逻辑统一恢复时钟, 此处不提前解冻。
                if self.seek_gate.is_none() {
                    self.clock.resume();
                }
            }
            EndPlaybackAction::OpenPlaylistIndex(index) => {
                self.open_playlist_index_after_end(index);
            }
        }
    }

    fn open_playlist_index_after_end(&mut self, index: usize) {
        self.save_state();
        self.playlist.set_cursor(index);
        if let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) {
            self.open_media(&path);
            self.save_state();
        }
    }

    /// 保存当前文件的续播位置与音量偏好到磁盘。
    pub fn save_state(&mut self) {
        // Save progress first so the persisted playlist and selected index always
        // refer to the same current item used for the resume key.
        self.save_playback_progress();
        self.prefs.volume = self.volume;
        self.prefs.last_playlist = self
            .playlist
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        self.prefs.last_index = self.playlist.current_index().unwrap_or(0);
        self.persist_preferences();
    }

    /// 仅保存当前播放进度（不保存播放列表等其他状态）
    fn save_playback_progress(&mut self) {
        if self.video.is_none() {
            return;
        }
        let Some(key) = self
            .playlist
            .current()
            .map(|p| p.to_string_lossy().to_string())
        else {
            return;
        };
        let pos = self.timeline().position_ms;
        self.prefs
            .set_resume_point(&key, saved_resume_point(pos, self.duration_ms));
    }

    /// 周期性保存播放进度（仅在播放中且距上次保存超过10秒时保存）
    fn maybe_save_playback_progress(&mut self) {
        if self.machine.state() != player_core::PlaybackState::Playing {
            return;
        }
        if self.video.is_none() {
            return;
        }
        let now = std::time::Instant::now();
        if now.duration_since(self.last_save_instant) < std::time::Duration::from_secs(10) {
            return;
        }
        self.save_playback_progress();
        self.persist_preferences();
        self.last_save_instant = now;
    }

    fn persist_preferences(&self) {
        if !self.prefs_path.as_os_str().is_empty() {
            let _ = self.prefs.save(&self.prefs_path);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.teardown();
    }
}

fn saved_resume_point(pos_ms: u64, duration_ms: u64) -> u64 {
    if duration_ms > 0 && pos_ms + 5000 < duration_ms {
        pos_ms
    } else {
        // 接近结尾(或时长未知): 清除续播点, 下次从头播。
        0
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "m4v", "flv", "ts"];

fn is_video_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn clamp_rate_pct(pct: u16) -> u16 {
    pct.clamp(25, 400)
}

fn dir_videos(dir: &Path) -> Vec<std::path::PathBuf> {
    // Directory opens are deterministic: collect supported files, sort paths, then
    // let the caller choose the initial cursor.
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_ext(p))
        .collect();
    out.sort();
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndPlaybackAction {
    PauseAtEnd,
    RepeatCurrent,
    OpenPlaylistIndex(usize),
}

fn end_playback_action(
    mode: persist::PlaybackMode,
    playlist_len: usize,
    current_index: Option<usize>,
) -> EndPlaybackAction {
    match mode {
        persist::PlaybackMode::StopAtEnd => EndPlaybackAction::PauseAtEnd,
        persist::PlaybackMode::RepeatOne => EndPlaybackAction::RepeatCurrent,
        persist::PlaybackMode::LoopPlaylist => {
            let Some(index) = current_index else {
                return EndPlaybackAction::RepeatCurrent;
            };
            if playlist_len <= 1 {
                EndPlaybackAction::RepeatCurrent
            } else {
                EndPlaybackAction::OpenPlaylistIndex((index + 1) % playlist_len)
            }
        }
    }
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
