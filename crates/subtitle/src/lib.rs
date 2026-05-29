//! 字幕解析与查询。首版支持 .srt。纯逻辑, 无系统依赖。
mod model;
mod srt;
pub use model::{Cue, Subtitles};
pub use srt::{parse_srt, parse_timestamp};
