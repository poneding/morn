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
    PlayIndex(usize),
    StepFrame,
    SetLoopA,
    SetLoopB,
    ClearLoop,
    ToggleMute,
    OpenDialog,
    OpenFolder,
    SelectSubtitleTrack(usize),
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
        assert_eq!(Command::PlayIndex(2), Command::PlayIndex(2));
        assert_eq!(Command::StepFrame, Command::StepFrame);
        assert_eq!(Command::SetLoopA, Command::SetLoopA);
        assert_eq!(Command::SetLoopB, Command::SetLoopB);
        assert_eq!(Command::ClearLoop, Command::ClearLoop);
        assert_eq!(Command::ToggleMute, Command::ToggleMute);
        assert_eq!(Command::OpenDialog, Command::OpenDialog);
        assert_eq!(Command::OpenFolder, Command::OpenFolder);
        assert_eq!(
            Command::SelectSubtitleTrack(1),
            Command::SelectSubtitleTrack(1)
        );
        assert_ne!(Command::Play, Command::Pause);
    }
}
