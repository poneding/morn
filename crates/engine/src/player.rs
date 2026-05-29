use crate::decode_thread::DecodeThread;
use crate::timeline::Timeline;
use audio::{AudioHandle, AudioOutput};
use media::AudioDecoder;
use player_core::{Command, Playlist, StateMachine};
use ringbuf::traits::Producer;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct Player {
    machine: StateMachine,
    // 预留: Next/Prev 命令将操作播放列表(当前为空实现)。
    #[allow(dead_code)]
    playlist: Playlist,
    volume: u8,
    duration_ms: u64,
    video: Option<DecodeThread>,
    audio_out: Option<AudioHandle>,
    audio_join: Option<JoinHandle<()>>,
    audio_stop: Arc<AtomicBool>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            machine: StateMachine::new(),
            playlist: Playlist::new(),
            volume: 100,
            duration_ms: 0,
            video: None,
            audio_out: None,
            audio_join: None,
            audio_stop: Arc::new(AtomicBool::new(false)),
        }
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
        }
    }

    /// 取视频解码线程句柄(供 UI 拉帧)。
    pub fn video(&self) -> Option<&DecodeThread> {
        self.video.as_ref()
    }

    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Open(path) => self.open(&path),
            Command::Play => {
                if self.video.is_some() {
                    let _ = self.machine.apply(player_core::Transition::Play);
                }
            }
            Command::Pause => {
                let _ = self.machine.apply(player_core::Transition::Pause);
            }
            Command::Stop => {
                let _ = self.machine.apply(player_core::Transition::Stop);
                self.teardown();
            }
            Command::SetVolume(v) => self.volume = v.min(100),
            Command::SeekTo(_ms) => {}
            Command::SetRate(_) | Command::Next | Command::Prev => {}
        }
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
                let join = std::thread::spawn(move || {
                    let mut adec = match AudioDecoder::open(&apath) {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    'outer: while !stop_t.load(Ordering::Relaxed) {
                        match adec.next_chunk() {
                            Ok(Some(chunk)) => {
                                let mut i = 0;
                                while i < chunk.samples.len() {
                                    if stop_t.load(Ordering::Relaxed) {
                                        break 'outer;
                                    }
                                    if producer.try_push(chunk.samples[i]).is_ok() {
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
        let _ = self.machine.apply(player_core::Transition::Play);
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
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
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
}
