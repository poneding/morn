use crate::error::MediaError;
use ff::media::Type;
use ffmpeg_next as ff;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SubtitleTrack {
    pub stream_index: usize,
    pub label: String,
}

/// 枚举容器内的字幕流(任意编码)。
pub fn list_subtitle_tracks(path: &Path) -> Result<Vec<SubtitleTrack>, MediaError> {
    ff::init()?;
    let ictx = ff::format::input(&path)?;
    let mut out = Vec::new();
    for stream in ictx.streams() {
        if stream.parameters().medium() == Type::Subtitle {
            let lang = stream
                .metadata()
                .get("language")
                .unwrap_or("und")
                .to_string();
            out.push(SubtitleTrack {
                stream_index: stream.index(),
                label: format!("轨道 {} ({})", stream.index(), lang),
            });
        }
    }
    Ok(out)
}

/// 解码指定字幕流为文本 cues。位图字幕(PGS/VOBSUB)不支持, 返回的 cues 为空。
pub fn decode_text_subtitle(
    path: &Path,
    stream_index: usize,
) -> Result<subtitle::Subtitles, MediaError> {
    use subtitle::Cue;
    ff::init()?;
    let mut ictx = ff::format::input(&path)?;
    let stream = ictx
        .stream(stream_index)
        .ok_or(MediaError::NoStream("subtitle"))?;
    let time_base = f64::from(stream.time_base());
    let params = stream.parameters();
    let mut decoder = ff::codec::context::Context::from_parameters(params)?
        .decoder()
        .subtitle()?;

    let mut cues = Vec::new();
    let mut packet = ff::codec::packet::Packet::empty();
    while packet.read(&mut ictx).is_ok() {
        if packet.stream() != stream_index {
            continue;
        }
        let mut sub = ff::Subtitle::default();
        if decoder.decode(&packet, &mut sub).unwrap_or(false) {
            let pts = packet.pts().unwrap_or(0);
            let start_ms = (pts as f64 * time_base * 1000.0).max(0.0) as u64;
            let dur_ms = (packet.duration() as f64 * time_base * 1000.0).max(0.0) as u64;
            let mut text = String::new();
            for rect in sub.rects() {
                match rect {
                    ff::codec::subtitle::Rect::Text(t) => text.push_str(t.get()),
                    ff::codec::subtitle::Rect::Ass(a) => {
                        // ASS dialogue 行: 取最后一个逗号后的文本(首版启发式)
                        let line = a.get();
                        if let Some(idx) = line.rfind(',') {
                            text.push_str(&line[idx + 1..]);
                        }
                    }
                    ff::codec::subtitle::Rect::Bitmap(_) => {} // 位图字幕不支持
                    ff::codec::subtitle::Rect::None(_) => {}   // 空 rect
                }
            }
            let text = text.trim().to_string();
            if !text.is_empty() && dur_ms > 0 {
                cues.push(Cue {
                    start_ms,
                    end_ms: start_ms + dur_ms,
                    text,
                });
            }
        }
    }
    cues.sort_by_key(|c| c.start_ms);
    Ok(subtitle::Subtitles::from_cues(cues))
}
