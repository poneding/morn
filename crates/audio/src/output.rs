use crate::clock::MasterClock;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use ringbuf::traits::{Consumer, Split};
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

pub type SampleProducer = <HeapRb<f32> as Split>::Prod;

/// 把解码得到的交错 PCM 按播放倍速与设备采样率转换成输出端应消费的样本。
/// 使用 time-stretch 改变时长, 避免重采样造成的音高变化。
pub struct PlaybackRateConverter {
    channels: usize,
    source_rate: u32,
    output_rate: u32,
    rate_pct: u16,
    stretch: signalsmith_stretch::Stretch,
}

impl PlaybackRateConverter {
    pub fn new(channels: u16, source_rate: u32, output_rate: u32) -> Self {
        let channels = channels.max(1);
        let output_rate = output_rate.max(1);
        Self {
            channels: channels as usize,
            source_rate: source_rate.max(1),
            output_rate,
            rate_pct: 100,
            stretch: signalsmith_stretch::Stretch::preset_default(channels as u32, output_rate),
        }
    }

    pub fn set_rate(&mut self, pct: u16) {
        self.rate_pct = pct.max(1);
    }

    pub fn reset(&mut self) {
        self.stretch.reset();
    }

    fn input_frames_per_output_frame(&self) -> f64 {
        (self.rate_pct as f64 / 100.0) * (self.source_rate as f64 / self.output_rate as f64)
    }

    pub fn convert(&mut self, samples: &[f32]) -> Vec<f32> {
        let channels = self.channels;
        let input_frames = samples.len() / channels;
        if input_frames == 0 {
            return Vec::new();
        }

        // 1.0x 且采样率一致: 原样透传, 不进时间拉伸器。拉伸器有固有处理延迟(首块输出近乎静音),
        // 而主时钟按"输出样本数"线性推算媒体时间, 经它后音频会稳定滞后→画面超前。仅倍速≠1 时才需要它。
        if self.rate_pct == 100 && self.source_rate == self.output_rate {
            return samples[..input_frames * channels].to_vec();
        }

        let output_frames =
            (input_frames as f64 / self.input_frames_per_output_frame()).round() as usize;
        let output_frames = output_frames.max(1);
        let mut out = vec![0.0; output_frames * channels];
        self.stretch
            .process(&samples[..input_frames * channels], &mut out);
        out
    }
}

pub struct AudioOutput {
    pub stream: cpal::Stream,
    pub clock: MasterClock,
    pub producer: SampleProducer,
    pub channels: u16,
    pub sample_rate: u32,
}

impl AudioOutput {
    /// 打开默认输出设备并启动播放。返回的 producer 用于从解码线程推入交错 f32 样本。
    ///
    /// `volume`(0..=100)在回调里按播放时刻施加增益(而非解码时), 故音量改动近乎即时生效。
    /// `flush` 置位时回调清空环形缓冲里的陈旧样本(seek 后丢弃旧音频), 由回调复位。
    pub fn start(volume: Arc<AtomicU8>, flush: Arc<AtomicBool>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "无默认音频输出设备".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("无默认输出配置: {e}"))?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels;
        let sample_rate = config.sample_rate;
        if !is_supported_sample_format(sample_format) {
            return Err(format!("不支持的采样格式: {sample_format:?}"));
        }

        let clock = MasterClock::new(sample_rate);
        let clock_cb = clock.clone();

        // ~1 秒缓冲
        let rb = HeapRb::<f32>::new(sample_rate as usize * channels as usize);
        let (producer, mut consumer) = rb.split();

        let ch = channels as u64;
        let err_fn = |e| eprintln!("音频流错误: {e}");

