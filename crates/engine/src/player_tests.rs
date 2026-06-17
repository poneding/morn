//! Player integration-style unit tests.
//!
//! The engine is where playlist state, persisted preferences, decode threads,
//! audio clock handoff, seek gates, and end-of-playback policy meet.  These tests
//! keep those contracts explicit without requiring the egui app shell.  Most tests
//! drive `Player::handle` with real fixture media so command behavior, timeline
//! snapshots, playlist mutation, and persisted resume points are exercised through
//! the same public API the UI uses.
//!
//! Timing-sensitive tests prefer the video-only fixture when audio device state
//! would make local CI or developer machines nondeterministic.  That keeps seek
//! assertions focused on the engine's target/keyframe behavior rather than on
//! whether a local audio callback happened to advance during the assertion window.
//!
//! The restore tests cover the startup blank-frame path: opening the last selected
//! media, seeking to the saved resume point, pausing, and still presenting the
//! first decoded frame once it arrives.  This is intentionally tested at the
//! engine level because the UI only knows whether a texture has been uploaded.
//!
//! Playlist tests check both non-destructive commands and destructive file
//! deletion.  Removing the current item must switch to the next available media
//! and pause; removing a non-current item must not restart playback; history
//! changes must not accidentally mutate the active playlist.
//!
//! Persistence tests use temporary preference files so setter methods prove they
//! save immediately.  They also protect the split between full state saves and
//! lightweight periodic resume-point updates.
//!
//! The small source assertion near the end is intentionally narrow: it protects
//! the public screenshot-directory accessors from disappearing during module
//! splits while the behavior tests verify path resolution and preference writes.

use super::*;
use player_core::{Command, PlaybackState};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/tests/fixtures/sample.mp4")
}

fn video_only_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/tests/fixtures/sample_video_only.mp4")
}

fn unique_temp_dir(prefix: &str) -> std::io::Result<std::path::PathBuf> {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let dir = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), suffix));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn copy_sample_files(
    dir: &std::path::Path,
    names: &[&str],
) -> std::io::Result<Vec<std::path::PathBuf>> {
    let sample = fixture().canonicalize()?;
    names
        .iter()
        .map(|name| {
            let path = dir.join(name);
            std::fs::copy(&sample, &path)?;
            Ok(path)
        })
        .collect()
}

