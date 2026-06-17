use media::{DecodeOptions, VideoDecoder};
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4")
}

#[test]
fn decodes_correctly_regardless_of_hw_availability() {
    let path = fixture();
    assert!(path.exists(), "先运行 tests/gen_fixture.sh");

    let mut dec = VideoDecoder::open_with_options(&path, DecodeOptions::default()).unwrap();
    assert_sample_frame_count(decode_frame_count(&mut dec, true));
    eprintln!(
        "解码路径: {}",
        if dec.is_hardware() {
            "硬件"
        } else {
            "软件"
        }
    );
}

#[test]
fn forcing_software_still_works() {
    let path = fixture();
    let opts = DecodeOptions {
        try_hardware: false,
    };
    let mut dec = VideoDecoder::open_with_options(&path, opts).unwrap();
    assert!(!dec.is_hardware());
    assert_sample_frame_count(decode_frame_count(&mut dec, false));
}

#[test]
fn observed_hardware_matches_forced_software() {
    // 强制软解时, 解码一帧后 observed_hardware() 必为 false。
    let path = fixture();
    let opts = DecodeOptions {
        try_hardware: false,
    };
    let mut dec = VideoDecoder::open_with_options(&path, opts).unwrap();
    assert!(dec.next_frame().unwrap().is_some());
    assert!(!dec.observed_hardware());
}

fn decode_frame_count(decoder: &mut VideoDecoder, check_rgba_len: bool) -> usize {
    let mut count = 0;
    while let Some(frame) = decoder.next_frame().unwrap() {
        if check_rgba_len {
            assert_eq!(frame.rgba.len(), 160 * 120 * 4);
        }
        count += 1;
    }
    count
}

fn assert_sample_frame_count(count: usize) {
    assert!((23..=27).contains(&count));
}
