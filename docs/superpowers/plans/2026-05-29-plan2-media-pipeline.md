# 计划 2: 媒体管线 (软解码 + 音频 + 渲染) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 `media`(FFmpeg 软解码 + 解封装 + YUV→RGB)、`audio`(cpal 输出 + 原子帧计数主时钟)、`render`(wgpu 纹理上传)三个 crate,产出一个能打开文件、音画同步播放的最小命令行可执行程序 `playground`。

**Architecture:** `media` 用 `ffmpeg-next` 安全绑定做软解码,解码线程通过有界通道把视频帧(RGBA)和音频帧(f32 交错)推给消费方。`audio` 用 cpal,音频回调从 ringbuf 取样本,同时用 `AtomicU64` 累计已消费帧数 → 提供毫秒级主时钟。`render` 把 RGBA 帧用 wgpu 上传为纹理(本计划先验证上传正确,真正显示在计划 3 接入 egui)。硬件解码不在本计划,见计划 2.5。

**Tech Stack:** ffmpeg-next 8.1, cpal 0.17, ringbuf 0.5, wgpu 29.0, crossbeam-channel 0.5。

**前置依赖:** 计划 1 已完成(workspace、`sync::decide_frame` 可用)。

---

## 系统前置要求

`ffmpeg-next` 默认通过 pkg-config 链接系统 FFmpeg。执行本计划前需安装 FFmpeg 开发库:
- macOS: `brew install ffmpeg pkg-config`
- Ubuntu/Debian: `sudo apt install libavformat-dev libavcodec-dev libavutil-dev libswscale-dev libswresample-dev pkg-config clang`
- Windows: 用 vcpkg 安装 ffmpeg,或设置 `FFMPEG_DIR` 环境变量指向预编译库。

体积优化(裁剪 FFmpeg)在打包阶段处理,不影响本计划开发。开发期用系统 FFmpeg 即可。

## 文件结构

```
crates/
├── media/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # 导出 Frame 类型与 Decoder
│       ├── frame.rs            # VideoFrame (RGBA) / AudioChunk (f32) 数据类型
│       ├── decoder.rs          # 打开文件、解封装、软解码循环、推帧
│       └── error.rs            # MediaError
├── audio/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── clock.rs            # MasterClock: AtomicU64 帧计数 → 毫秒
│       └── output.rs           # cpal 输出流 + ringbuf 喂样本
├── render/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── texture.rs          # wgpu 纹理创建与每帧上传
└── playground/                 # 本计划的验证用可执行程序
    ├── Cargo.toml
    └── src/main.rs             # 串起 media+audio,命令行播放(无 GUI)
```

---
## Task 1: media crate 骨架与帧数据类型

定义跨线程传递的帧类型。`VideoFrame` 持有 RGBA 像素 + PTS(毫秒);`AudioChunk` 持有交错 f32 样本 + PTS。纯数据,可单测。

**Files:**
- Create: `crates/media/Cargo.toml`
- Create: `crates/media/src/lib.rs`
- Create: `crates/media/src/frame.rs`
- Create: `crates/media/src/error.rs`

- [ ] **Step 1: 创建 media crate 清单**

`crates/media/Cargo.toml`:
```toml
[package]
name = "media"
version.workspace = true
edition.workspace = true

[dependencies]
ffmpeg-next = "8.1"
crossbeam-channel = "0.5"
thiserror = "2"
```

在 workspace 根 `Cargo.toml` 的 `[workspace.dependencies]` 追加(供后续 crate 共用):
```toml
crossbeam-channel = "0.5"
thiserror = "2"
```

- [ ] **Step 2: 写失败测试**

`crates/media/src/frame.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_frame_reports_dimensions_and_pts() {
        let f = VideoFrame::new(2, 2, 40, vec![0u8; 2 * 2 * 4]);
        assert_eq!(f.width, 2);
        assert_eq!(f.height, 2);
        assert_eq!(f.pts_ms, 40);
        assert_eq!(f.rgba.len(), 16);
    }

    #[test]
    fn audio_chunk_holds_interleaved_samples() {
        let c = AudioChunk::new(vec![0.1, -0.1, 0.2, -0.2], 2, 100);
        assert_eq!(c.channels, 2);
        assert_eq!(c.pts_ms, 100);
        assert_eq!(c.frame_count(), 2); // 4 samples / 2 channels
    }
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p media frame`
Expected: 编译失败, "cannot find type `VideoFrame`"。

