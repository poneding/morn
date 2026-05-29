use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("ffmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),
    #[error("no {0} stream found")]
    NoStream(&'static str),
}
