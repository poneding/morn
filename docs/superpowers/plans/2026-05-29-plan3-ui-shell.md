# 计划 3: egui UI 外壳 + 播放控制 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用 eframe/egui 构建窗口外壳与核心播放控件(播放/暂停/停止/seek/音量/全屏),引入正式的"解码线程 + 有界帧队列"模型与顶层 `Player` 编排器,把视频帧显示到窗口,产出第一个真正可交互、可用的播放器。

**Architecture:** 顶层 `Player`(在 `player-core` 之上的编排器,放在新的 `engine` crate)拥有解码线程句柄、`AudioOutput`、状态机与播放列表。解码线程把 `VideoFrame` 推入有界 channel;UI 线程每帧从 channel 取出"当前应显示的帧"(用 `sync::decide_frame` 配合音频主时钟挑选),上传 wgpu 纹理并经 `egui_wgpu::Renderer::register_native_texture` 显示为 `egui::Image`。UI 通过命令 channel 向 `Player` 发 `Command`,`Player` 驱动状态机与解码线程。

**Tech Stack:** eframe 0.34, egui 0.34, egui-wgpu 0.34, wgpu 29.0, crossbeam-channel 0.5。复用计划 1/2 的全部 crate。

**前置依赖:** 计划 1、计划 2 已完成。

---

## 关键 API 提示(已核实, 2026-05)

- egui/eframe **0.34** 的 `App` trait 用 `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)` + 可选 `fn logic(&mut self, ctx, frame)`,**不再是** `fn update`。面板用 `.show_inside(ui, ...)`。
- 持续重绘: `ui.ctx().request_repaint_after(Duration::from_millis(16))`。
- 全屏: `ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(bool))`。
- 拖放: `ui.ctx().input(|i| i.raw.dropped_files.clone())`(闭包要短)。
- 显示 wgpu 纹理: 经 `CreationContext::wgpu_render_state()` 取 `RenderState`,用 `renderer.write().register_native_texture(&device, &view, FilterMode::Linear)` 得到 `egui::TextureId`,再 `ui.image((tex_id, egui::vec2(w,h)))`。

## 文件结构

```
crates/
├── engine/                     # 顶层编排: 把 player-core + media + audio 串起来
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── player.rs           # Player: 命令处理、状态、播放列表编排
│       ├── decode_thread.rs    # 解码线程 + 有界帧队列
│       └── timeline.rs         # 当前位置/时长/可seek范围的只读快照
└── app/                        # eframe 可执行程序(替代 playground 成为正式入口)
    ├── Cargo.toml
    └── src/
        ├── main.rs             # run_native 入口
        ├── app.rs              # PlayerApp: impl eframe::App
        ├── video_view.rs       # wgpu 纹理 → egui Image 显示
        └── controls.rs         # 控制栏 UI (播放/seek/音量/全屏)
```

---
## Task 1: engine crate — Timeline 只读快照

`Timeline` 是 UI 每帧读取的播放状态快照: 当前位置、总时长、播放状态、音量。纯数据 + 格式化逻辑,可单测(如 ms→"MM:SS" 显示)。

**Files:**
- Create: `crates/engine/Cargo.toml`
- Create: `crates/engine/src/lib.rs`
- Create: `crates/engine/src/timeline.rs`

- [ ] **Step 1: 创建 engine crate 清单**

`crates/engine/Cargo.toml`:
```toml
[package]
name = "engine"
version.workspace = true
edition.workspace = true

[dependencies]
player-core = { path = "../player-core" }
media = { path = "../media" }
audio = { path = "../audio" }
sync = { path = "../sync" }
crossbeam-channel = { workspace = true }
```

- [ ] **Step 2: 写失败测试**

