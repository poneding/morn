//! 顶层播放编排: Player + 解码线程 + Timeline 快照。
mod decode_thread;
mod player;
mod timeline;
pub use decode_thread::{DecodeThread, FramePull};
pub use player::Player;
pub use timeline::Timeline;
