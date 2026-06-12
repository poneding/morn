use crate::wall_clock::WallClock;
use audio::MasterClock;
use std::time::Instant;

/// 统一播放时钟: 有音轨时用音频主时钟(由真实播放样本驱动), 无音轨时用墙钟。
/// 供引擎读取播放位置, 并在 seek/倍速/暂停时统一操作。
pub enum PlayClock {
    Audio(MasterClock),
    Wall(WallClock),
}

impl PlayClock {
    pub fn position_ms(&self) -> u64 {
        match self {
            PlayClock::Audio(c) => c.position_ms(),
            PlayClock::Wall(c) => c.position_ms_at(Instant::now()),
        }
    }

    /// 平滑播放位置: 供视频 present 取帧。音频时钟在回调间隙用墙钟插值; 墙钟本就连续。
    pub fn position_ms_smooth(&self) -> u64 {
        match self {
            PlayClock::Audio(c) => c.position_ms_smooth(),
            PlayClock::Wall(c) => c.position_ms_at(Instant::now()),
        }
    }

    pub fn reset_to(&mut self, ms: u64) {
        match self {
            PlayClock::Audio(c) => c.reset_to(ms),
            PlayClock::Wall(c) => c.reset_to_at(ms, Instant::now()),
        }
    }

    pub fn set_rate(&mut self, pct: u16) {
        match self {
            PlayClock::Audio(c) => c.set_rate(pct),
            PlayClock::Wall(c) => c.set_rate_at(pct, Instant::now()),
        }
    }

    pub fn pause(&mut self) {
        // 音频主时钟由暂停 cpal 流自然冻结(回调停止累计), 此处只需处理墙钟。
        if let PlayClock::Wall(c) = self {
            c.pause_at(Instant::now());
        }
    }

    pub fn resume(&mut self) {
        if let PlayClock::Wall(c) = self {
            c.resume_at(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_variant_delegates_position() {
        let mc = MasterClock::new(48_000);
        mc.add_frames(48_000);
        let pc = PlayClock::Audio(mc);
        assert_eq!(pc.position_ms(), 1000);
    }

    #[test]
    fn wall_variant_reports_near_zero_at_start() {
        let pc = PlayClock::Wall(WallClock::new());
        assert!(pc.position_ms() < 50); // 刚建好, 约 0
    }
}
