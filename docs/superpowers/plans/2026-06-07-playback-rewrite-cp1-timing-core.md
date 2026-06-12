# 播放内核重写 CP1 — 时序核心 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: 用 superpowers:subagent-driven-development 或 superpowers:executing-plans 逐任务实现。步骤用 `- [ ]` 勾选跟踪。
> 配套 spec: `docs/superpowers/specs/2026-06-07-playback-rewrite-design.md`(检查点 CP1)。
> 我(Claude)无法亲见/听 eframe 窗口: 纯逻辑走 TDD; 涉及 GUI/cpal 的集成任务以 `cargo test --workspace` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` 全绿为完成下限, 运行手感由用户在 CP1 检查点验收。

**Goal:** 替换播放时序核心——统一主时钟(音频按真实样本走时 + 无音轨墙钟回退)、把 A/V 选帧从 UI 移进引擎、视频呈现改为播放态连续重绘——在现有解码线程结构上完成, 根治"播几秒卡一下"、音画漂移、纯视频不走时。

**Architecture:** 音频仍是主时钟, 但 cpal 回调**只累计真正取到的样本**(欠载即停, 不空转); 无音轨时用**墙钟**驱动。选帧逻辑从 `app/video_view` 移入 `engine::Player::present_frame()`, 由主时钟决定丢/显/留。`app` 播放态**连续 `request_repaint`**, 只负责把 `present_frame()` 给的帧上传纹理。

**Tech Stack:** Rust, ffmpeg-next, cpal, eframe/egui 0.34, wgpu, crossbeam-channel。

---

## 文件结构

- 修改 `crates/audio/src/output.rs` — cpal 回调只计真实样本(核心同步修复)。
- 新建 `crates/engine/src/wall_clock.rs` — 无音轨时的墙钟(可单测)。
- 新建 `crates/engine/src/play_clock.rs` — 统一时钟封装(音频/墙钟二选一), 供引擎读取播放位置。
- 修改 `crates/sync/src/clock.rs` — 在 `decide_frame` 之上加"选帧推进 + 丢帧上限"纯函数。
- 修改 `crates/engine/src/player.rs` — 新增 `present_frame()`/`current_frame_rgba()`; 引擎持有当前帧并按时钟推进; 用 `PlayClock` 取代静态 `fallback_position_ms`; 暂停冻结时钟。
- 修改 `crates/engine/src/lib.rs` — 导出新模块。
- 修改 `crates/app/src/video_view.rs` — `show()` 改为调 `player.present_frame()` 上传; 移除 UI 侧选帧逻辑。
- 修改 `crates/app/src/app.rs` — 播放态连续 `request_repaint`; 移除基于 `frame_pending` 的重绘分支。

---

## Task 1: 墙钟 WallClock(无音轨走时)

**Files:**
- Create: `crates/engine/src/wall_clock.rs`
- Modify: `crates/engine/src/lib.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/engine/src/wall_clock.rs` 写入(注入时间, 可确定性测试):

```rust
/// 无音频轨时驱动播放位置的墙钟: 位置 = 锚点 + 已过墙钟 × 倍速。暂停冻结, seek 重锚。
pub struct WallClock {
    anchor_ms: u64,
    anchor_at: std::time::Instant,
    rate_pct: u16,
    paused_at: Option<std::time::Instant>,
}

impl WallClock {
    pub fn new() -> Self {
        Self { anchor_ms: 0, anchor_at: std::time::Instant::now(), rate_pct: 100, paused_at: None }
    }

    pub fn position_ms_at(&self, now: std::time::Instant) -> u64 {
        let ref_now = self.paused_at.unwrap_or(now);
        let elapsed = ref_now.saturating_duration_since(self.anchor_at).as_millis() as u64;
        self.anchor_ms + elapsed * self.rate_pct as u64 / 100
    }

    pub fn reset_to_at(&mut self, ms: u64, now: std::time::Instant) {
        self.anchor_ms = ms;
        self.anchor_at = now;
        if self.paused_at.is_some() { self.paused_at = Some(now); }
    }

    pub fn set_rate_at(&mut self, pct: u16, now: std::time::Instant) {
        self.anchor_ms = self.position_ms_at(now);
        self.anchor_at = now;
        self.rate_pct = pct.max(1);
    }

    pub fn pause_at(&mut self, now: std::time::Instant) {
        if self.paused_at.is_none() { self.paused_at = Some(now); }
    }

