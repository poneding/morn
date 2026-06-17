//! Video frame presentation and clock handoff.
//!
//! Decoding happens on worker threads; this module decides which decoded video frame
//! should be visible for the current master-clock position.  It also releases seek
//! gates when the requested landing frame arrives, counts dropped frames, and hands
//! over from audio time to wall-clock time when audio ends before video.
//!
//! `present_frame` returns `None` when the texture can be reused.  That is important
//! for paused media and for startup restore, where the app should keep repainting
//! until the first frame appears but should not upload the same image every frame.

use super::*;

/// 诊断探针: MORN_DEBUG 置位时, present 每秒打印关键指标到 stderr。
pub(super) struct DebugProbe {
    enabled: bool,
    t: std::time::Instant,
    calls: u64,
    last_calls: u64,
    shown: u64,
    last_shown: u64,
    last_drops: u32,
    last_clock_ms: u64,
    last_decoded: u64,
    last_now_ms: u64,
    clock_updates: u32,
    max_jump_ms: u64,
    last_shown_at: Option<std::time::Instant>,
    max_shown_gap_ms: u64,
}

impl DebugProbe {
    pub(super) fn new() -> Self {
        Self {
            enabled: std::env::var("MORN_DEBUG").is_ok(),
            t: std::time::Instant::now(),
            calls: 0,
            last_calls: 0,
            shown: 0,
            last_shown: 0,
            last_drops: 0,
            last_clock_ms: 0,
            last_decoded: 0,
            last_now_ms: 0,
            clock_updates: 0,
            max_jump_ms: 0,
            last_shown_at: None,
            max_shown_gap_ms: 0,
        }
    }
}

impl Player {
    /// 按主时钟推进选帧, 返回本次需要新上传的帧。None 表示画面不变。
    pub fn present_frame(&mut self) -> Option<&media::VideoFrame> {
        self.maybe_release_seek_gate();
        self.maybe_handover_to_wall();
        let now = self.clock.position_ms_smooth();
        self.note_present_clock_step(now);

        let mut changed = false;
        let mut drops = 0u32;
        loop {
            let frame = match self.next_present_candidate() {
                Some(frame) => frame,
                None => break,
            };
            match self.present_candidate_action(now, frame.pts_ms, drops) {
                sync::AdvanceAction::Show => {
                    self.show_present_frame(frame);
                    changed = true;
                    break;
                }
                sync::AdvanceAction::DropAndContinue => {
                    drops += 1;
                }
                sync::AdvanceAction::HoldKeepCurrent => {
                    self.pending_frame = Some(frame);
                    break;
                }
            }
        }
        self.present_drops = self.present_drops.saturating_add(drops);
        self.debug_log(now);
        changed.then(|| self.current_frame.as_ref()).flatten()
    }

    fn next_present_candidate(&mut self) -> Option<media::VideoFrame> {
        self.pending_frame
            .take()
            .or_else(|| self.video.as_ref()?.try_recv_frame())
    }

    fn present_candidate_action(&self, now: u64, pts_ms: u64, drops: u32) -> sync::AdvanceAction {
        let action = sync::advance_action(now, pts_ms, PRESENT_TOL_MS, drops, MAX_DROP_PER_PRESENT);
        if action == sync::AdvanceAction::HoldKeepCurrent
            && self.current_frame.is_none()
            && self.machine.state() == player_core::PlaybackState::Paused
        {
            sync::AdvanceAction::Show
        } else {
            action
        }
    }

    fn show_present_frame(&mut self, frame: media::VideoFrame) {
        self.current_frame = Some(frame);
        self.dbg.shown += 1;
        self.note_visible_frame_gap();
    }

    fn note_present_clock_step(&mut self, now: u64) {
        self.dbg.calls += 1;
        if !self.dbg.enabled || now == self.dbg.last_now_ms {
            return;
        }
        self.dbg.clock_updates += 1;
        let jump = now.saturating_sub(self.dbg.last_now_ms);
        if self.dbg.last_now_ms != 0 && jump > self.dbg.max_jump_ms {
            self.dbg.max_jump_ms = jump;
        }
        self.dbg.last_now_ms = now;
    }

