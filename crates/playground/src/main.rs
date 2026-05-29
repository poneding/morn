use audio::AudioOutput;
use media::{AudioDecoder, VideoDecoder};
use ringbuf::traits::Producer;
use std::env;
use std::thread;
use std::time::Duration;

fn main() {
    let path = env::args().nth(1).expect("用法: playground <视频文件>");
    let path = std::path::PathBuf::from(path);

    // 启动音频输出
    let mut out = AudioOutput::start().expect("启动音频失败");
    let clock = out.clock.clone();

    // 音频解码线程: 把样本推入 ringbuf
    let apath = path.clone();
    thread::spawn(move || {
        let mut adec = AudioDecoder::open(&apath).expect("打开音频失败");
        while let Some(chunk) = adec.next_chunk().expect("音频解码错误") {
            let mut i = 0;
            while i < chunk.samples.len() {
                // 推满时短暂让出, 避免忙等
                if out.producer.try_push(chunk.samples[i]).is_ok() {
                    i += 1;
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    });

    // 视频解码 + 按主时钟同步显示(打印代替真实渲染)
    let mut vdec = VideoDecoder::open(&path).expect("打开视频失败");
    println!("视频: {}x{}", vdec.width(), vdec.height());
    let tol_ms: u64 = 15;
    let mut shown = 0u32;
    while let Some(frame) = vdec.next_frame().expect("视频解码错误") {
        loop {
            let master = clock.position_ms();
            match sync::decide_frame(master, frame.pts_ms, tol_ms) {
                sync::FrameDecision::Display => {
                    shown += 1;
                    println!(
                        "显示帧 {shown} @ pts={}ms 主时钟={}ms",
                        frame.pts_ms, master
                    );
                    break;
                }
                sync::FrameDecision::Drop => {
                    println!("丢弃帧 @ pts={}ms 主时钟={}ms", frame.pts_ms, master);
                    break;
                }
                sync::FrameDecision::Wait { remaining_ms } => {
                    thread::sleep(Duration::from_millis(remaining_ms.min(50)));
                }
            }
        }
    }
    println!("播放结束, 共显示 {shown} 帧");
}
