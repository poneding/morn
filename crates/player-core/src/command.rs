use std::path::PathBuf;

/// UI 发往播放核心的命令。
/// 时间单位为毫秒, 音量为 0..=100, 倍速为百分比 (100 = 1.0x)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open(PathBuf),
    Play,
    Pause,
    Stop,
    SeekTo(u64),
    SetVolume(u8),
    SetRate(u16),
    Next,
    Prev,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn commands_are_constructible_and_comparable() {
        assert_eq!(Command::Play, Command::Play);
        assert_eq!(
            Command::Open(PathBuf::from("/v.mp4")),
            Command::Open(PathBuf::from("/v.mp4"))
        );
        assert_eq!(Command::SeekTo(1500), Command::SeekTo(1500));
        assert_eq!(Command::SetVolume(80), Command::SetVolume(80));
        assert_eq!(Command::SetRate(150), Command::SetRate(150));
        assert_ne!(Command::Play, Command::Pause);
    }
}
