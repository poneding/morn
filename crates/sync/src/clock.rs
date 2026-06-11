/// 对单个视频帧的调度决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDecision {
    /// 立即显示该帧。
    Display,
    /// 该帧已过期, 丢弃以追赶主时钟。
    Drop,
    /// 该帧尚未到显示时间, 等待指定毫秒后再处理。
    Wait { remaining_ms: u64 },
}

/// 给定主时钟位置 `master_ms` 与帧呈现时间 `frame_pts_ms`,
/// 在容差 `tolerance_ms`(毫秒, 非负窗口)内判定该帧应如何处理。
pub fn decide_frame(master_ms: u64, frame_pts_ms: u64, tolerance_ms: u64) -> FrameDecision {
    let diff = frame_pts_ms as i64 - master_ms as i64; // 正: 帧在未来; 负: 帧已过去
    if diff.unsigned_abs() <= tolerance_ms {
        FrameDecision::Display
    } else if diff < 0 {
        FrameDecision::Drop
    } else {
        FrameDecision::Wait {
            remaining_ms: diff as u64,
        }
    }
}

/// present 循环对"队列头一帧"的推进动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvanceAction {
    /// 到点: 把这帧设为当前显示帧。
    Show,
    /// 已过期: 丢弃, 继续看下一帧(追赶)。
    DropAndContinue,
    /// 未到点: 留作 pending, 保持当前画面。
    HoldKeepCurrent,
}

/// 给定主时钟 `master_ms` 与队列头帧 `frame_pts_ms`, 决定 present 推进动作。
/// `drops_so_far` 为本次 present 已丢弃帧数, `max_drop` 为单次上限——
/// 巨量迟帧时达上限即强制显示当前帧, 避免长时间无画面。
pub fn advance_action(
    master_ms: u64,
    frame_pts_ms: u64,
    tolerance_ms: u64,
    drops_so_far: u32,
    max_drop: u32,
) -> AdvanceAction {
    match decide_frame(master_ms, frame_pts_ms, tolerance_ms) {
        FrameDecision::Display => AdvanceAction::Show,
        FrameDecision::Wait { .. } => AdvanceAction::HoldKeepCurrent,
        FrameDecision::Drop => {
            if drops_so_far >= max_drop {
                AdvanceAction::Show
            } else {
                AdvanceAction::DropAndContinue
            }
        }
    }
}

/// seek 后对单个音频块的裁剪判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkGate {
    /// 整块都在目标之前: 丢弃, 继续解下一块。
    SkipAll,
    /// 从本块开始播放: 先丢弃前 `trim_frames` 帧, 实际起播位置为 `anchor_ms`,
    /// 主时钟应以 `anchor_ms` 锚定(= 实际播出的首个样本的媒体时间)。
    PlayFrom { trim_frames: usize, anchor_ms: u64 },
}

