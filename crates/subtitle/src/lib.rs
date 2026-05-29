//! 字幕解析与查询。首版支持 .srt。纯逻辑, 无系统依赖。
mod model;
pub use model::{Cue, Subtitles};
