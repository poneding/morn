use crate::error::MediaError;
use crate::frame::VideoFrame;
use ff::format::Pixel;
use ff::media::Type;
use ff::software::scaling::{context::Context as Scaler, flag::Flags};
use ff::util::frame::video::Video as FfVideo;
use ffmpeg_next as ff;
use std::path::Path;

pub struct VideoDecoder {
    ictx: ff::format::context::Input,
    decoder: ff::decoder::Video,
    scaler: Scaler,
    stream_index: usize,
    time_base: f64,
    width: u32,
    height: u32,
    eof: bool,
}

impl VideoDecoder {
    pub fn open(path: &Path) -> Result<Self, MediaError> {
        ff::init()?;
        let ictx = ff::format::input(&path)?;
        let stream = ictx
            .streams()
            .best(Type::Video)
            .ok_or(MediaError::NoStream("video"))?;
        let stream_index = stream.index();
        let time_base = f64::from(stream.time_base());
        let ctx = ff::codec::context::Context::from_parameters(stream.parameters())?;
        let decoder = ctx.decoder().video()?;
        let (width, height) = (decoder.width(), decoder.height());
        let scaler = Scaler::get(
            decoder.format(),
            width,
            height,
            Pixel::RGBA,
            width,
            height,
            Flags::BILINEAR,
        )?;
        Ok(Self {
            ictx,
            decoder,
            scaler,
            stream_index,
            time_base,
            width,
            height,
            eof: false,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }

    /// 返回下一帧 RGBA, 文件结束返回 Ok(None)。
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>, MediaError> {
        loop {
            let mut decoded = FfVideo::empty();
            if self.decoder.receive_frame(&mut decoded).is_ok() {
                return Ok(Some(self.scale(&decoded)?));
            }
            if self.eof {
                return Ok(None);
            }
            match self.read_video_packet()? {
                Some(packet) => {
                    self.decoder.send_packet(&packet)?;
                }
                None => {
                    self.decoder.send_eof()?;
                    self.eof = true;
                }
            }
        }
    }

    fn read_video_packet(&mut self) -> Result<Option<ff::codec::packet::Packet>, MediaError> {
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

    fn scale(&mut self, decoded: &FfVideo) -> Result<VideoFrame, MediaError> {
        let mut rgba = FfVideo::empty();
        self.scaler.run(decoded, &mut rgba)?;
        let pts = decoded.pts().unwrap_or(0);
        let pts_ms = (pts as f64 * self.time_base * 1000.0).max(0.0) as u64;
        let stride = rgba.stride(0);
        let row_bytes = (self.width * 4) as usize;
        let src = rgba.data(0);
        let mut out = Vec::with_capacity(row_bytes * self.height as usize);
        for y in 0..self.height as usize {
            let start = y * stride;
            out.extend_from_slice(&src[start..start + row_bytes]);
        }
        Ok(VideoFrame::new(self.width, self.height, pts_ms, out))
    }
}