/// seek 后音频块的裁剪判定: demuxer seek 通常落在目标之前的关键帧,
/// 其后解出的块覆盖 [chunk_pts_ms, chunk_pts_ms + frame_count/sample_rate)。
/// 目标之前的样本必须丢弃, 否则"时钟在目标、内容在关键帧"→ 永久音画偏移。
pub fn gate_audio_chunk(
    chunk_pts_ms: u64,
    frame_count: usize,
    sample_rate: u32,
    target_ms: u64,
) -> ChunkGate {
    let rate = sample_rate.max(1) as u64;
    let end_ms = chunk_pts_ms + frame_count as u64 * 1000 / rate;
    if end_ms <= target_ms {
        return ChunkGate::SkipAll;
    }
    if chunk_pts_ms >= target_ms {
        return ChunkGate::PlayFrom {
            trim_frames: 0,
            anchor_ms: chunk_pts_ms,
        };
    }
    let trim_frames = ((target_ms - chunk_pts_ms) * rate / 1000) as usize;
    let trim_frames = trim_frames.min(frame_count);
    let anchor_ms = chunk_pts_ms + trim_frames as u64 * 1000 / rate;
    ChunkGate::PlayFrom {
        trim_frames,
        anchor_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 容差 10ms: 帧 PTS 落在 [master-10, master+10] 内即认为应显示。
    const TOL: u64 = 10;

    #[test]
    fn frame_at_master_displays() {
        assert_eq!(decide_frame(1000, 1000, TOL), FrameDecision::Display);
    }

    #[test]
    fn frame_within_tolerance_displays() {
        assert_eq!(decide_frame(1000, 1005, TOL), FrameDecision::Display);
        assert_eq!(decide_frame(1000, 995, TOL), FrameDecision::Display);
    }

    #[test]
    fn frame_far_behind_master_is_dropped() {
        // 帧 PTS 远早于主时钟 → 该帧已过期, 丢弃以追赶。
        assert_eq!(decide_frame(2000, 1500, TOL), FrameDecision::Drop);
    }

    #[test]
    fn frame_ahead_of_master_waits() {
        // 帧 PTS 远晚于主时钟 → 还没到显示时间, 等待。
        let d = decide_frame(1000, 1200, TOL);
        assert_eq!(d, FrameDecision::Wait { remaining_ms: 200 });
    }

    #[test]
    fn frame_exactly_at_tolerance_edge_displays() {
        // diff == +tol 和 diff == -tol 都在容差内(闭区间)。
        assert_eq!(decide_frame(1000, 1010, TOL), FrameDecision::Display);
        assert_eq!(decide_frame(1000, 990, TOL), FrameDecision::Display);
    }

    #[test]
    fn frame_just_past_tolerance_transitions() {
        // 刚越过容差: 未来 → Wait{完整差值}, 过去 → Drop。
        assert_eq!(
            decide_frame(1000, 1011, TOL),
            FrameDecision::Wait { remaining_ms: 11 }
        );
        assert_eq!(decide_frame(1000, 989, TOL), FrameDecision::Drop);
    }

    #[test]
    fn advance_shows_due_frame() {
        assert_eq!(advance_action(1000, 1000, TOL, 0, 8), AdvanceAction::Show);
    }

    #[test]
    fn advance_holds_future_frame() {
        assert_eq!(
            advance_action(1000, 2000, TOL, 0, 8),
            AdvanceAction::HoldKeepCurrent
        );
    }

    #[test]
    fn advance_drops_late_frame_until_cap() {
        assert_eq!(
            advance_action(2000, 1000, TOL, 0, 8),
            AdvanceAction::DropAndContinue
        );
        // 达到丢帧上限: 不再丢, 强制显示以尽快出画面。
        assert_eq!(advance_action(2000, 1000, TOL, 8, 8), AdvanceAction::Show);
    }

    // —— gate_audio_chunk: seek 后音频块的裁剪判定 ——
    // demuxer seek 落在关键帧(常早于目标), 之后解出的音频块必须裁到目标,
    // 且时钟按实际首个播放样本的 PTS 锚定, 否则每次 seek 都产生永久音画偏移。

    #[test]
    fn gate_skips_chunk_entirely_before_target() {
        // 块覆盖 [1000, 1010), 目标 1500 → 整块丢弃。
        assert_eq!(
            gate_audio_chunk(1000, 480, 48_000, 1500),
            ChunkGate::SkipAll
        );
        // 块尾恰好等于目标(区间右开)→ 仍整块丢弃。
        assert_eq!(
            gate_audio_chunk(1490, 480, 48_000, 1500),
            ChunkGate::SkipAll
        );
    }

    #[test]
    fn gate_trims_chunk_straddling_target() {
        // 块覆盖 [1000, 2000), 目标 1500 → 裁掉前 24000 帧, 从 1500 锚定。
        assert_eq!(
            gate_audio_chunk(1000, 48_000, 48_000, 1500),
            ChunkGate::PlayFrom {
                trim_frames: 24_000,
                anchor_ms: 1500
            }
        );
    }

    #[test]
    fn gate_keeps_chunk_at_or_after_target_and_anchors_to_its_pts() {
        // 块起点已在目标之后(seek 落点晚于目标或目标处无音频)→ 不裁, 锚定到块 PTS。
        assert_eq!(
            gate_audio_chunk(1600, 480, 48_000, 1500),
            ChunkGate::PlayFrom {
                trim_frames: 0,
                anchor_ms: 1600
            }
        );
        // 块起点恰为目标 → 不裁。
        assert_eq!(
            gate_audio_chunk(1500, 480, 48_000, 1500),
            ChunkGate::PlayFrom {
                trim_frames: 0,
                anchor_ms: 1500
            }
        );
    }
}
