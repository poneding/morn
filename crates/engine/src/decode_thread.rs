use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use media::{MediaError, VideoDecoder, VideoFrame};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    dimensions: (u32, u32),
    // 帧带 serial(发出时的 applied_seek_seq): 取帧侧只放行 serial == 当前请求代次的帧,
    // seek 前已发出/发送中的旧帧被静默丢弃(ffplay 的 serial flush 思路)。
    rx: Receiver<(u64, VideoFrame)>,
    seek_tx: Sender<u64>,
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    hw_active: Arc<AtomicBool>,
    frame_pending: Arc<AtomicBool>,
    // 最近一帧已发出的 PTS(毫秒); seek 后复位为 0。供 seek 恢复期判断视频是否已追到目标。
    last_pts: Arc<AtomicU64>,
    // seek 代次: request_seek 递增 requested, 解码线程应用 seek 后把 applied 追平。
    // seek-gate 仅在 applied>=本次请求代次时才信任 last_pts, 避免旧帧 PTS 误触发(向后 seek 竞态)。
    requested_seek_seq: Arc<AtomicU64>,
    applied_seek_seq: Arc<AtomicU64>,
    // 累计已解码发出的帧数(诊断用: 算解码 fps)。
    decoded_total: Arc<AtomicU64>,
    join: Option<JoinHandle<()>>,
}