- [ ] **Step 4: 实现帧类型**

`crates/media/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("ffmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),
    #[error("no {0} stream found")]
    NoStream(&'static str),
}
```

`crates/media/src/frame.rs` 顶部(测试模块之上):
```rust
/// 解码并转换为 RGBA 后的视频帧, 可直接上传 GPU。
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub pts_ms: u64,
    pub rgba: Vec<u8>,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, pts_ms: u64, rgba: Vec<u8>) -> Self {
        Self { width, height, pts_ms, rgba }
    }
}

/// 解码后的一段音频, 交错 f32 样本。
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub pts_ms: u64,
}

impl AudioChunk {
    pub fn new(samples: Vec<f32>, channels: u16, pts_ms: u64) -> Self {
        Self { samples, channels, pts_ms }
    }

    pub fn frame_count(&self) -> usize {
        if self.channels == 0 { 0 } else { self.samples.len() / self.channels as usize }
    }
}
```

`crates/media/src/lib.rs`:
```rust
//! FFmpeg 软解码管线: 解封装、解码、YUV→RGB。硬解见计划 2.5。
mod error;
mod frame;
pub use error::MediaError;
pub use frame::{AudioChunk, VideoFrame};
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p media frame`
Expected: PASS, 2 个测试通过。

- [ ] **Step 6: Commit**

```bash
git add crates/media Cargo.toml
git commit -m "feat(media): 帧数据类型 (VideoFrame/AudioChunk) 与错误类型"
```

---

## Task 2: 生成测试视频样本

后续解码任务需要一个真实但极小的视频文件作为集成测试输入。用系统 ffmpeg CLI 生成一个 1 秒、纯色、带静音音轨的 mp4。

**Files:**
- Create: `crates/media/tests/fixtures/` (目录)
- Create: 脚本 `crates/media/tests/gen_fixture.sh`

- [ ] **Step 1: 写生成脚本**

`crates/media/tests/gen_fixture.sh`:
```bash
#!/usr/bin/env bash
# 生成一个 1 秒、160x120、25fps、纯色视频 + 静音 AAC 音轨的测试 mp4。
set -euo pipefail
DIR="$(dirname "$0")/fixtures"
mkdir -p "$DIR"
ffmpeg -y \
  -f lavfi -i "color=c=red:s=160x120:r=25:d=1" \
  -f lavfi -i "anullsrc=channel_layout=stereo:sample_rate=44100" \
  -shortest -c:v libx264 -pix_fmt yuv420p -c:a aac \
  "$DIR/sample.mp4"
echo "生成: $DIR/sample.mp4"
```

- [ ] **Step 2: 运行脚本生成样本**

Run: `bash crates/media/tests/gen_fixture.sh`
Expected: 输出 "生成: .../sample.mp4",且文件存在(`ls crates/media/tests/fixtures/sample.mp4`)。

- [ ] **Step 3: 提交脚本(不提交生成的视频)**

在仓库根 `.gitignore` 追加一行:
```
crates/media/tests/fixtures/
```

```bash
git add crates/media/tests/gen_fixture.sh .gitignore
git commit -m "test(media): 测试视频样本生成脚本"
```

注: 测试样本由脚本本地生成、不入库,避免二进制文件污染仓库。CI 中在测试前先跑此脚本。

---
## Task 3: 视频解码与 YUV→RGB

打开文件,找到最佳视频流,逐包解码,用 sws scaler 转成 RGBA,产出 `VideoFrame`。这是集成层,用 Task 2 的样本做集成测试。

**Files:**
- Create: `crates/media/src/decoder.rs`
- Modify: `crates/media/src/lib.rs`
- Create: `crates/media/tests/decode_video.rs`

- [ ] **Step 1: 写失败的集成测试**

