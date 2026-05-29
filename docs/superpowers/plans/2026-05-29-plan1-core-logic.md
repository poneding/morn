# 计划 1: 项目骨架 + 纯逻辑基础 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 搭建 Cargo workspace 骨架,并用 TDD 实现四个无系统依赖的纯逻辑 crate:`player-core`、`persist`、`subtitle`、`sync`,产出一组全部测试通过的库。

**Architecture:** 单 workspace 多 crate。纯逻辑层不依赖 FFmpeg/wgpu/cpal/egui,所有逻辑可脱离视频文件单元测试。状态机、时钟对齐、字幕解析、偏好存储各为一个 crate,边界清晰。后续计划(媒体管线、UI)在此基础上接线。

**Tech Stack:** Rust 1.94 (edition 2021), Cargo workspace, serde + serde_json (persist), 标准库为主。

---

## 文件结构

```
morn/
├── Cargo.toml                      # workspace 根
├── crates/
│   ├── player-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # 导出公共类型
│   │       ├── state.rs            # PlaybackState 状态机
│   │       ├── command.rs          # Command 枚举
│   │       └── playlist.rs         # Playlist 队列管理
│   ├── persist/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── prefs.rs            # Preferences + ResumePoint 读写
│   ├── subtitle/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── model.rs            # Cue / Subtitles 数据模型
│   │       └── srt.rs              # .srt 解析
│   └── sync/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           └── clock.rs            # 音频主时钟 + 视频帧调度决策
```

注: `.ass` 解析较复杂,首版 subtitle crate 仅实现 `.srt`;`.ass` 留待功能层计划补充(spec 已列为首版字幕格式,在此标注为已知缺口,见计划末尾)。

---
## Task 1: Workspace 骨架

**Files:**
- Create: `Cargo.toml`
- Create: `crates/player-core/Cargo.toml`
- Create: `crates/player-core/src/lib.rs`

- [ ] **Step 1: 创建 workspace 根 Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: 创建 player-core crate 清单**

`crates/player-core/Cargo.toml`:
```toml
[package]
name = "player-core"
version.workspace = true
edition.workspace = true

[dependencies]
```

- [ ] **Step 3: 创建占位 lib.rs**

`crates/player-core/src/lib.rs`:
```rust
//! 播放控制核心: 状态机、命令、播放列表。无 GUI/FFmpeg 依赖。
```

- [ ] **Step 4: 验证 workspace 可构建**

