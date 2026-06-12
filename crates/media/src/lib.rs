//! FFmpeg 软解码管线: 解封装、解码、YUV→RGB。硬解见计划 2.5。
mod audio_decoder;
mod decoder;
mod error;
mod frame;
mod hwaccel;
mod subtitle_streams;
pub use audio_decoder::AudioDecoder;
pub use decoder::VideoDecoder;
pub use error::MediaError;
pub use frame::{AudioChunk, VideoFrame};
pub use hwaccel::DecodeOptions;
pub use subtitle_streams::{decode_text_subtitle, list_subtitle_tracks, SubtitleTrack};

/// 把 FFmpeg 日志压到 Fatal(默认), 设 MORN_DEBUG 时保留默认级别。
/// seek 后解码器状态重建期会按 Error 级别刷无害告警(如 HE-AAC SBR 的
/// "env_facs_q ... is invalid"), 对 GUI 播放器只是 stderr 噪声。
pub fn quiet_ffmpeg_logs_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if std::env::var("MORN_DEBUG").is_err() {
            ffmpeg_next::util::log::set_level(ffmpeg_next::util::log::Level::Fatal);
        }
    });
}
