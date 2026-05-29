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
