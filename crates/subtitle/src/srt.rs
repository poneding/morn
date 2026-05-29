use crate::model::{Cue, Subtitles};

/// 解析 "HH:MM:SS,mmm" 为毫秒。任何格式错误返回 None。
pub fn parse_timestamp(s: &str) -> Option<u64> {
    let (hms, millis) = s.trim().split_once(',')?;
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let sec: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let ms: u64 = millis.split_whitespace().next()?.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// 解析整段 .srt 文本。格式错误的块被跳过。
pub fn parse_srt(input: &str) -> Subtitles {
    let mut cues = Vec::new();
    // 块以空行分隔。统一换行符后按双换行切分。
    let normalized = input.replace("\r\n", "\n");
    for block in normalized.split("\n\n") {
        let block = block.trim_matches('\n');
        if block.is_empty() {
            continue;
        }
        let mut lines = block.lines();
        // 第一行是序号, 跳过(容错: 即使缺失也尝试)。
        let first = lines.next();
        // 找到包含 "-->" 的时间行。
        let time_line = if first.is_some_and(|l| l.contains("-->")) {
            first
        } else {
            lines.next()
        };
        let Some(time_line) = time_line else { continue };
        let Some((start_s, end_s)) = time_line.split_once("-->") else { continue };
        let (Some(start_ms), Some(end_ms)) =
            (parse_timestamp(start_s), parse_timestamp(end_s)) else { continue };
        let text: Vec<&str> = lines.collect();
        let text = text.join("\n");
        if text.is_empty() {
            continue;
        }
        cues.push(Cue { start_ms, end_ms, text });
    }
    cues.sort_by_key(|c| c.start_ms);
    Subtitles::from_cues(cues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_cues() {
        let input = "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n\
                     2\n00:00:03,000 --> 00:00:04,500\nWorld line2\nsecond\n";
        let subs = parse_srt(input);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs.text_at(1500), Some("Hello"));
        assert_eq!(subs.text_at(3500), Some("World line2\nsecond"));
    }

    #[test]
    fn timestamp_to_ms_is_correct() {
        assert_eq!(parse_timestamp("01:02:03,456"), Some(3_723_456));
        assert_eq!(parse_timestamp("00:00:00,000"), Some(0));
        assert_eq!(parse_timestamp("garbage"), None);
    }

    #[test]
    fn malformed_block_is_skipped_not_fatal() {
        let input = "1\nNOT A TIMESTAMP\nbad\n\n\
                     2\n00:00:03,000 --> 00:00:04,000\nGood\n";
        let subs = parse_srt(input);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs.text_at(3500), Some("Good"));
    }

    #[test]
    fn block_without_sequence_number_is_parsed() {
        // 缺序号: 第一行即时间行, 走 time_line = first 分支。
        let input = "00:00:01,000 --> 00:00:02,000\nNoSeqNum\n";
        let subs = parse_srt(input);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs.text_at(1500), Some("NoSeqNum"));
    }

    #[test]
    fn time_line_with_coordinates_still_parses() {
        // 时间行带坐标扩展(合法 SRT), 不应丢弃该 cue。
        let input = "1\n00:00:01,000 --> 00:00:02,000 X1:100 X2:200 Y1:50 Y2:80\nPositioned\n";
        let subs = parse_srt(input);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs.text_at(1500), Some("Positioned"));
    }

    #[test]
    fn crlf_line_endings_are_normalized() {
        let input = "1\r\n00:00:01,000 --> 00:00:02,000\r\nWindows\r\n";
        let subs = parse_srt(input);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs.text_at(1500), Some("Windows"));
    }
}
