//! 音频输出 (cpal) 与音频主时钟。
mod clock;
mod output;
pub use clock::MasterClock;
pub use output::{apply_gain, AudioHandle, AudioOutput, PlaybackRateConverter, SampleProducer};
