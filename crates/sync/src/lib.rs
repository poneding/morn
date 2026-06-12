//! 音视频同步: 音频主时钟与视频帧调度决策。纯计算, 无系统依赖。
mod clock;
pub use clock::{
    advance_action, decide_frame, gate_audio_chunk, AdvanceAction, ChunkGate, FrameDecision,
};