impl DecodeThread {
    /// 启动解码线程, `queue_cap` 为有界队列容量(背压上限)。
    pub fn spawn(path: &Path, queue_cap: usize) -> Result<Self, MediaError> {
        let decoder = VideoDecoder::open(path)?; // 在调用线程先验证可打开
        let dimensions = (decoder.width(), decoder.height());
        drop(decoder);
        let owned_path = path.to_path_buf(); // VideoDecoder !Send, 移动路径而非解码器
        let (tx, rx): (Sender<(u64, VideoFrame)>, Receiver<(u64, VideoFrame)>) = bounded(queue_cap);
        let (seek_tx, seek_rx) = crossbeam_channel::unbounded::<u64>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let ended = Arc::new(AtomicBool::new(false));
        let ended_thread = ended.clone();
        let hw_active = Arc::new(AtomicBool::new(false));
        let hw_active_t = hw_active.clone();
        let frame_pending = Arc::new(AtomicBool::new(false));
        let frame_pending_t = frame_pending.clone();
        let last_pts = Arc::new(AtomicU64::new(0));
        let last_pts_t = last_pts.clone();
        let requested_seek_seq = Arc::new(AtomicU64::new(0));
        let requested_seek_seq_t = requested_seek_seq.clone();
        let applied_seek_seq = Arc::new(AtomicU64::new(0));
        let applied_seek_seq_t = applied_seek_seq.clone();
        let decoded_total = Arc::new(AtomicU64::new(0));
        let decoded_total_t = decoded_total.clone();

        let join = std::thread::spawn(move || {
            // 解码器在工作线程内打开, 满足 !Send 约束; 上面已校验过可打开。
            let mut decoder = match VideoDecoder::open(&owned_path) {
                Ok(d) => d,
                Err(_) => return,
            };
            let mut eof = false;
            while !stop_thread.load(Ordering::Relaxed) {
                // 排空所有待处理 seek 请求, 只保留最后一个并应用一次。
                let mut pending = None;
                while let Ok(t) = seek_rx.try_recv() {
                    pending = Some(t);
                }
                if let Some(t) = pending {
                    let _ = decoder.seek_ms(t);
                    ended_thread.store(false, Ordering::Relaxed);
                    eof = false;
                    // 复位: 否则 seek 前的旧 PTS 会让 seek-gate 误判"已追到目标"(尤其向后 seek)。
                    last_pts_t.store(0, Ordering::Relaxed);
                    // 标记本批 seek 已应用(取当前请求代次), 之后 last_pts 才反映 seek 后的解码进度。
                    applied_seek_seq_t.store(
                        requested_seek_seq_t.load(Ordering::Relaxed),
                        Ordering::Relaxed,
                    );
                }
                if eof {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                match decoder.next_frame() {
                    Ok(Some(frame)) => {
                        ended_thread.store(false, Ordering::Relaxed);
                        hw_active_t.store(decoder.observed_hardware(), Ordering::Relaxed);
                        let pts = frame.pts_ms;
                        let serial = applied_seek_seq_t.load(Ordering::Relaxed);
                        if tx.send((serial, frame)).is_err() {
                            break;
                        }
                        last_pts_t.store(pts, Ordering::Relaxed);
                        frame_pending_t.store(true, Ordering::Relaxed);
                        decoded_total_t.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(None) | Err(_) => {
                        ended_thread.store(true, Ordering::Relaxed);
                        eof = true;
                    }
                }
            }
        });

        Ok(Self {
            dimensions,
            rx,
            seek_tx,
            stop,
            ended,
            hw_active,
            frame_pending,
            last_pts,
            requested_seek_seq,
            applied_seek_seq,
            decoded_total,
            join: Some(join),
        })
    }

    /// 阻塞取下一帧; 队列空且线程结束返回 End。seek 前的陈旧帧被静默丢弃。
    pub fn recv_frame(&self) -> FramePull {
        loop {
            match self.rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok((serial, f)) => {
                    if serial == self.requested_seek_seq.load(Ordering::Relaxed) {
                        return FramePull::Frame(f);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return FramePull::End,
                Err(RecvTimeoutError::Timeout) => {
                    if self.ended.load(Ordering::Relaxed) {
                        return FramePull::End;
                    }
                    if self.stop.load(Ordering::Relaxed) {
                        return FramePull::End;
                    }
                }
            }
        }
    }

    /// 非阻塞取帧; 无帧返回 None。UI 线程用这个避免卡顿。
    /// 只放行当前 seek 代次的帧: 旧 serial 帧(seek 前已发出/发送中)直接丢弃,
    /// 否则向后 seek 时旧帧的"未来 PTS"会被呈现端 Hold 住, 永久阻塞新帧。
    pub fn try_recv_frame(&self) -> Option<VideoFrame> {
        loop {
            let (serial, f) = self.rx.try_recv().ok()?;
            if serial == self.requested_seek_seq.load(Ordering::Relaxed) {
                return Some(f);
            }
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }

    /// 请求 seek 到目标毫秒, 由解码线程在下一轮循环开头应用。返回本次请求的代次号,
    /// 调用方据此配合 applied_seek_seq() 判断该 seek 是否已被解码线程实际应用。
    pub fn request_seek(&self, ms: u64) -> u64 {
        let seq = self.requested_seek_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.seek_tx.send(ms);
        seq
    }

    /// 当前是否实际硬件解码(由解码线程按帧更新)。
    pub fn is_hardware(&self) -> bool {
        self.hw_active.load(Ordering::Relaxed)
    }

    /// 解码线程每发出一帧置 true, UI 用 take_frame_pending() 读+清,
    /// 把"有新帧可显示"翻译成一次即时 repaint, 替代固定 16ms 轮询。
    pub fn take_frame_pending(&self) -> bool {
        self.frame_pending.swap(false, Ordering::Relaxed)
    }

    /// 最近一帧已发出的 PTS(毫秒)。seek 后复位为 0, 仅反映 seek 之后的解码进度。
    pub fn last_decoded_pts_ms(&self) -> u64 {
        self.last_pts.load(Ordering::Relaxed)
    }

    /// 解码是否已到流尾(或出错终止)。
    pub fn is_ended(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }

    /// 累计已解码发出的帧数(诊断用)。
    pub fn decoded_total(&self) -> u64 {
        self.decoded_total.load(Ordering::Relaxed)
    }

    /// 当前帧队列里待消费的帧数(诊断用: 看解码是否跟得上)。
    pub fn queue_len(&self) -> usize {
        self.rx.len()
    }

    /// 解码线程已应用的 seek 代次。>= request_seek 返回值时, 表示该 seek 已生效。
    pub fn applied_seek_seq(&self) -> u64 {
        self.applied_seek_seq.load(Ordering::Relaxed)
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

    #[test]
    fn sets_frame_pending_flag_when_a_frame_is_decoded() {
        let path = fixture();
        assert!(path.exists(), "先运行 media 的 gen_fixture.sh");
        let handle = DecodeThread::spawn(&path, 8).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut observed = false;
        while std::time::Instant::now() < deadline {
            if handle.take_frame_pending() {
                observed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            observed,
            "frame_pending flag was never set within 3s of decoding"
        );
        handle.stop();
    }

    #[test]
    fn exposes_video_dimensions_after_spawn() {
        let path = fixture();
        assert!(path.exists(), "先运行 media 的 gen_fixture.sh");

        let handle = DecodeThread::spawn(&path, 8).unwrap();

        assert_eq!(handle.dimensions(), (160, 120));
        handle.stop();
    }

    #[test]
    fn frames_emitted_before_seek_are_filtered_out_after_seek() {
        // 竞态根因复现: seek 请求发出后, 解码线程可能仍把 seek 前的旧帧发进队列
        // (发送阻塞在先)。旧帧若被消费, 向后 seek 时其"未来 PTS"会被 Hold 为
        // pending, 永久阻塞新帧 → 画面冻结。带 serial 过滤后, 取帧方在 seek 后
        // 取到的第一帧必然是 seek 之后解出的(applied_seek_seq 已追平)。
        let path = fixture();
        assert!(path.exists(), "先运行 media 的 gen_fixture.sh");

        let handle = DecodeThread::spawn(&path, 8).unwrap();
        // 等队列填满: 此时队列里全是 seek 前(serial=0)的旧帧, 解码线程阻塞在发送上。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while handle.queue_len() < 8 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(handle.queue_len(), 8, "队列未填满, 无法构造竞态前提");

        handle.request_seek(0);

        // 取帧: 返回的第一帧必须是 seek 之后解出的——即此刻 applied_seek_seq 已 ≥1。
        // 无 serial 过滤时, 这里会立刻拿到队列里的旧帧(applied 仍为 0)而失败。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some(_f) = handle.try_recv_frame() {
                assert!(
                    handle.applied_seek_seq() >= 1,
                    "seek 后取到的首帧来自 seek 之前(陈旧帧未被过滤)"
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "3s 内未取到 seek 后的新帧"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        handle.stop();
    }
}
