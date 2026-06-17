//! Playlist open/append command regressions.
//!
//! Single-file opens append/select exactly that file, and multi-file opens
//! preserve the selected order.  Directory scanning lives in the sibling-command
//! tests.
//!
//! These cases also protect persistence boundaries: opening a new item should save
//! the updated playlist immediately, while exposing playlist paths as a borrowed
//! slice keeps UI snapshots cheap and avoids accidental mutation from callers.

use super::*;

fn two_sample_files(
    prefix: &str,
) -> std::io::Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    let dir = unique_temp_dir(prefix)?;
    let files = copy_sample_files(&dir, &["a.mp4", "b.mp4"])?;
    Ok((dir, files[0].clone(), files[1].clone()))
}

#[test]
fn open_file_command_does_not_expand_to_sibling_videos() -> std::io::Result<()> {
    let (dir, a, _b) = two_sample_files("morn_open_single")?;

    let mut p = Player::new();
    p.handle(Command::Open(a.clone()));

    assert_eq!(p.playlist_paths(), vec![a]);
    p.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn open_file_command_appends_to_existing_playlist_and_selects_new_file() -> std::io::Result<()> {
    let (dir, a, b) = two_sample_files("morn_open_single_append")?;

    let mut p = Player::new();
    p.handle(Command::OpenFiles(vec![a.clone()]));
    p.handle(Command::Open(b.clone()));

    assert_eq!(p.playlist_paths(), vec![a, b]);
    assert_eq!(p.current_index(), Some(1));
    p.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn open_files_command_appends_to_existing_playlist_and_selects_first_new_file(
) -> std::io::Result<()> {
    let dir = unique_temp_dir("morn_open_files_append")?;
    let files = copy_sample_files(&dir, &["a.mp4", "b.mp4", "c.mp4"])?;
    let a = files[0].clone();
    let b = files[1].clone();
    let c = files[2].clone();

    let mut p = Player::new();
    p.handle(Command::OpenFiles(vec![a.clone()]));
    p.handle(Command::OpenFiles(vec![b.clone(), c.clone()]));

    assert_eq!(p.playlist_paths(), vec![a, b, c]);
    assert_eq!(p.current_index(), Some(1));
    p.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn opening_file_persists_latest_playlist_item_immediately() -> std::io::Result<()> {
    let (dir, a, b) = two_sample_files("morn_save_latest_open")?;
    let prefs_path = dir.join("prefs.json");

    let mut p = Player::with_prefs(prefs_path.clone());
    p.handle(Command::Open(a.clone()));
    p.handle(Command::Open(b.clone()));

    let restored = Player::with_prefs(prefs_path);
    assert_eq!(restored.playlist_paths(), vec![a, b]);
    assert_eq!(restored.current_index(), Some(1));
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn playlist_paths_are_exposed_as_borrowed_slice() {
    fn assert_slice(_: &[std::path::PathBuf]) {}

    let p = Player::new();

    assert_slice(p.playlist_paths());
}

#[test]
fn open_files_command_uses_selected_files_only() -> std::io::Result<()> {
    let (dir, a, b) = two_sample_files("morn_open_files")?;

    let mut p = Player::new();
    p.handle(Command::OpenFiles(vec![b.clone(), a.clone()]));

    assert_eq!(p.playlist_paths(), vec![b, a]);
    assert_eq!(p.current_index(), Some(0));
    p.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}
