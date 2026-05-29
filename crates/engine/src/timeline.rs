use player_core::PlaybackState;

/// UI 每帧读取的播放状态快照。
#[derive(Debug, Clone, Copy)]
pub struct Timeline {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub state: PlaybackState,
    pub volume: u8,
}

impl Timeline {
    pub fn progress(&self) -> f32 {
        if self.duration_ms == 0 {
            0.0
        } else {
            self.position_ms as f32 / self.duration_ms as f32
        }
    }

    pub fn position_label(&self) -> String {
        fmt_ms(self.position_ms)
    }

    pub fn duration_label(&self) -> String {
        fmt_ms(self.duration_ms)
    }
}

fn fmt_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use player_core::PlaybackState;

    #[test]
    fn formats_position_as_mm_ss() {
        let t = Timeline {
            position_ms: 65_000,
            duration_ms: 125_000,
            state: PlaybackState::Playing,
            volume: 100,
        };
        assert_eq!(t.position_label(), "01:05");
        assert_eq!(t.duration_label(), "02:05");
    }

    #[test]
    fn progress_fraction_is_ratio() {
        let t = Timeline {
            position_ms: 50_000,
            duration_ms: 100_000,
            state: PlaybackState::Playing,
            volume: 100,
        };
        assert!((t.progress() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn progress_is_zero_when_duration_unknown() {
        let t = Timeline {
            position_ms: 5_000,
            duration_ms: 0,
            state: PlaybackState::Stopped,
            volume: 100,
        };
        assert_eq!(t.progress(), 0.0);
    }
}