`crates/media/tests/decode_video.rs`:
```rust
use media::VideoDecoder;
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4")
}

#[test]
fn decodes_all_video_frames_to_rgba() {
    let path = fixture();
    assert!(path.exists(), "先运行 tests/gen_fixture.sh 生成样本");

    let mut dec = VideoDecoder::open(&path).unwrap();
    assert_eq!(dec.width(), 160);
    assert_eq!(dec.height(), 120);

    let mut count = 0u32;
    let mut last_pts = 0u64;
    while let Some(frame) = dec.next_frame().unwrap() {
        assert_eq!(frame.width, 160);
        assert_eq!(frame.height, 120);
        assert_eq!(frame.rgba.len(), 160 * 120 * 4);
        assert!(frame.pts_ms >= last_pts, "PTS 应单调不减");
        last_pts = frame.pts_ms;
        count += 1;
    }
    // 1 秒 @ 25fps ≈ 25 帧 (允许 ±2 帧编码差异)
    assert!((23..=27).contains(&count), "解码帧数 {count} 不在预期范围");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test -p media --test decode_video`
Expected: 编译失败, "cannot find type `VideoDecoder`"。

- [ ] **Step 3: 实现解码器(打开 + 元数据)**

`crates/media/src/decoder.rs`:
```rust
use crate::error::MediaError;
use crate::frame::VideoFrame;
use ffmpeg_next as ff;
use ff::format::Pixel;
use ff::media::Type;
use ff::software::scaling::{context::Context as Scaler, flag::Flags};
use ff::util::frame::video::Video as FfVideo;
use std::path::Path;

pub struct VideoDecoder {
    ictx: ff::format::context::Input,
    decoder: ff::decoder::Video,
    scaler: Scaler,
    stream_index: usize,
    time_base: f64,
    width: u32,
    height: u32,
    eof: bool,
}

impl VideoDecoder {
    pub fn open(path: &Path) -> Result<Self, MediaError> {
        ff::init()?;
        let ictx = ff::format::input(&path)?;
        let stream = ictx
            .streams()
            .best(Type::Video)
            .ok_or(MediaError::NoStream("video"))?;
        let stream_index = stream.index();
        let time_base = f64::from(stream.time_base());
        let ctx = ff::codec::context::Context::from_parameters(stream.parameters())?;
        let decoder = ctx.decoder().video()?;
        let (width, height) = (decoder.width(), decoder.height());
        let scaler = Scaler::get(
            decoder.format(),
            width,
            height,
            Pixel::RGBA,
            width,
            height,
            Flags::BILINEAR,
        )?;
        Ok(Self {
            ictx,
            decoder,
            scaler,
            stream_index,
            time_base,
            width,
            height,
            eof: false,
        })
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
}
```

- [ ] **Step 4: 实现解码循环 (next_frame)**

在 `decoder.rs` 的 `impl VideoDecoder` 块内继续添加。注意 send/receive 模型: 一个包可能产出 0 或多帧,需要内部缓冲已发送状态。这里用简单策略——每次 `next_frame` 先尝试 receive,取不到再 send 下一个包:
```rust
impl VideoDecoder {
    /// 返回下一帧 RGBA, 文件结束返回 Ok(None)。
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>, MediaError> {
        loop {
            // 先尝试从解码器取已就绪的帧
            let mut decoded = FfVideo::empty();
            if self.decoder.receive_frame(&mut decoded).is_ok() {
                return Ok(Some(self.scale(&decoded)?));
            }
            if self.eof {
                return Ok(None);
            }
            // 没有就绪帧, 喂下一个视频包
            match self.read_video_packet()? {
                Some(packet) => {
                    self.decoder.send_packet(&packet)?;
                }
                None => {
                    // 无更多包, 发 EOF 并继续 drain
                    self.decoder.send_eof()?;
                    self.eof = true;
                }
            }
        }
    }

    fn read_video_packet(&mut self) -> Result<Option<ff::codec::packet::Packet>, MediaError> {
        // ictx.packets() 借用 ictx, 这里手动迭代以保留 &mut self 结构
        let mut packet = ff::codec::packet::Packet::empty();
        loop {
            match packet.read(&mut self.ictx) {
                Ok(()) => {
                    if unsafe { (*packet.as_ptr()).stream_index } as usize == self.stream_index {
                        return Ok(Some(packet));
                    }
                    // 非视频包, 跳过继续读
                }
                Err(ff::Error::Eof) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn scale(&mut self, decoded: &FfVideo) -> Result<VideoFrame, MediaError> {
        let mut rgba = FfVideo::empty();
        self.scaler.run(decoded, &mut rgba)?;
        let pts = decoded.pts().unwrap_or(0);
        let pts_ms = (pts as f64 * self.time_base * 1000.0).max(0.0) as u64;
        // rgba.data(0) 含行对齐(stride), 需按 stride 逐行拷贝去除 padding
        let stride = rgba.stride(0);
        let row_bytes = (self.width * 4) as usize;
        let src = rgba.data(0);
        let mut out = Vec::with_capacity(row_bytes * self.height as usize);
        for y in 0..self.height as usize {
            let start = y * stride;
            out.extend_from_slice(&src[start..start + row_bytes]);
        }
        Ok(VideoFrame::new(self.width, self.height, pts_ms, out))
    }
}
```

