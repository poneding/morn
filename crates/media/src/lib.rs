//! FFmpeg 软解码管线: 解封装、解码、YUV→RGB。硬解见计划 2.5。
mod audio_decoder;
mod decoder;
mod error;
mod frame;
pub use audio_decoder::AudioDecoder;
pub use decoder::VideoDecoder;
pub use error::MediaError;
pub use frame::{AudioChunk, VideoFrame};
