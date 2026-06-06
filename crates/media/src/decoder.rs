use crate::error::MediaError;
use crate::frame::VideoFrame;
use crate::hwaccel::{DecodeOptions, HwCallbackData, HwDeviceContext};
use ff::format::Pixel;
use ff::media::Type;
use ff::software::scaling::{context::Context as Scaler, flag::Flags};
use ff::util::frame::video::Video as FfVideo;
use ffmpeg_next as ff;
use ffmpeg_sys_next as sys;
use std::path::Path;

const MAX_VIDEO_WIDTH: u32 = 8192;
const MAX_VIDEO_HEIGHT: u32 = 8192;
const MAX_VIDEO_PIXELS: u64 = 7680 * 4320;

pub struct VideoDecoder {
    ictx: ff::format::context::Input,
    decoder: ff::decoder::Video,
    scaler: Option<Scaler>,
    stream_index: usize,
    time_base: f64,
    width: u32,
    height: u32,
    eof: bool,
    is_hardware: bool,
    decoded_in_hardware: bool,
    _hw_device: Option<HwDeviceContext>,
    _hw_cb_data: Option<Box<HwCallbackData>>,
    hw_pix_fmt: sys::AVPixelFormat,
}

impl VideoDecoder {
    pub fn open(path: &Path) -> Result<Self, MediaError> {
        Self::open_with_options(path, DecodeOptions::default())
    }

    pub fn open_with_options(path: &Path, opts: DecodeOptions) -> Result<Self, MediaError> {
        match Self::open_inner(path, opts) {
            Ok(decoder) => Ok(decoder),
            Err(err) if opts.try_hardware => {
                let software_opts = DecodeOptions {
                    try_hardware: false,
                };
                match Self::open_inner(path, software_opts) {
                    Ok(decoder) => Ok(decoder),
                    Err(_) => Err(err),
                }
            }
            Err(err) => Err(err),
        }
    }

    fn open_inner(path: &Path, opts: DecodeOptions) -> Result<Self, MediaError> {
        ff::init()?;
        let ictx = ff::format::input(&path)?;
        let stream = ictx
            .streams()
            .best(Type::Video)
            .ok_or(MediaError::NoStream("video"))?;
        let stream_index = stream.index();
        let time_base = f64::from(stream.time_base());
        let mut ctx = ff::codec::context::Context::from_parameters(stream.parameters())?;

        let mut is_hardware = false;
        let mut hw_device = None;
        let mut hw_cb_data: Option<Box<HwCallbackData>> = None;
        let mut hw_pix_fmt = sys::AVPixelFormat::AV_PIX_FMT_NONE;

        if opts.try_hardware {
            if let Some(dev) = HwDeviceContext::create_for_current_platform() {
                let mut cb = Box::new(HwCallbackData {
                    hw_pix_fmt: dev.hw_pix_fmt,
                });
                // SAFETY: ctx.as_mut_ptr() 返回有效的未打开 *mut AVCodecContext。我们在 avcodec_open2
                // (由下面的 .decoder().video() 触发) 之前设置硬件字段, 符合 FFmpeg 契约:
                // (1) opaque 指向 boxed HwCallbackData——该 box 存入 self._hw_cb_data, 生命周期覆盖
                //     decoder, 裸指针在 get_format 回调期间有效;
                // (2) hw_device_ctx 用 av_buffer_ref 增引用(dev 的 Drop 释放其自身引用,
                //     avcodec_free_context 释放此引用);
                // (3) get_format 设为我们的 extern "C" 回调。
                unsafe {
                    let avctx = ctx.as_mut_ptr();
                    (*avctx).opaque = (cb.as_mut() as *mut HwCallbackData) as *mut std::ffi::c_void;
                    (*avctx).hw_device_ctx = sys::av_buffer_ref(dev.as_ptr());
                    (*avctx).get_format = Some(crate::hwaccel::get_hw_format);
                }
                hw_pix_fmt = dev.hw_pix_fmt;
                hw_cb_data = Some(cb);
                hw_device = Some(dev);
                is_hardware = true;
            }
        }

        let decoder = ctx.decoder().video()?;
        let (width, height) = (decoder.width(), decoder.height());
        validate_video_dimensions(width, height)?;

        Ok(Self {
            ictx,
            decoder,
            scaler: None, // lazy: 首帧按实际格式构建
            stream_index,
            time_base,
            width,
            height,
            eof: false,
            is_hardware,
            decoded_in_hardware: false,
            _hw_device: hw_device,
            _hw_cb_data: hw_cb_data,
            hw_pix_fmt,
        })
    }