- [ ] **Step 5: 挂载并导出**

`crates/media/src/lib.rs` 改为:
```rust
//! FFmpeg 软解码管线: 解封装、解码、YUV→RGB。硬解见计划 2.5。
mod decoder;
mod error;
mod frame;
pub use decoder::VideoDecoder;
pub use error::MediaError;
pub use frame::{AudioChunk, VideoFrame};
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p media --test decode_video`
Expected: PASS。若 `read` 方法签名报错,确认 ffmpeg-next 8.1 的 `Packet::read(&mut self, &mut Input)` 签名;它返回 `Result<(), Error>`。

- [ ] **Step 7: Commit**

```bash
git add crates/media/src/decoder.rs crates/media/src/lib.rs crates/media/tests/decode_video.rs
git commit -m "feat(media): 视频软解码与 YUV→RGBA 转换"
```

---
## Task 4: audio crate — 主时钟

`MasterClock` 包一个 `AtomicU64`(已消费音频帧数)+ 采样率,提供 `position_ms()`。音频回调每消费一批样本就 `add_frames()`。这部分逻辑可脱离 cpal 单测。

**Files:**
- Create: `crates/audio/Cargo.toml`
- Create: `crates/audio/src/lib.rs`
- Create: `crates/audio/src/clock.rs`

- [ ] **Step 1: 创建 audio crate 清单**

`crates/audio/Cargo.toml`:
```toml
[package]
name = "audio"
version.workspace = true
edition.workspace = true

[dependencies]
cpal = "0.17"
ringbuf = "0.5"
media = { path = "../media" }
```

- [ ] **Step 2: 写失败测试**

`crates/audio/src/clock.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero() {
        let c = MasterClock::new(44_100);
        assert_eq!(c.position_ms(), 0);
    }

    #[test]
    fn one_second_of_frames_is_1000ms() {
        let c = MasterClock::new(44_100);
        c.add_frames(44_100);
        assert_eq!(c.position_ms(), 1000);
    }

    #[test]
    fn half_second() {
        let c = MasterClock::new(48_000);
        c.add_frames(24_000);
        assert_eq!(c.position_ms(), 500);
    }

    #[test]
    fn accumulates_across_calls() {
        let c = MasterClock::new(1000);
        c.add_frames(250);
        c.add_frames(250);
        assert_eq!(c.position_ms(), 500);
    }
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p audio clock`
Expected: 编译失败, "cannot find type `MasterClock`"。

- [ ] **Step 4: 实现 MasterClock**

`crates/audio/src/clock.rs` 顶部:
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 音频主时钟。回调线程累加已消费帧数, 任意线程读取播放位置。
#[derive(Clone)]
pub struct MasterClock {
    frames_played: Arc<AtomicU64>,
    sample_rate: u32,
}

