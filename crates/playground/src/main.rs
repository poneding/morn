//! Command-line playback playground for exercising media/audio/sync without egui.
//!
//! This binary is intentionally small and diagnostic-oriented: it opens one media
//! path, feeds decoded audio into the real output stream, and prints video frame
//! display/drop decisions against the audio master clock.  It is useful when
//! isolating decoder or clock behavior from the desktop app UI.

use audio::{AudioOutput, MasterClock};
use media::{AudioDecoder, VideoDecoder};
use ringbuf::traits::Producer;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let path = read_media_path();
    let out = start_audio_output();
    let clock = out.clock.clone();

    spawn_audio_thread(path.clone(), out);
    play_video_frames(&path, clock);
}

fn read_media_path() -> PathBuf {
    let path = env::args().nth(1).expect("用法: playground <视频文件>");
    PathBuf::from(path)
}

fn start_audio_output() -> AudioOutput {
    // playground 不调音量/不 seek: 满音量 + 永不置位的 flush/gate
    AudioOutput::start(
        Arc::new(AtomicU8::new(100)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("启动音频失败")
}

fn spawn_audio_thread(path: PathBuf, mut out: AudioOutput) {
    thread::spawn(move || {
        let mut adec = AudioDecoder::open_path(&path).expect("打开音频失败");
        while let Some(chunk) = adec.next_chunk().expect("音频解码错误") {
            push_chunk_samples(&mut out, &chunk.samples);
        }
    });
}

fn push_chunk_samples(out: &mut AudioOutput, samples: &[f32]) {
    let mut i = 0;
    while i < samples.len() {
        if out.producer.try_push(samples[i]).is_ok() {
            i += 1;
        } else {
            // 推满时短暂让出, 避免忙等
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn play_video_frames(path: &Path, clock: MasterClock) {
    let mut vdec = VideoDecoder::open_path(path).expect("打开视频失败");
    println!("视频: {}x{}", vdec.width(), vdec.height());
    let tol_ms: u64 = 15;
    let mut shown = 0u32;
    while let Some(frame) = vdec.next_frame().expect("视频解码错误") {
        shown += display_or_drop_frame(&clock, frame.pts_ms, tol_ms, shown + 1) as u32;
    }
    println!("播放结束, 共显示 {shown} 帧");
}

fn display_or_drop_frame(
    clock: &MasterClock,
    pts_ms: u64,
    tol_ms: u64,
    display_index: u32,
) -> bool {
    loop {
        let master = clock.position_ms();
        match sync::decide_frame(master, pts_ms, tol_ms) {
            sync::FrameDecision::Display => {
                println!("显示帧 {display_index} @ pts={pts_ms}ms 主时钟={master}ms");
                return true;
            }
            sync::FrameDecision::Drop => {
                println!("丢弃帧 @ pts={pts_ms}ms 主时钟={master}ms");
                return false;
            }
            sync::FrameDecision::Wait { remaining_ms } => {
                thread::sleep(Duration::from_millis(remaining_ms.min(50)));
            }
        }
    }
}
