use crate::decode_thread::DecodeThread;
use crate::timeline::Timeline;
use audio::{AudioHandle, AudioOutput};
use media::AudioDecoder;
use player_core::{Command, Playlist, StateMachine};
use ringbuf::traits::Producer;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct Player {
    machine: StateMachine,
    playlist: Playlist,
    volume: u8,
    volume_shared: Arc<AtomicU8>,
    muted: bool,
    volume_before_mute: u8,
    duration_ms: u64,
    video: Option<DecodeThread>,
    audio_out: Option<AudioHandle>,
    audio_join: Option<JoinHandle<()>>,
    audio_stop: Arc<AtomicBool>,
    // u64::MAX 表示无待处理 seek; 否则为目标毫秒, 音频线程消费后复位。
    audio_seek: Arc<AtomicU64>,
    subtitles: Option<subtitle::Subtitles>,
    sub_tracks: Vec<media::SubtitleTrack>,
    loop_a: Option<u64>,
    loop_b: Option<u64>,
    prefs: persist::Preferences,
    prefs_path: std::path::PathBuf,
}

impl Player {
    pub fn new() -> Self {
        Self {
            machine: StateMachine::new(),
            playlist: Playlist::new(),
            volume: 100,
            volume_shared: Arc::new(AtomicU8::new(100)),
            muted: false,
            volume_before_mute: 100,
            duration_ms: 0,
            video: None,
            audio_out: None,
            audio_join: None,
            audio_stop: Arc::new(AtomicBool::new(false)),
            audio_seek: Arc::new(AtomicU64::new(u64::MAX)),
            subtitles: None,
            sub_tracks: Vec::new(),
            loop_a: None,
            loop_b: None,
            prefs: persist::Preferences::default(),
            prefs_path: std::path::PathBuf::new(),
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

    pub fn timeline(&self) -> Timeline {
        let position_ms = self
            .audio_out
            .as_ref()
            .map(|a| a.clock.position_ms())
            .unwrap_or(0);
        Timeline {
            position_ms,
            duration_ms: self.duration_ms,
            state: self.machine.state(),
            volume: self.volume,
            hardware_decode: self
                .video
                .as_ref()
                .map(|v| v.is_hardware())
                .unwrap_or(false),
            muted: self.muted,
        }
    }

    /// 取视频解码线程句柄(供 UI 拉帧)。
    pub fn video(&self) -> Option<&DecodeThread> {
        self.video.as_ref()
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

    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Open(path) => {
                let items = sibling_videos(&path);
                let idx = items.iter().position(|p| *p == path).unwrap_or(0);
                self.playlist.set_items(items, idx);
                self.open(&path);
            }
            Command::Play => {
                if self.video.is_some() && self.machine.apply(player_core::Transition::Play).is_ok()
                {
                    if let Some(a) = &self.audio_out {
                        a.resume();
                    }
                }
            }
            Command::Pause => {
                if self.machine.apply(player_core::Transition::Pause).is_ok() {
                    if let Some(a) = &self.audio_out {
                        a.pause();
                    }
                }
            }
            Command::Stop => {
                let _ = self.machine.apply(player_core::Transition::Stop);
                self.teardown();
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
                if let Some(v) = &self.video {
                    v.request_seek(ms);
                    // 排空已缓冲的旧帧(尤其向后 seek 时, 旧帧 PTS 在目标之后不会被 decide_frame 丢弃)
                    while v.try_recv_frame().is_some() {}
                }
                self.audio_seek.store(ms, Ordering::Relaxed);
                if let Some(a) = &self.audio_out {
                    a.clock.reset_to(ms);
                }
            }
            Command::SetRate(pct) => {
                if let Some(a) = &self.audio_out {
                    a.clock.set_rate(pct);
                }
            }
            Command::StepFrame => self.step_frame(),
            Command::SetLoopA => self.loop_a = Some(self.timeline().position_ms),
            Command::SetLoopB => self.loop_b = Some(self.timeline().position_ms),
            Command::ClearLoop => {
                self.loop_a = None;
                self.loop_b = None;
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
            Command::SelectSubtitleTrack(idx) => {
                if let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) {
                    if let Ok(s) = media::decode_text_subtitle(&path, idx) {
                        self.subtitles = Some(s);
                    }
                }
            }
        }
    }

    /// 暂停状态下手动前进一帧(假设约 30fps → 33ms)。
    pub fn step_frame(&mut self) {
        if self.machine.state() == player_core::PlaybackState::Paused {
            let pos = self.timeline().position_ms;
            if let Some(a) = &self.audio_out {
                a.clock.reset_to(pos + 33);
            }
        }
    }

    /// UI 每帧调用: 到达 B 点则跳回 A 点(AB 循环)。
    pub fn tick(&mut self) {
        if let (Some(a), Some(b)) = (self.loop_a, self.loop_b) {
            if self.timeline().position_ms >= b {
                self.handle(player_core::Command::SeekTo(a));
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

        let video = match DecodeThread::spawn(path, 16) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("打开视频失败: {e}");
                return;
            }
        };

        match AudioOutput::start() {
            Ok(out) => {
                let (handle, mut producer) = out.split();
                self.duration_ms = probe_duration_ms(path).unwrap_or(0);
                let apath = path.to_path_buf();
                let stop = Arc::new(AtomicBool::new(false));
                let stop_t = stop.clone();
                let vol_shared = self.volume_shared.clone();
                let seek_t = self.audio_seek.clone();
                let join = std::thread::spawn(move || {
                    let mut adec = match AudioDecoder::open(&apath) {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    'outer: while !stop_t.load(Ordering::Relaxed) {
                        // 消费待处理 seek; swap 保证仅触发一次。
                        // 已知局限: cpal ringbuf 中约 1s 旧样本会在新位置音频追上前短暂续播,
                        // 精确清空 ringbuf 推迟到后续任务。
                        let st = seek_t.swap(u64::MAX, Ordering::Relaxed);
                        if st != u64::MAX {
                            let _ = adec.seek_ms(st);
                        }
                        match adec.next_chunk() {
                            Ok(Some(chunk)) => {
                                let mut buf = chunk.samples;
                                audio::apply_gain(&mut buf, vol_shared.load(Ordering::Relaxed));
                                let mut i = 0;
                                while i < buf.len() {
                                    if stop_t.load(Ordering::Relaxed) {
                                        break 'outer;
                                    }
                                    if producer.try_push(buf[i]).is_ok() {
                                        i += 1;
                                    } else {
                                        std::thread::sleep(std::time::Duration::from_millis(2));
                                    }
                                }
                            }
                            Ok(None) | Err(_) => break,
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

        // 续播: 若该文件记录了有意义的进度(>3s), 打开后直接 seek 到该位置。
        let key = path.to_string_lossy().to_string();
        if let Some(ms) = self.prefs.resume_point(&key) {
            if ms > 3000 {
                if let Some(v) = &self.video {
                    v.request_seek(ms);
                }
                self.audio_seek.store(ms, Ordering::Relaxed);
                if let Some(a) = &self.audio_out {
                    a.clock.reset_to(ms);
                }
            }
        }
    }

    fn teardown(&mut self) {
        self.audio_stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.audio_join.take() {
            let _ = j.join();
        }
        self.audio_out = None;
        if let Some(v) = self.video.take() {
            v.stop();
        }
        self.duration_ms = 0;
        self.audio_stop = Arc::new(AtomicBool::new(false));
        self.audio_seek = Arc::new(AtomicU64::new(u64::MAX));
        // AB 循环点属于单个文件; 切换/停止时清除, 避免 tick() 用旧文件的点误 seek。
        self.loop_a = None;
        self.loop_b = None;
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

/// `video` 所在目录下所有视频(按文件名排序)。读失败/空则返回 [video]。
fn sibling_videos(video: &Path) -> Vec<std::path::PathBuf> {
    let Some(dir) = video.parent() else {
        return vec![video.to_path_buf()];
    };
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_ext(p))
        .collect();
    if out.is_empty() {
        return vec![video.to_path_buf()];
    }
    out.sort();
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use player_core::{Command, PlaybackState};

    #[test]
    fn new_player_is_stopped_with_default_volume() {
        let p = Player::new();
        let t = p.timeline();
        assert_eq!(t.state, PlaybackState::Stopped);
        assert_eq!(t.volume, 100);
    }

    #[test]
    fn set_volume_command_updates_timeline() {
        let mut p = Player::new();
        p.handle(Command::SetVolume(40));
        assert_eq!(p.timeline().volume, 40);
    }

    #[test]
    fn play_without_media_stays_stopped() {
        let mut p = Player::new();
        p.handle(Command::Play);
        assert_eq!(p.timeline().state, PlaybackState::Stopped);
    }

    #[test]
    fn setters_update_prefs() {
        let mut p = Player::new();
        p.set_seek_step(20);
        p.set_language("en");
        p.set_theme("dark");
        p.set_subtitle_font_size(32.0);
        assert_eq!(p.prefs().seek_step_secs, 20);
        assert_eq!(p.prefs().language, "en");
        assert_eq!(p.prefs().theme, "dark");
        assert_eq!(p.prefs().subtitle_font_size, 32.0);
    }

    #[test]
    fn is_video_ext_filters() {
        assert!(super::is_video_ext(std::path::Path::new("/x/a.mp4")));
        assert!(super::is_video_ext(std::path::Path::new("/x/a.MKV")));
        assert!(!super::is_video_ext(std::path::Path::new("/x/a.txt")));
        assert!(!super::is_video_ext(std::path::Path::new("/x/a")));
    }

    #[test]
    fn sibling_videos_lists_sorted() {
        let dir = std::env::temp_dir().join(format!("morn_sib_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["b.mp4", "a.mp4", "note.txt", "c.mkv"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        let target = dir.join("a.mp4");
        let got = super::sibling_videos(&target);
        let names: Vec<_> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a.mp4", "b.mp4", "c.mkv"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
