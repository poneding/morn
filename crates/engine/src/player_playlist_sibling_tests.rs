//! Sibling video expansion command regressions.
//!
//! The sibling command is intentionally separate from file-open commands: only an
//! explicit sibling request should scan the containing directory.

use super::*;

#[test]
fn open_sibling_videos_loads_directory_and_selects_requested_file() -> std::io::Result<()> {
    let dir = unique_temp_dir("morn_open_siblings")?;
    let a = dir.join("a.mp4");
    let b = dir.join("b.mp4");
    let c = dir.join("c.mp4");
    let note = dir.join("note.txt");
    let write_a = std::fs::write(&a, b"dummy");
    write_a?;
    let write_b = std::fs::write(&b, b"dummy");
    write_b?;
    let write_c = std::fs::write(&c, b"dummy");
    write_c?;
    let write_note = std::fs::write(note, b"not a video");
    write_note?;

    let mut p = Player::new();
    p.handle(Command::OpenSiblingVideos(b.clone()));

    assert_eq!(p.playlist_paths(), vec![a, b, c]);
    assert_eq!(p.current_index(), Some(1));
    p.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}