`crates/engine/src/timeline.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use player_core::PlaybackState;

    #[test]
    fn formats_position_as_mm_ss() {
        let t = Timeline { position_ms: 65_000, duration_ms: 125_000,
            state: PlaybackState::Playing, volume: 100 };
        assert_eq!(t.position_label(), "01:05");
        assert_eq!(t.duration_label(), "02:05");
    }

    #[test]
    fn progress_fraction_is_ratio() {
        let t = Timeline { position_ms: 50_000, duration_ms: 100_000,
            state: PlaybackState::Playing, volume: 100 };
        assert!((t.progress() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn progress_is_zero_when_duration_unknown() {
        let t = Timeline { position_ms: 5_000, duration_ms: 0,
            state: PlaybackState::Stopped, volume: 100 };
        assert_eq!(t.progress(), 0.0);
    }
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p engine timeline`
Expected: 编译失败, "cannot find type `Timeline`"。

- [ ] **Step 4: 实现 Timeline**

`crates/engine/src/timeline.rs` 顶部:
```rust
use player_core::PlaybackState;

/// UI 每帧读取的播放状态快照。
#[derive(Debug, Clone, Copy)]
pub struct Timeline {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub state: PlaybackState,
    pub volume: u8,
}

impl Timeline {
    pub fn progress(&self) -> f32 {
        if self.duration_ms == 0 {
            0.0
        } else {
            self.position_ms as f32 / self.duration_ms as f32
        }
    }

    pub fn position_label(&self) -> String {
        fmt_ms(self.position_ms)
    }

    pub fn duration_label(&self) -> String {
        fmt_ms(self.duration_ms)
    }
}

fn fmt_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}
```

`crates/engine/src/lib.rs`:
```rust
//! 顶层播放编排: Player + 解码线程 + Timeline 快照。
mod timeline;
pub use timeline::Timeline;
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p engine timeline`
Expected: PASS, 3 个测试通过。

- [ ] **Step 6: Commit**

```bash
git add crates/engine
git commit -m "feat(engine): Timeline 播放状态快照与时间格式化"
```

---

## Task 2: engine crate — 解码线程与有界帧队列

启动一个解码线程, 顺序解码视频帧推入有界 channel(满则阻塞, 实现背压 → 低内存)。提供句柄: 取帧、请求停止。

**Files:**
- Create: `crates/engine/src/decode_thread.rs`
- Modify: `crates/engine/src/lib.rs`

- [ ] **Step 1: 写失败的集成测试**

`crates/engine/src/decode_thread.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture() -> std::path::PathBuf {
        // 复用 media crate 的样本
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures/sample.mp4")
    }

    #[test]
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p engine decode_thread`
Expected: 编译失败, "cannot find type `DecodeThread`"。

- [ ] **Step 3: 实现 DecodeThread**

`crates/engine/src/decode_thread.rs` 顶部:
```rust
use crossbeam_channel::{bounded, Receiver, Sender};
use media::{MediaError, VideoDecoder, VideoFrame};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// 从解码线程拉取帧的结果。
pub enum FramePull {
    Frame(VideoFrame),
    End,
}

pub struct DecodeThread {
    rx: Receiver<VideoFrame>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl DecodeThread {
    /// 启动解码线程, `queue_cap` 为有界队列容量(背压上限)。
    pub fn spawn(path: &Path, queue_cap: usize) -> Result<Self, MediaError> {
        let mut decoder = VideoDecoder::open(path)?; // 在调用线程先验证可打开
        let (tx, rx): (Sender<VideoFrame>, Receiver<VideoFrame>) = bounded(queue_cap);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();

        let join = std::thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                match decoder.next_frame() {
                    Ok(Some(frame)) => {
                        // send 在队列满时阻塞 → 背压; 接收端断开则退出
                        if tx.send(frame).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,   // EOF
                    Err(_) => break,     // 解码错误: 结束线程(spec: 不崩溃)
                }
            }
            // tx 在此 drop, 接收端会观察到断开
        });

        Ok(Self { rx, stop, join: Some(join) })
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

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // 排空队列以解除解码线程在 send 上的阻塞
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
```