        let stream = match sample_format {
            SampleFormat::F32 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [f32], _| {
                        if flush.swap(false, Ordering::Relaxed) {
                            while consumer.try_pop().is_some() {}
                        }
                        let g = volume.load(Ordering::Relaxed).min(100) as f32 / 100.0;
                        let mut real = 0u64;
                        for slot in data.iter_mut() {
                            match consumer.try_pop() {
                                Some(s) => {
                                    *slot = s * g;
                                    real += 1;
                                }
                                None => *slot = 0.0, // 欠载补静音, 但不计入主时钟
                            }
                        }
                        clock_cb.add_frames(real / ch);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?,
            SampleFormat::U16 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [u16], _| {
                        if flush.swap(false, Ordering::Relaxed) {
                            while consumer.try_pop().is_some() {}
                        }
                        let g = volume.load(Ordering::Relaxed).min(100) as f32 / 100.0;
                        let mut real = 0u64;
                        for slot in data.iter_mut() {
                            match consumer.try_pop() {
                                Some(s) => {
                                    *slot = u16::from_sample(s * g);
                                    real += 1;
                                }
                                None => *slot = u16::from_sample(0.0),
                            }
                        }
                        clock_cb.add_frames(real / ch);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?,
            SampleFormat::I16 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [i16], _| {
                        if flush.swap(false, Ordering::Relaxed) {
                            while consumer.try_pop().is_some() {}
                        }
                        let g = volume.load(Ordering::Relaxed).min(100) as f32 / 100.0;
                        let mut real = 0u64;
                        for slot in data.iter_mut() {
                            match consumer.try_pop() {
                                Some(s) => {
                                    *slot = i16::from_sample(s * g);
                                    real += 1;
                                }
                                None => *slot = i16::from_sample(0.0),
                            }
                        }
                        clock_cb.add_frames(real / ch);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?,
            other => return Err(format!("不支持的采样格式: {other:?}")),
        };

        stream.play().map_err(|e| format!("启动音频流失败: {e}"))?;

        Ok(Self {
            stream,
            clock,
            producer,
            channels,
            sample_rate,
        })
    }
}

fn is_supported_sample_format(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
    )
}

/// 不含 producer 的音频句柄, 留在 Player 中。
pub struct AudioHandle {
    pub stream: cpal::Stream,
    pub clock: MasterClock,
    pub channels: u16,
    pub sample_rate: u32,
}

impl AudioHandle {
    pub fn pause(&self) {
        use cpal::traits::StreamTrait;
        let _ = self.stream.pause();
    }
    pub fn resume(&self) {
        use cpal::traits::StreamTrait;
        let _ = self.stream.play();
    }
}

/// 把 0..=100 的音量作为线性增益作用于交错样本(就地)。
pub fn apply_gain(samples: &mut [f32], volume: u8) {
    let g = volume.min(100) as f32 / 100.0;
    if (g - 1.0).abs() < f32::EPSILON {
        return;
    }
    for s in samples.iter_mut() {
        *s *= g;
    }
}

impl AudioOutput {
    /// 拆分为 (留存句柄, 样本生产端)。producer 移入解码线程。
    pub fn split(self) -> (AudioHandle, SampleProducer) {
        let AudioOutput {
            stream,
            clock,
            producer,
            channels,
            sample_rate,
        } = self;
        (
            AudioHandle {
                stream,
                clock,
                channels,
                sample_rate,
            },
            producer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_gain, PlaybackRateConverter};

    #[test]
    fn gain_100_is_unchanged() {
        let mut s = [0.5f32, -0.5];
        apply_gain(&mut s, 100);
        assert!((s[0] - 0.5).abs() < 1e-6);
    }
    #[test]
    fn gain_50_halves() {
        let mut s = [0.8f32, -0.8];
        apply_gain(&mut s, 50);
        assert!((s[0] - 0.4).abs() < 1e-6);
        assert!((s[1] + 0.4).abs() < 1e-6);
    }
    #[test]
    fn gain_0_is_silence() {
        let mut s = [0.9f32, -0.9];
        apply_gain(&mut s, 0);
        assert_eq!(s, [0.0, 0.0]);
    }

    #[test]
    fn playback_rate_converter_shortens_audio_at_double_speed() {
        let mut c = PlaybackRateConverter::new(1, 1000, 1000);
        c.set_rate(200);
        let out = c.convert(&[0.0, 1.0, 2.0, 3.0, 4.0]);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn passthrough_at_normal_speed_returns_input_unchanged() {
        // 1.0x 且采样率一致时必须原样透传, 不经 signalsmith 时间拉伸器 ——
        // 否则拉伸器的固有延迟会让实际音频滞后于主时钟, 表现为画面稳定超前声音。
        let mut c = PlaybackRateConverter::new(2, 48_000, 48_000);
        let input = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3];
        let out = c.convert(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn playback_rate_converter_expands_audio_below_normal_speed() {
        let mut c = PlaybackRateConverter::new(1, 1000, 1000);
        c.set_rate(50);
        let out = c.convert(&[0.0, 1.0, 2.0]);
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn playback_rate_converter_matches_output_sample_rate() {
        let mut c = PlaybackRateConverter::new(1, 1000, 2000);
        let out = c.convert(&[0.0, 1.0, 2.0]);
        assert_eq!(out.len(), 6);
    }

    fn sine(freq_hz: f32, sample_rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                let phase = i as f32 * freq_hz * std::f32::consts::TAU / sample_rate as f32;
                phase.sin()
            })
            .collect()
    }

    fn estimate_frequency(samples: &[f32], sample_rate: u32) -> f32 {
        let mut crossings = Vec::new();
        for i in 1..samples.len() {
            if samples[i - 1] <= 0.0 && samples[i] > 0.0 {
                crossings.push(i);
            }
        }
        let first = crossings.first().copied().unwrap();
        let last = crossings.last().copied().unwrap();
        let periods = crossings.len().saturating_sub(1) as f32;
        periods * sample_rate as f32 / (last - first) as f32
    }

    #[test]
    fn playback_rate_converter_preserves_pitch_when_speeding_up() {
        let sample_rate = 48_000;
        let input = sine(440.0, sample_rate, sample_rate as usize);
        let mut c = PlaybackRateConverter::new(1, sample_rate, sample_rate);
        c.set_rate(200);

        let out = c.convert(&input);
        let estimated = estimate_frequency(&out, sample_rate);

        assert!(
            (estimated - 440.0).abs() < 8.0,
            "expected pitch near 440Hz after 2x tempo, got {estimated:.1}Hz"
        );
    }

    #[test]
    fn supported_output_sample_formats_include_unsigned_16_bit() {
        assert!(super::is_supported_sample_format(cpal::SampleFormat::F32));
        assert!(super::is_supported_sample_format(cpal::SampleFormat::I16));
        assert!(super::is_supported_sample_format(cpal::SampleFormat::U16));
    }
}
