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
    let mut count = 0;
    while let Some(f) = dec.next_frame().unwrap() {
        assert_eq!(f.rgba.len(), 160 * 120 * 4);
        count += 1;
    }
    assert!((23..=27).contains(&count));
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
    let mut count = 0;
    while dec.next_frame().unwrap().is_some() {
        count += 1;
    }
    assert!((23..=27).contains(&count));
}
