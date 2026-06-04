use crate::decode_thread::DecodeThread;
use crate::timeline::Timeline;
use audio::{AudioHandle, AudioOutput};
use media::AudioDecoder;
use player_core::{Command, Playlist, StateMachine};
use ringbuf::traits::Producer;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

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
    subtitles: Option<subtitle::Subtitles>,
    sub_tracks: Vec<media::SubtitleTrack>,
    prefs: persist::Preferences,
    prefs_path: std::path::PathBuf,
    playback_ended: bool,
    fallback_position_ms: u64,
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
            subtitles: None,
            sub_tracks: Vec::new(),
            prefs: persist::Preferences::default(),
            prefs_path: std::path::PathBuf::new(),
            playback_ended: false,
            fallback_position_ms: 0,
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
    pub fn restore_last_session_paused(&mut self) -> bool {
        let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) else {
            return false;
        };
        if !path.is_file() || !is_video_ext(&path) {
            return false;
        }

        self.open(&path);
        if self.video.is_none() {
            return false;
        }
        self.pause_playback();
        true
    }

    pub fn set_language(&mut self, v: &str) {
        self.prefs.language = v.to_string();
    }
    pub fn set_seek_step(&mut self, secs: u64) {
        self.prefs.seek_step_secs = secs;
    }
    pub fn set_theme(&mut self, v: &str) {
        self.prefs.theme = v.to_string();
    }
    pub fn set_subtitle_font_size(&mut self, size: f32) {
        self.prefs.subtitle_font_size = size;
    }
    pub fn set_playback_mode(&mut self, mode: persist::PlaybackMode) {
        self.prefs.playback_mode = mode;
    }
    pub fn set_check_updates_on_startup(&mut self, enabled: bool) {
        self.prefs.check_updates_on_startup = enabled;
        if !enabled {
            self.prefs.check_beta_updates = false;
        }
    }
    pub fn set_check_beta_updates(&mut self, enabled: bool) {
        self.prefs.check_beta_updates = enabled && self.prefs.check_updates_on_startup;
    }
    pub fn set_screenshot_dir(&mut self, path: &str) {
        self.prefs.screenshot_dir = persist::resolve_screenshot_dir(path)
            .to_string_lossy()
            .into_owned();
    }

    fn raw_position_ms(&self) -> u64 {
        self.audio_out
            .as_ref()
            .map(|a| a.clock.position_ms())
            .unwrap_or(self.fallback_position_ms)
    }

    pub fn timeline(&self) -> Timeline {
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

    pub fn playlist_paths(&self) -> Vec<std::path::PathBuf> {
        self.playlist.iter().map(|p| p.to_path_buf()).collect()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.playlist.current_index()
    }

    /// 打开目录: 把目录内所有视频(排序后)作为播放列表, 从第一个开始播。
    pub fn open_folder(&mut self, dir: &Path) {
        let items = dir_videos(dir);
        if let Some(first) = items.first().cloned() {
            self.playlist.set_items(items, 0);
            self.open(&first);
        }
    }

    fn open_sibling_videos(&mut self, path: &Path) {
        let Some(dir) = path.parent() else {
            return;
        };
        let items = dir_videos(dir);
        let index = items.iter().position(|item| item == path).unwrap_or(0);
        if let Some(selected) = items.get(index).cloned() {
            self.playlist.set_items(items, index);
            self.open(&selected);
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

    fn handle_set_volume(&mut self, v: u8) {
        self.volume = v.min(100);
        self.volume_shared.store(v.min(100), Ordering::Relaxed);
    }

    fn seek_to(&mut self, ms: u64) {
        let target = if self.duration_ms > 0 {
            ms.min(self.duration_ms)
        } else {
            ms
        };
        self.fallback_position_ms = target;
        self.playback_ended = false;
        if let Some(v) = &self.video {
            v.request_seek(target);
            // 排空已缓冲的旧帧(尤其向后 seek 时, 旧帧 PTS 在目标之后不会被 decide_frame 丢弃)
            while v.try_recv_frame().is_some() {}
        }
        self.audio_seek.store(target, Ordering::Relaxed);
        // 丢弃环形缓冲里约 1s 的旧音频, 否则 seek 后旧声音还会续播一会儿。
        self.audio_flush.store(true, Ordering::Relaxed);
        if let Some(a) = &self.audio_out {
            a.clock.reset_to(target);
        }
    }

    fn pause_playback(&mut self) {
        if self.machine.apply(player_core::Transition::Pause).is_ok() {
            if let Some(a) = &self.audio_out {
                a.pause();
            }
        }
    }

    fn stop_playback(&mut self) {
        let _ = self.machine.apply(player_core::Transition::Stop);
        self.teardown();
    }

    fn remove_playlist_index(&mut self, index: usize) {
        let was_current = self.playlist.current_index() == Some(index);
        let had_media = self.video.is_some() || self.duration_ms > 0;

        if self.playlist.remove_index(index).is_none() {
            return;
        }

        if !was_current {
            return;
        }

        let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) else {
            self.stop_playback();
            return;
        };

        if had_media {
            self.open(&path);
            self.pause_playback();
        }
    }

    fn delete_playlist_file_index(&mut self, index: usize) {
        let Some(path) = self.playlist.iter().nth(index).map(|p| p.to_path_buf()) else {
            return;
        };
        self.remove_playlist_index(index);
        let _ = std::fs::remove_file(&path);
        let key = path.to_string_lossy().to_string();
        self.prefs.history.retain(|p| p != &key);
    }

    fn delete_history_file_index(&mut self, index: usize) {
        let Some(path) = self.prefs.history.get(index).cloned() else {
            return;
        };
        let path_buf = std::path::PathBuf::from(&path);
        if let Some(playlist_index) = self.playlist.iter().position(|p| p == &path_buf) {
            self.remove_playlist_index(playlist_index);
        }
        let _ = std::fs::remove_file(path_buf);
        self.prefs.history.retain(|p| p != &path);
    }

    pub fn tick(&mut self) {
        if self.machine.state() != player_core::PlaybackState::Playing || self.duration_ms == 0 {
            return;
        }
        if self.raw_position_ms() < self.duration_ms {
            return;
        }

        match end_playback_action(
            self.prefs.playback_mode,
            self.playlist.len(),
            self.playlist.current_index(),
        ) {
            EndPlaybackAction::PauseAtEnd => {
                if let Some(a) = &self.audio_out {
                    a.clock.reset_to(self.duration_ms);
                    a.pause();
                }
                let _ = self.machine.apply(player_core::Transition::Pause);
                self.playback_ended = true;
            }
            EndPlaybackAction::RepeatCurrent => {
                self.seek_to(0);
                if let Some(a) = &self.audio_out {
                    a.resume();
                }
            }
            EndPlaybackAction::OpenPlaylistIndex(index) => {
                self.playlist.set_cursor(index);
                if let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) {
                    self.open(&path);
                }
            }
        }
    }

    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Open(path) => {
                self.playlist.set_items(vec![path.clone()], 0);
                self.open(&path);
            }
            Command::OpenFiles(paths) => {
                let items: Vec<_> = paths
                    .into_iter()
                    .filter(|path| path.is_file() && is_video_ext(path))
                    .collect();
                if let Some(first) = items.first().cloned() {
                    self.playlist.set_items(items, 0);
                    self.open(&first);
                }
            }
            Command::Play => {
                if self.video.is_some() && self.machine.apply(player_core::Transition::Play).is_ok()
                {
                    if self.playback_ended {
                        self.seek_to(0);
                    }
                    if let Some(a) = &self.audio_out {
                        a.resume();
                    }
                }
            }
            Command::Pause => {
                self.pause_playback();
            }
            Command::Stop => {
                self.stop_playback();
            }
            Command::SetVolume(v) => {
                self.muted = false;
                self.handle_set_volume(v);
            }
            Command::ToggleMute => {
                if self.muted {
                    self.muted = false;
                    self.handle_set_volume(self.volume_before_mute);
                } else {
                    self.muted = true;
                    self.volume_before_mute = self.volume;
                    self.handle_set_volume(0);
                }
            }
            Command::OpenDialog => {}
            Command::OpenFolder => {}
            Command::SeekTo(ms) => {
                self.seek_to(ms);
            }
            Command::SetRate(pct) => {
                let pct = clamp_rate_pct(pct);
                self.rate_pct = pct;
                self.rate_shared.store(pct as u32, Ordering::Relaxed);
                if let Some(a) = &self.audio_out {
                    a.clock.set_rate(pct);
                    self.audio_flush.store(true, Ordering::Relaxed);
                }
            }
            Command::Next => {
                if let Some(p) = self.playlist.next().map(|p| p.to_path_buf()) {
                    self.open(&p);
                }
            }
            Command::Prev => {
                if let Some(p) = self.playlist.prev().map(|p| p.to_path_buf()) {
                    self.open(&p);
                }
            }
            Command::PlayIndex(i) => {
                self.playlist.set_cursor(i);
                if let Some(p) = self.playlist.current().map(|p| p.to_path_buf()) {
                    self.open(&p);
                }
            }
            Command::RemovePlaylistIndex(i) => {
                self.remove_playlist_index(i);
            }
            Command::ClearPlaylist => {
                self.playlist.clear();
                self.stop_playback();
            }
            Command::RemoveHistoryIndex(i) => {
                player_core::remove_history_index(&mut self.prefs.history, i);
            }
            Command::ClearHistory => {
                player_core::clear_history(&mut self.prefs.history);
            }
            Command::RevealFile(_) => {}
            Command::OpenSiblingVideos(path) => {
                self.open_sibling_videos(&path);
            }
            Command::DeletePlaylistFileIndex(i) => {
                self.delete_playlist_file_index(i);
            }
            Command::DeleteHistoryFileIndex(i) => {
                self.delete_history_file_index(i);
            }
            Command::SelectSubtitleTrack(idx) => {
                if let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) {
                    if let Ok(s) = media::decode_text_subtitle(&path, idx) {
                        self.subtitles = Some(s);
                    }
                }
            }
        }
    }

    /// 保存当前文件的续播位置与音量偏好到磁盘。
    pub fn save_state(&mut self) {
        let key = self
            .playlist
            .current()
            .map(|p| p.to_string_lossy().to_string());
        if let Some(key) = key {
            let pos = self.timeline().position_ms;
            if self.duration_ms > 0 && pos + 5000 < self.duration_ms {
                self.prefs.set_resume_point(&key, pos);
            } else {
                // 接近结尾(或时长未知): 清除续播点, 下次从头播。
                self.prefs.set_resume_point(&key, 0);
            }
        }
        self.prefs.volume = self.volume;
        self.prefs.last_playlist = self
            .playlist
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        self.prefs.last_index = self.playlist.current_index().unwrap_or(0);
        let _ = self.prefs.save(&self.prefs_path);
    }

    fn open(&mut self, path: &Path) {
        self.teardown();
        self.playback_ended = false;
        self.fallback_position_ms = 0;

        let video = match DecodeThread::spawn(path, 16) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("打开视频失败: {e}");
                return;
            }
        };

        match AudioOutput::start(self.volume_shared.clone(), self.audio_flush.clone()) {
            Ok(out) => {
                out.clock.set_rate(self.rate_pct);
                let output_rate = out.sample_rate;
                let (handle, mut producer) = out.split();
                self.duration_ms = probe_duration_ms(path).unwrap_or(0);
                let apath = path.to_path_buf();
                let stop = Arc::new(AtomicBool::new(false));
                let stop_t = stop.clone();
                let seek_t = self.audio_seek.clone();
                let rate_t = self.rate_shared.clone();
                let join = std::thread::spawn(move || {
                    let mut adec = match AudioDecoder::open(&apath) {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    let mut converter = audio::PlaybackRateConverter::new(
                        adec.channels(),
                        adec.sample_rate(),
                        output_rate,
                    );
                    let mut current_rate = rate_t.load(Ordering::Relaxed).clamp(25, 400) as u16;
                    converter.set_rate(current_rate);
                    'outer: while !stop_t.load(Ordering::Relaxed) {
                        // 消费待处理 seek; swap 保证仅触发一次。
                        let st = seek_t.swap(u64::MAX, Ordering::Relaxed);
                        if st != u64::MAX {
                            let _ = adec.seek_ms(st);
                            converter.reset();
                        }
                        let requested_rate = rate_t.load(Ordering::Relaxed).clamp(25, 400) as u16;
                        if requested_rate != current_rate {
                            current_rate = requested_rate;
                            converter.set_rate(current_rate);
                        }
                        match adec.next_chunk() {
                            Ok(Some(chunk)) => {
                                let buf = converter.convert(&chunk.samples);
                                let mut i = 0;
                                while i < buf.len() {
                                    if stop_t.load(Ordering::Relaxed) {
                                        break 'outer;
                                    }
                                    // 新 seek 到来时立即放弃当前(旧位置)这块样本, 回到循环顶部去 seek,
                                    // 配合回调 flush 让新位置音频尽快接上。
                                    if seek_t.load(Ordering::Relaxed) != u64::MAX {
                                        continue 'outer;
                                    }
                                    if rate_t.load(Ordering::Relaxed).clamp(25, 400) as u16
                                        != current_rate
                                    {
                                        continue 'outer;
                                    }
                                    if producer.try_push(buf[i]).is_ok() {
                                        i += 1;
                                    } else {
                                        std::thread::sleep(std::time::Duration::from_millis(2));
                                    }
                                }
                            }
                            Ok(None) | Err(_) => {
                                std::thread::sleep(std::time::Duration::from_millis(10));
                            }
                        }
                    }
                });
                self.audio_out = Some(handle);
                self.audio_join = Some(join);
                self.audio_stop = stop;
            }
            Err(e) => {
                eprintln!("启动音频失败: {e}, 仅播放视频(静音)");
                self.duration_ms = probe_duration_ms(path).unwrap_or(0);
            }
        }

        self.video = Some(video);
        let history_key = path.to_string_lossy().to_string();
        player_core::push_history(&mut self.prefs.history, &history_key, 50);
        self.subtitles = sidecar_subtitle(path);
        self.sub_tracks = media::list_subtitle_tracks(path).unwrap_or_default();
        let _ = self.machine.apply(player_core::Transition::Play);

        // 续播: 若该文件记录了进度, 打开后直接 seek 到该位置。
        let key = path.to_string_lossy().to_string();
        if let Some(ms) = self.prefs.resume_point(&key) {
            if ms > 0 {
                self.seek_to(ms);
            }
        }
    }

    fn teardown(&mut self) {
        self.playback_ended = false;
        self.audio_stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.audio_join.take() {
            let _ = j.join();
        }
        self.audio_out = None;
        if let Some(v) = self.video.take() {
            v.stop();
        }
        self.duration_ms = 0;
        self.fallback_position_ms = 0;
        self.audio_stop = Arc::new(AtomicBool::new(false));
        self.audio_seek = Arc::new(AtomicU64::new(u64::MAX));
        self.audio_flush = Arc::new(AtomicBool::new(false));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.teardown();
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

fn sidecar_subtitle(video: &Path) -> Option<subtitle::Subtitles> {
    for ext in ["srt", "ass", "ssa"] {
        let p = video.with_extension(ext);
        if p.exists() {
            if let Ok(s) = subtitle::load_file(&p) {
                return Some(s);
            }
        }
    }
    None
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

fn probe_duration_ms(path: &Path) -> Option<u64> {
    use ffmpeg_next as ff;
    ff::init().ok()?;
    let ictx = ff::format::input(&path).ok()?;
    let dur = ictx.duration(); // AV_TIME_BASE (微秒)
    if dur > 0 {
        Some((dur as u64) / 1000)
    } else {
        None
    }
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
mod tests {
    use super::*;
    use player_core::{Command, PlaybackState};

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/tests/fixtures/sample.mp4")
    }

    #[test]
    fn new_player_is_stopped_with_default_volume() {
        let p = Player::new();
        let t = p.timeline();
        assert_eq!(t.state, PlaybackState::Stopped);
        assert_eq!(t.volume, 100);
        assert_eq!(t.rate_pct, 100);
    }

    #[test]
    fn set_volume_command_updates_timeline() {
        let mut p = Player::new();
        p.handle(Command::SetVolume(40));
        assert_eq!(p.timeline().volume, 40);
    }

    #[test]
    fn set_rate_command_updates_timeline_even_without_media() {
        let mut p = Player::new();
        p.handle(Command::SetRate(175));
        assert_eq!(p.timeline().rate_pct, 175);
    }

    #[test]
    fn current_video_dimensions_follow_opened_media() {
        let path = fixture();
        assert!(path.exists(), "先运行 media 的 gen_fixture.sh");

        let mut p = Player::new();
        assert_eq!(p.current_video_dimensions(), None);

        p.handle(Command::Open(path));

        assert_eq!(p.current_video_dimensions(), Some((160, 120)));
        p.handle(Command::Stop);
        assert_eq!(p.current_video_dimensions(), None);
    }

    #[test]
    fn play_without_media_stays_stopped() {
        let mut p = Player::new();
        p.handle(Command::Play);
        assert_eq!(p.timeline().state, PlaybackState::Stopped);
    }

    #[test]
    fn open_file_command_does_not_expand_to_sibling_videos() {
        let dir = std::env::temp_dir().join(format!(
            "morn_open_single_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures/sample.mp4")
            .canonicalize()
            .expect("先运行 media 的 gen_fixture.sh");
        let a = dir.join("a.mp4");
        let b = dir.join("b.mp4");
        std::fs::copy(&sample, &a).unwrap();
        std::fs::copy(&sample, &b).unwrap();

        let mut p = Player::new();
        p.handle(Command::Open(a.clone()));

        assert_eq!(p.playlist_paths(), vec![a]);
        p.handle(Command::Stop);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn open_files_command_uses_selected_files_only() {
        let dir = std::env::temp_dir().join(format!(
            "morn_open_files_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures/sample.mp4")
            .canonicalize()
            .expect("先运行 media 的 gen_fixture.sh");
        let a = dir.join("a.mp4");
        let b = dir.join("b.mp4");
        let c = dir.join("c.mp4");
        std::fs::copy(&sample, &a).unwrap();
        std::fs::copy(&sample, &b).unwrap();
        std::fs::copy(&sample, &c).unwrap();

        let mut p = Player::new();
        p.handle(Command::OpenFiles(vec![b.clone(), a.clone()]));

        assert_eq!(p.playlist_paths(), vec![b, a]);
        assert_eq!(p.current_index(), Some(0));
        p.handle(Command::Stop);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn open_sibling_videos_loads_directory_and_selects_requested_file() {
        let dir = std::env::temp_dir().join(format!(
            "morn_open_siblings_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.mp4");
        let b = dir.join("b.mp4");
        let c = dir.join("c.mp4");
        let note = dir.join("note.txt");
        std::fs::write(&a, b"dummy").unwrap();
        std::fs::write(&b, b"dummy").unwrap();
        std::fs::write(&c, b"dummy").unwrap();
        std::fs::write(note, b"not a video").unwrap();

        let mut p = Player::new();
        p.handle(Command::OpenSiblingVideos(b.clone()));

        assert_eq!(p.playlist_paths(), vec![a, b, c]);
        assert_eq!(p.current_index(), Some(1));
        p.handle(Command::Stop);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn remove_playlist_index_updates_restored_playlist_without_opening_media() {
        let dir = std::env::temp_dir().join(format!(
            "morn_remove_restored_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let prefs_path = dir.join("prefs.json");
        let mut prefs = persist::Preferences::default();
        prefs.last_playlist = vec!["/a.mp4".into(), "/b.mp4".into(), "/c.mp4".into()];
        prefs.last_index = 1;
        prefs.save(&prefs_path).unwrap();

        let mut p = Player::with_prefs(prefs_path);
        p.handle(Command::RemovePlaylistIndex(0));

        assert_eq!(
            p.playlist_paths(),
            vec![
                std::path::PathBuf::from("/b.mp4"),
                std::path::PathBuf::from("/c.mp4")
            ]
        );
        assert_eq!(p.current_index(), Some(0));
        assert_eq!(p.timeline().state, PlaybackState::Stopped);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn removing_current_playing_item_switches_to_adjacent_paused() {
        let dir = std::env::temp_dir().join(format!(
            "morn_remove_current_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures/sample.mp4")
            .canonicalize()
            .expect("先运行 media 的 gen_fixture.sh");
        let a = dir.join("a.mp4");
        let b = dir.join("b.mp4");
        std::fs::copy(&sample, &a).unwrap();
        std::fs::copy(&sample, &b).unwrap();

        let mut p = Player::new();
        p.handle(Command::OpenFiles(vec![a.clone(), b.clone()]));
        assert_eq!(p.current_index(), Some(0));
        assert_eq!(p.timeline().state, PlaybackState::Playing);

        p.handle(Command::RemovePlaylistIndex(0));

        assert_eq!(p.playlist_paths(), vec![b]);
        assert_eq!(p.current_index(), Some(0));
        assert_eq!(p.timeline().state, PlaybackState::Paused);
        p.handle(Command::Stop);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn clear_playlist_stops_and_removes_items() {
        let dir = std::env::temp_dir().join(format!(
            "morn_clear_playlist_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures/sample.mp4")
            .canonicalize()
            .expect("先运行 media 的 gen_fixture.sh");
        let a = dir.join("a.mp4");
        std::fs::copy(&sample, &a).unwrap();

        let mut p = Player::new();
        p.handle(Command::Open(a));
        assert_eq!(p.timeline().state, PlaybackState::Playing);

        p.handle(Command::ClearPlaylist);

        assert!(p.playlist_paths().is_empty());
        assert_eq!(p.current_index(), None);
        assert_eq!(p.timeline().state, PlaybackState::Stopped);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn history_remove_and_clear_commands_update_history_only() {
        let dir = std::env::temp_dir().join(format!(
            "morn_history_remove_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let prefs_path = dir.join("prefs.json");
        let mut prefs = persist::Preferences::default();
        prefs.history = vec!["/a.mp4".into(), "/b.mp4".into(), "/c.mp4".into()];
        prefs.save(&prefs_path).unwrap();

        let mut p = Player::with_prefs(prefs_path);
        p.handle(Command::RemoveHistoryIndex(1));
        assert_eq!(p.history(), &["/a.mp4".to_string(), "/c.mp4".to_string()]);

        p.handle(Command::ClearHistory);
        assert!(p.history().is_empty());
        assert_eq!(p.timeline().state, PlaybackState::Stopped);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn delete_current_playlist_file_removes_disk_file_and_switches_to_adjacent_paused() {
        let dir = std::env::temp_dir().join(format!(
            "morn_delete_current_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures/sample.mp4")
            .canonicalize()
            .expect("先运行 media 的 gen_fixture.sh");
        let a = dir.join("a.mp4");
        let b = dir.join("b.mp4");
        std::fs::copy(&sample, &a).unwrap();
        std::fs::copy(&sample, &b).unwrap();

        let mut p = Player::new();
        p.handle(Command::OpenFiles(vec![a.clone(), b.clone()]));
        assert_eq!(p.current_index(), Some(0));

        p.handle(Command::DeletePlaylistFileIndex(0));

        assert!(!a.exists());
        assert_eq!(p.playlist_paths(), vec![b]);
        assert_eq!(p.current_index(), Some(0));
        assert_eq!(p.timeline().state, PlaybackState::Paused);
        p.handle(Command::Stop);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn delete_history_file_removes_disk_file_and_history_entry() {
        let dir = std::env::temp_dir().join(format!(
            "morn_delete_history_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("old.mp4");
        std::fs::write(&file, b"old").unwrap();
        let prefs_path = dir.join("prefs.json");
        let mut prefs = persist::Preferences::default();
        prefs.history = vec![file.to_string_lossy().to_string()];
        prefs.save(&prefs_path).unwrap();

        let mut p = Player::with_prefs(prefs_path);
        p.handle(Command::DeleteHistoryFileIndex(0));

        assert!(!file.exists());
        assert!(p.history().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn setters_update_prefs() {
        let mut p = Player::new();
        p.set_seek_step(20);
        p.set_language("en");
        p.set_theme("dark");
        p.set_subtitle_font_size(32.0);
        p.set_playback_mode(persist::PlaybackMode::LoopPlaylist);
        p.set_check_updates_on_startup(true);
        p.set_check_beta_updates(true);
        p.set_screenshot_dir("/tmp/morn-shots");
        assert_eq!(p.prefs().seek_step_secs, 20);
        assert_eq!(p.prefs().language, "en");
        assert_eq!(p.prefs().theme, "dark");
        assert_eq!(p.prefs().subtitle_font_size, 32.0);
        assert_eq!(p.prefs().playback_mode, persist::PlaybackMode::LoopPlaylist);
        assert!(p.prefs().check_updates_on_startup);
        assert!(p.prefs().check_beta_updates);
        assert_eq!(p.prefs().screenshot_dir, "/tmp/morn-shots");

        p.set_check_updates_on_startup(false);
        assert!(!p.prefs().check_updates_on_startup);
        assert!(
            !p.prefs().check_beta_updates,
            "beta updates cannot stay enabled when startup checks are disabled"
        );
    }

    #[test]
    fn player_exposes_screenshot_directory_preference_setter() {
        let source = include_str!("player.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("set_screenshot_dir"));
        assert!(source.contains("screenshot_dir(&self)"));
        assert!(source.contains("prefs.screenshot_dir"));
    }

    #[test]
    fn screenshot_directory_accessors_resolve_legacy_tilde_path() {
        let mut p = Player::new();

        p.set_screenshot_dir("~\\Pictures\\Morn");

        assert_eq!(
            p.prefs().screenshot_dir,
            persist::resolve_screenshot_dir("~\\Pictures\\Morn").to_string_lossy()
        );
        assert_eq!(
            p.screenshot_dir(),
            persist::resolve_screenshot_dir("~\\Pictures\\Morn")
        );
    }

    #[test]
    fn stop_mode_pauses_at_end() {
        assert_eq!(
            super::end_playback_action(persist::PlaybackMode::StopAtEnd, 2, Some(0)),
            super::EndPlaybackAction::PauseAtEnd
        );
    }

    #[test]
    fn repeat_mode_restarts_current_item() {
        assert_eq!(
            super::end_playback_action(persist::PlaybackMode::RepeatOne, 2, Some(0)),
            super::EndPlaybackAction::RepeatCurrent
        );
    }

    #[test]
    fn loop_mode_advances_and_wraps_playlist() {
        assert_eq!(
            super::end_playback_action(persist::PlaybackMode::LoopPlaylist, 3, Some(0)),
            super::EndPlaybackAction::OpenPlaylistIndex(1)
        );
        assert_eq!(
            super::end_playback_action(persist::PlaybackMode::LoopPlaylist, 3, Some(2)),
            super::EndPlaybackAction::OpenPlaylistIndex(0)
        );
        assert_eq!(
            super::end_playback_action(persist::PlaybackMode::LoopPlaylist, 1, Some(0)),
            super::EndPlaybackAction::RepeatCurrent
        );
    }

    #[test]
    fn is_video_ext_filters() {
        assert!(super::is_video_ext(std::path::Path::new("/x/a.mp4")));
        assert!(super::is_video_ext(std::path::Path::new("/x/a.MKV")));
        assert!(!super::is_video_ext(std::path::Path::new("/x/a.txt")));
        assert!(!super::is_video_ext(std::path::Path::new("/x/a")));
    }

    #[test]
    fn dir_videos_lists_sorted() {
        let dir = std::env::temp_dir().join(format!("morn_dir_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["b.mp4", "a.mp4", "note.txt", "c.mkv"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        let got = super::dir_videos(&dir);
        let names: Vec<_> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a.mp4", "b.mp4", "c.mkv"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restore_last_session_opens_last_video_paused_at_resume_point() {
        let video = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures/sample.mp4")
            .canonicalize()
            .expect("先运行 media 的 gen_fixture.sh");
        let dir = std::env::temp_dir().join(format!(
            "morn_restore_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let prefs_path = dir.join("prefs.json");
        let key = video.to_string_lossy().to_string();
        let mut prefs = persist::Preferences::default();
        prefs.last_playlist = vec![key.clone()];
        prefs.last_index = 0;
        prefs.set_resume_point(&key, 500);
        prefs.save(&prefs_path).unwrap();

        let mut player = Player::with_prefs(prefs_path);
        assert!(player.restore_last_session_paused());

        let timeline = player.timeline();
        assert_eq!(timeline.state, PlaybackState::Paused);
        assert!(
            (500..=800).contains(&timeline.position_ms),
            "expected restore near 500ms, got {}",
            timeline.position_ms
        );

        player.handle(Command::Stop);
        std::fs::remove_dir_all(dir).ok();
    }
}
