use super::*;

fn playback_fixture(name: &str) -> std::path::PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../media/tests/fixtures")
        .join(name);
    assert!(path.exists(), "先运行 media 的 gen_fixture.sh");
    path
}

fn open_fixture(path: std::path::PathBuf) -> Player {
    let mut player = Player::new();
    player.handle(Command::Open(path));
    player
}

fn wait_for_seek_release(player: &mut Player, tick: bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while player.seek_pending() && std::time::Instant::now() < deadline {
        player.present_frame();
        if tick {
            player.tick();
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn video_only_file_advances_position_while_playing() {
    // 无音轨文件: 音频设备能打开但没有音频流, 播放位置必须由墙钟接管前进,
    // 否则时钟挂在永不走时的音频钟上 → 画面永远冻在首帧。
    let mut p = open_fixture(playback_fixture("sample_video_only.mp4"));
    assert_eq!(p.timeline().state, PlaybackState::Playing);

    let pos = drive_until_position(&mut p, 400, std::time::Duration::from_secs(3));
    assert!(pos >= 400, "纯视频文件位置应随墙钟前进, 实际停在 {pos}ms");
    p.handle(Command::Stop);
}

#[test]
fn backward_ui_seek_snaps_to_previous_keyframe_for_speed() {
    // 速度优先的用户 seek 参考 mpv/IINA 默认相对 seek: 快退吸附到
    // 目标之前的关键帧, 不解码追赶到精确目标。
    // sample.mp4 只有 0ms 一个关键帧, 从 ~400ms 退到 100ms 应落回 0ms 附近。
    let mut p = open_fixture(video_only_fixture());
    // 预热: 播放推进到 400ms 之后再快退。
    drive_until_position(&mut p, 400, std::time::Duration::from_secs(3));

    p.handle(Command::SeekTo(100));
    assert!(p.seek_pending(), "SeekTo 后闸门应挂起");

    wait_for_seek_release(&mut p, false);
    assert!(!p.seek_pending(), "落点帧到达后闸门应放行");
    let pos = p.timeline().position_ms;
    assert!(
        pos <= 50,
        "速度优先快退应吸附到前关键帧 0ms 附近, 实际 {pos}ms"
    );

    // 起播后继续推进(音频/墙钟驱动)。
    let pos = drive_until_position(&mut p, 300, std::time::Duration::from_secs(3));
    assert!(pos >= 300, "seek 起播后应继续推进, 实际 {pos}ms");
    p.handle(Command::Stop);
}

#[test]
fn forward_ui_seek_snaps_to_next_keyframe_for_speed() {
    // 速度优先的用户快进吸附到目标之后的关键帧, 像 mpv/IINA 默认相对 seek
    // 一样优先快速出画。sample_gop.mp4 的下一关键帧在 1000ms。
    let mut p = open_fixture(playback_fixture("sample_gop.mp4"));
    drive_until_position(&mut p, 150, std::time::Duration::from_secs(3));

    p.handle(Command::SeekTo(900));
    wait_for_seek_release(&mut p, false);
    assert!(!p.seek_pending(), "闸门应放行");
    let pos = p.timeline().position_ms;
    assert!(
        (990..=1100).contains(&pos),
        "速度优先快进应吸附到下一关键帧 1000ms 附近, 实际 {pos}ms"
    );
    p.handle(Command::Stop);
}

#[test]
fn user_seek_mode_prioritizes_keyframe_speed() {
    assert_eq!(
        super::super::user_seek_mode(5_000, 10_000),
        SeekMode::KeyframeForward
    );
    assert_eq!(
        super::super::user_seek_mode(5_000, 900_000),
        SeekMode::KeyframeForward
    );
    assert_eq!(
        super::super::user_seek_mode(5_000, 4_000),
        SeekMode::KeyframeBackward
    );
    assert_eq!(
        super::super::user_seek_mode(5_000, 5_000),
        SeekMode::KeyframeForward
    );
}

#[test]
fn paused_forward_seek_snaps_to_next_keyframe_for_speed() {
    // 暂停态用户快进同样速度优先: 请求 900ms, sample_gop.mp4 的下一关键帧
    // 在 1000ms, 时间线应对齐到可立即显示的关键帧。
    let mut p = open_fixture(playback_fixture("sample_gop.mp4"));
    p.handle(Command::Pause);

    p.handle(Command::SeekTo(900));
    wait_for_seek_release(&mut p, true);
    assert!(!p.seek_pending(), "精确步长 seek 闸门应放行");
    let pos = p.timeline().position_ms;
    assert!(
        (990..=1100).contains(&pos),
        "速度优先暂停快进应吸附到下一关键帧 1000ms 附近, 实际 {pos}ms"
    );
    p.handle(Command::Stop);
}

#[test]
fn resume_seek_is_exact_and_playback_continues_from_target() {
    // 内部 seek(续播恢复)保持精确模式: 闸门等到"PTS≥目标"的帧才放行,
    // 位置不回落关键帧; 恢复播放后从目标处继续推进。
    let video = video_only_fixture()
        .canonicalize()
        .expect("先运行 media 的 gen_fixture.sh");
    let dir = std::env::temp_dir().join(format!(
        "morn_resume_exact_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let prefs_path = dir.join("prefs.json");
    let key = video.to_string_lossy().to_string();
    let mut prefs = persist::Preferences::default();
    prefs.last_playlist = vec![key.clone()];
    prefs.last_index = 0;
    prefs.set_resume_point(&key, 500);
    prefs.save(&prefs_path).unwrap();

    let mut player = Player::with_prefs(prefs_path);
    assert!(player.restore_last_session_paused());
    player.handle(Command::Play);

    // 驱动闸门放行: 精确模式应停在目标附近(不回落到关键帧 0)。
    wait_for_seek_release(&mut player, true);
    assert!(!player.seek_pending(), "精确闸门应放行");
    let pos = player.timeline().position_ms;
    assert!(
        (480..=650).contains(&pos),
        "精确 seek 放行后位置应仍在目标附近, 实际 {pos}ms"
    );

    let pos = drive_until_position(&mut player, 700, std::time::Duration::from_secs(3));
    assert!(pos >= 700, "续播后应从目标处继续推进, 实际 {pos}ms");
    player.handle(Command::Stop);
    let _cleanup = std::fs::remove_dir_all(dir);
}

#[test]
fn audio_eof_before_video_end_hands_over_to_wall_clock() {
    // 音频(0.3s)先于视频(1s)结束: 音频 EOF 后主时钟必须切到墙钟继续走,
    // 否则位置冻结在音频结束处 → 视频冻结且结束动作永不触发。
    let mut p = open_fixture(playback_fixture("sample_short_audio.mp4"));

    let pos = drive_until_position(&mut p, 800, std::time::Duration::from_secs(5));
    assert!(
        pos >= 800,
        "音频先结束后位置应由墙钟接管继续前进, 实际停在 {pos}ms"
    );
    p.handle(Command::Stop);
}
