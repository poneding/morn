//! Error types returned by media decoders.
//!
//! The media crate keeps FFmpeg errors intact where possible, and adds small
//! domain errors for stream selection, hardware-frame transfer, and dimension
//! validation so engine callers can choose a fallback path without parsing text.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("ffmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),
    #[error("no {0} stream found")]
    NoStream(&'static str),
    #[error("hardware frame transfer failed")]
    HwTransfer,
    #[error("invalid video dimensions: {width}x{height}")]
    InvalidVideoDimensions { width: u32, height: u32 },
}
