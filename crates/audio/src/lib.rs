//! 音频输出 (cpal) 与音频主时钟。
mod clock;
mod output;
pub use clock::MasterClock;
pub use output::{AudioOutput, SampleProducer};
