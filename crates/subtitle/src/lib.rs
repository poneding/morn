//! 字幕解析与查询: .srt 与 .ass。纯逻辑, 无系统依赖。
mod ass;
mod model;
mod srt;
pub use ass::{parse_ass, parse_ass_time};
pub use model::{Cue, Subtitles};
pub use srt::{parse_srt, parse_timestamp};

use std::path::Path;
/// 按文件扩展名解析字幕文件。
pub fn load_file(path: &Path) -> std::io::Result<Subtitles> {
    let content = std::fs::read_to_string(path)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    Ok(match ext.as_str() {
        "ass" | "ssa" => parse_ass(&content),
        _ => parse_srt(&content),
    })
}
