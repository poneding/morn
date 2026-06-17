//! Minimal ASS/SSA subtitle parser.
//!
//! The player only needs timed plain text cues, so this parser reads the
//! `[Events]` section, honors its `Format:` column order, and keeps `Dialogue:`
//! text after removing inline override tags.  It intentionally does not evaluate
//! style definitions, positioning, karaoke timing, or drawing commands.

use crate::model::{Cue, Subtitles};

/// 解析 .ass 时间 "H:MM:SS.cc"(centiseconds)为毫秒。
pub fn parse_ass_time(s: &str) -> Option<u64> {
    let (hms, cs) = s.trim().split_once('.')?;
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let sec: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let cs: u64 = cs.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + cs * 10)
}

/// 剥离 .ass 行内样式覆盖标签 {\...}。
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '{' => in_tag = true,
            '}' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // .ass 用 \N 表示换行
    out.replace("\\N", "\n")
}

/// 解析 .ass 文本, 只提取 [Events] 段的 Dialogue 行。
pub fn parse_ass(input: &str) -> Subtitles {
    let mut cues = Vec::new();
    let mut format_fields: Vec<String> = Vec::new();
    let mut in_events = false;

    for line in input.lines() {
        let line = line.trim();
        if update_events_section(line, &mut in_events) {
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(fields) = parse_format_fields(line) {
            format_fields = fields;
            continue;
        }
        if let Some(cue) = parse_dialogue_line(line, &format_fields) {
            cues.push(cue);
        }
    }
    cues.sort_by_key(|c| c.start_ms);
    Subtitles::from_cues(cues)
}

fn update_events_section(line: &str, in_events: &mut bool) -> bool {
    if !line.starts_with('[') {
        return false;
    }
    *in_events = line.eq_ignore_ascii_case("[Events]");
    true
}

fn parse_format_fields(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix("Format:")?;
    Some(rest.split(',').map(|s| s.trim().to_lowercase()).collect())
}

fn parse_dialogue_line(line: &str, format_fields: &[String]) -> Option<Cue> {
    let rest = line.strip_prefix("Dialogue:")?;
    let parts = split_dialogue_parts(rest, format_fields);
    let start_ms = field_time(format_fields, &parts, "start")?;
    let end_ms = field_time(format_fields, &parts, "end")?;
    let text = dialogue_text(format_fields, &parts);
    if text.is_empty() {
        return None;
    }
    Some(Cue {
        start_ms,
        end_ms,
        text,
    })
}

fn split_dialogue_parts<'a>(rest: &'a str, format_fields: &[String]) -> Vec<&'a str> {
    // 按 Format 字段数切分; Text 是最后一个且可能含逗号 → 限制 splitn
    let n = format_fields.len().max(10);
    let parts: Vec<&str> = rest.splitn(n, ',').collect();
    if parts.len() < n {
        return Vec::new();
    }
    parts
}

fn field_time(format_fields: &[String], parts: &[&str], name: &str) -> Option<u64> {
    let index = field_index(format_fields, name)?;
    let value = parts.get(index)?;
    parse_ass_time(value.trim())
}

fn dialogue_text(format_fields: &[String], parts: &[&str]) -> String {
    let text_idx = match field_index(format_fields, "text") {
        Some(idx) => idx,
        None => parts.len().saturating_sub(1),
    };
    let text = match parts.get(text_idx) {
        Some(text) => text.trim(),
        None => "",
    };
    strip_tags(text)
}

fn field_index(format_fields: &[String], name: &str) -> Option<usize> {
    format_fields.iter().position(|field| field == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dialogue_lines() {
        let input = "\
[Script Info]
Title: test

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Hello there
Dialogue: 0,0:00:03.00,0:00:04.50,Default,,0,0,0,,{\\i1}Styled{\\i0} text
";
        let subs = parse_ass(input);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs.text_at(1500), Some("Hello there"));
        // 样式标签 {..} 应被剥离
        assert_eq!(subs.text_at(3500), Some("Styled text"));
    }

    #[test]
    fn ass_timestamp_to_ms() {
        assert_eq!(parse_ass_time("0:00:01.00"), Some(1000));
        assert_eq!(parse_ass_time("1:02:03.50"), Some(3_723_500));
        assert_eq!(parse_ass_time("bad"), None);
    }
}
