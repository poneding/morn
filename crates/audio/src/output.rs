use crate::clock::MasterClock;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use ringbuf::traits::{Consumer, Split};
use ringbuf::HeapRb;

pub type SampleProducer = <HeapRb<f32> as Split>::Prod;

pub struct AudioOutput {
    pub stream: cpal::Stream,
    pub clock: MasterClock,
    pub producer: SampleProducer,
    pub channels: u16,
    pub sample_rate: u32,
}

impl AudioOutput {
    /// 打开默认输出设备并启动播放。返回的 producer 用于从解码线程推入交错 f32 样本。
    pub fn start() -> Result<Self, cpal::BuildStreamError> {
        let host = cpal::default_host();
        let device = host.default_output_device().expect("无默认音频输出设备");
        let supported = device.default_output_config().expect("无默认输出配置");
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels;
        let sample_rate = config.sample_rate;

        let clock = MasterClock::new(sample_rate);
        let clock_cb = clock.clone();

        // ~1 秒缓冲
        let rb = HeapRb::<f32>::new(sample_rate as usize * channels as usize);
        let (producer, mut consumer) = rb.split();

        let ch = channels as u64;
        let err_fn = |e| eprintln!("音频流错误: {e}");

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    let mut filled = 0u64;
                    for slot in data.iter_mut() {
                        *slot = consumer.try_pop().unwrap_or(0.0);
                        filled += 1;
                    }
                    clock_cb.add_frames(filled / ch);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _| {
                    let mut filled = 0u64;
                    for slot in data.iter_mut() {
                        let s = consumer.try_pop().unwrap_or(0.0);
                        *slot = i16::from_sample(s);
                        filled += 1;
                    }
                    clock_cb.add_frames(filled / ch);
                },
                err_fn,
                None,
            )?,
            other => panic!("不支持的采样格式: {other:?}"),
        };

        stream.play().expect("启动音频流失败");

        Ok(Self {
            stream,
            clock,
            producer,
            channels,
            sample_rate,
        })
    }
}