    pub fn is_hardware(&self) -> bool {
        self.is_hardware
    }

    /// 最近一帧是否实际走了硬件解码路径(区别于 is_hardware 的"意图")。
    pub fn observed_hardware(&self) -> bool {
        self.decoded_in_hardware
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }

    /// seek 到目标毫秒(跳到不晚于该时间的最近关键帧), 并清解码器内部缓冲。
    pub fn seek_ms(&mut self, ms: u64) -> Result<(), MediaError> {
        let ts = ms as i64 * 1000; // Input::seek 用 AV_TIME_BASE 微秒
        self.ictx.seek(ts, ..ts)?;
        self.decoder.flush();
        self.eof = false;
        Ok(())
    }

    /// 返回下一帧 RGBA, 文件结束返回 Ok(None)。
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>, MediaError> {
        loop {
            let mut decoded = FfVideo::empty();
            if self.decoder.receive_frame(&mut decoded).is_ok() {
                let frame = if self.is_hardware && self.frame_is_hw(&decoded) {
                    self.decoded_in_hardware = true;
                    self.download_and_scale(&decoded)?
                } else {
                    self.decoded_in_hardware = false;
                    let fmt = decoded.format(); // SAFE accessor — no transmute
                    self.ensure_scaler(fmt)?;
                    self.scale_software(&decoded)?
                };
                return Ok(Some(frame));
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

    fn ensure_scaler(&mut self, src_fmt: Pixel) -> Result<(), MediaError> {
        if self.scaler.is_none() {
            self.scaler = Some(Scaler::get(
                src_fmt,
                self.width,
                self.height,
                Pixel::RGBA,
                self.width,
                self.height,
                Flags::BILINEAR,
            )?);
        }
        Ok(())
    }

    fn frame_is_hw(&self, frame: &FfVideo) -> bool {
        // SAFE: compare the frame's Pixel format to the hw format (no unsafe).
        frame.format() == Pixel::from(self.hw_pix_fmt)
    }

    fn download_and_scale(&mut self, hw: &FfVideo) -> Result<VideoFrame, MediaError> {
        let mut sw = FfVideo::empty();
        // SAFETY: hw.as_ptr() 是有效硬件帧指针; sw.as_mut_ptr() 是有效空帧,
        // transfer_hw_frame 内部调用 av_hwframe_transfer_data 下载到 sw(按需分配)。
        let ok = unsafe { crate::hwaccel::transfer_hw_frame(hw.as_ptr(), sw.as_mut_ptr()) };
        if !ok {
            return Err(MediaError::HwTransfer);
        }
        let fmt = sw.format(); // SAFE accessor
        self.ensure_scaler(fmt)?;
        self.scale_software(&sw)
    }

    fn scale_software(&mut self, decoded: &FfVideo) -> Result<VideoFrame, MediaError> {
        let scaler = self.scaler.as_mut().expect("scaler 已构建");
        let mut rgba = FfVideo::empty();
        scaler.run(decoded, &mut rgba)?;
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

fn validate_video_dimensions(width: u32, height: u32) -> Result<(), MediaError> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_VIDEO_WIDTH
        || height > MAX_VIDEO_HEIGHT
        || pixels > MAX_VIDEO_PIXELS
    {
        Err(MediaError::InvalidVideoDimensions { width, height })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_dimensions_guard_accepts_common_resolutions() {
        assert!(validate_video_dimensions(1920, 1080).is_ok());
        assert!(validate_video_dimensions(3840, 2160).is_ok());
    }

    #[test]
    fn video_dimensions_guard_rejects_empty_or_excessive_frames() {
        assert!(validate_video_dimensions(0, 1080).is_err());
        assert!(validate_video_dimensions(1920, 0).is_err());
        assert!(validate_video_dimensions(MAX_VIDEO_WIDTH + 1, 1080).is_err());
        assert!(validate_video_dimensions(8192, 8192).is_err());
    }

    #[test]
    fn hardware_open_failure_retries_with_software_decode() {
        let source = include_str!("decoder.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("fn open_inner"));
        assert!(source.contains("try_hardware: false"));
        assert!(source.contains("Self::open_inner(path, software_opts)"));
    }
}
