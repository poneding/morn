//! Media open, teardown, and audio decode orchestration.
//!
//! Opening a file resets the previous playback graph, starts video decoding, then
//! chooses either an audio-backed master clock or a wall-clock fallback.  The audio
//! thread owns sample conversion and clock anchoring so the UI thread can keep
//! presenting frames without blocking on decoder I/O.
//!
//! Seek and rate changes are delivered through atomics.  The decode loop checks
//! those controls between chunks and while pushing samples, which lets it abandon
//! stale audio quickly after a user seek while preserving precise anchoring at the
//! first chunk that overlaps the requested timestamp.

use super::*;
use audio::{AudioOutput, SampleProducer};
use media::AudioDecoder;

/// 音频线程的时钟锚定请求: 在下一个可用块上把主时钟锚到实际播放内容的 PTS。
enum PendingAnchor {
    /// 锚到下一块的 PTS: 打开文件时容纳非零 start_time; 倍速切换冲掉缓冲后以内容为准。
    FirstChunk,
    /// seek 目标: 之前的块整块跳过, 跨目标的块裁剪后从目标处锚定。
    Target(u64),
}

struct AudioThreadArgs {
    // Everything the audio worker needs is copied or reference-counted so the
    // thread never borrows `Player` directly.
    path: std::path::PathBuf,
    output_rate: u32,
    stop: Arc<AtomicBool>,
    seek: Arc<AtomicU64>,
    rate: Arc<AtomicU32>,
    ended: Arc<AtomicBool>,
    flush: Arc<AtomicBool>,
    clock: audio::MasterClock,
}

impl AudioThreadArgs {
    fn requested_rate(&self) -> u16 {
        self.rate.load(Ordering::Relaxed).clamp(25, 400) as u16
    }
}

struct AudioDecodeState {
    // Decoder state stays together with the rate converter because a seek or rate
    // change resets both the sample cursor and pending clock anchor.
    decoder: AudioDecoder,
    converter: audio::PlaybackRateConverter,
    current_rate: u16,
    pending_anchor: Option<PendingAnchor>,
}

impl AudioDecodeState {
    fn from_args(args: &AudioThreadArgs) -> Option<Self> {
        let decoder = match AudioDecoder::open_with_rate(&args.path, args.output_rate) {
            Ok(decoder) => decoder,
            Err(_) => {
                // 无音频流(纯视频)或打开失败: 上报后退出, 引擎切墙钟走时。
                args.ended.store(true, Ordering::Relaxed);
                return None;
            }
        };
        let mut converter = audio::PlaybackRateConverter::new(
            decoder.channels(),
            decoder.sample_rate(),
            args.output_rate,
        );
        let current_rate = args.requested_rate();
        converter.set_rate(current_rate);
        Some(Self {
            decoder,
            converter,
            current_rate,
            pending_anchor: Some(PendingAnchor::FirstChunk),
        })
    }
}

enum AudioPumpStep {
    Continue,
    Stop,
}

impl Player {
    pub(super) fn open_media(&mut self, path: &Path) {
        self.teardown();
        self.playback_ended = false;

        let video = match DecodeThread::spawn(path, 16) {
            Ok(video) => video,
            Err(err) => {
                eprintln!("打开视频失败: {err}");
                return;
            }
        };

        self.start_audio_or_wall_clock(path);
        self.video = Some(video);
        self.install_opened_media_metadata(path);
        self.start_playback_after_open();
        self.restore_resume_point_after_open(path);
    }

    pub(super) fn teardown(&mut self) {
        self.playback_ended = false;
        let _ = self.machine.apply(player_core::Transition::Stop);
        self.audio_stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.audio_join.take() {
            if join.join().is_err() {
                eprintln!("音频线程异常退出");
            }
        }
        self.audio_out = None;
        if let Some(video) = self.video.take() {
            video.stop();
        }
        self.reset_playback_resources();
    }

    fn start_audio_or_wall_clock(&mut self, path: &Path) {
        match AudioOutput::start(
            self.volume_shared.clone(),
            self.audio_flush.clone(),
            self.audio_gate.clone(),
        ) {
            Ok(output) => self.install_audio_output(path, output),
            Err(err) => {
                eprintln!("启动音频失败: {err}, 仅播放视频(静音)");
                self.start_wall_clock_audio_fallback(path);
            }
        }
    }

