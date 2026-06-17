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

/// seek 模式: 精确(解码追赶到目标, 首帧 PTS≥目标)或关键帧吸附(落点即出帧, 秒播)。
/// 吸附必须带方向: 快进只向前吸附(目标后的首个关键帧), 快退向后——否则长 GOP
/// 文件快进会落回当前位置之前, 画面倒退。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekMode {
    Exact,
    KeyframeBackward,
    KeyframeForward,
}

/// 帧通道载荷: (发出时的 seek 代次 serial, 帧)。
type SerialFrame = (u64, VideoFrame);

pub struct DecodeThread {
    dimensions: (u32, u32),
    // 帧带 serial(发出时的 applied_seek_seq): 取帧侧只放行 serial == 当前请求代次的帧,
    // seek 前已发出/发送中的旧帧被静默丢弃(ffplay 的 serial flush 思路)。
    rx: Receiver<SerialFrame>,
    seek_tx: Sender<(u64, SeekMode)>,
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    hw_active: Arc<AtomicBool>,
    frame_pending: Arc<AtomicBool>,
    // seek 应用后首个已发出帧的 PTS(毫秒); 尚未出帧时为 u64::MAX。供 seek 闸门
    // 对齐到真实落点。必须保持首帧 PTS，不可被后续解码帧覆盖，否则 UI 消费慢或
    // 无音频设备时，闸门会错误对齐到已经解码到队列里的后续帧。
    landing_pts: Arc<AtomicU64>,
    // seek 代次: request_seek 递增 requested, 解码线程应用 seek 后把 applied 追平。
    // seek-gate 仅在 applied>=本次请求代次时才信任 landing_pts, 避免旧帧 PTS 误触发(向后 seek 竞态)。
    requested_seek_seq: Arc<AtomicU64>,
    applied_seek_seq: Arc<AtomicU64>,
    // 累计已解码发出的帧数(诊断用: 算解码 fps)。
    decoded_total: Arc<AtomicU64>,
    join: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct DecodeThreadShared {
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    hw_active: Arc<AtomicBool>,
    frame_pending: Arc<AtomicBool>,
    landing_pts: Arc<AtomicU64>,
    requested_seek_seq: Arc<AtomicU64>,
    applied_seek_seq: Arc<AtomicU64>,
    decoded_total: Arc<AtomicU64>,
}

impl DecodeThreadShared {
    fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            ended: Arc::new(AtomicBool::new(false)),
            hw_active: Arc::new(AtomicBool::new(false)),
            frame_pending: Arc::new(AtomicBool::new(false)),
            landing_pts: Arc::new(AtomicU64::new(u64::MAX)),
            requested_seek_seq: Arc::new(AtomicU64::new(0)),
            applied_seek_seq: Arc::new(AtomicU64::new(0)),
            decoded_total: Arc::new(AtomicU64::new(0)),
        }
    }
}

struct DecodeWorker {
    decoder: VideoDecoder,
    tx: Sender<SerialFrame>,
    seek_rx: Receiver<(u64, SeekMode)>,
    shared: DecodeThreadShared,
    eof: bool,
}

impl DecodeWorker {
    fn from_path(
        path: std::path::PathBuf,
        tx: Sender<SerialFrame>,
        seek_rx: Receiver<(u64, SeekMode)>,
        shared: DecodeThreadShared,
    ) -> Option<Self> {
        let decoder = VideoDecoder::open_path(&path).ok()?;
        Some(Self {
            decoder,
            tx,
            seek_rx,
            shared,
            eof: false,
        })
    }

