use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 音频主时钟。回调线程累加已消费帧数, 任意线程读取播放位置。
#[derive(Clone)]
pub struct MasterClock {
    frames_played: Arc<AtomicU64>,
    sample_rate: u32,
}

impl MasterClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            frames_played: Arc::new(AtomicU64::new(0)),
            sample_rate: sample_rate.max(1),
        }
    }

    /// 音频回调消费了 `n` 个音频帧(每帧含所有声道一份样本)后调用。
    pub fn add_frames(&self, n: u64) {
        self.frames_played.fetch_add(n, Ordering::Relaxed);
    }

    /// 当前播放位置(毫秒)。
    pub fn position_ms(&self) -> u64 {
        let f = self.frames_played.load(Ordering::Relaxed);
        f * 1000 / self.sample_rate as u64
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
}
