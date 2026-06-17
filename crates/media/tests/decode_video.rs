//! Video decoder integration tests.
//!
//! The fixture is small but still goes through demuxing, decoding, timestamp
//! conversion, and RGBA output.  The expected frame range allows minor FFmpeg build
//! differences while still catching missing frames, wrong dimensions, and
//! non-monotonic PTS output.

use media::VideoDecoder;
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4")
}

#[test]
fn decodes_all_video_frames_to_rgba() {
    let path = fixture();
    assert!(path.exists(), "先运行 tests/gen_fixture.sh 生成样本");

    let mut dec = VideoDecoder::open_path(&path).unwrap();
    assert_eq!(dec.width(), 160);
    assert_eq!(dec.height(), 120);

    let mut count = 0u32;
    let mut last_pts = 0u64;
    while let Some(frame) = dec.next_frame().unwrap() {
        assert_eq!(frame.width, 160);
        assert_eq!(frame.height, 120);
        assert_eq!(frame.rgba.len(), 160 * 120 * 4);
        assert!(frame.pts_ms >= last_pts, "PTS 应单调不减");
        last_pts = frame.pts_ms;
        count += 1;
    }
    assert!((23..=27).contains(&count), "解码帧数 {count} 不在预期范围");
}

#[test]
fn open_nonexistent_file_returns_err() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/does_not_exist.mp4");
    let missing_result = VideoDecoder::open_path(&missing);
    assert!(missing_result.is_err());
}