/// 驱动播放循环(模拟 app 的重绘): 反复 present_frame+tick 直到位置达到 target_ms 或超时。
/// 返回最终位置。
fn drive_until_position(p: &mut Player, target_ms: u64, timeout: std::time::Duration) -> u64 {
    let deadline = std::time::Instant::now() + timeout;
    let mut pos = p.timeline().position_ms;
    while std::time::Instant::now() < deadline {
        p.present_frame();
        p.tick();
        pos = p.timeline().position_ms;
        if pos >= target_ms {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    pos
}

#[path = "player_history_tests.rs"]
mod history_tests;
#[path = "player_playback_tests.rs"]
mod playback_tests;
#[path = "player_playlist_open_tests.rs"]
mod playlist_open_tests;
#[path = "player_playlist_sibling_tests.rs"]
mod playlist_sibling_tests;
#[path = "player_playlist_tests.rs"]
mod playlist_tests;
#[path = "player_restore_tests.rs"]
mod restore_tests;

#[test]
fn new_player_is_stopped_with_default_volume() {
    let p = Player::new();
    let t = p.timeline();
    assert_eq!(t.state, PlaybackState::Stopped);
    assert_eq!(t.volume, 100);
    assert_eq!(t.rate_pct, 100);
}

#[test]
fn set_volume_command_updates_timeline() {
    let mut p = Player::new();
    p.handle(Command::SetVolume(40));
    assert_eq!(p.timeline().volume, 40);
}

#[test]
fn set_rate_command_updates_timeline_even_without_media() {
    let mut p = Player::new();
    p.handle(Command::SetRate(175));
    assert_eq!(p.timeline().rate_pct, 175);
}

#[test]
fn current_video_dimensions_follow_opened_media() {
    let path = fixture();
    assert!(path.exists(), "先运行 media 的 gen_fixture.sh");

    let mut p = Player::new();
    assert_eq!(p.current_video_dimensions(), None);

    p.handle(Command::Open(path));

    assert_eq!(p.current_video_dimensions(), Some((160, 120)));
    p.handle(Command::Stop);
    assert_eq!(p.current_video_dimensions(), None);
}

#[test]
fn play_without_media_stays_stopped() {
    let mut p = Player::new();
    p.handle(Command::Play);
    assert_eq!(p.timeline().state, PlaybackState::Stopped);
}

#[test]
fn setters_update_prefs() {
    let mut p = Player::new();
    p.set_seek_step(20);
    p.set_language("en");
    p.set_theme("dark");
    p.set_subtitle_font_size(32.0);
    p.set_playback_mode(persist::PlaybackMode::LoopPlaylist);
    p.set_check_updates_on_startup(true);
    p.set_check_beta_updates(true);
    p.set_screenshot_dir("/tmp/morn-shots");
    assert_eq!(p.prefs().seek_step_secs, 20);
    assert_eq!(p.prefs().language, "en");
    assert_eq!(p.prefs().theme, "dark");
    assert_eq!(p.prefs().subtitle_font_size, 32.0);
    assert_eq!(p.prefs().playback_mode, persist::PlaybackMode::LoopPlaylist);
    assert!(p.prefs().check_updates_on_startup);
    assert!(p.prefs().check_beta_updates);
    assert_eq!(p.prefs().screenshot_dir, "/tmp/morn-shots");

    p.set_check_updates_on_startup(false);
    assert!(!p.prefs().check_updates_on_startup);
    assert!(
        !p.prefs().check_beta_updates,
        "beta updates cannot stay enabled when startup checks are disabled"
    );
}

#[test]
fn preference_setters_persist_immediately() -> std::io::Result<()> {
    let dir = unique_temp_dir("morn_prefs_setters")?;
    let prefs_path = dir.join("prefs.json");

    let mut p = Player::with_prefs(prefs_path.clone());
    p.set_seek_step(20);
    p.set_theme("dark");

    let loaded = persist::Preferences::load(&prefs_path)?;
    assert_eq!(loaded.seek_step_secs, 20);
    assert_eq!(loaded.theme, "dark");
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn player_exposes_screenshot_directory_preference_setter() {
    let source = [
        include_str!("player.rs"),
        include_str!("player_commands.rs"),
        include_str!("player_open.rs"),
        include_str!("player_present.rs"),
        include_str!("player_seek.rs"),
    ]
    .join("\n");

    assert!(source.contains("set_screenshot_dir"));
    assert!(source.contains("screenshot_dir(&self)"));
    assert!(source.contains("prefs.screenshot_dir"));
}

#[test]
fn screenshot_directory_accessors_resolve_legacy_tilde_path() {
    let mut p = Player::new();

    p.set_screenshot_dir("~\\Pictures\\Morn");

    assert_eq!(
        p.prefs().screenshot_dir,
        persist::resolve_screenshot_dir("~\\Pictures\\Morn").to_string_lossy()
    );
    assert_eq!(
        p.screenshot_dir(),
        persist::resolve_screenshot_dir("~\\Pictures\\Morn")
    );
}

#[test]
fn stop_mode_pauses_at_end() {
    assert_eq!(
        super::end_playback_action(persist::PlaybackMode::StopAtEnd, 2, Some(0)),
        super::EndPlaybackAction::PauseAtEnd
    );
}

#[test]
fn repeat_mode_restarts_current_item() {
    assert_eq!(
        super::end_playback_action(persist::PlaybackMode::RepeatOne, 2, Some(0)),
        super::EndPlaybackAction::RepeatCurrent
    );
}

#[test]
fn loop_mode_advances_and_wraps_playlist() {
    assert_eq!(
        super::end_playback_action(persist::PlaybackMode::LoopPlaylist, 3, Some(0)),
        super::EndPlaybackAction::OpenPlaylistIndex(1)
    );
    assert_eq!(
        super::end_playback_action(persist::PlaybackMode::LoopPlaylist, 3, Some(2)),
        super::EndPlaybackAction::OpenPlaylistIndex(0)
    );
    assert_eq!(
        super::end_playback_action(persist::PlaybackMode::LoopPlaylist, 1, Some(0)),
        super::EndPlaybackAction::RepeatCurrent
    );
}

#[test]
fn is_video_ext_filters() {
    assert!(super::is_video_ext(std::path::Path::new("/x/a.mp4")));
    assert!(super::is_video_ext(std::path::Path::new("/x/a.MKV")));
    assert!(!super::is_video_ext(std::path::Path::new("/x/a.txt")));
    assert!(!super::is_video_ext(std::path::Path::new("/x/a")));
}

#[test]
fn dir_videos_lists_sorted() {
    let dir = std::env::temp_dir().join(format!("morn_dir_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["b.mp4", "a.mp4", "note.txt", "c.mkv"] {
        let write_result = std::fs::write(dir.join(n), b"x");
        write_result.unwrap();
    }
    let got = super::dir_videos(&dir);
    let names: Vec<_> = got
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["a.mp4", "b.mp4", "c.mkv"]);
    let _cleanup = std::fs::remove_dir_all(&dir);
}
