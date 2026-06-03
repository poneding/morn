use std::path::PathBuf;

/// UI 发往播放核心的命令。
/// 时间单位为毫秒, 音量为 0..=100, 倍速为百分比 (100 = 1.0x)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open(PathBuf),
    OpenFiles(Vec<PathBuf>),
    Play,
    Pause,
    Stop,
    SeekTo(u64),
    SetVolume(u8),
    SetRate(u16),
    Next,
    Prev,
    PlayIndex(usize),
    RemovePlaylistIndex(usize),
    ClearPlaylist,
    RemoveHistoryIndex(usize),
    ClearHistory,
    DeletePlaylistFileIndex(usize),
    DeleteHistoryFileIndex(usize),
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
        assert_eq!(
            Command::OpenFiles(vec![PathBuf::from("/a.mp4"), PathBuf::from("/b.mp4")]),
            Command::OpenFiles(vec![PathBuf::from("/a.mp4"), PathBuf::from("/b.mp4")])
        );
        assert_eq!(Command::SeekTo(1500), Command::SeekTo(1500));
        assert_eq!(Command::SetVolume(80), Command::SetVolume(80));
        assert_eq!(Command::SetRate(150), Command::SetRate(150));
        assert_eq!(Command::PlayIndex(2), Command::PlayIndex(2));
        assert_eq!(
            Command::RemovePlaylistIndex(2),
            Command::RemovePlaylistIndex(2)
        );
        assert_eq!(Command::ClearPlaylist, Command::ClearPlaylist);
        assert_eq!(
            Command::RemoveHistoryIndex(2),
            Command::RemoveHistoryIndex(2)
        );
        assert_eq!(Command::ClearHistory, Command::ClearHistory);
        assert_eq!(
            Command::DeletePlaylistFileIndex(2),
            Command::DeletePlaylistFileIndex(2)
        );
        assert_eq!(
            Command::DeleteHistoryFileIndex(2),
            Command::DeleteHistoryFileIndex(2)
        );
        assert_eq!(Command::ToggleMute, Command::ToggleMute);
        assert_eq!(Command::OpenDialog, Command::OpenDialog);
        assert_eq!(Command::OpenFolder, Command::OpenFolder);
        assert_eq!(
            Command::SelectSubtitleTrack(1),
            Command::SelectSubtitleTrack(1)
        );
        assert_ne!(Command::Play, Command::Pause);
    }

    #[test]
    fn command_api_excludes_removed_frame_step() {
        let source = include_str!("command.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(
            !source.contains(concat!("Step", "Frame")),
            "command API still exposes removed step-frame playback"
        );
    }
}
