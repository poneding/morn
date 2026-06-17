//! History mutation command regressions.
//!
//! History edits must not mutate the active playlist, and destructive history
//! deletion must remove both the persisted history entry and the disk file.

use super::*;

#[test]
fn history_remove_and_clear_commands_update_history_only() -> std::io::Result<()> {
    let dir = unique_temp_dir("morn_history_remove")?;
    let prefs_path = dir.join("prefs.json");
    let mut prefs = persist::Preferences::default();
    prefs.history = vec!["/a.mp4".into(), "/b.mp4".into(), "/c.mp4".into()];
    prefs.save(&prefs_path)?;

    let mut p = Player::with_prefs(prefs_path);
    p.handle(Command::RemoveHistoryIndex(1));
    assert_eq!(p.history(), &["/a.mp4".to_string(), "/c.mp4".to_string()]);

    p.handle(Command::ClearHistory);
    assert!(p.history().is_empty());
    assert_eq!(p.timeline().state, PlaybackState::Stopped);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn delete_history_file_removes_disk_file_and_history_entry() -> std::io::Result<()> {
    let dir = unique_temp_dir("morn_delete_history")?;
    let file = dir.join("old.mp4");
    let write_result = std::fs::write(&file, b"old");
    write_result?;
    let prefs_path = dir.join("prefs.json");
    let mut prefs = persist::Preferences::default();
    prefs.history = vec![file.to_string_lossy().to_string()];
    prefs.save(&prefs_path)?;

    let mut p = Player::with_prefs(prefs_path);
    p.handle(Command::DeleteHistoryFileIndex(0));

    assert!(!file.exists());
    assert!(p.history().is_empty());
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}
