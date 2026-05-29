use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// 音频主时钟。回调线程累加已消费帧数, 任意线程读取播放位置。
#[derive(Clone)]
pub struct MasterClock {
    frames_played: Arc<AtomicU64>,
    sample_rate: u32,
    rate: Arc<AtomicU32>,
}

impl MasterClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            frames_played: Arc::new(AtomicU64::new(0)),
            sample_rate: sample_rate.max(1),
            rate: Arc::new(AtomicU32::new(100)),
        }
    }

    /// 音频回调消费了 `n` 个音频帧(每帧含所有声道一份样本)后调用。
    pub fn add_frames(&self, n: u64) {
        self.frames_played.fetch_add(n, Ordering::Relaxed);
    }

    /// 当前播放位置(毫秒)。
    pub fn position_ms(&self) -> u64 {
        let f = self.frames_played.load(Ordering::Relaxed);
        let base = f * 1000 / self.sample_rate as u64;
        base * self.rate.load(Ordering::Relaxed) as u64 / 100
    }

    /// 设置倍速百分比 (100 = 1.0x)。影响 position_ms 读数(视频帧节奏)。
    pub fn set_rate(&self, pct: u16) {
        self.rate.store(pct as u32, Ordering::Relaxed);
    }

    /// seek 后重置时钟基准。
    pub fn reset_to(&self, ms: u64) {
        let frames = ms * self.sample_rate as u64 / 1000;
        self.frames_played.store(frames, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero() {
        let c = MasterClock::new(44_100);
        assert_eq!(c.position_ms(), 0);
    }

    #[test]
    fn one_second_of_frames_is_1000ms() {
        let c = MasterClock::new(44_100);
        c.add_frames(44_100);
        assert_eq!(c.position_ms(), 1000);
    }

    #[test]
    fn half_second() {
        let c = MasterClock::new(48_000);
        c.add_frames(24_000);
        assert_eq!(c.position_ms(), 500);
    }

    #[test]
    fn accumulates_across_calls() {
        let c = MasterClock::new(1000);
        c.add_frames(250);
        c.add_frames(250);
        assert_eq!(c.position_ms(), 500);
    }

    #[test]
    fn default_rate_is_realtime() {
        let c = MasterClock::new(1000);
        c.add_frames(1000);
        assert_eq!(c.position_ms(), 1000);
    }

    #[test]
    fn double_rate_doubles_position() {
        let c = MasterClock::new(1000);
        c.add_frames(1000);
        c.set_rate(200);
        assert_eq!(c.position_ms(), 2000);
    }
}
