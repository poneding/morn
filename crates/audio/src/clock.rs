use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// 平滑插值上限(毫秒): 两次音频回调之间用墙钟插值, 但最多补这么多。
/// 防止音频长时间停顿(欠载/暂停)时, 插值让时钟无限跑飞。
pub(crate) const MAX_INTERP_MS: u64 = 50;

/// 音频主时钟。回调线程累加已消费帧数, 任意线程读取播放位置。
///
/// `position_ms` 是"阶梯"位置(只随音频回调跳变, 确定性, 供 seekbar/seek 锚定/测试);
/// `position_ms_smooth` 在回调间隙额外用墙钟插值, 让视频 present 看到连续推进的时钟,
/// 消除"按音频回调跳变"导致的取帧节奏抖动(24fps@60Hz 的不规则 judder)。
#[derive(Clone)]
pub struct MasterClock {
    frames_played: Arc<AtomicU64>,
    anchor_frames: Arc<AtomicU64>,
    anchor_ms: Arc<AtomicU64>,
    sample_rate: u32,
    rate: Arc<AtomicU32>,
    // baseline 是 Copy: clone 后各副本共享同一时间基准。last_update 记录上次走时更新相对 baseline 的纳秒。
    baseline: Instant,
    last_update_nanos: Arc<AtomicU64>,
}