    fn run(mut self) {
        while !self.shared.stop.load(Ordering::Relaxed) {
            self.apply_latest_seek();
            if self.eof {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            if !self.decode_and_publish_next_frame() {
                break;
            }
        }
    }

    fn apply_latest_seek(&mut self) {
        let Some((target_ms, mode)) = latest_seek_request(&self.seek_rx) else {
            return;
        };
        apply_seek_to_decoder(&mut self.decoder, target_ms, mode);
        self.shared.ended.store(false, Ordering::Relaxed);
        self.eof = false;
        self.shared.landing_pts.store(u64::MAX, Ordering::Relaxed);
        self.shared.applied_seek_seq.store(
            self.shared.requested_seek_seq.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    fn decode_and_publish_next_frame(&mut self) -> bool {
        match self.decoder.next_frame() {
            Ok(Some(frame)) => self.publish_frame(frame),
            Ok(None) | Err(_) => {
                self.shared.ended.store(true, Ordering::Relaxed);
                self.eof = true;
                true
            }
        }
    }

    fn publish_frame(&mut self, frame: VideoFrame) -> bool {
        self.shared.ended.store(false, Ordering::Relaxed);
        self.shared
            .hw_active
            .store(self.decoder.observed_hardware(), Ordering::Relaxed);
        let pts = frame.pts_ms;
        let serial = self.shared.applied_seek_seq.load(Ordering::Relaxed);
        let send_result = self.tx.send((serial, frame));
        if send_result.is_err() {
            return false;
        }
        // 只记录本次 seek 后首个发出的帧 PTS。解码线程可能在 UI 消费前继续前跑，
        // 后续帧不能覆盖落点，否则 seek 闸门会对齐到错误的后续位置。
        let _ = self.shared.landing_pts.compare_exchange(
            u64::MAX,
            pts,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
        self.shared.frame_pending.store(true, Ordering::Relaxed);
        self.shared.decoded_total.fetch_add(1, Ordering::Relaxed);
        true
    }
}

fn latest_seek_request(seek_rx: &Receiver<(u64, SeekMode)>) -> Option<(u64, SeekMode)> {
    let mut pending = None;
    while let Ok(request) = seek_rx.try_recv() {
        pending = Some(request);
    }
    pending
}

fn apply_seek_to_decoder(decoder: &mut VideoDecoder, target_ms: u64, mode: SeekMode) {
    let result = match mode {
        SeekMode::Exact => decoder.seek_ms(target_ms),
        SeekMode::KeyframeBackward => decoder.seek_ms_keyframe(target_ms),
        SeekMode::KeyframeForward => decoder.seek_ms_keyframe_forward(target_ms),
    };
    if let Err(err) = result {
        eprintln!("视频 seek 失败({target_ms}ms): {err}");
    }
}

impl DecodeThread {
    /// 启动解码线程, `queue_cap` 为有界队列容量(背压上限)。
    pub fn spawn(path: &Path, queue_cap: usize) -> Result<Self, MediaError> {
        let decoder = VideoDecoder::open_path(path)?; // 在调用线程先验证可打开
        let dimensions = (decoder.width(), decoder.height());
        drop(decoder);
        let owned_path = path.to_path_buf(); // VideoDecoder !Send, 移动路径而非解码器
        let (tx, rx): (Sender<SerialFrame>, Receiver<SerialFrame>) = bounded(queue_cap);
        let (seek_tx, seek_rx) = crossbeam_channel::unbounded::<(u64, SeekMode)>();
        let shared = DecodeThreadShared::new();
        let worker_shared = shared.clone();

        let join = std::thread::spawn(move || {
            // 解码器在工作线程内打开, 满足 !Send 约束; 上面已校验过可打开。
            if let Some(worker) = DecodeWorker::from_path(owned_path, tx, seek_rx, worker_shared) {
                worker.run();
            }
        });

        Ok(Self {
            dimensions,
            rx,
            seek_tx,
            stop: shared.stop,
            ended: shared.ended,
            hw_active: shared.hw_active,
            frame_pending: shared.frame_pending,
            landing_pts: shared.landing_pts,
            requested_seek_seq: shared.requested_seek_seq,
            applied_seek_seq: shared.applied_seek_seq,
            decoded_total: shared.decoded_total,
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

    /// 请求 seek 到目标毫秒(按指定模式), 由解码线程在下一轮循环开头应用。返回本次
    /// 请求的代次号, 调用方据此配合 applied_seek_seq() 判断该 seek 是否已实际生效。
    pub fn request_seek(&self, ms: u64, mode: SeekMode) -> u64 {
        let seq = self.requested_seek_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let send_result = self.seek_tx.send((ms, mode));
        if send_result.is_err() {
            self.ended.store(true, Ordering::Relaxed);
        }
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

    /// seek 应用后首个已发出帧的 PTS(毫秒); 尚未出帧时为 None。
    /// 精确 seek: 首帧即 ≥ 目标(闸门据此放行); 关键帧吸附: 首帧
    /// 即吸附落点(时钟/音频对齐到它)，即使解码线程继续预读也保持稳定。
    pub fn latest_pts_after_seek(&self) -> Option<u64> {
        match self.landing_pts.load(Ordering::Relaxed) {
            u64::MAX => None,
            pts => Some(pts),
        }
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
        self.finish_thread();
    }

    fn finish_thread(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        while self.rx.try_recv().is_ok() {}
        if let Some(j) = self.join.take() {
            if j.join().is_err() {
                eprintln!("解码线程异常退出");
            }
        }
    }
}

impl Drop for DecodeThread {
    fn drop(&mut self) {
        self.finish_thread();
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
    fn keyframe_seek_landing_pts_is_stable_until_next_seek() {
        // seek 闸门需要的是“本次 seek 后第一帧”的 PTS 来对齐时钟。
        // 若只暴露最新已解码 PTS，解码线程在 UI 消费前继续前跑会覆盖落点，
        // 无音频设备/墙钟环境下就会把 seek 放行位置对齐到 600ms/1500ms 等后续帧。
        let path = fixture();
        assert!(path.exists(), "先运行 media 的 gen_fixture.sh");

        let handle = DecodeThread::spawn(&path, 16).unwrap();
        handle.request_seek(100, SeekMode::KeyframeBackward);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while handle.latest_pts_after_seek().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        // 不消费队列，给解码线程机会继续发出后续帧；落点 PTS 仍应稳定为首帧 0ms。
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert_eq!(
            handle.latest_pts_after_seek(),
            Some(0),
            "关键帧 seek 的落点 PTS 不应被后续解码帧覆盖"
        );
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

        handle.request_seek(0, SeekMode::Exact);

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
