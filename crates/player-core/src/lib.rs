//! 播放控制核心: 状态机、命令、播放列表。无 GUI/FFmpeg 依赖。

mod state;
pub use state::{InvalidTransition, PlaybackState, StateMachine, Transition};