    pub fn resume_at(&mut self, now: std::time::Instant) {
        if let Some(p) = self.paused_at.take() {
            // 把暂停期间的时长平移到锚点, 保证位置连续。
            self.anchor_at += now.saturating_duration_since(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn advances_with_wall_time_at_rate() {
        let t0 = Instant::now();
        let c = WallClock { anchor_ms: 1000, anchor_at: t0, rate_pct: 100, paused_at: None };
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(500)), 1500);
        let c2 = WallClock { anchor_ms: 0, anchor_at: t0, rate_pct: 200, paused_at: None };
        assert_eq!(c2.position_ms_at(t0 + Duration::from_millis(500)), 1000);
    }

    #[test]
    fn pause_freezes_then_resume_continues() {
        let t0 = Instant::now();
        let mut c = WallClock { anchor_ms: 0, anchor_at: t0, rate_pct: 100, paused_at: None };
        c.pause_at(t0 + Duration::from_millis(400));
        // 暂停后位置冻结在 400, 无论"现在"过去多久。
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(900)), 400);
        c.resume_at(t0 + Duration::from_millis(900));
        // 恢复后从 400 继续: 再过 100ms → 500。
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(1000)), 500);
    }

    #[test]
    fn reset_to_reanchors() {
        let t0 = Instant::now();
        let mut c = WallClock::new();
        c.reset_to_at(5000, t0);
        assert_eq!(c.position_ms_at(t0 + Duration::from_millis(250)), 5250);
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p engine wall_clock`
Expected: 编译失败 / 测试不通过(模块未导出前)。

- [ ] **Step 3: 导出模块**

`crates/engine/src/lib.rs` 加: `mod wall_clock;` 与 `pub use wall_clock::WallClock;`(若 lib.rs 用 `pub mod` 风格则照此)。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p engine wall_clock`
Expected: 3 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/engine/src/wall_clock.rs crates/engine/src/lib.rs
git commit -m "feat(engine): 加无音轨墙钟 WallClock"
```

---

## Task 2: cpal 回调只计真实样本(音画同步核心修复)

**Files:**
- Modify: `crates/audio/src/output.rs`(三个 `build_output_stream` 分支的回调)

- [ ] **Step 1: 改回调计数逻辑**

把每个采样格式分支里"`*slot = consumer.try_pop().unwrap_or(0.0) * g; filled += 1;`"改为**只统计真正取到的样本**:

F32 分支示例(其余 U16/I16 同理, 区别仅在写入 slot 的转换):
```rust
move |data: &mut [f32], _| {
    if flush.swap(false, Ordering::Relaxed) {
        while consumer.try_pop().is_some() {}
    }
    let g = volume.load(Ordering::Relaxed).min(100) as f32 / 100.0;
    let mut real = 0u64;
    for slot in data.iter_mut() {
        match consumer.try_pop() {
            Some(s) => { *slot = s * g; real += 1; }
            None => { *slot = 0.0; } // 欠载补静音, 但不计入时钟
        }
    }
    clock_cb.add_frames(real / ch);
}
```

> 理由: 主时钟按 `add_frames` 累计走时; 只计真实样本后, 欠载/seek 空档时钟自动停住, 不再空转→根治漂移与 seek 后追逐。EOF 不再依赖"时钟≥时长"(CP1 暂保留现有结束判定; CP2/CP4 改显式 EOF)。

- [ ] **Step 2: 加回归测试(MasterClock 欠载停住)**

`crates/audio/src/clock.rs` 的 `#[cfg(test)] mod tests` 末尾加:
```rust
#[test]
fn underrun_does_not_advance_clock() {
    // 回调欠载时应调用 add_frames(0) → 位置不前进(防止视频追逐空转的时钟)。
    let c = MasterClock::new(48_000);
    c.add_frames(48_000); // 1s 真实音频
    assert_eq!(c.position_ms(), 1000);
    c.add_frames(0);      // 一次欠载回调: 不前进
    assert_eq!(c.position_ms(), 1000);
}
```

- [ ] **Step 3: 跑测试 + clippy**

Run: `cargo test -p audio && cargo clippy -p audio --all-targets -- -D warnings`
Expected: 全 PASS, 无告警。

- [ ] **Step 4: 提交**

```bash
git add crates/audio/src/output.rs crates/audio/src/clock.rs
git commit -m "fix(audio): 主时钟只计真实播放样本, 欠载不空转(修音画漂移)"
```

---

## Task 3: 选帧推进纯函数 + 丢帧上限

**Files:**
- Modify: `crates/sync/src/clock.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/sync/src/clock.rs` 末尾(`mod tests` 内)加, 并在文件中实现 `advance_action`:

```rust
// —— 实现(加在 decide_frame 之后)——
/// present 循环对"队列头一帧"的推进动作。drops_so_far 为本次 present 已丢弃帧数,
/// max_drop 为单次上限(防止巨量丢帧时长时间无画面: 达上限则强制显示当前这帧)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvanceAction { Show, DropAndContinue, HoldKeepCurrent }

pub fn advance_action(
    master_ms: u64, frame_pts_ms: u64, tolerance_ms: u64, drops_so_far: u32, max_drop: u32,
) -> AdvanceAction {
    match decide_frame(master_ms, frame_pts_ms, tolerance_ms) {
        FrameDecision::Display => AdvanceAction::Show,
        FrameDecision::Wait { .. } => AdvanceAction::HoldKeepCurrent,
        FrameDecision::Drop => {
            if drops_so_far >= max_drop { AdvanceAction::Show } else { AdvanceAction::DropAndContinue }
        }
    }
}

// —— 测试 ——
#[test]
fn advance_shows_due_frame() {
    assert_eq!(advance_action(1000, 1000, 15, 0, 8), AdvanceAction::Show);
}
#[test]
fn advance_holds_future_frame() {
    assert_eq!(advance_action(1000, 2000, 15, 0, 8), AdvanceAction::HoldKeepCurrent);
}
#[test]
fn advance_drops_late_frame_until_cap() {
    assert_eq!(advance_action(2000, 1000, 15, 0, 8), AdvanceAction::DropAndContinue);
    // 达到丢帧上限: 不再丢, 强制显示以尽快出画面。
    assert_eq!(advance_action(2000, 1000, 15, 8, 8), AdvanceAction::Show);
}
```

- [ ] **Step 2: 跑测试确认先失败再通过**

Run: `cargo test -p sync advance`
Expected: 写实现前 FAIL(`advance_action` 未定义)→ 写好后 4 项 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/sync/src/clock.rs
git commit -m "feat(sync): 加 present 选帧推进 advance_action(含丢帧上限)"
```

---

## Task 4: PlayClock 统一时钟封装

**Files:**
- Create: `crates/engine/src/play_clock.rs`
- Modify: `crates/engine/src/lib.rs`

- [ ] **Step 1: 实现 + 测试**

`crates/engine/src/play_clock.rs`:
```rust
use crate::wall_clock::WallClock;
use audio::MasterClock;

/// 统一播放时钟: 有音轨用音频主时钟(真实样本驱动), 否则用墙钟。供引擎读播放位置。
pub enum PlayClock {
    Audio(MasterClock),
    Wall(WallClock),
}

impl PlayClock {
    pub fn position_ms(&self) -> u64 {
        match self {
            PlayClock::Audio(c) => c.position_ms(),
            PlayClock::Wall(c) => c.position_ms_at(std::time::Instant::now()),
        }
    }
    pub fn reset_to(&mut self, ms: u64) {
        match self {
            PlayClock::Audio(c) => c.reset_to(ms),
            PlayClock::Wall(c) => c.reset_to_at(ms, std::time::Instant::now()),
        }
    }
    pub fn set_rate(&mut self, pct: u16) {
        match self {
            PlayClock::Audio(c) => c.set_rate(pct),
            PlayClock::Wall(c) => c.set_rate_at(pct, std::time::Instant::now()),
        }
    }
    pub fn pause(&mut self) {
        if let PlayClock::Wall(c) = self { c.pause_at(std::time::Instant::now()); }
        // 音频时钟由暂停 cpal 流自然冻结, 无需在此处理。
    }
    pub fn resume(&mut self) {
        if let PlayClock::Wall(c) = self { c.resume_at(std::time::Instant::now()); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_variant_delegates_position() {
        let mc = MasterClock::new(48_000);
        mc.add_frames(48_000);
        let pc = PlayClock::Audio(mc);
        assert_eq!(pc.position_ms(), 1000);
    }
    #[test]
    fn wall_variant_reports_zero_at_start() {
        let pc = PlayClock::Wall(WallClock::new());
        assert!(pc.position_ms() < 50); // 刚建好, 约 0
    }
}
```
`lib.rs` 加 `mod play_clock; pub use play_clock::PlayClock;`。

> 注: `MasterClock` 需可被 engine 持有一份用于读位置。现状 `AudioHandle.clock: MasterClock` 是 `Clone`(内部 Arc)。引擎构造 `PlayClock::Audio(audio_handle.clock.clone())`。

- [ ] **Step 2: 跑测试**

Run: `cargo test -p engine play_clock`
Expected: 2 项 PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/engine/src/play_clock.rs crates/engine/src/lib.rs
git commit -m "feat(engine): PlayClock 统一音频/墙钟时钟来源"
```

---

## Task 5: Player 持有时钟与当前帧, 新增 present_frame()

**Files:**
- Modify: `crates/engine/src/player.rs`

- [ ] **Step 1: 字段与时钟接管**

在 `Player` 结构: 删除 `fallback_position_ms: u64`; 增加
```rust
clock: PlayClock,
current_frame: Option<media::VideoFrame>, // 当前显示帧(跨重绘保留)
pending_frame: Option<media::VideoFrame>, // 已取出但未到点的未来帧
present_drops: u32,                        // 调试/统计用(可留待 CP4 HUD)
```
`new()` 初始化 `clock: PlayClock::Wall(WallClock::new())`, 帧字段 `None`, `present_drops: 0`。
`use crate::play_clock::PlayClock; use crate::wall_clock::WallClock;`。

- [ ] **Step 2: open() 里按是否有音轨建时钟**

`open()` 成功拿到 `audio_out` 时: `self.clock = PlayClock::Audio(handle.clock.clone());`(在 `self.audio_out = Some(handle);` 之前用 clone)。音频启动失败的纯视频分支: `self.clock = PlayClock::Wall(WallClock::new());`。`teardown()` 里复位 `self.clock = PlayClock::Wall(WallClock::new()); self.current_frame=None; self.pending_frame=None;`。

- [ ] **Step 3: raw_position_ms / seek / pause / rate 改走 clock**

- `raw_position_ms(&self)` → `self.clock.position_ms()`(删掉读 audio_out/fallback 的旧实现)。
- `seek_to`: 保留 `request_seek` + 排空队列 + audio_seek/flush; 把 `a.clock.reset_to(target)` 改为 `self.clock.reset_to(target)`; 清 `self.current_frame=None; self.pending_frame=None;`(seek 丢弃旧帧); CP1 暂移除上轮的 `awaiting_seek` 闸门相关代码(seek 精修在 CP2 用 serial 重做), 仅保留暂停音频→恢复的简单路径或直接不暂停(以 CP1 先把同步/卡顿修对为目标)。
- `pause_playback`: 在 `a.pause()` 同时 `self.clock.pause();`。
- `Command::Play` 恢复: `a.resume()` 同时 `self.clock.resume();`。
- `SetRate`: `self.clock.set_rate(pct);`(替代 `a.clock.set_rate`)+ 现有音频 flush。
- `tick()`: 删除上轮 `awaiting_seek` 释放块(CP1 不用); 其余结束判定暂留。

- [ ] **Step 4: 实现 present_frame() 与 current_frame_rgba()**

```rust
const PRESENT_TOL_MS: u64 = 15;
const MAX_DROP_PER_PRESENT: u32 = 8;

/// 按主时钟推进并返回"本次需要新上传的帧"。None=画面不变(沿用上一帧纹理)。
/// 暂停时主时钟冻结, 未来帧被 Hold, 自然保持当前画面。
pub fn present_frame(&mut self) -> Option<&media::VideoFrame> {
    let now = self.clock.position_ms();
    let dt = self.video.as_ref()?;
    let mut changed = false;
    let mut drops = 0u32;
    loop {
        let vf = match self.pending_frame.take() {
            Some(f) => f,
            None => match dt.try_recv_frame() { Some(f) => f, None => break },
        };
        match sync::advance_action(now, vf.pts_ms, PRESENT_TOL_MS, drops, MAX_DROP_PER_PRESENT) {
            sync::AdvanceAction::Show => { self.current_frame = Some(vf); changed = true; break; }
            sync::AdvanceAction::DropAndContinue => { drops += 1; continue; }
            sync::AdvanceAction::HoldKeepCurrent => { self.pending_frame = Some(vf); break; }
        }
    }
    self.present_drops = self.present_drops.saturating_add(drops);
    if changed { self.current_frame.as_ref() } else { None }
}

/// 当前显示帧的 RGBA(供截图)。
pub fn current_frame_rgba(&self) -> Option<(&[u8], u32, u32)> {
    self.current_frame.as_ref().map(|f| (f.rgba.as_slice(), f.width, f.height))
}
```

> 说明: 选帧从 UI 移入引擎。`current_frame` 跨重绘保留 → 暂停/未到点时返回 None, UI 沿用旧纹理(不再每帧重传)。

- [ ] **Step 5: 编译 + 既有测试**

Run: `cargo test -p engine && cargo clippy -p engine --all-targets -- -D warnings`
Expected: 通过(可能需按编译器提示微调借用/导入)。既有 player 测试应仍绿(timeline 位置现由 PlayClock 提供; 纯逻辑测试不依赖真实音频, Wall 时钟起点≈0, 注意涉及位置的测试容差)。

- [ ] **Step 6: 提交**

```bash
git add crates/engine/src/player.rs
git commit -m "feat(engine): 选帧入引擎 present_frame + PlayClock 接管走时/暂停/seek"
```

---

## Task 6: app 集成——present_frame 上传 + 播放态连续重绘

**Files:**
- Modify: `crates/app/src/video_view.rs`
- Modify: `crates/app/src/app.rs`

- [ ] **Step 1: video_view.show 改为消费 present_frame**

`show(&mut self, ui, frame, player: &mut Player)`(签名改 `&mut Player`):
- 删除文件内 UI 侧选帧: `frame_action`/`decide_frame`/`pending`/`recovering_after_seek`/`next_repaint_delay_ms`/`future_frame_repaint_delay_ms`/`last_master_ms` 及相关 `show()` 内取帧循环与 `request_repaint_after`。
- 新 `show()` 主体:
```rust
if let Some(vf) = player.present_frame() {
    self.upload(frame, vf); // upload 改为接 &VideoFrame, 按引用上传(不再 move rgba)
}
// 其余: 用 self.tex_id/self.size 画 fit_rect 纹理; 空态; 字幕叠加(player.current_subtitle())。
```
- `upload(&mut self, frame, vf: &media::VideoFrame)`: 纹理 `ensure_size` 后 `tex.upload(queue, &vf.rgba)`; 删除把 rgba move 进 `last_frame` 的逻辑(截图改走 `player.current_frame_rgba()`)。
- 删除 `VideoView.last_frame` 字段及 `last_frame()`(截图改用 player)。

- [ ] **Step 2: 截图改用 player.current_frame_rgba()**

`app.rs` 中截图处(搜索 `last_frame(` 调用)改为 `self.player.current_frame_rgba()`。保持其余截图逻辑不变。

- [ ] **Step 3: 调用点改 &mut + 连续重绘**

- `app.rs:1183` 改 `self.video_view.show(ui, frame, &mut self.player)`。
- 删除 `app.rs:1599-1604` 基于 `take_frame_pending` 的重绘分支。
- 在 update 末尾(交互判定之后)加: 播放态连续重绘——
```rust
if self.player.timeline().state == player_core::PlaybackState::Playing && !interacting {
    ctx.request_repaint(); // 贴显示器刷新连续呈现; 暂停/停止则自然静止
}
```
- 保留既有交互/缩放/通知相关重绘逻辑。

- [ ] **Step 4: 全量编译 / 测试 / clippy / fmt**

Run:
```bash
cargo fmt --all
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
Expected: 全绿。(video_view 内引用旧选帧逻辑的源码断言测试若失效, 一并更新或删除——它们断言的是被移除的实现细节。)

- [ ] **Step 5: 提交**

```bash
git add crates/app/src/video_view.rs crates/app/src/app.rs
git commit -m "feat(app): 视频呈现改连续重绘 + 由引擎 present_frame 驱动"
```

---

## CP1 检查点(用户验收)

构建运行: `cargo run --release -p app`。请确认:
1. 正常播放**流畅、不再周期性卡住**。
2. **音画基本同步**、不漂移。
3. **暂停/恢复**正常(画面冻结、恢复接上)。
4. 纯视频(无音轨)文件能正常走时播放。

seek 在 CP1 可能仍偏慢(精修留 CP2 用统一 demuxer + serial)。把任何异常或(后续 CP4 的)HUD 数字回贴, 我据此进入 CP2。

---

## 自检(写完计划对照 spec)

- **覆盖**: CP1 spec 项=新时钟(真实样本 T2 + 墙钟 T1/T4)、同步入引擎(T3/T5)、连续重绘呈现(T6)、暂停冻结(T5)。均有任务。seek 精修/统一 demuxer/缓冲池/HUD 明确属 CP2–CP4, 不在本计划。
- **占位扫描**: 无 TBD/TODO; 纯逻辑任务含真实测试代码; 集成任务给出签名与行为, 验证含具体命令。
- **类型一致**: `advance_action`/`AdvanceAction`(sync)、`PlayClock`(engine)、`present_frame`/`current_frame_rgba`(Player)、`WallClock` 方法名在各任务间一致。
- **已知取舍**: CP1 在现有 `DecodeThread`/音频解码线程上做; 移除上轮临时 `awaiting_seek` 闸门(seek 精修在 CP2 重做), 故 CP1 期间 seek 可能偏慢——已在检查点说明。
