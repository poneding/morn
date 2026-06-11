use crate::error::MediaError;
use crate::frame::AudioChunk;
use ff::format::sample::{Sample, Type as SampleType};
use ff::media::Type;
use ff::util::frame::audio::Audio as FfAudio;
use ff::ChannelLayout;
use ffmpeg_next as ff;
use std::path::Path;

pub struct AudioDecoder {
    ictx: ff::format::context::Input,
    decoder: ff::decoder::Audio,
    resampler: ff::software::resampling::Context,
    stream_index: usize,
    time_base: f64,
    channels: u16,
    sample_rate: u32,
    eof: bool,
}

impl AudioDecoder {
    pub fn open(path: &Path) -> Result<Self, MediaError> {
        Self::open_inner(path, None)
    }

    /// 打开音频并把样本重采样到指定输出采样率(通常是音频设备采样率)。
    /// 这样 1.0x 播放时无需再经时间拉伸器重采样, 可直接透传, 避免其固有延迟造成音画不同步。
    pub fn open_with_rate(path: &Path, output_rate: u32) -> Result<Self, MediaError> {
        Self::open_inner(path, Some(output_rate))
    }

    fn open_inner(path: &Path, output_rate: Option<u32>) -> Result<Self, MediaError> {
        ff::init()?;
        crate::quiet_ffmpeg_logs_once();
        let ictx = ff::format::input(&path)?;
        let stream = ictx
            .streams()
            .best(Type::Audio)
            .ok_or(MediaError::NoStream("audio"))?;
        let stream_index = stream.index();
        let time_base = f64::from(stream.time_base());
        let ctx = ff::codec::context::Context::from_parameters(stream.parameters())?;
        let decoder = ctx.decoder().audio()?;

        let src_rate = decoder.rate();
        let out_rate = output_rate.unwrap_or(src_rate).max(1);
        let out_layout = ChannelLayout::STEREO;
        let resampler = ff::software::resampling::Context::get(
            decoder.format(),
            decoder.channel_layout(),
            src_rate,
            Sample::F32(SampleType::Packed),
            out_layout,
            out_rate,
        )?;

        Ok(Self {
            ictx,
            decoder,
            resampler,
            stream_index,
            time_base,
            channels: 2,
            sample_rate: out_rate,
            eof: false,
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// seek 到目标毫秒(跳到不晚于该时间的最近关键帧), 并清解码器内部缓冲。
    pub fn seek_ms(&mut self, ms: u64) -> Result<(), MediaError> {
        let ts = ms as i64 * 1000; // Input::seek 用 AV_TIME_BASE 微秒
        self.ictx.seek(ts, ..ts)?;
        self.decoder.flush();
        self.eof = false;
        Ok(())
    }

    pub fn next_chunk(&mut self) -> Result<Option<AudioChunk>, MediaError> {
        loop {
            let mut decoded = FfAudio::empty();
            if self.decoder.receive_frame(&mut decoded).is_ok() {
                return Ok(Some(self.resample(&decoded)?));
            }
            if self.eof {
                return Ok(None);
            }
            match self.read_audio_packet()? {
                Some(packet) => self.decoder.send_packet(&packet)?,
                None => {
                    self.decoder.send_eof()?;
                    self.eof = true;
                }
            }
        }
    }

    fn read_audio_packet(&mut self) -> Result<Option<ff::codec::packet::Packet>, MediaError> {
        let mut packet = ff::codec::packet::Packet::empty();
        loop {
            match packet.read(&mut self.ictx) {
                Ok(()) => {
                    if packet.stream() == self.stream_index {
                        return Ok(Some(packet));
                    }
                }
                Err(ff::Error::Eof) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn resample(&mut self, decoded: &FfAudio) -> Result<AudioChunk, MediaError> {
        let mut out = FfAudio::empty();
        self.resampler.run(decoded, &mut out)?;
        let pts = decoded.pts().unwrap_or(0);
        let pts_ms = (pts as f64 * self.time_base * 1000.0).max(0.0) as u64;
        let frame_count = out.samples();
        let total = frame_count * self.channels as usize;
        let raw = out.data(0);
        let mut samples = Vec::with_capacity(total);
        let bytes = &raw[..total * std::mem::size_of::<f32>()];
        for chunk in bytes.chunks_exact(4) {
            samples.push(f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(AudioChunk::new(samples, self.channels, pts_ms))
    }
}
