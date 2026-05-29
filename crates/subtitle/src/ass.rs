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
        if line.starts_with('[') {
            in_events = line.eq_ignore_ascii_case("[Events]");
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Format:") {
            format_fields = rest.split(',').map(|s| s.trim().to_lowercase()).collect();
            continue;
        }
        if let Some(rest) = line.strip_prefix("Dialogue:") {
            // 按 Format 字段数切分; Text 是最后一个且可能含逗号 → 限制 splitn
            let n = format_fields.len().max(10);
            let parts: Vec<&str> = rest.splitn(n, ',').collect();
            if parts.len() < n {
                continue;
            }
            let idx = |name: &str| format_fields.iter().position(|f| f == name);
            let start = idx("start")
                .and_then(|i| parts.get(i))
                .and_then(|s| parse_ass_time(s.trim()));
            let end = idx("end")
                .and_then(|i| parts.get(i))
                .and_then(|s| parse_ass_time(s.trim()));
            let text_idx = idx("text").unwrap_or(parts.len() - 1);
            let (Some(start_ms), Some(end_ms)) = (start, end) else {
                continue;
            };
            let text = strip_tags(parts.get(text_idx).unwrap_or(&"").trim());
            if text.is_empty() {
                continue;
            }
            cues.push(Cue {
                start_ms,
                end_ms,
                text,
            });
        }
    }
    cues.sort_by_key(|c| c.start_ms);
    Subtitles::from_cues(cues)
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