impl MasterClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            frames_played: Arc::new(AtomicU64::new(0)),
            sample_rate: sample_rate.max(1),
        }
    }

    /// 音频回调消费了 `n` 个音频帧(每帧含所有声道一份样本)后调用。
    pub fn add_frames(&self, n: u64) {
        self.frames_played.fetch_add(n, Ordering::Relaxed);
    }

    /// 当前播放位置(毫秒)。
    pub fn position_ms(&self) -> u64 {
        let f = self.frames_played.load(Ordering::Relaxed);
        f * 1000 / self.sample_rate as u64
    }

    /// seek 后重置时钟基准。
    pub fn reset_to(&self, ms: u64) {
        let frames = ms * self.sample_rate as u64 / 1000;
        self.frames_played.store(frames, Ordering::Relaxed);
    }
}
```

`crates/audio/src/lib.rs`:
```rust
//! 音频输出 (cpal) 与音频主时钟。
mod clock;
pub use clock::MasterClock;
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p audio clock`
Expected: PASS, 4 个测试通过。

- [ ] **Step 6: Commit**

```bash
git add crates/audio
git commit -m "feat(audio): 原子帧计数主时钟"
```

---

## Task 5: audio crate — cpal 输出流

建立 cpal 输出流,从 ringbuf 取样本喂给回调,回调推进主时钟。提供 `AudioOutput::start()` 返回 `(producer, MasterClock, Stream)`。这是系统集成层,需人工验证有声。

**Files:**
- Create: `crates/audio/src/output.rs`
- Modify: `crates/audio/src/lib.rs`

- [ ] **Step 1: 实现 cpal 输出**

`crates/audio/src/output.rs`:
```rust
use crate::clock::MasterClock;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;

pub type SampleProducer = <HeapRb<f32> as Split>::Prod;

pub struct AudioOutput {
    pub stream: cpal::Stream,
    pub clock: MasterClock,
    pub producer: SampleProducer,
    pub channels: u16,
    pub sample_rate: u32,
}

impl AudioOutput {
    /// 打开默认输出设备并启动播放。返回的 producer 用于从解码线程推入交错 f32 样本。
    pub fn start() -> Result<Self, cpal::BuildStreamError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("无默认音频输出设备");
        let supported = device.default_output_config().expect("无默认输出配置");
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels;
        let sample_rate = config.sample_rate.0;

        let clock = MasterClock::new(sample_rate);
        let clock_cb = clock.clone();

        // ~1 秒缓冲
        let rb = HeapRb::<f32>::new(sample_rate as usize * channels as usize);
        let (producer, mut consumer) = rb.split();

        let ch = channels as u64;
        let err_fn = |e| eprintln!("音频流错误: {e}");

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    let mut filled = 0u64;
                    for slot in data.iter_mut() {
                        *slot = consumer.try_pop().unwrap_or(0.0);
                        filled += 1;
                    }
                    clock_cb.add_frames(filled / ch);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _| {
                    let mut filled = 0u64;
                    for slot in data.iter_mut() {
                        let s = consumer.try_pop().unwrap_or(0.0);
                        *slot = i16::from_sample(s);
                        filled += 1;
                    }
                    clock_cb.add_frames(filled / ch);
                },
                err_fn,
                None,
            )?,
            other => panic!("不支持的采样格式: {other:?}"),
        };

        stream.play().expect("启动音频流失败");

        Ok(Self { stream, clock, producer, channels, sample_rate })
    }
}
```

注: `try_pop` 在 ringbuf 0.5 返回 `Option<f32>`;欠载时填 0.0(静音),对应 spec 的"音频问题不崩溃"。

- [ ] **Step 2: 挂载并导出**

`crates/audio/src/lib.rs` 改为:
```rust
//! 音频输出 (cpal) 与音频主时钟。
mod clock;
mod output;
pub use clock::MasterClock;
pub use output::{AudioOutput, SampleProducer};
```

- [ ] **Step 3: 验证编译**

Run: `cargo build -p audio`
Expected: 编译成功(实际发声在 Task 8 playground 中人工验证)。

- [ ] **Step 4: Commit**

```bash
git add crates/audio/src/output.rs crates/audio/src/lib.rs
git commit -m "feat(audio): cpal 输出流与 ringbuf 喂样本"
```

---
## Task 6: media crate — 音频解码与重采样

解码音频流,重采样为交错 f32(cpal 的通用格式),产出 `AudioChunk`。结构与视频解码器对称。

**Files:**
- Create: `crates/media/src/audio_decoder.rs`
- Modify: `crates/media/src/lib.rs`
- Create: `crates/media/tests/decode_audio.rs`

- [ ] **Step 1: 写失败的集成测试**

`crates/media/tests/decode_audio.rs`:
```rust
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
        // 交错样本数应是声道数的整数倍
        assert_eq!(chunk.samples.len() % chunk.channels as usize, 0);
        total_frames += chunk.frame_count();
    }
    // 样本是 1 秒静音, 总帧数应接近 sample_rate (±10%)
    let expected = dec.sample_rate() as usize;
    assert!(total_frames > expected * 9 / 10, "音频帧数 {total_frames} 偏少");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p media --test decode_audio`
Expected: 编译失败, "cannot find type `AudioDecoder`"。

- [ ] **Step 3: 实现音频解码器(打开)**

`crates/media/src/audio_decoder.rs`:
```rust
use crate::error::MediaError;
use crate::frame::AudioChunk;
use ffmpeg_next as ff;
use ff::format::sample::{Sample, Type as SampleType};
use ff::media::Type;
use ff::util::frame::audio::Audio as FfAudio;
use ff::ChannelLayout;
use std::path::Path;

