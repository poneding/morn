use std::time::Instant;

/// 无音频轨时驱动播放位置的墙钟: 位置 = 锚点 + 已过墙钟 × 倍速。暂停冻结, seek 重锚。
/// 方法以注入的 `now: Instant` 计算, 便于确定性单测; 引擎侧用 `Instant::now()` 调用。
pub struct WallClock {
    anchor_ms: u64,
    anchor_at: Instant,
    rate_pct: u16,
    paused_at: Option<Instant>,
}

impl WallClock {
    pub fn new() -> Self {
        Self {
            anchor_ms: 0,
            anchor_at: Instant::now(),
            rate_pct: 100,
            paused_at: None,
        }
    }

    pub fn position_ms_at(&self, now: Instant) -> u64 {
        let ref_now = self.paused_at.unwrap_or(now);
        let elapsed = ref_now
            .saturating_duration_since(self.anchor_at)
            .as_millis() as u64;
        self.anchor_ms + elapsed * self.rate_pct as u64 / 100
    }

    pub fn reset_to_at(&mut self, ms: u64, now: Instant) {
        self.anchor_ms = ms;
        self.anchor_at = now;
        if self.paused_at.is_some() {
            self.paused_at = Some(now);
        }
    }

    pub fn set_rate_at(&mut self, pct: u16, now: Instant) {
        self.anchor_ms = self.position_ms_at(now);
        self.anchor_at = now;
        self.rate_pct = pct.max(1);
    }

    pub fn pause_at(&mut self, now: Instant) {
        if self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
    }

    pub fn resume_at(&mut self, now: Instant) {
        if let Some(p) = self.paused_at.take() {
            // 把暂停期间的时长平移到锚点, 保证位置连续。
            self.anchor_at += now.saturating_duration_since(p);
        }
    }
}

impl Default for WallClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn advances_with_wall_time_at_rate() {
        let t0 = Instant::now();
        let c = WallClock {
            anchor_ms: 1000,
            anchor_at: t0,
            rate_pct: 100,
            paused_at: None,
        };
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(500)), 1500);
        let c2 = WallClock {
            anchor_ms: 0,
            anchor_at: t0,
            rate_pct: 200,
            paused_at: None,
        };
        assert_eq!(c2.position_ms_at(t0 + Duration::from_millis(500)), 1000);
    }

    #[test]
    fn pause_freezes_then_resume_continues() {
        let t0 = Instant::now();
        let mut c = WallClock {
            anchor_ms: 0,
            anchor_at: t0,
            rate_pct: 100,
            paused_at: None,
        };
        c.pause_at(t0 + Duration::from_millis(400));
        // 暂停后位置冻结在 400, 无论"现在"过去多久。
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(900)), 400);
        c.resume_at(t0 + Duration::from_millis(900));
        // 恢复后从 400 继续: 再过 100ms → 500。
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(1000)), 500);
    }

    #[test]
    fn reset_to_reanchors() {
        let t0 = Instant::now();
        let mut c = WallClock::new();
        c.reset_to_at(5000, t0);
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(250)), 5250);
    }
}