impl MasterClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            frames_played: Arc::new(AtomicU64::new(0)),
            anchor_frames: Arc::new(AtomicU64::new(0)),
            anchor_ms: Arc::new(AtomicU64::new(0)),
            sample_rate: sample_rate.max(1),
            rate: Arc::new(AtomicU32::new(100)),
            baseline: Instant::now(),
            last_update_nanos: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 记录"刚刚更新过走时"的墙钟时刻; position_ms_smooth 据此插值。
    fn mark_updated(&self) {
        let ns = self.baseline.elapsed().as_nanos() as u64;
        self.last_update_nanos.store(ns, Ordering::Relaxed);
    }

    /// 音频回调消费了 `n` 个音频帧(每帧含所有声道一份样本)后调用。
    /// 只有真实消费(n>0)才刷新走时标记: 欠载回调不算"在走时",
    /// 否则停滞检测(墙钟接管)与回调间插值都会被空回调误导。
    pub fn add_frames(&self, n: u64) {
        if n == 0 {
            return;
        }
        self.frames_played.fetch_add(n, Ordering::Relaxed);
        self.mark_updated();
    }

    /// 距上次真实走时更新(真实样本/重锚)的毫秒数。音频结束或缺失时持续增长,
    /// 供引擎判定"音频钟已停摆, 该切墙钟接管"。
    pub fn stalled_for_ms(&self) -> u64 {
        let now_ns = self.baseline.elapsed().as_nanos() as u64;
        let last_ns = self.last_update_nanos.load(Ordering::Relaxed);
        now_ns.saturating_sub(last_ns) / 1_000_000
    }

    /// 当前播放位置(毫秒, 阶梯值: 只随音频回调跳变)。
    pub fn position_ms(&self) -> u64 {
        let frames = self.frames_played.load(Ordering::Relaxed);
        let anchor_frames = self.anchor_frames.load(Ordering::Relaxed);
        let anchor_ms = self.anchor_ms.load(Ordering::Relaxed);
        let elapsed_frames = frames.saturating_sub(anchor_frames);
        let elapsed_ms = elapsed_frames * 1000 / self.sample_rate as u64;
        anchor_ms + elapsed_ms * self.rate.load(Ordering::Relaxed) as u64 / 100
    }

    /// 平滑播放位置: 阶梯位置 + 自上次更新以来的墙钟插值(上限 MAX_INTERP_MS)。供视频 present 取帧。
    pub fn position_ms_smooth(&self) -> u64 {
        let stepped = self.position_ms();
        let now_ns = self.baseline.elapsed().as_nanos() as u64;
        let last_ns = self.last_update_nanos.load(Ordering::Relaxed);
        let since_ms = now_ns.saturating_sub(last_ns) / 1_000_000;
        let interp = since_ms.min(MAX_INTERP_MS) * self.rate.load(Ordering::Relaxed) as u64 / 100;
        stepped + interp
    }

    /// 设置倍速百分比 (100 = 1.0x)。保留当前媒体位置, 只影响后续走时。
    pub fn set_rate(&self, pct: u16) {
        let pos = self.position_ms();
        let frames = self.frames_played.load(Ordering::Relaxed);
        self.anchor_ms.store(pos, Ordering::Relaxed);
        self.anchor_frames.store(frames, Ordering::Relaxed);
        self.rate.store(pct.max(1) as u32, Ordering::Relaxed);
        self.mark_updated();
    }

    /// seek 后重置时钟基准。
    pub fn reset_to(&self, ms: u64) {
        let frames = self.frames_played.load(Ordering::Relaxed);
        self.anchor_ms.store(ms, Ordering::Relaxed);
        self.anchor_frames.store(frames, Ordering::Relaxed);
        self.mark_updated();
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
    fn double_rate_affects_future_position() {
        let c = MasterClock::new(1000);
        c.add_frames(1000);
        c.set_rate(200);
        c.add_frames(500);
        assert_eq!(c.position_ms(), 2000);
    }

    #[test]
    fn set_rate_preserves_current_position() {
        let c = MasterClock::new(1000);
        c.add_frames(1000);
        c.set_rate(200);
        assert_eq!(c.position_ms(), 1000);
        c.add_frames(500);
        assert_eq!(c.position_ms(), 2000);
    }

    #[test]
    fn reset_to_uses_media_position_at_current_rate() {
        let c = MasterClock::new(1000);
        c.set_rate(200);
        c.reset_to(10_000);
        assert_eq!(c.position_ms(), 10_000);
        c.add_frames(500);
        assert_eq!(c.position_ms(), 11_000);
    }

    #[test]
    fn underrun_does_not_advance_clock() {
        // 回调欠载时应 add_frames(0) → 位置不前进(防止视频追逐空转的时钟)。
        let c = MasterClock::new(48_000);
        c.add_frames(48_000); // 1s 真实音频
        assert_eq!(c.position_ms(), 1000);
        c.add_frames(0); // 一次欠载回调: 不前进
        assert_eq!(c.position_ms(), 1000);
    }

    #[test]
    fn stall_timer_ignores_underrun_callbacks() {
        // 欠载回调(add_frames(0))不得刷新走时标记: 停滞时长用于判定
        // "音频已结束/缺失 → 该交给墙钟接管", 被空回调刷新就永远判不出停滞。
        let c = MasterClock::new(48_000);
        c.add_frames(480);
        std::thread::sleep(std::time::Duration::from_millis(30));
        c.add_frames(0);
        assert!(
            c.stalled_for_ms() >= 25,
            "欠载回调不应清零停滞计时, 实际 {}ms",
            c.stalled_for_ms()
        );
        c.add_frames(480);
        assert!(
            c.stalled_for_ms() < 25,
            "真实样本应刷新停滞计时, 实际 {}ms",
            c.stalled_for_ms()
        );
    }

    #[test]
    fn smoothed_position_interpolates_between_updates_capped() {
        // 平滑时钟: 两次音频回调之间用墙钟插值, 让 present 看到连续推进的时钟(消除 24fps@60Hz 节奏抖动)。
        let c = MasterClock::new(1000);
        c.add_frames(1000); // 阶梯位置 = 1000ms
        std::thread::sleep(std::time::Duration::from_millis(20));
        let s = c.position_ms_smooth();
        assert!(
            (1000..=1000 + super::MAX_INTERP_MS).contains(&s),
            "平滑位置 {s} 应在 [1000, {}] 内(约 1000+20ms 插值, 受上限约束)",
            1000 + super::MAX_INTERP_MS
        );
        // 阶梯位置不受插值影响, 保持确定性(供 seekbar/seek 锚定/测试)。
        assert_eq!(c.position_ms(), 1000);
    }
}