    fn install_audio_output(&mut self, path: &Path, output: AudioOutput) {
        output.clock.set_rate(self.rate_pct);
        let output_rate = output.sample_rate;
        let (handle, producer) = output.split();
        // Once audio is available it becomes the master clock; video presentation
        // then tracks actual device playback rather than wall time.
        self.clock = PlayClock::Audio(handle.clock.clone());
        self.duration_ms = probe_duration_ms(path).unwrap_or(0);

        let stop = Arc::new(AtomicBool::new(false));
        let args = AudioThreadArgs {
            path: path.to_path_buf(),
            output_rate,
            stop: stop.clone(),
            seek: self.audio_seek.clone(),
            rate: self.rate_shared.clone(),
            ended: self.audio_ended.clone(),
            flush: self.audio_flush.clone(),
            clock: handle.clock.clone(),
        };
        self.audio_out = Some(handle);
        self.audio_join = Some(spawn_audio_decode_thread(args, producer));
        self.audio_stop = stop;
    }

    fn start_wall_clock_audio_fallback(&mut self, path: &Path) {
        self.duration_ms = probe_duration_ms(path).unwrap_or(0);
        self.clock = PlayClock::Wall(WallClock::new());
        self.clock.set_rate(self.rate_pct);
    }

    fn install_opened_media_metadata(&mut self, path: &Path) {
        let history_key = path.to_string_lossy().to_string();
        player_core::push_history(&mut self.prefs.history, &history_key, 50);
        self.subtitles = sidecar_subtitle(path);
        self.sub_tracks = media::list_subtitle_tracks(path).unwrap_or_default();
    }

    fn start_playback_after_open(&mut self) {
        if self.machine.apply(player_core::Transition::Play).is_err() {
            eprintln!("打开媒体后进入播放态失败");
        }
    }

    fn restore_resume_point_after_open(&mut self, path: &Path) {
        let key = path.to_string_lossy().to_string();
        if let Some(ms) = self.prefs.resume_point(&key) {
            if ms > 0 {
                self.seek_to(ms);
            }
        }
    }

    fn reset_playback_resources(&mut self) {
        self.duration_ms = 0;
        self.clock = PlayClock::Wall(WallClock::new());
        self.current_frame = None;
        self.pending_frame = None;
        self.present_drops = 0;
        self.audio_stop = Arc::new(AtomicBool::new(false));
        self.audio_seek = Arc::new(AtomicU64::new(u64::MAX));
        self.audio_flush = Arc::new(AtomicBool::new(false));
        self.audio_ended = Arc::new(AtomicBool::new(false));
        self.audio_gate = Arc::new(AtomicBool::new(false));
        self.seek_gate = None;
    }
}

fn spawn_audio_decode_thread(
    args: AudioThreadArgs,
    producer: SampleProducer,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || run_audio_decode_thread(args, producer))
}

fn run_audio_decode_thread(args: AudioThreadArgs, mut producer: SampleProducer) {
    let Some(mut state) = AudioDecodeState::from_args(&args) else {
        return;
    };

    while !args.stop.load(Ordering::Relaxed) {
        if service_audio_controls(&args, &mut state) {
            continue;
        }
        if matches!(
            pump_next_audio_chunk(&args, &mut state, &mut producer),
            AudioPumpStep::Stop
        ) {
            break;
        }
    }
}

fn service_audio_controls(args: &AudioThreadArgs, state: &mut AudioDecodeState) -> bool {
    // Control messages are handled before decoding more data so user seeks and
    // rate changes do not wait behind buffered chunks.
    consume_pending_audio_seek(args, state);
    update_audio_rate(args, state);
    wait_for_audio_flush(args)
}

fn consume_pending_audio_seek(args: &AudioThreadArgs, state: &mut AudioDecodeState) {
    let target = args.seek.swap(u64::MAX, Ordering::Relaxed);
    if target == u64::MAX {
        return;
    }
    if let Err(err) = state.decoder.seek_ms(target) {
        eprintln!("音频 seek 失败({target}ms): {err}");
    }
    state.converter.reset();
    // seek 回有音频的区域后重新供样本; EOF 状态由下面的解码结果再判。
    args.ended.store(false, Ordering::Relaxed);
    state.pending_anchor = Some(PendingAnchor::Target(target));
}

fn update_audio_rate(args: &AudioThreadArgs, state: &mut AudioDecodeState) {
    let requested_rate = args.requested_rate();
    if requested_rate == state.current_rate {
        return;
    }
    state.current_rate = requested_rate;
    state.converter.set_rate(state.current_rate);
    // 倍速切换冲掉了环形缓冲(≤1s 已转换样本): 内容跳到解码位置,
    // 重锚到下一块 PTS, 否则时钟与内容产生等于被丢缓冲时长的偏移。
    state
        .pending_anchor
        .get_or_insert(PendingAnchor::FirstChunk);
}