pub struct AudioDecoder {
    ictx: ff::format::context::Input,
    decoder: ff::decoder::Audio,
    resampler: ff::software::resampling::Context,
    stream_index: usize,
    time_base: f64,
    channels: u16,
    sample_rate: u32,
    eof: bool,
}

impl AudioDecoder {
    pub fn open(path: &Path) -> Result<Self, MediaError> {
        ff::init()?;
        let ictx = ff::format::input(&path)?;
        let stream = ictx
            .streams()
            .best(Type::Audio)
            .ok_or(MediaError::NoStream("audio"))?;
        let stream_index = stream.index();
        let time_base = f64::from(stream.time_base());
        let ctx = ff::codec::context::Context::from_parameters(stream.parameters())?;
        let decoder = ctx.decoder().audio()?;

        let out_rate = decoder.rate();
        let out_layout = ChannelLayout::STEREO;
        let resampler = ff::software::resampling::Context::get(
            decoder.format(),
            decoder.channel_layout(),
            decoder.rate(),
            Sample::F32(SampleType::Packed),
            out_layout,
            out_rate,
        )?;

        Ok(Self {
            ictx,
            decoder,
            resampler,
            stream_index,
            time_base,
            channels: 2, // 重采样目标固定 stereo
            sample_rate: out_rate,
            eof: false,
        })
    }

    pub fn channels(&self) -> u16 { self.channels }
    pub fn sample_rate(&self) -> u32 { self.sample_rate }
}
```

- [ ] **Step 4: 实现解码循环 (next_chunk)**

在 `audio_decoder.rs` 的 `impl AudioDecoder` 块内继续:
```rust
impl AudioDecoder {
    pub fn next_chunk(&mut self) -> Result<Option<AudioChunk>, MediaError> {
        loop {
            let mut decoded = FfAudio::empty();
            if self.decoder.receive_frame(&mut decoded).is_ok() {
                return Ok(Some(self.resample(&decoded)?));
            }
            if self.eof {
                return Ok(None);
            }
            match self.read_audio_packet()? {
                Some(packet) => self.decoder.send_packet(&packet)?,
                None => {
                    self.decoder.send_eof()?;
                    self.eof = true;
                }
            }
        }
    }

