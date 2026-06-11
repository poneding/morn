use media::{AudioDecoder, VideoDecoder};
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4")
}

#[test]
fn video_seek_still_decodes() {
    let path = fixture();
    if !path.exists() {
        return; // 无 fixture 时跳过(见 tests/gen_fixture.sh)
    }

    let mut dec = VideoDecoder::open(&path).unwrap();
    dec.seek_ms(500).unwrap();
    // seek 后仍应能解出帧, 说明 seek+flush 没有破坏解码状态。
    let frame = dec.next_frame().unwrap();
    assert!(frame.is_some(), "seek 后应仍能解码出视频帧");
}

#[test]
fn video_seek_emits_first_frame_at_or_after_target() {
    // 精确 seek: demuxer 只能落在关键帧(sample.mp4 仅 0ms 一个), 关键帧→目标
    // 之间的帧必须在解码器内部跳过(不缩放、不产出), 首个产出帧的 PTS ≥ 目标。
    // 否则这些帧涌进帧队列, 呈现端逐个丢弃, 长 GOP 时 seek 观感"卡住等很久"。
    let path = fixture();
    if !path.exists() {
        return; // 无 fixture 时跳过(见 tests/gen_fixture.sh)
    }

    let mut dec = VideoDecoder::open(&path).unwrap();
    dec.seek_ms(500).unwrap();
    let frame = dec
        .next_frame()
        .unwrap()
        .expect("seek(500) 后应能解出目标帧");
    assert!(
        (500..=600).contains(&frame.pts_ms),
        "seek(500) 后首帧 PTS 应在目标附近(25fps → 520ms), 实际 {}ms",
        frame.pts_ms
    );
    // 后续帧继续正常推进。
    let next = dec.next_frame().unwrap().expect("目标后应继续出帧");
    assert!(next.pts_ms > frame.pts_ms);
}

#[test]
fn video_keyframe_seek_emits_keyframe_immediately() {
    // 关键帧吸附 seek(UI 快进用): 落到 ≤目标 的关键帧后直接出帧, 不解码追赶,
    // 因此首帧 PTS ≤ 目标且几乎零延迟——配合引擎把时钟对齐到该帧实现"秒播"。
    let path = fixture();
    if !path.exists() {
        return; // 无 fixture 时跳过(见 tests/gen_fixture.sh)
    }

    let mut dec = VideoDecoder::open(&path).unwrap();
    dec.seek_ms_keyframe(500).unwrap();
    let frame = dec.next_frame().unwrap().expect("关键帧 seek 后应立即出帧");
    assert!(
        frame.pts_ms <= 500,
        "关键帧吸附首帧 PTS 应 ≤ 目标(sample.mp4 关键帧在 0), 实际 {}ms",
        frame.pts_ms
    );
}

#[test]
fn audio_seek_still_decodes() {
    let path = fixture();
    if !path.exists() {
        return; // 无 fixture 时跳过(见 tests/gen_fixture.sh)
    }

    let mut dec = AudioDecoder::open(&path).unwrap();
    dec.seek_ms(500).unwrap();
    // seek 后仍应能解出音频块。
    let chunk = dec.next_chunk().unwrap();
    assert!(chunk.is_some(), "seek 后应仍能解码出音频块");
}
