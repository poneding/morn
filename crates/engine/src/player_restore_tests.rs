//! Last-session restore regressions.
//!
//! Restore opens the saved playlist item paused at the resume point and must still
//! present a frame even when the restore point is not exactly on a decoded frame.

use super::*;

fn restored_player_at(
    temp_prefix: &str,
    resume_ms: u64,
) -> std::io::Result<(Player, std::path::PathBuf)> {
    let video = fixture().canonicalize()?;
    let dir = unique_temp_dir(temp_prefix)?;
    let prefs_path = dir.join("prefs.json");
    let key = video.to_string_lossy().to_string();
    let mut prefs = persist::Preferences::default();
    prefs.last_playlist = vec![key.clone()];
    prefs.last_index = 0;
    prefs.set_resume_point(&key, resume_ms);
    prefs.save(&prefs_path)?;

    let mut player = Player::with_prefs(prefs_path);
    assert!(player.restore_last_session_paused());
    Ok((player, dir))
}

fn wait_for_current_frame(player: &mut Player) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while player.current_frame_rgba().is_none() && std::time::Instant::now() < deadline {
        player.present_frame();
        player.tick();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    player.current_frame_rgba().is_some()
}

#[test]
fn restore_last_session_opens_last_video_paused_at_resume_point() -> std::io::Result<()> {
    let (mut player, dir) = restored_player_at("morn_restore", 500)?;

    let timeline = player.timeline();
    assert_eq!(timeline.state, PlaybackState::Paused);
    assert!(
        (500..=800).contains(&timeline.position_ms),
        "expected restore near 500ms, got {}",
        timeline.position_ms
    );

    player.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn restore_last_session_paused_presents_first_seek_frame() -> std::io::Result<()> {
    let (mut player, dir) = restored_player_at("morn_restore_frame", 501)?;

    assert!(
        wait_for_current_frame(&mut player),
        "恢复到非帧边界并暂停时应显示恢复点之后的首帧, 不能把它当未来帧一直 hold"
    );

    player.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn restore_last_session_paused_presents_first_seek_frame_after_startup_delay() -> std::io::Result<()>
{
    let (mut player, dir) = restored_player_at("morn_restore_delayed_frame", 501)?;

    std::thread::sleep(std::time::Duration::from_millis(250));
    assert!(
        wait_for_current_frame(&mut player),
        "启动恢复可能晚于音频平滑外插窗口, 暂停态仍应立即显示恢复点后的首帧"
    );

    player.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
    Ok(())
}