    fn note_visible_frame_gap(&mut self) {
        if !self.dbg.enabled {
            return;
        }
        let now = std::time::Instant::now();
        if let Some(prev) = self.dbg.last_shown_at {
            let gap = now.duration_since(prev).as_millis() as u64;
            if gap > self.dbg.max_shown_gap_ms {
                self.dbg.max_shown_gap_ms = gap;
            }
        }
        self.dbg.last_shown_at = Some(now);
    }

    /// 诊断: MORN_DEBUG 置位时每秒打印一行关键指标, 定位卡顿/不同步的环节。
    fn debug_log(&mut self, now_ms: u64) {
        if !self.dbg.enabled {
            return;
        }
        let elapsed = self.dbg.t.elapsed();
        if elapsed < std::time::Duration::from_secs(1) {
            return;
        }
        let sample = self.debug_sample(now_ms, elapsed.as_secs_f64().max(0.001));
        eprintln!(
            "[morn] decode={:.0}fps hw={} q={} | present_calls={:.0}/s shown={:.0}fps drop={:.0}/s | clock={}ms adv={:.0}ms/s drift={}ms | clock_steps={:.0}/s max_jump={}ms max_frame_gap={}ms src={}",
            sample.decode_fps,
            sample.hardware,
            sample.queue_len,
            sample.calls_fps,
            sample.shown_fps,
            sample.drop_fps,
            now_ms,
            sample.clock_adv,
            sample.drift,
            sample.clock_steps,
            sample.max_jump_ms,
            sample.max_frame_gap_ms,
            sample.clock_source,
        );
        self.reset_debug_probe_sample(now_ms, sample.decoded_total);
    }

    fn debug_sample(&self, now_ms: u64, secs: f64) -> DebugSample {
        let decoded_total = self.video.as_ref().map(|v| v.decoded_total()).unwrap_or(0);
        let frame_pts = self
            .current_frame
            .as_ref()
            .map(|f| f.pts_ms as i64)
            .unwrap_or(now_ms as i64);
        DebugSample {
            decoded_total,
            decode_fps: decoded_total.saturating_sub(self.dbg.last_decoded) as f64 / secs,
            queue_len: self.video.as_ref().map(|v| v.queue_len()).unwrap_or(0),
            hardware: self
                .video
                .as_ref()
                .map(|v| v.is_hardware())
                .unwrap_or(false),
            calls_fps: self.dbg.calls.saturating_sub(self.dbg.last_calls) as f64 / secs,
            shown_fps: self.dbg.shown.saturating_sub(self.dbg.last_shown) as f64 / secs,
            drop_fps: self.present_drops.saturating_sub(self.dbg.last_drops) as f64 / secs,
            clock_adv: now_ms.saturating_sub(self.dbg.last_clock_ms) as f64 / secs,
            drift: now_ms as i64 - frame_pts,
            clock_steps: self.dbg.clock_updates as f64 / secs,
            max_jump_ms: self.dbg.max_jump_ms,
            max_frame_gap_ms: self.dbg.max_shown_gap_ms,
            clock_source: if matches!(self.clock, PlayClock::Audio(_)) {
                "audio"
            } else {
                "wall"
            },
        }
    }

    fn reset_debug_probe_sample(&mut self, now_ms: u64, decoded_total: u64) {
        self.dbg.t = std::time::Instant::now();
        self.dbg.last_decoded = decoded_total;
        self.dbg.last_calls = self.dbg.calls;
        self.dbg.last_shown = self.dbg.shown;
        self.dbg.last_drops = self.present_drops;
        self.dbg.last_clock_ms = now_ms;
        self.dbg.clock_updates = 0;
        self.dbg.max_jump_ms = 0;
        self.dbg.max_shown_gap_ms = 0;
    }
}

struct DebugSample {
    decoded_total: u64,
    decode_fps: f64,
    queue_len: usize,
    hardware: bool,
    calls_fps: f64,
    shown_fps: f64,
    drop_fps: f64,
    clock_adv: f64,
    drift: i64,
    clock_steps: f64,
    max_jump_ms: u64,
    max_frame_gap_ms: u64,
    clock_source: &'static str,
}
