use media::AudioDecoder;
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4")
}

#[test]
fn decodes_audio_to_f32_chunks() {
    let path = fixture();
    assert!(path.exists(), "先运行 tests/gen_fixture.sh 生成样本");

    let mut dec = AudioDecoder::open(&path).unwrap();
    assert!(dec.channels() >= 1);
    assert!(dec.sample_rate() > 0);

    let mut total_frames = 0usize;
    while let Some(chunk) = dec.next_chunk().unwrap() {
        assert_eq!(chunk.channels, dec.channels());
        assert_eq!(chunk.samples.len() % chunk.channels as usize, 0);
        total_frames += chunk.frame_count();
    }
    let expected = dec.sample_rate() as usize;
    assert!(
        total_frames > expected * 9 / 10,
        "音频帧数 {total_frames} 偏少"
    );
}

#[test]
fn open_nonexistent_file_returns_err() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/does_not_exist.mp4");
    assert!(AudioDecoder::open(&missing).is_err());
}
