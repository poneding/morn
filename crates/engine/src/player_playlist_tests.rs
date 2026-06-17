//! Playlist mutation command regressions.
//!
//! These tests cover restored playlist edits, current-item removal, destructive
//! file deletion, and full playlist cleanup.  They stay separate from open-command
//! and history tests so each file has one command family to scan.

use super::*;

fn open_sample_playlist(
    prefix: &str,
    names: &[&str],
) -> std::io::Result<(std::path::PathBuf, Vec<std::path::PathBuf>, Player)> {
    let dir = unique_temp_dir(prefix)?;
    let files = copy_sample_files(&dir, names)?;
    let mut player = Player::new();
    player.handle(Command::OpenFiles(files.clone()));
    assert_eq!(player.current_index(), Some(0));
    Ok((dir, files, player))
}

fn open_two_file_playlist(
    prefix: &str,
) -> std::io::Result<(
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
    Player,
)> {
    let (dir, files, player) = open_sample_playlist(prefix, &["a.mp4", "b.mp4"])?;
    Ok((dir, files[0].clone(), files[1].clone(), player))
}

#[test]
fn remove_playlist_index_updates_restored_playlist_without_opening_media() -> std::io::Result<()> {
    let dir = unique_temp_dir("morn_remove_restored")?;
    let prefs_path = dir.join("prefs.json");
    let mut prefs = persist::Preferences::default();
    prefs.last_playlist = vec!["/a.mp4".into(), "/b.mp4".into(), "/c.mp4".into()];
    prefs.last_index = 1;
    prefs.save(&prefs_path)?;

    let mut p = Player::with_prefs(prefs_path);
    p.handle(Command::RemovePlaylistIndex(0));

    assert_eq!(
        p.playlist_paths(),
        vec![
            std::path::PathBuf::from("/b.mp4"),
            std::path::PathBuf::from("/c.mp4")
        ]
    );
    assert_eq!(p.current_index(), Some(0));
    assert_eq!(p.timeline().state, PlaybackState::Stopped);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn removing_current_playing_item_switches_to_adjacent_paused() -> std::io::Result<()> {
    let (dir, _a, b, mut p) = open_two_file_playlist("morn_remove_current")?;
    assert_eq!(p.timeline().state, PlaybackState::Playing);

    p.handle(Command::RemovePlaylistIndex(0));

    assert_eq!(p.playlist_paths(), vec![b]);
    assert_eq!(p.current_index(), Some(0));
    assert_eq!(p.timeline().state, PlaybackState::Paused);
    p.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn clear_playlist_stops_and_removes_items() -> std::io::Result<()> {
    let (dir, _files, mut p) = open_sample_playlist("morn_clear_playlist", &["a.mp4"])?;
    assert_eq!(p.timeline().state, PlaybackState::Playing);

    p.handle(Command::ClearPlaylist);

    assert!(p.playlist_paths().is_empty());
    assert_eq!(p.current_index(), None);
    assert_eq!(p.timeline().state, PlaybackState::Stopped);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn delete_current_playlist_file_removes_disk_file_and_switches_to_adjacent_paused(
) -> std::io::Result<()> {
    let (dir, a, b, mut p) = open_two_file_playlist("morn_delete_current")?;

    p.handle(Command::DeletePlaylistFileIndex(0));

    assert!(!a.exists());
    assert_eq!(p.playlist_paths(), vec![b]);
    assert_eq!(p.current_index(), Some(0));
    assert_eq!(p.timeline().state, PlaybackState::Paused);
    p.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}