fn wait_for_audio_flush(args: &AudioThreadArgs) -> bool {
    if !args.flush.load(Ordering::Relaxed) {
        return false;
    }
    // 等回调完成环形缓冲清空再推新样本, 否则目标样本会被一并清掉。
    std::thread::sleep(std::time::Duration::from_millis(1));
    true
}

fn pump_next_audio_chunk(
    args: &AudioThreadArgs,
    state: &mut AudioDecodeState,
    producer: &mut SampleProducer,
) -> AudioPumpStep {
    match state.decoder.next_chunk() {
        Ok(Some(chunk)) => push_decoded_audio_chunk(args, state, producer, chunk),
        Ok(None) => {
            // 音频 EOF: 上报给引擎切墙钟续走(音频先于视频结束的文件)。
            args.ended.store(true, Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(10));
            AudioPumpStep::Continue
        }
        Err(err) => {
            eprintln!("音频解码失败: {err}");
            std::thread::sleep(std::time::Duration::from_millis(10));
            AudioPumpStep::Continue
        }
    }
}

fn push_decoded_audio_chunk(
    args: &AudioThreadArgs,
    state: &mut AudioDecodeState,
    producer: &mut SampleProducer,
    chunk: media::AudioChunk,
) -> AudioPumpStep {
    // Anchoring can skip part or all of the first chunk after open/seek; only the
    // playable suffix is converted and pushed into the ring buffer.
    let channels = state.decoder.channels().max(1) as usize;
    let Some(start) = audio_chunk_start(args, state, &chunk, channels) else {
        return AudioPumpStep::Continue;
    };
    let buffer = state.converter.convert(&chunk.samples[start..]);
    push_audio_samples(args, state, producer, &buffer)
}

fn audio_chunk_start(
    args: &AudioThreadArgs,
    state: &mut AudioDecodeState,
    chunk: &media::AudioChunk,
    channels: usize,
) -> Option<usize> {
    let Some(anchor) = state.pending_anchor.take() else {
        return Some(0);
    };
    let frames = chunk.samples.len() / channels;
    match anchor {
        PendingAnchor::FirstChunk => {
            args.clock.reset_to(chunk.pts_ms);
            Some(0)
        }
        PendingAnchor::Target(target) => {
            match sync::gate_audio_chunk(chunk.pts_ms, frames, args.output_rate, target) {
                sync::ChunkGate::SkipAll => {
                    // 整块在目标前: 丢弃, 锚定留给后续块。
                    state.pending_anchor = Some(PendingAnchor::Target(target));
                    None
                }
                sync::ChunkGate::PlayFrom {
                    trim_frames,
                    anchor_ms,
                } => {
                    args.clock.reset_to(anchor_ms);
                    Some(trim_frames * channels)
                }
            }
        }
    }
}

fn push_audio_samples(
    args: &AudioThreadArgs,
    state: &AudioDecodeState,
    producer: &mut SampleProducer,
    buffer: &[f32],
) -> AudioPumpStep {
    use ringbuf::traits::Producer;

    let mut index = 0;
    while index < buffer.len() {
        if args.stop.load(Ordering::Relaxed) {
            return AudioPumpStep::Stop;
        }
        // 新 seek 或倍速切换到来时立即放弃旧位置样本, 回到循环顶部处理控制信号。
        if args.seek.load(Ordering::Relaxed) != u64::MAX
            || args.requested_rate() != state.current_rate
        {
            return AudioPumpStep::Continue;
        }
        if producer.try_push(buffer[index]).is_ok() {
            index += 1;
        } else {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    AudioPumpStep::Continue
}

fn sidecar_subtitle(video: &Path) -> Option<subtitle::Subtitles> {
    sidecar_subtitle_paths(video).find_map(|path| load_existing_subtitle(&path))
}

fn sidecar_subtitle_paths(video: &Path) -> impl Iterator<Item = std::path::PathBuf> + '_ {
    ["srt", "ass", "ssa"]
        .into_iter()
        .map(|ext| video.with_extension(ext))
}

fn load_existing_subtitle(path: &Path) -> Option<subtitle::Subtitles> {
    path.exists()
        .then(|| subtitle::load_file(path).ok())
        .flatten()
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