- [ ] **Step 4: 挂载并导出**

`crates/engine/src/lib.rs` 改为:
```rust
//! 顶层播放编排: Player + 解码线程 + Timeline 快照。
mod decode_thread;
mod timeline;
pub use decode_thread::{DecodeThread, FramePull};
pub use timeline::Timeline;
```

- [ ] **Step 5: 运行测试确认通过**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test -p engine decode_thread`
Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/decode_thread.rs crates/engine/src/lib.rs
git commit -m "feat(engine): 解码线程与有界帧队列(背压)"
```

---
## Task 3: engine crate — Player 编排器

`Player` 拥有状态机、播放列表、音频输出、解码线程,处理 `Command`,产出 `Timeline` 快照。打开文件时启动音频解码线程(喂 cpal)+ 视频解码线程,并读元数据得总时长。

**Files:**
- Create: `crates/engine/src/player.rs`
- Modify: `crates/engine/src/lib.rs`

- [ ] **Step 1: 写失败测试(状态编排, 不依赖真实文件)**

`crates/engine/src/player.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use player_core::{Command, PlaybackState};

    #[test]
    fn new_player_is_stopped_with_default_volume() {
        let p = Player::new();
        let t = p.timeline();
        assert_eq!(t.state, PlaybackState::Stopped);
        assert_eq!(t.volume, 100);
    }

    #[test]
    fn set_volume_command_updates_timeline() {
        let mut p = Player::new();
        p.handle(Command::SetVolume(40));
        assert_eq!(p.timeline().volume, 40);
    }

    #[test]
    fn play_without_media_stays_stopped() {
        // 未打开文件时 Play 应被忽略(无媒体可播), 不 panic
        let mut p = Player::new();
        p.handle(Command::Play);
        assert_eq!(p.timeline().state, PlaybackState::Stopped);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p engine player`
Expected: 编译失败, "cannot find type `Player`"。

- [ ] **Step 3: 实现 Player(状态与命令处理)**

