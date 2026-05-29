use media::list_subtitle_tracks;
use std::path::Path;

#[test]
fn lists_tracks_without_error() {
    // 样本无字幕轨, 应返回空 Vec 而非报错。
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4");
    if !path.exists() {
        return;
    }
    let tracks = list_subtitle_tracks(&path).unwrap();
    assert!(tracks.is_empty() || tracks.iter().all(|t| !t.label.is_empty()));
}
