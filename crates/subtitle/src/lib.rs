//! 字幕解析与查询: .srt 与 .ass。纯逻辑, 无系统依赖。
mod ass;
mod model;
mod srt;
pub use ass::{parse_ass, parse_ass_time};
pub use model::{Cue, Subtitles};
pub use srt::{parse_srt, parse_timestamp};

use std::path::Path;

const MAX_SUBTITLE_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// 按文件扩展名解析字幕文件。
pub fn load_file(path: &Path) -> std::io::Result<Subtitles> {
    validate_subtitle_file_size(std::fs::metadata(path)?.len())?;
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

fn validate_subtitle_file_size(len: u64) -> std::io::Result<()> {
    if len > MAX_SUBTITLE_FILE_BYTES {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "subtitle file is too large",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn subtitle_file_size_guard_accepts_common_files_and_rejects_huge_inputs() {
        assert!(super::validate_subtitle_file_size(1024).is_ok());
        assert!(super::validate_subtitle_file_size(super::MAX_SUBTITLE_FILE_BYTES).is_ok());
        assert!(super::validate_subtitle_file_size(super::MAX_SUBTITLE_FILE_BYTES + 1).is_err());
    }
}