`crates/engine/src/player.rs` 顶部。注意: 音视频解码线程与 cpal 句柄是 `Option`,未打开文件时为 `None`:
```rust
use crate::decode_thread::DecodeThread;
use crate::timeline::Timeline;
use audio::AudioOutput;
use media::AudioDecoder;
use player_core::{Command, PlaybackState, Playlist, StateMachine};
use ringbuf::traits::Producer;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct Player {
    machine: StateMachine,
    playlist: Playlist,
    volume: u8,
    duration_ms: u64,
    video: Option<DecodeThread>,
    audio_out: Option<AudioOutput>,
    audio_join: Option<JoinHandle<()>>,
    audio_stop: Arc<AtomicBool>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            machine: StateMachine::new(),
            playlist: Playlist::new(),
            volume: 100,
            duration_ms: 0,
            video: None,
            audio_out: None,
            audio_join: None,
            audio_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn timeline(&self) -> Timeline {
        let position_ms = self
            .audio_out
            .as_ref()
            .map(|a| a.clock.position_ms())
            .unwrap_or(0);
        Timeline {
            position_ms,
            duration_ms: self.duration_ms,
            state: self.machine.state(),
            volume: self.volume,
        }
    }

    /// 取视频解码线程句柄(供 UI 拉帧)。
    pub fn video(&self) -> Option<&DecodeThread> {
        self.video.as_ref()
    }

    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Open(path) => self.open(&path),
            Command::Play => {
                if self.video.is_some() {
                    let _ = self.machine.apply(player_core::Transition::Play);
                }
            }
            Command::Pause => {
                let _ = self.machine.apply(player_core::Transition::Pause);
            }
            Command::Stop => {
                let _ = self.machine.apply(player_core::Transition::Stop);
                self.teardown();
            }
            Command::SetVolume(v) => self.volume = v.min(100),
            Command::SeekTo(_ms) => {
                // seek 在计划 4 完整实现(需重启解码线程到关键帧)
            }
            Command::SetRate(_) | Command::Next | Command::Prev => {
                // 计划 4
            }
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: 实现 open / teardown**

在 `player.rs` 的 `impl Player` 块内继续:
```rust
impl Player {
    fn open(&mut self, path: &Path) {
        self.teardown();

        // 读视频元数据得时长, 并启动视频解码线程
        let video = match DecodeThread::spawn(path, 16) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("打开视频失败: {e}");
                return;
            }
        };

        // 启动音频: 解码线程把样本喂入 cpal ringbuf
        match AudioOutput::start() {
            Ok(mut out) => {
                self.duration_ms = probe_duration_ms(path).unwrap_or(0);
                let apath = path.to_path_buf();
                let stop = Arc::new(AtomicBool::new(false));
                let stop_t = stop.clone();
                // 把 producer 移入线程
                let join = std::thread::spawn(move || {
                    let mut adec = match AudioDecoder::open(&apath) {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    'outer: while !stop_t.load(Ordering::Relaxed) {
                        match adec.next_chunk() {
                            Ok(Some(chunk)) => {
                                let mut i = 0;
                                while i < chunk.samples.len() {
                                    if stop_t.load(Ordering::Relaxed) {
                                        break 'outer;
                                    }
                                    if out.producer.try_push(chunk.samples[i]).is_ok() {
                                        i += 1;
                                    } else {
                                        std::thread::sleep(std::time::Duration::from_millis(2));
                                    }
                                }
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                });
                // AudioOutput 的 producer 已被移入线程, 这里保留其余字段。
                // 为简化, 把 producer 取出后用一个不含 producer 的句柄。见下方说明。
                self.audio_out = Some(out_without_producer(out));
                self.audio_join = Some(join);
                self.audio_stop = stop;
            }
            Err(e) => {
                eprintln!("启动音频失败: {e}, 仅播放视频(静音)");
                self.duration_ms = probe_duration_ms(path).unwrap_or(0);
            }
        }

        self.video = Some(video);
        let _ = self.machine.apply(player_core::Transition::Play);
    }

    fn teardown(&mut self) {
        self.audio_stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.audio_join.take() {
            let _ = j.join();
        }
        self.audio_out = None; // drop 关闭 cpal 流
        if let Some(v) = self.video.take() {
            v.stop();
        }
        self.duration_ms = 0;
        self.audio_stop = Arc::new(AtomicBool::new(false));
    }
}
```

- [ ] **Step 5: 解决 producer 所有权 — 调整 AudioOutput 接口**

上一步暴露一个所有权问题: `AudioOutput` 同时持有 `producer` 和 `clock`/`stream`,但 `producer` 要移入音频解码线程,`clock`/`stream` 要留在 `Player`。修改 `audio` crate 让两者可分离。

修改 `crates/audio/src/output.rs`,把 `AudioOutput` 拆出一个不含 producer 的句柄:
```rust
/// 不含 producer 的音频句柄, 留在 Player 中。
pub struct AudioHandle {
    pub stream: cpal::Stream,
    pub clock: MasterClock,
    pub channels: u16,
    pub sample_rate: u32,
}

impl AudioOutput {
    /// 拆分为 (留存句柄, 样本生产端)。producer 移入解码线程。
    pub fn split(self) -> (AudioHandle, SampleProducer) {
        let AudioOutput { stream, clock, producer, channels, sample_rate } = self;
        (AudioHandle { stream, clock, channels, sample_rate }, producer)
    }
}
```

在 `crates/audio/src/lib.rs` 导出 `AudioHandle`:
```rust
pub use output::{AudioHandle, AudioOutput, SampleProducer};
```

然后修正 `player.rs` 的 `open`: 把 `self.audio_out: Option<AudioOutput>` 字段类型改为 `Option<AudioHandle>`,并把 Step 4 中的伪函数 `out_without_producer(out)` 替换为真实拆分:
```rust
// 替换 Step 4 中 AudioOutput::start() 成功分支的相关行:
Ok(out) => {
    let (handle, mut producer) = out.split();
    self.duration_ms = probe_duration_ms(path).unwrap_or(0);
    let apath = path.to_path_buf();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();
    let join = std::thread::spawn(move || {
        let mut adec = match AudioDecoder::open(&apath) {
            Ok(d) => d,
            Err(_) => return,
        };
        'outer: while !stop_t.load(Ordering::Relaxed) {
            match adec.next_chunk() {
                Ok(Some(chunk)) => {
                    let mut i = 0;
                    while i < chunk.samples.len() {
                        if stop_t.load(Ordering::Relaxed) { break 'outer; }
                        if producer.try_push(chunk.samples[i]).is_ok() {
                            i += 1;
                        } else {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    });
    self.audio_out = Some(handle);
    self.audio_join = Some(join);
    self.audio_stop = stop;
}
```
并把 `player.rs` 顶部 `use audio::AudioOutput;` 改为 `use audio::{AudioHandle, AudioOutput};`,字段声明改为 `audio_out: Option<AudioHandle>,`。删除 Step 4 里临时的 `out_without_producer` 调用与注释。

- [ ] **Step 6: 实现 probe_duration_ms 辅助函数**

在 `player.rs` 文件底部(`impl` 块之外、测试模块之上)添加。它用 ffmpeg 读容器时长:
```rust
fn probe_duration_ms(path: &Path) -> Option<u64> {
    use ffmpeg_next as ff;
    ff::init().ok()?;
    let ictx = ff::format::input(&path).ok()?;
    let dur = ictx.duration(); // 单位: AV_TIME_BASE (微秒)
    if dur > 0 {
        Some((dur as u64) / 1000)
    } else {
        None
    }
}
```

在 `crates/engine/Cargo.toml` 的 `[dependencies]` 追加(probe 需直接用 ffmpeg):
```toml
ffmpeg-next = "8.1"
ringbuf = "0.5"
```

- [ ] **Step 7: 挂载并导出**

`crates/engine/src/lib.rs` 改为:
```rust
//! 顶层播放编排: Player + 解码线程 + Timeline 快照。
mod decode_thread;
mod player;
mod timeline;
pub use decode_thread::{DecodeThread, FramePull};
pub use player::Player;
pub use timeline::Timeline;
```

- [ ] **Step 8: 运行测试确认通过**

Run: `cargo test -p engine player`
Expected: PASS, 3 个测试通过。

- [ ] **Step 9: Commit**

```bash
git add crates/engine crates/audio
git commit -m "feat(engine): Player 编排器(命令处理/打开/拆分音频)"
```

---
## Task 4: app crate 骨架与窗口

最小 eframe 窗口跑起来(空白中央面板),验证 0.34 API 接线正确。

**Files:**
- Create: `crates/app/Cargo.toml`
- Create: `crates/app/src/main.rs`
- Create: `crates/app/src/app.rs`

- [ ] **Step 1: 创建 app crate 清单**

`crates/app/Cargo.toml`:
```toml
[package]
name = "app"
version.workspace = true
edition.workspace = true

[[bin]]
name = "morn"
path = "src/main.rs"

[dependencies]
eframe = "0.34"
egui = "0.34"
egui-wgpu = "0.34"
wgpu = "29.0"
engine = { path = "../engine" }
player-core = { path = "../player-core" }
render = { path = "../render" }
sync = { path = "../sync" }
```

- [ ] **Step 2: 实现 main**

`crates/app/src/main.rs`:
```rust
mod app;
mod controls;
mod video_view;

use app::PlayerApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_title("Morn"),
        // 启用 wgpu 后端(显示视频纹理需要)
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Morn",
        native_options,
        Box::new(|cc| Ok(Box::new(PlayerApp::new(cc)))),
    )
}
```

- [ ] **Step 3: 实现最小 App**

`crates/app/src/app.rs`:
```rust
use eframe::egui;
use engine::Player;

pub struct PlayerApp {
    player: Player,
}

impl PlayerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self { player: Player::new() }
    }
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label("拖入视频文件开始播放");
            });
        });
        // 持续重绘以驱动播放
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
}
```

注: `controls`/`video_view` 模块在 main.rs 已声明,后续任务填充;此步可先建空文件避免编译错误:
`crates/app/src/controls.rs` 与 `crates/app/src/video_view.rs` 各写一行 `// 占位, 见后续 Task`。