Run: `cargo build`
Expected: 编译成功, 输出 `Compiling player-core v0.1.0` 且无错误。

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/player-core/Cargo.toml crates/player-core/src/lib.rs
git commit -m "chore: 初始化 cargo workspace 与 player-core 骨架"
```

---

## Task 2: PlaybackState 状态机

播放状态机管理 Stopped/Playing/Paused 三态及合法转换。非法转换返回错误而非 panic。

**Files:**
- Create: `crates/player-core/src/state.rs`
- Modify: `crates/player-core/src/lib.rs`

- [ ] **Step 1: 写失败测试**

`crates/player-core/src/state.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_stopped() {
        let m = StateMachine::new();
        assert_eq!(m.state(), PlaybackState::Stopped);
    }

    #[test]
    fn play_from_stopped_goes_playing() {
        let mut m = StateMachine::new();
        m.apply(Transition::Play).unwrap();
        assert_eq!(m.state(), PlaybackState::Playing);
    }

    #[test]
    fn pause_from_playing_goes_paused() {
        let mut m = StateMachine::new();
        m.apply(Transition::Play).unwrap();
        m.apply(Transition::Pause).unwrap();
        assert_eq!(m.state(), PlaybackState::Paused);
    }

    #[test]
    fn pause_from_stopped_is_error() {
        let mut m = StateMachine::new();
        assert!(m.apply(Transition::Pause).is_err());
        assert_eq!(m.state(), PlaybackState::Stopped);
    }

    #[test]
    fn stop_from_any_goes_stopped() {
        let mut m = StateMachine::new();
        m.apply(Transition::Play).unwrap();
        m.apply(Transition::Stop).unwrap();
        assert_eq!(m.state(), PlaybackState::Stopped);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p player-core state`
Expected: 编译失败, "cannot find type `StateMachine`"。

- [ ] **Step 3: 实现状态机**

在 `state.rs` 顶部(测试模块之上)添加:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Play,
    Pause,
    Stop,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: PlaybackState,
    pub transition: Transition,
}

pub struct StateMachine {
    state: PlaybackState,
}

impl StateMachine {
    pub fn new() -> Self {
        Self { state: PlaybackState::Stopped }
    }

    pub fn state(&self) -> PlaybackState {
        self.state
    }

    pub fn apply(&mut self, t: Transition) -> Result<PlaybackState, InvalidTransition> {
        use PlaybackState::*;
        use Transition::*;
        let next = match (self.state, t) {
            (Stopped, Play) => Playing,
            (Paused, Play) => Playing,
            (Playing, Pause) => Paused,
            (_, Stop) => Stopped,
            (from, transition) => return Err(InvalidTransition { from, transition }),
        };
        self.state = next;
        Ok(next)
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: 在 lib.rs 挂载模块并导出**

`crates/player-core/src/lib.rs` 追加:
```rust
mod state;
pub use state::{InvalidTransition, PlaybackState, StateMachine, Transition};
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p player-core state`
Expected: PASS, 5 个测试全部通过。

- [ ] **Step 6: Commit**

```bash
git add crates/player-core/src/state.rs crates/player-core/src/lib.rs
git commit -m "feat(player-core): 播放状态机与合法转换校验"
```

---
## Task 3: Command 枚举

定义 UI → 核心的命令集合。这是个纯数据类型,测试确保其可构造、可比较、覆盖 spec 的核心操作。

**Files:**
- Create: `crates/player-core/src/command.rs`
- Modify: `crates/player-core/src/lib.rs`

- [ ] **Step 1: 写失败测试**

`crates/player-core/src/command.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn commands_are_constructible_and_comparable() {
        assert_eq!(Command::Play, Command::Play);
        assert_eq!(
            Command::Open(PathBuf::from("/v.mp4")),
            Command::Open(PathBuf::from("/v.mp4"))
        );
        assert_eq!(Command::SeekTo(1500), Command::SeekTo(1500));
        assert_eq!(Command::SetVolume(80), Command::SetVolume(80));
        assert_eq!(Command::SetRate(150), Command::SetRate(150));
        assert_ne!(Command::Play, Command::Pause);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p player-core command`
Expected: 编译失败, "cannot find type `Command`"。

- [ ] **Step 3: 实现 Command**

在 `command.rs` 顶部添加。注释说明单位约定(非显而易见):
```rust
use std::path::PathBuf;

/// UI 发往播放核心的命令。
/// 时间单位为毫秒, 音量为 0..=100, 倍速为百分比 (100 = 1.0x)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open(PathBuf),
    Play,
    Pause,
    Stop,
    SeekTo(u64),
    SetVolume(u8),
    SetRate(u16),
    Next,
    Prev,
}
```

- [ ] **Step 4: 在 lib.rs 挂载并导出**

`crates/player-core/src/lib.rs` 追加:
```rust
mod command;
pub use command::Command;
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p player-core command`
Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add crates/player-core/src/command.rs crates/player-core/src/lib.rs
git commit -m "feat(player-core): 定义 UI 命令枚举"
```

---

## Task 4: Playlist 队列管理

管理多文件队列与当前索引,支持 next/prev,边界安全(空列表、首尾)。

**Files:**
- Create: `crates/player-core/src/playlist.rs`
- Modify: `crates/player-core/src/lib.rs`

- [ ] **Step 1: 写失败测试**

`crates/player-core/src/playlist.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf { PathBuf::from(s) }

    #[test]
    fn empty_has_no_current() {
        let pl = Playlist::new();
        assert!(pl.current().is_none());
        assert_eq!(pl.len(), 0);
    }

    #[test]
    fn add_sets_first_as_current() {
        let mut pl = Playlist::new();
        pl.add(p("/a.mp4"));
        pl.add(p("/b.mp4"));
        assert_eq!(pl.current(), Some(&p("/a.mp4")));
        assert_eq!(pl.len(), 2);
    }

    #[test]
    fn next_advances_and_stops_at_end() {
        let mut pl = Playlist::new();
        pl.add(p("/a.mp4"));
        pl.add(p("/b.mp4"));
        assert_eq!(pl.next(), Some(&p("/b.mp4")));
        assert_eq!(pl.next(), None);
        assert_eq!(pl.current(), Some(&p("/b.mp4")));
    }

    #[test]
    fn prev_goes_back_and_stops_at_start() {
        let mut pl = Playlist::new();
        pl.add(p("/a.mp4"));
        pl.add(p("/b.mp4"));
        pl.next();
        assert_eq!(pl.prev(), Some(&p("/a.mp4")));
        assert_eq!(pl.prev(), None);
    }

    #[test]
    fn next_on_empty_is_none() {
        let mut pl = Playlist::new();
        assert_eq!(pl.next(), None);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p player-core playlist`
Expected: 编译失败, "cannot find type `Playlist`"。

- [ ] **Step 3: 实现 Playlist**

在 `playlist.rs` 顶部添加:
```rust
use std::path::{Path, PathBuf};

pub struct Playlist {
    items: Vec<PathBuf>,
    cursor: usize,
}

impl Playlist {
    pub fn new() -> Self {
        Self { items: Vec::new(), cursor: 0 }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn add(&mut self, path: PathBuf) {
        self.items.push(path);
    }

    pub fn current(&self) -> Option<&Path> {
        self.items.get(self.cursor).map(|p| p.as_path())
    }

    pub fn next(&mut self) -> Option<&Path> {
        if self.cursor + 1 < self.items.len() {
            self.cursor += 1;
            self.current()
        } else {
            None
        }
    }

    pub fn prev(&mut self) -> Option<&Path> {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.current()
        } else {
            None
        }
    }
}

impl Default for Playlist {
    fn default() -> Self {
        Self::new()
    }
}
```

注: 测试中 `current()` 与 `&p("/a.mp4")` 比较依赖 `Path` 与 `PathBuf` 的 `PartialEq`(标准库已提供 `Path: PartialEq<PathBuf>`),无需额外实现。

- [ ] **Step 4: 在 lib.rs 挂载并导出**

`crates/player-core/src/lib.rs` 追加:
```rust
mod playlist;
pub use playlist::Playlist;
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p player-core playlist`
Expected: PASS, 5 个测试通过。

- [ ] **Step 6: Commit**

```bash
git add crates/player-core/src/playlist.rs crates/player-core/src/lib.rs
git commit -m "feat(player-core): 播放列表队列与边界安全的 next/prev"
```

---
## Task 5: sync crate — 音频主时钟与帧调度决策

纯计算逻辑: 给定主时钟当前位置(音频已播放到的毫秒)与一个视频帧的呈现时间戳(PTS),决定该帧应"显示 / 丢弃 / 等待"。这是音画同步的核心判定,不触碰任何系统 API,可用模拟时间戳完整单测。

**Files:**
- Create: `crates/sync/Cargo.toml`
- Create: `crates/sync/src/lib.rs`
- Create: `crates/sync/src/clock.rs`

- [ ] **Step 1: 创建 sync crate 清单**

`crates/sync/Cargo.toml`:
```toml
[package]
name = "sync"
version.workspace = true
edition.workspace = true

[dependencies]
```

- [ ] **Step 2: 创建 lib.rs 占位**

`crates/sync/src/lib.rs`:
```rust
//! 音视频同步: 音频主时钟与视频帧调度决策。纯计算, 无系统依赖。
mod clock;
pub use clock::{FrameDecision, decide_frame};
```

- [ ] **Step 3: 写失败测试**

`crates/sync/src/clock.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 容差 10ms: 帧 PTS 落在 [master-10, master+10] 内即认为应显示。
    const TOL: i64 = 10;

    #[test]
    fn frame_at_master_displays() {
        assert_eq!(decide_frame(1000, 1000, TOL), FrameDecision::Display);
    }

    #[test]
    fn frame_within_tolerance_displays() {
        assert_eq!(decide_frame(1000, 1005, TOL), FrameDecision::Display);
        assert_eq!(decide_frame(1000, 995, TOL), FrameDecision::Display);
    }

    #[test]
    fn frame_far_behind_master_is_dropped() {
        // 帧 PTS 远早于主时钟 → 该帧已过期, 丢弃以追赶。
        assert_eq!(decide_frame(2000, 1500, TOL), FrameDecision::Drop);
    }

    #[test]
    fn frame_ahead_of_master_waits() {
        // 帧 PTS 远晚于主时钟 → 还没到显示时间, 等待。
        let d = decide_frame(1000, 1200, TOL);
        assert_eq!(d, FrameDecision::Wait { remaining_ms: 200 });
    }
}
```

- [ ] **Step 4: 运行测试确认失败**

Run: `cargo test -p sync`
Expected: 编译失败, "cannot find function `decide_frame`"。

- [ ] **Step 5: 实现决策逻辑**

在 `clock.rs` 顶部(测试模块之上)添加:
```rust
/// 对单个视频帧的调度决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDecision {
    /// 立即显示该帧。
    Display,
    /// 该帧已过期, 丢弃以追赶主时钟。
    Drop,
    /// 该帧尚未到显示时间, 等待指定毫秒后再处理。
    Wait { remaining_ms: u64 },
}

/// 给定主时钟位置 `master_ms` 与帧呈现时间 `frame_pts_ms`,
/// 在容差 `tolerance_ms` 内判定该帧应如何处理。
pub fn decide_frame(master_ms: u64, frame_pts_ms: u64, tolerance_ms: i64) -> FrameDecision {
    let diff = frame_pts_ms as i64 - master_ms as i64; // 正: 帧在未来; 负: 帧已过去
    if diff.abs() <= tolerance_ms {
        FrameDecision::Display
    } else if diff < 0 {
        FrameDecision::Drop
    } else {
        FrameDecision::Wait { remaining_ms: diff as u64 }
    }
}
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p sync`
Expected: PASS, 4 个测试通过。

- [ ] **Step 7: Commit**

```bash
git add crates/sync
git commit -m "feat(sync): 音频主时钟帧调度决策 (显示/丢弃/等待)"
```

---
## Task 6: subtitle crate — 数据模型

字幕的内存表示: 一个 `Cue` 是一段带起止时间(毫秒)的文本; `Subtitles` 是按时间排序的 cue 集合, 支持按当前播放位置查询应显示的文本。

**Files:**
- Create: `crates/subtitle/Cargo.toml`
- Create: `crates/subtitle/src/lib.rs`
- Create: `crates/subtitle/src/model.rs`

- [ ] **Step 1: 创建 subtitle crate 清单**

`crates/subtitle/Cargo.toml`:
```toml
[package]
name = "subtitle"
version.workspace = true
edition.workspace = true

[dependencies]
```

- [ ] **Step 2: 创建 lib.rs**

`crates/subtitle/src/lib.rs`:
```rust
//! 字幕解析与查询。首版支持 .srt。纯逻辑, 无系统依赖。
mod model;
pub use model::{Cue, Subtitles};
```

- [ ] **Step 3: 写失败测试**

`crates/subtitle/src/model.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Subtitles {
        Subtitles::from_cues(vec![
            Cue { start_ms: 1000, end_ms: 2000, text: "Hello".into() },
            Cue { start_ms: 3000, end_ms: 4000, text: "World".into() },
        ])
    }

    #[test]
    fn text_at_returns_active_cue() {
        let s = sample();
        assert_eq!(s.text_at(1500), Some("Hello"));
        assert_eq!(s.text_at(3500), Some("World"));
    }

    #[test]
    fn text_at_gap_returns_none() {
        let s = sample();
        assert_eq!(s.text_at(2500), None);
        assert_eq!(s.text_at(0), None);
    }

    #[test]
    fn boundaries_are_inclusive_start_exclusive_end() {
        let s = sample();
        assert_eq!(s.text_at(1000), Some("Hello")); // start inclusive
        assert_eq!(s.text_at(2000), None);          // end exclusive
    }
}
```

- [ ] **Step 4: 运行测试确认失败**

Run: `cargo test -p subtitle model`
Expected: 编译失败, "cannot find type `Subtitles`"。

- [ ] **Step 5: 实现数据模型**

在 `model.rs` 顶部添加:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Subtitles {
    cues: Vec<Cue>,
}

impl Subtitles {
    /// 传入的 cues 假定已按 start_ms 升序、互不重叠。
    pub fn from_cues(cues: Vec<Cue>) -> Self {
        Self { cues }
    }

    pub fn len(&self) -> usize {
        self.cues.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// 返回时刻 `ms` 处应显示的字幕文本。区间为 [start, end)。
    pub fn text_at(&self, ms: u64) -> Option<&str> {
        self.cues
            .iter()
            .find(|c| ms >= c.start_ms && ms < c.end_ms)
            .map(|c| c.text.as_str())
    }
}
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p subtitle model`
Expected: PASS, 3 个测试通过。

- [ ] **Step 7: Commit**

```bash
git add crates/subtitle
git commit -m "feat(subtitle): Cue/Subtitles 数据模型与按时刻查询"
```

---

## Task 7: subtitle crate — .srt 解析

解析标准 .srt 文本为 `Subtitles`。容错: 跳过格式错误的块而非整体失败(对应 spec 的"错误不崩溃")。

**Files:**
- Create: `crates/subtitle/src/srt.rs`
- Modify: `crates/subtitle/src/lib.rs`

- [ ] **Step 1: 写失败测试**

`crates/subtitle/src/srt.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_cues() {
        let input = "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n\
                     2\n00:00:03,000 --> 00:00:04,500\nWorld line2\nsecond\n";
        let subs = parse_srt(input);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs.text_at(1500), Some("Hello"));
        assert_eq!(subs.text_at(3500), Some("World line2\nsecond"));
    }

    #[test]
    fn timestamp_to_ms_is_correct() {
        assert_eq!(parse_timestamp("01:02:03,456"), Some(3_723_456));
        assert_eq!(parse_timestamp("00:00:00,000"), Some(0));
        assert_eq!(parse_timestamp("garbage"), None);
    }

    #[test]
    fn malformed_block_is_skipped_not_fatal() {
        let input = "1\nNOT A TIMESTAMP\nbad\n\n\
                     2\n00:00:03,000 --> 00:00:04,000\nGood\n";
        let subs = parse_srt(input);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs.text_at(3500), Some("Good"));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p subtitle srt`
Expected: 编译失败, "cannot find function `parse_srt`"。

- [ ] **Step 3: 实现解析(第一部分: 时间戳)**

在 `srt.rs` 顶部添加:
```rust
use crate::model::{Cue, Subtitles};

/// 解析 "HH:MM:SS,mmm" 为毫秒。任何格式错误返回 None。
pub fn parse_timestamp(s: &str) -> Option<u64> {
    let (hms, millis) = s.trim().split_once(',')?;
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let sec: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let ms: u64 = millis.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}
```

- [ ] **Step 4: 实现解析(第二部分: 块解析)**

在 `srt.rs` 继续添加(`parse_timestamp` 之下、测试模块之上):
```rust
/// 解析整段 .srt 文本。格式错误的块被跳过。
pub fn parse_srt(input: &str) -> Subtitles {
    let mut cues = Vec::new();
    // 块以空行分隔。统一换行符后按双换行切分。
    let normalized = input.replace("\r\n", "\n");
    for block in normalized.split("\n\n") {
        let block = block.trim_matches('\n');
        if block.is_empty() {
            continue;
        }
        let mut lines = block.lines();
        // 第一行是序号, 跳过(容错: 即使缺失也尝试)。
        let first = lines.next();
        // 找到包含 "-->" 的时间行。
        let time_line = if first.map(|l| l.contains("-->")).unwrap_or(false) {
            first
        } else {
            lines.next()
        };
        let Some(time_line) = time_line else { continue };
        let Some((start_s, end_s)) = time_line.split_once("-->") else { continue };
        let (Some(start_ms), Some(end_ms)) =
            (parse_timestamp(start_s), parse_timestamp(end_s)) else { continue };
        let text: Vec<&str> = lines.collect();
        let text = text.join("\n");
        if text.is_empty() {
            continue;
        }
        cues.push(Cue { start_ms, end_ms, text });
    }
    cues.sort_by_key(|c| c.start_ms);
    Subtitles::from_cues(cues)
}
```

- [ ] **Step 5: 在 lib.rs 挂载并导出**

`crates/subtitle/src/lib.rs` 改为:
```rust
//! 字幕解析与查询。首版支持 .srt。纯逻辑, 无系统依赖。
mod model;
mod srt;
pub use model::{Cue, Subtitles};
pub use srt::{parse_srt, parse_timestamp};
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p subtitle`
Expected: PASS, model + srt 全部测试通过。

- [ ] **Step 7: Commit**

```bash
git add crates/subtitle/src/srt.rs crates/subtitle/src/lib.rs
git commit -m "feat(subtitle): 容错的 .srt 解析"
```

---
## Task 8: persist crate — 偏好与续播存储

存储用户偏好(音量、窗口大小)与每个文件的续播位置。用 serde + JSON 序列化到内存中的结构,提供存/取/序列化接口。文件 IO 用注入的路径,测试用 tempdir 真实读写(不 mock,验证序列化往返)。

**Files:**
- Create: `crates/persist/Cargo.toml`
- Create: `crates/persist/src/lib.rs`
- Create: `crates/persist/src/prefs.rs`

- [ ] **Step 1: 创建 persist crate 清单**

`crates/persist/Cargo.toml`:
```toml
[package]
name = "persist"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: 创建 lib.rs**

`crates/persist/src/lib.rs`:
```rust
//! 偏好与播放记忆的本地 JSON 存储。
mod prefs;
pub use prefs::Preferences;
```

- [ ] **Step 3: 写失败测试**

`crates/persist/src/prefs.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let p = Preferences::default();
        assert_eq!(p.volume, 100);
        assert_eq!(p.window_size, (1280, 720));
        assert!(p.resume_point("/any.mp4").is_none());
    }

    #[test]
    fn resume_point_roundtrip() {
        let mut p = Preferences::default();
        p.set_resume_point("/v.mp4", 42_000);
        assert_eq!(p.resume_point("/v.mp4"), Some(42_000));
    }

    #[test]
    fn save_then_load_roundtrips_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");

        let mut p = Preferences::default();
        p.volume = 55;
        p.window_size = (1920, 1080);
        p.set_resume_point("/v.mp4", 12_345);
        p.save(&path).unwrap();

        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.volume, 55);
        assert_eq!(loaded.window_size, (1920, 1080));
        assert_eq!(loaded.resume_point("/v.mp4"), Some(12_345));
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.volume, 100);
    }
}
```

- [ ] **Step 4: 运行测试确认失败**

Run: `cargo test -p persist`
Expected: 编译失败, "cannot find type `Preferences`"。

- [ ] **Step 5: 实现 Preferences**

在 `prefs.rs` 顶部添加:
```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    pub volume: u8,
    pub window_size: (u32, u32),
    /// 文件路径(字符串) → 续播位置(毫秒)。
    resume_points: HashMap<String, u64>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            volume: 100,
            window_size: (1280, 720),
            resume_points: HashMap::new(),
        }
    }
}

impl Preferences {
    pub fn resume_point(&self, file: &str) -> Option<u64> {
        self.resume_points.get(file).copied()
    }

    pub fn set_resume_point(&mut self, file: &str, ms: u64) {
        self.resume_points.insert(file.to_string(), ms);
    }

    /// 从 JSON 文件加载。文件不存在时返回默认值(非错误)。
    pub fn load(path: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(serde_json::from_str(&s)
                .unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// 序列化为 JSON 写入文件。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p persist`
Expected: PASS, 4 个测试通过。

- [ ] **Step 7: Commit**

```bash
git add crates/persist
git commit -m "feat(persist): 偏好与续播位置的 JSON 存储"
```

---

## Task 9: 全量验证与收尾

确认整个 workspace 干净、所有测试通过、无 clippy 警告。

**Files:** 无新增

- [ ] **Step 1: 全量测试**

Run: `cargo test`
Expected: 所有 crate (player-core, sync, subtitle, persist) 测试全部 PASS。

- [ ] **Step 2: Clippy 检查**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 无警告、无错误。若有, 修复后重跑。

- [ ] **Step 3: 格式化**

Run: `cargo fmt --all && cargo fmt --all --check`
Expected: 第二条命令无输出(已格式化)。

- [ ] **Step 4: 提交收尾(若 fmt 有改动)**

```bash
git add -A
git commit -m "style: cargo fmt 全量格式化"
```

---

## 已知缺口 (移交下一计划)

- **`.ass` 字幕解析**: spec 列为首版字幕格式, 本计划仅实现 `.srt`。`.ass` 格式复杂(样式/定位), 放到功能层计划(计划 4)实现。
- **`player-core` 顶层编排**: 本计划实现了状态机、命令、播放列表三个零件, 但把它们组合成"接收 Command → 驱动状态机 + 播放列表 → 广播状态"的顶层 `Player` 编排器, 依赖媒体管线的存在, 放到计划 2/3 接线时实现。
- **`sync` 主时钟的实际推进**: 本计划实现了"给定主时钟位置判定单帧"的纯函数, 主时钟由 `audio` crate(计划 2)在播放时实际推进。

