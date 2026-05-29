#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Subtitles {
    cues: Vec<Cue>,
}

impl Subtitles {
    /// 传入的 cues 假定已按 start_ms 升序、互不重叠。
    pub fn from_cues(cues: Vec<Cue>) -> Self {
        Self { cues }
    }

    pub fn len(&self) -> usize {
        self.cues.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// 返回时刻 `ms` 处应显示的字幕文本。区间为 [start, end)。
    pub fn text_at(&self, ms: u64) -> Option<&str> {
        self.cues
            .iter()
            .find(|c| ms >= c.start_ms && ms < c.end_ms)
            .map(|c| c.text.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Subtitles {
        Subtitles::from_cues(vec![
            Cue {
                start_ms: 1000,
                end_ms: 2000,
                text: "Hello".into(),
            },
            Cue {
                start_ms: 3000,
                end_ms: 4000,
                text: "World".into(),
            },
        ])
    }

    #[test]
    fn text_at_returns_active_cue() {
        let s = sample();
        assert_eq!(s.text_at(1500), Some("Hello"));
        assert_eq!(s.text_at(3500), Some("World"));
    }

    #[test]
    fn text_at_gap_returns_none() {
        let s = sample();
        assert_eq!(s.text_at(2500), None);
        assert_eq!(s.text_at(0), None);
    }

    #[test]
    fn boundaries_are_inclusive_start_exclusive_end() {
        let s = sample();
        assert_eq!(s.text_at(1000), Some("Hello")); // start inclusive
        assert_eq!(s.text_at(2000), None); // end exclusive
    }
}
