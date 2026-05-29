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
}
