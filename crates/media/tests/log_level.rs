use ffmpeg_next as ff;

#[test]
fn ffmpeg_logs_are_quiet_by_default() {
    // seek 后 HE-AAC SBR 状态重建期, FFmpeg 会按 Error 级别刷
    // "env_facs_q ... is invalid"——无害但刷屏。默认(未设 MORN_DEBUG)
    // 应把 av_log 压到 Fatal; 调试时(MORN_DEBUG=1)保留默认级别。
    std::env::remove_var("MORN_DEBUG");
    media::quiet_ffmpeg_logs_once();
    assert_eq!(ff::util::log::get_level(), Ok(ff::util::log::Level::Fatal));
}
