use crossbeam_channel::{bounded, Receiver, Sender};
use media::{MediaError, VideoDecoder, VideoFrame};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

// VideoDecoder 内含 ffmpeg 的 SwsContext(裸指针), 因此 !Send,
// 不能把已打开的解码器移入线程。改为: 调用线程先打开一次做可用性校验,
// 再把路径移入解码线程内重新打开。详见 spawn。

/// 从解码线程拉取帧的结果。
pub enum FramePull {
    Frame(VideoFrame),
    End,
}

pub struct DecodeThread {
    rx: Receiver<VideoFrame>,
    stop: Arc<AtomicBool>,
    hw_active: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl DecodeThread {
    /// 启动解码线程, `queue_cap` 为有界队列容量(背压上限)。
    pub fn spawn(path: &Path, queue_cap: usize) -> Result<Self, MediaError> {
        drop(VideoDecoder::open(path)?); // 在调用线程先验证可打开
        let owned_path = path.to_path_buf(); // VideoDecoder !Send, 移动路径而非解码器
        let (tx, rx): (Sender<VideoFrame>, Receiver<VideoFrame>) = bounded(queue_cap);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let hw_active = Arc::new(AtomicBool::new(false));
        let hw_active_t = hw_active.clone();

        let join = std::thread::spawn(move || {
            // 解码器在工作线程内打开, 满足 !Send 约束; 上面已校验过可打开。
            let mut decoder = match VideoDecoder::open(&owned_path) {
                Ok(d) => d,
                Err(_) => return,
            };
            while !stop_thread.load(Ordering::Relaxed) {
                match decoder.next_frame() {
                    Ok(Some(frame)) => {
                        hw_active_t.store(decoder.observed_hardware(), Ordering::Relaxed);
                        if tx.send(frame).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            rx,
            stop,
            hw_active,
            join: Some(join),
        })
    }

    /// 阻塞取下一帧; 队列空且线程结束返回 End。
    pub fn recv_frame(&self) -> FramePull {
        match self.rx.recv() {
            Ok(f) => FramePull::Frame(f),
            Err(_) => FramePull::End,
        }
    }

    /// 非阻塞取帧; 无帧返回 None。UI 线程用这个避免卡顿。
    pub fn try_recv_frame(&self) -> Option<VideoFrame> {
        self.rx.try_recv().ok()
    }

    /// 当前是否实际硬件解码(由解码线程按帧更新)。
    pub fn is_hardware(&self) -> bool {
        self.hw_active.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        while self.rx.try_recv().is_ok() {}
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for DecodeThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        while self.rx.try_recv().is_ok() {}
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/tests/fixtures/sample.mp4")
    }

    #[test]
    #[allow(clippy::while_let_loop)] // 测试显式用 loop+match 以覆盖 End 分支
    fn streams_frames_then_signals_end() {
        let path = fixture();
        assert!(path.exists(), "先运行 media 的 gen_fixture.sh");

        let handle = DecodeThread::spawn(&path, 8).unwrap();
        let mut count = 0;
        loop {
            match handle.recv_frame() {
                FramePull::Frame(f) => {
                    assert_eq!(f.width, 160);
                    count += 1;
                }
                FramePull::End => break,
            }
        }
        assert!((23..=27).contains(&count), "帧数 {count} 不符");
        handle.stop();
    }
}