    fn read_audio_packet(&mut self) -> Result<Option<ff::codec::packet::Packet>, MediaError> {
        let mut packet = ff::codec::packet::Packet::empty();
        loop {
            match packet.read(&mut self.ictx) {
                Ok(()) => {
                    if unsafe { (*packet.as_ptr()).stream_index } as usize == self.stream_index {
                        return Ok(Some(packet));
                    }
                }
                Err(ff::Error::Eof) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn resample(&mut self, decoded: &FfAudio) -> Result<AudioChunk, MediaError> {
        let mut out = FfAudio::empty();
        self.resampler.run(decoded, &mut out)?;
        let pts = decoded.pts().unwrap_or(0);
        let pts_ms = (pts as f64 * self.time_base * 1000.0).max(0.0) as u64;
        // 重采样输出为 packed f32: plane(0) 含全部交错样本
        let frame_count = out.samples();
        let total = frame_count * self.channels as usize;
        let raw = out.data(0);
        let mut samples = Vec::with_capacity(total);
        let bytes = &raw[..total * std::mem::size_of::<f32>()];
        for chunk in bytes.chunks_exact(4) {
            samples.push(f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        Ok(AudioChunk::new(samples, self.channels, pts_ms))
    }
}
```

- [ ] **Step 5: 挂载并导出**

`crates/media/src/lib.rs` 改为:
```rust
//! FFmpeg 软解码管线: 解封装、解码、YUV→RGB。硬解见计划 2.5。
mod audio_decoder;
mod decoder;
mod error;
mod frame;
pub use audio_decoder::AudioDecoder;
pub use decoder::VideoDecoder;
pub use error::MediaError;
pub use frame::{AudioChunk, VideoFrame};
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p media --test decode_audio`
Expected: PASS。若 `out.samples()` 返回的是单声道帧数而非交错总数,以 `out.samples()` 为每声道帧数、乘以声道数得交错总长(本实现已如此处理)。

- [ ] **Step 7: Commit**

```bash
git add crates/media/src/audio_decoder.rs crates/media/src/lib.rs crates/media/tests/decode_audio.rs
git commit -m "feat(media): 音频软解码与 f32 重采样"
```

---
## Task 7: render crate — wgpu 纹理上传

封装"创建一个 RGBA 纹理 + 每帧上传像素"。本计划只验证纹理创建与上传不崩溃(headless),真正显示到窗口在计划 3 接入 egui。

**Files:**
- Create: `crates/render/Cargo.toml`
- Create: `crates/render/src/lib.rs`
- Create: `crates/render/src/texture.rs`
- Create: `crates/render/tests/upload.rs`

- [ ] **Step 1: 创建 render crate 清单**

`crates/render/Cargo.toml`:
```toml
[package]
name = "render"
version.workspace = true
edition.workspace = true

[dependencies]
wgpu = "29.0"
pollster = "0.4"

[dev-dependencies]
pollster = "0.4"
```

- [ ] **Step 2: 写失败测试**

`crates/render/tests/upload.rs`:
```rust
use render::VideoTexture;

#[test]
fn creates_and_uploads_without_panicking() {
    // 申请一个 headless 适配器; 无 GPU 环境则跳过(CI 容器常见)。
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()));
    let Ok(adapter) = adapter else {
        eprintln!("无可用 GPU 适配器, 跳过测试");
        return;
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();

    let mut tex = VideoTexture::new(&device, 4, 4);
    assert_eq!(tex.size(), (4, 4));

    let pixels = vec![255u8; 4 * 4 * 4];
    tex.upload(&queue, &pixels); // 不 panic 即通过

    // 尺寸变化时重建
    tex.ensure_size(&device, 8, 8);
    assert_eq!(tex.size(), (8, 8));
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p render`
Expected: 编译失败, "cannot find type `VideoTexture`"。

- [ ] **Step 4: 实现 VideoTexture**

`crates/render/src/texture.rs`:
```rust
/// 持有一个 Rgba8Unorm 纹理, 支持每帧上传与按需重建。
pub struct VideoTexture {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
}

impl VideoTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = Self::create(device, width, height);
        Self { texture, width, height }
    }

    fn create(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video_frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// 帧尺寸变化时重建底层纹理。
    pub fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if (width, height) != (self.width, self.height) {
            self.texture = Self::create(device, width, height);
            self.width = width;
            self.height = height;
        }
    }

    /// 上传一帧 RGBA 像素 (长度须为 width*height*4)。
    pub fn upload(&mut self, queue: &wgpu::Queue, rgba: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.width),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn create_view(&self) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor::default())
    }
}
```

`crates/render/src/lib.rs`:
```rust
//! wgpu 视频纹理上传。注: wgpu 29 起类型名为 TexelCopyTextureInfo/TexelCopyBufferLayout
//! (旧名 ImageCopyTexture/ImageDataLayout 已移除)。
mod texture;
pub use texture::VideoTexture;
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p render`
Expected: PASS(有 GPU)或打印跳过信息后 PASS(无 GPU)。

- [ ] **Step 6: Commit**

```bash
git add crates/render
git commit -m "feat(render): wgpu 视频纹理创建与每帧上传"
```

---

## Task 8: playground 可执行程序 — 端到端验证

把 `media` + `audio` 串起来: 解码音频喂给 cpal,解码视频按主时钟用 `sync::decide_frame` 决定显示时机(此处无窗口,仅打印"显示第 N 帧 @ Xms"),验证音画同步逻辑端到端跑通且声音正常。

**Files:**
- Create: `crates/playground/Cargo.toml`
- Create: `crates/playground/src/main.rs`

- [ ] **Step 1: 创建 playground 清单**

`crates/playground/Cargo.toml`:
```toml
[package]
name = "playground"
version.workspace = true
edition.workspace = true

[dependencies]
media = { path = "../media" }
audio = { path = "../audio" }
sync = { path = "../sync" }
ringbuf = "0.5"
```

- [ ] **Step 2: 实现 main**

`crates/playground/src/main.rs`:
```rust
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
    let tol_ms: i64 = 15;
    let mut shown = 0u32;
    while let Some(frame) = vdec.next_frame().expect("视频解码错误") {
        loop {
            let master = clock.position_ms();
            match sync::decide_frame(master, frame.pts_ms, tol_ms) {
                sync::FrameDecision::Display => {
                    shown += 1;
                    println!("显示帧 {shown} @ pts={}ms 主时钟={}ms", frame.pts_ms, master);
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
```

- [ ] **Step 3: 构建**

Run: `cargo build -p playground`
Expected: 编译成功。

- [ ] **Step 4: 人工端到端验证**

Run: `cargo run -p playground -- crates/media/tests/fixtures/sample.mp4`
Expected(人工确认):
- 打印 `视频: 160x120`
- 依次打印"显示帧 N",帧序号递增,主时钟随时间推进
- 程序在约 1 秒后打印"播放结束, 共显示 ~25 帧"并退出
- (样本是静音,无声正常;用一个有声的真实视频再跑一次确认能听到声音)

注: 这是 spec 中标注的"音画同步需人工验证"项。无法自动断言主观同步,但帧序号递增 + 主时钟推进 + 正常退出可作为管线连通的客观证据。

- [ ] **Step 5: Commit**

```bash
git add crates/playground
git commit -m "feat(playground): 端到端音画同步验证程序"
```

---

## Task 9: 全量验证

- [ ] **Step 1: 全量测试**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test`
Expected: 计划 1 + 计划 2 所有测试 PASS(render 测试在无 GPU 时跳过)。

- [ ] **Step 2: Clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 无警告。注: `decoder.rs`/`audio_decoder.rs` 中的 `unsafe { (*packet.as_ptr()).stream_index }` 需加 `// SAFETY: 仅读取已初始化包的 stream_index 字段` 注释。

- [ ] **Step 3: 格式化**

Run: `cargo fmt --all && cargo fmt --all --check`
Expected: 无输出。

- [ ] **Step 4: Commit(若有改动)**

```bash
git add -A
git commit -m "style: 计划2 fmt 与 clippy 收尾"
```

---

## 已知缺口 (移交后续计划)

- **硬件解码**: 见计划 2.5(unsafe FFI 层)。本计划全软解。
- **真实窗口显示**: `render` 仅验证纹理上传;接入 egui 窗口、把纹理显示出来在计划 3。
- **seek**: 解码器目前只能顺序读;seek(跳关键帧 + 清队列)在计划 3 接入播放控制时实现。
- **有界帧队列 + 解码线程解耦**: playground 用了简化的直读模型;计划 3 引入正式的有界通道与独立解码线程。
- **暂停/倍速**: 主时钟目前只随音频前进;暂停与倍速在计划 3/4 处理。