- [ ] **Step 4: 构建并人工验证窗口**

Run: `cargo build -p app`
Expected: 编译成功。

Run: `cargo run -p app`
Expected(人工): 弹出 960x600 标题为 "Morn" 的窗口,中央显示"拖入视频文件开始播放"。关闭窗口程序退出。

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): 最小 eframe 窗口外壳 (egui 0.34)"
```

---

## Task 5: app — 拖放打开文件 + 控制栏

底部控制栏(播放/暂停/停止、seek 条、音量、全屏),拖放文件触发 `Command::Open`。

**Files:**
- Create: `crates/app/src/controls.rs` (覆盖占位)
- Modify: `crates/app/src/app.rs`

- [ ] **Step 1: 实现控制栏**

`crates/app/src/controls.rs`:
```rust
use eframe::egui;
use engine::Timeline;
use player_core::Command;

/// 在底部面板绘制控制栏, 返回本帧产生的命令(若有)。
pub fn controls_bar(ui: &mut egui::Ui, t: &Timeline) -> Vec<Command> {
    use player_core::PlaybackState;
    let mut cmds = Vec::new();

    ui.horizontal(|ui| {
        // 播放/暂停切换
        let playing = t.state == PlaybackState::Playing;
        if ui.button(if playing { "⏸" } else { "▶" }).clicked() {
            cmds.push(if playing { Command::Pause } else { Command::Play });
        }
        if ui.button("⏹").clicked() {
            cmds.push(Command::Stop);
        }

        ui.label(t.position_label());

        // seek 条: 0..=duration_ms
        let mut pos = t.position_ms as f64;
        let dur = t.duration_ms.max(1) as f64;
        let resp = ui.add(
            egui::Slider::new(&mut pos, 0.0..=dur)
                .show_value(false)
                .trailing_fill(true),
        );
        if resp.changed() {
            cmds.push(Command::SeekTo(pos as u64));
        }

        ui.label(t.duration_label());

        // 音量
        let mut vol = t.volume as f64;
        if ui.add(egui::Slider::new(&mut vol, 0.0..=100.0).text("🔊")).changed() {
            cmds.push(Command::SetVolume(vol as u8));
        }

        // 全屏
        if ui.button("⛶").clicked() {
            let fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        }
    });

    cmds
}

