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
