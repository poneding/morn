//! FFmpeg 软解码管线: 解封装、解码、YUV→RGB。硬解见计划 2.5。
mod error;
mod frame;
pub use error::MediaError;
pub use frame::{AudioChunk, VideoFrame};