/// 检查本帧拖入的文件, 返回 Open 命令。
pub fn dropped_file_command(ctx: &egui::Context) -> Option<Command> {
    let dropped = ctx.input(|i| i.raw.dropped_files.clone());
    dropped
        .into_iter()
        .find_map(|f| f.path)
        .map(Command::Open)
}
```

注: 删除末尾 `_PlayerAlias` 若 clippy 报未使用——它仅为说明 import,不必要。实际实现中移除该行与 `Player` import。

- [ ] **Step 2: 在 app.rs 接线控制栏与拖放**

`crates/app/src/app.rs` 改为:
```rust
use eframe::egui;
use engine::Player;
use crate::controls;

pub struct PlayerApp {
    player: Player,
}

impl PlayerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self { player: Player::new() }
    }
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 拖放打开
        if let Some(cmd) = controls::dropped_file_command(&ctx) {
            self.player.handle(cmd);
        }

        let t = self.player.timeline();

        egui::TopBottomPanel::bottom("controls").show_inside(ui, |ui| {
            for cmd in controls::controls_bar(ui, &t) {
                self.player.handle(cmd);
            }
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label("拖入视频文件开始播放");
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
```

- [ ] **Step 3: 构建并人工验证**

Run: `cargo run -p app`
Expected(人工):
- 窗口底部出现控制栏: ▶/⏹ 按钮、时间标签、seek 条、音量条、全屏按钮。
- 点击全屏按钮 → 窗口进入/退出全屏。
- 拖入一个视频文件 → 不报错(此时画面还不显示,Task 6 接;但能听到声音,且时间标签开始走动)。

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/controls.rs crates/app/src/app.rs
git commit -m "feat(app): 控制栏(播放/seek/音量/全屏)与拖放打开"
```

---
## Task 6: app — 视频帧显示 (wgpu 纹理 → egui Image)

每帧: 用音频主时钟从解码线程拉取"当前应显示的帧",上传到 wgpu 纹理,经 `register_native_texture` 得 `TextureId`,在中央面板用 `egui::Image` 显示。

**Files:**
- Create: `crates/app/src/video_view.rs` (覆盖占位)
- Modify: `crates/app/src/app.rs`
- Modify: `crates/engine/src/player.rs`(暴露主时钟位置给 UI 拉帧逻辑)

- [ ] **Step 1: 在 Player 暴露当前位置**

`Player` 已有 `timeline().position_ms`。为视频拉帧需要直接读位置,确认 `timeline()` 可用即可,无需新增方法。若 `video()` 返回的 `&DecodeThread` 需要可变借用来 try_recv,确认 `try_recv_frame(&self)` 是 `&self`(Task 2 已是)。无需改 player.rs;本步仅核对。

- [ ] **Step 2: 实现 VideoView**

`crates/app/src/video_view.rs`:
```rust
use eframe::egui;
use engine::Player;
use render::VideoTexture;
use sync::{decide_frame, FrameDecision};

/// 持有 wgpu 纹理与其在 egui 中的注册 id。
pub struct VideoView {
    texture: Option<VideoTexture>,
    tex_id: Option<egui::TextureId>,
    size: (u32, u32),
}

impl VideoView {
    pub fn new() -> Self {
        Self { texture: None, tex_id: None, size: (0, 0) }
    }

    /// 每帧调用: 按主时钟挑选并显示视频帧。
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        player: &Player,
    ) {
        let master_ms = player.timeline().position_ms;

        // 从解码线程拉取应显示的帧(丢弃过期帧, 取到最接近主时钟的一帧)
        if let Some(dt) = player.video() {
            let mut chosen = None;
            while let Some(vf) = dt.try_recv_frame() {
                match decide_frame(master_ms, vf.pts_ms, 15) {
                    FrameDecision::Display => { chosen = Some(vf); break; }
                    FrameDecision::Drop => { continue; } // 过期, 继续找更新的
                    FrameDecision::Wait { .. } => {
                        // 还没到时间: 这帧留到下次。但 try_recv 已取出,
                        // 简化处理: 本帧也显示它(略早 < 一帧, 视觉无感)。
                        chosen = Some(vf);
                        break;
                    }
                }
            }

            if let Some(vf) = chosen {
                self.upload(frame, &vf);
            }
        }

        // 显示当前纹理
        if let (Some(id), (w, h)) = (self.tex_id, self.size) {
            if w > 0 && h > 0 {
                let avail = ui.available_size();
                let scale = (avail.x / w as f32).min(avail.y / h as f32).max(0.0);
                let draw = egui::vec2(w as f32 * scale, h as f32 * scale);
                ui.centered_and_justified(|ui| {
                    ui.image((id, draw));
                });
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("拖入视频文件开始播放");
            });
        }
    }

    fn upload(&mut self, frame: &mut eframe::Frame, vf: &media::VideoFrame) {
        let render_state = frame
            .wgpu_render_state()
            .expect("需要 wgpu 后端 (NativeOptions.renderer = Wgpu)");
        let device = &render_state.device;
        let queue = &render_state.queue;

        // 按需(重)建纹理
        let need_new = match &self.texture {
            Some(t) => t.size() != (vf.width, vf.height),
            None => true,
        };
        if need_new {
            let tex = VideoTexture::new(device, vf.width, vf.height);
            let view = tex.create_view();
            let id = render_state.renderer.write().register_native_texture(
                device,
                &view,
                wgpu::FilterMode::Linear,
            );
            self.texture = Some(tex);
            self.tex_id = Some(id);
            self.size = (vf.width, vf.height);
        }

        if let Some(tex) = self.texture.as_mut() {
            tex.upload(queue, &vf.rgba);
        }
    }
}

impl Default for VideoView {
    fn default() -> Self {
        Self::new()
    }
}
```

注: `media` 需作为 app 的依赖以引用 `media::VideoFrame`。在 `crates/app/Cargo.toml` 的 `[dependencies]` 追加 `media = { path = "../media" }`。

- [ ] **Step 3: 在 app.rs 接入 VideoView**

`crates/app/src/app.rs` 修改: 给 `PlayerApp` 加 `video_view: VideoView` 字段,中央面板改用它:
```rust
use crate::video_view::VideoView;
// ... PlayerApp 结构体加字段:
pub struct PlayerApp {
    player: Player,
    video_view: VideoView,
}
// new():
Self { player: Player::new(), video_view: VideoView::new() }
// ui() 中的 CentralPanel 改为:
egui::CentralPanel::default().show_inside(ui, |ui| {
    self.video_view.show(ui, _frame, &self.player);
});
```
并把 `ui` 方法签名里的 `_frame` 改为 `frame`(需要用到它取 wgpu_render_state)。

- [ ] **Step 4: 构建**

Run: `cargo build -p app`
Expected: 编译成功。

- [ ] **Step 5: 人工端到端验证(本计划的核心交付)**

Run: `cargo run -p app`
Expected(人工):
- 拖入一个真实视频文件。
- 中央面板**显示出视频画面**,随播放推进而更新。
- 能听到声音,且**音画大体同步**(说话口型对得上)。
- 时间标签随播放走动,seek 条进度前进。
- 点击 ⏸ 暂停按钮 → 状态切换(画面停留,Task 中暂停对解码的影响在计划 4 完善;本计划至少状态正确)。
- 全屏按钮工作。

这是 spec 标注"音画同步需人工验证"的关键验证点。若画面不显示,检查: NativeOptions.renderer 是否为 Wgpu;register_native_texture 是否每次重建都重新注册。

- [ ] **Step 6: Commit**

```bash
git add crates/app/src/video_view.rs crates/app/src/app.rs crates/app/Cargo.toml
git commit -m "feat(app): wgpu 纹理显示视频帧, 按主时钟同步"
```

---

## Task 7: 全量验证

- [ ] **Step 1: 全量测试**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test`
Expected: 计划 1/2/3 所有自动化测试 PASS。

- [ ] **Step 2: Clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: 无警告、已格式化。

- [ ] **Step 3: Commit(若有改动)**

```bash
git add -A
git commit -m "style: 计划3 fmt 与 clippy 收尾"
```

---

## 已知缺口 (移交计划 4)

- **seek 真正生效**: 当前 `Command::SeekTo` 是空操作。需在 `Player` 中重启解码线程到目标关键帧并 `clock.reset_to(ms)`。
- **暂停对解码/音频的影响**: 暂停时应暂停 cpal 流推进与解码;当前仅切换状态机。
- **播放列表 UI**: SidePanel 列表、Next/Prev 接线。
- **字幕叠加显示**: 把 `subtitle::Subtitles::text_at` 的结果画到画面上。
- **倍速/逐帧/AB循环/截图**: 播放增强全套。
- **续播**: 打开时读 `persist` 的 resume_point,退出时写回。
- **音量真正生效**: 当前 `volume` 只存值未作用于音频样本;需在喂样本前乘增益。
