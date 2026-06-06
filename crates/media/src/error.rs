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
