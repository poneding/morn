//! Subtitle stream discovery and lightweight text extraction.
//!
//! This module is intentionally small: the app only needs to list embedded
//! subtitle streams for a combo box and decode text-like streams on demand.
//! Bitmap formats such as PGS/VOBSUB are detected as subtitle streams but are not
//! converted into cues here, because rendering bitmap subtitle planes would require
//! a separate image composition path.
//!
//! Stream labels prefer the container language tag when present and fall back to
//! `und` so the UI always has a stable label.  Decoding keeps the original stream
//! index from FFmpeg; the player passes that index back through `SelectSubtitleTrack`
//! without trying to remap it against filtered lists.
//!
//! ASS subtitle packets expose the raw dialogue fields through FFmpeg.  The text
//! extraction below keeps the existing lightweight heuristic of taking the payload
//! after the last comma, which matches the current app display path without adding
//! a full ASS parser to the media crate.

use crate::error::MediaError;
use ff::media::Type;
use ffmpeg_next as ff;
use std::path::Path;
use subtitle::Cue;

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
            out.push(subtitle_track_for_stream(&stream));
        }
    }
    Ok(out)
}

/// 解码指定字幕流为文本 cues。位图字幕(PGS/VOBSUB)不支持, 返回的 cues 为空。
pub fn decode_text_subtitle(
    path: &Path,
    stream_index: usize,
) -> Result<subtitle::Subtitles, MediaError> {
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
    loop {
        let packet_result = packet.read(&mut ictx);
        if packet_result.is_err() {
            break;
        }
        if packet.stream() == stream_index {
            if let Some(cue) = decode_packet_cue(&mut decoder, &packet, time_base) {
                cues.push(cue);
            }
        }
    }
    cues.sort_by_key(|c| c.start_ms);
    Ok(subtitle::Subtitles::from_cues(cues))
}

fn subtitle_track_for_stream(stream: &ff::format::stream::Stream<'_>) -> SubtitleTrack {
    let metadata = stream.metadata();
    let lang = metadata.get("language").unwrap_or("und");
    SubtitleTrack {
        stream_index: stream.index(),
        label: format!("轨道 {} ({lang})", stream.index()),
    }
}

fn decode_packet_cue(
    decoder: &mut ff::codec::decoder::Subtitle,
    packet: &ff::codec::packet::Packet,
    time_base: f64,
) -> Option<Cue> {
    let mut sub = ff::Subtitle::default();
    if decoder.decode(packet, &mut sub).ok()? {
        cue_from_subtitle_packet(packet, &sub, time_base)
    } else {
        None
    }
}

fn cue_from_subtitle_packet(
    packet: &ff::codec::packet::Packet,
    sub: &ff::Subtitle,
    time_base: f64,
) -> Option<Cue> {
    let start_ms = packet_start_ms(packet, time_base);
    let dur_ms = packet_duration_ms(packet, time_base);
    let text = subtitle_text(sub);
    if text.is_empty() || dur_ms == 0 {
        return None;
    }
    Some(Cue {
        start_ms,
        end_ms: start_ms + dur_ms,
        text,
    })
}

fn packet_start_ms(packet: &ff::codec::packet::Packet, time_base: f64) -> u64 {
    let pts = packet.pts().unwrap_or_default();
    timestamp_ms(pts, time_base)
}

fn packet_duration_ms(packet: &ff::codec::packet::Packet, time_base: f64) -> u64 {
    timestamp_ms(packet.duration(), time_base)
}

fn timestamp_ms(value: i64, time_base: f64) -> u64 {
    (value as f64 * time_base * 1000.0).max(0.0) as u64
}

fn subtitle_text(sub: &ff::Subtitle) -> String {
    let mut text = String::new();
    for rect in sub.rects() {
        append_rect_text(&mut text, rect);
    }
    text.trim().to_string()
}

fn append_rect_text(text: &mut String, rect: ff::codec::subtitle::Rect<'_>) {
    match rect {
        ff::codec::subtitle::Rect::Text(rect) => text.push_str(rect.get()),
        ff::codec::subtitle::Rect::Ass(rect) => append_ass_text(text, rect.get()),
        ff::codec::subtitle::Rect::Bitmap(_) | ff::codec::subtitle::Rect::None(_) => {}
    }
}

fn append_ass_text(text: &mut String, line: &str) {
    // ASS dialogue 行: 取最后一个逗号后的文本(首版启发式)
    if let Some(idx) = line.rfind(',') {
        text.push_str(&line[idx + 1..]);
    }
}
