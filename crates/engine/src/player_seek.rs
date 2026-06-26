use super::*;

pub(super) fn user_seek_mode(current_ms: u64, target_ms: u64) -> SeekMode {
    if target_ms < current_ms {
        SeekMode::KeyframeBackward
    } else {
        SeekMode::KeyframeForward
    }
}

/// seek 闸门: seek 后冻结主时钟, 视频就绪才放行, 避免 seek 后音频先跑而视频追赶。
#[derive(Clone, Copy)]
pub(super) enum SeekGate {
    /// 精确模式(内部 seek): 等"当前代次且 PTS >= 目标"的帧(或流尾)。
    Exact { seq: u64, target_ms: u64 },
    /// 关键帧吸附(UI seek, 秒播): 等当前代次首帧落地, 放行时对齐到它。
    Keyframe { seq: u64, fallback_target_ms: u64 },
}

impl Player {
    /// 内部精确 seek(续播恢复/重播/结束重置): 解码追赶到目标, 起播帧 PTS >= 目标。
    pub(super) fn seek_to(&mut self, ms: u64) {
        self.seek_impl(ms, SeekMode::Exact);
    }

    /// 用户可见 seek(进度条/方向键): 速度优先, 像 mpv/IINA 默认相对 seek 一样吸附关键帧。
    pub(super) fn seek_to_user_target(&mut self, ms: u64) {
        self.seek_impl(ms, user_seek_mode(self.raw_position_ms(), ms));
    }

    fn seek_impl(&mut self, ms: u64, mode: SeekMode) {
        let target = if self.duration_ms > 0 {
            ms.min(self.duration_ms)
        } else {
            ms
        };
        self.playback_ended = false;
        let gate = self.request_video_seek_gate(target, mode);
        self.clear_presented_frame();
        self.prepare_audio_for_seek(target, mode);
        self.clock.reset_to(target);
        self.seek_gate = gate;
        self.audio_gate
            .store(self.seek_gate.is_some(), Ordering::Relaxed);
        if self.seek_gate.is_some() {
            self.clock.pause();
        }
    }

    fn request_video_seek_gate(&mut self, target: u64, mode: SeekMode) -> Option<SeekGate> {
        let video = self.video.as_ref()?;
        let seq = video.request_seek(target, mode);
        // 排空旧帧, 解除解码线程发送阻塞; 竞态溜进来的旧帧由 serial 过滤兜底。
        while video.try_recv_frame().is_some() {}
        Some(match mode {
            SeekMode::Exact => SeekGate::Exact {
                seq,
                target_ms: target,
            },
            SeekMode::KeyframeBackward | SeekMode::KeyframeForward => SeekGate::Keyframe {
                seq,
                fallback_target_ms: target,
            },
        })
    }

    fn prepare_audio_for_seek(&mut self, target: u64, mode: SeekMode) {
        if matches!(mode, SeekMode::Exact) {
            self.audio_seek.store(target, Ordering::Relaxed);
        }
        self.audio_flush.store(true, Ordering::Relaxed);
        self.restore_audio_clock_after_eof_handover();
    }

    fn restore_audio_clock_after_eof_handover(&mut self) {
        let Some(handle) = &self.audio_out else {
            return;
        };
        if !self
            .audio_join
            .as_ref()
            .is_some_and(|join| !join.is_finished())
        {
            return;
        }
        self.audio_ended.store(false, Ordering::Relaxed);
        if matches!(self.clock, PlayClock::Wall(_)) {
            self.clock = PlayClock::Audio(handle.clock.clone());
            self.clock.set_rate(self.rate_pct);
        }
    }

    /// seek 闸门是否仍在等待视频解出落点帧(app 在挂起期间保持重绘以驱动放行)。
    pub fn seek_pending(&self) -> bool {
        self.seek_gate.is_some()
    }

    /// 闸门放行判定: 精确模式等目标帧, 吸附模式等当前代次首帧并对齐音频/时钟。
    pub(super) fn maybe_release_seek_gate(&mut self) {
        if let Some(align) = self.seek_gate.and_then(|gate| self.seek_gate_release(gate)) {
            self.finish_seek_gate(align);
        }
    }

    fn seek_gate_release(&self, gate: SeekGate) -> Option<Option<u64>> {
        let Some(video) = &self.video else {
            return Some(None);
        };
        match gate {
            SeekGate::Exact { seq, target_ms } => exact_seek_gate_release(video, seq, target_ms),
            SeekGate::Keyframe {
                seq,
                fallback_target_ms,
            } => keyframe_seek_gate_release(video, seq, fallback_target_ms),
        }
    }

    /// 放行闸门; 吸附模式把时钟与音频对齐到落点 `align_to_ms`(精确模式传 None)。
    fn finish_seek_gate(&mut self, align_to_ms: Option<u64>) {
        self.seek_gate = None;
        if let Some(target) = align_to_ms {
            self.audio_seek.store(target, Ordering::Relaxed);
            self.audio_flush.store(true, Ordering::Relaxed);
            self.clock.reset_to(target);
        }
        self.audio_gate.store(false, Ordering::Relaxed);
        if self.machine.state() == player_core::PlaybackState::Playing {
            self.clock.resume();
        }
    }
}

fn exact_seek_gate_release(video: &DecodeThread, seq: u64, target_ms: u64) -> Option<Option<u64>> {
    let ready = video.applied_seek_seq() >= seq
        && (video
            .latest_pts_after_seek()
            .is_some_and(|pts| pts + PRESENT_TOL_MS >= target_ms)
            || video.is_ended());
    ready.then_some(None)
}

fn keyframe_seek_gate_release(
    video: &DecodeThread,
    seq: u64,
    fallback_target_ms: u64,
) -> Option<Option<u64>> {
    if video.applied_seek_seq() < seq {
        return None;
    }
    match video.latest_pts_after_seek() {
        Some(pts) => Some(Some(pts)),
        None if video.is_ended() => Some(Some(fallback_target_ms)),
        None => None,
    }
}
