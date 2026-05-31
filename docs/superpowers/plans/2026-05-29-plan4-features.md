# 计划 4: 功能层 (seek/播放列表/字幕/增强/续播) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在可用播放器之上补齐 spec 首版功能: seek 真正生效、暂停/音量作用于音频、播放列表面板与 Next/Prev、字幕叠加(.srt + .ass)、播放增强(倍速/截图)、续播状态记忆。完成后即达到 spec 首版全部功能。

**Architecture:** 大部分功能是把已有纯逻辑 crate(`subtitle`、`persist`、`player-core::Playlist`)接到 `engine::Player` 与 `app` UI 上,并补齐 `Player` 中计划 3 留空的命令处理。seek 通过重启解码线程到目标时间 + 重置主时钟实现。音量/倍速作用于音频样本喂入环节。

**Tech Stack:** 复用全部已有 crate;新增 `subtitle` 的 .ass 解析、`app` 的截图(image crate)。

**前置依赖:** 计划 1、2、3 已完成(计划 2.5 可选,不阻塞)。

## 文件结构

```
crates/
├── subtitle/src/ass.rs         # 新增: .ass 解析
├── engine/src/
│   ├── player.rs               # 修改: seek/pause/volume/rate/next/prev 实现
│   └── seek.rs                 # 新增: seek 时间→重启解码线程逻辑
├── audio/src/output.rs         # 修改: 音量增益、暂停
├── persist (已有)              # 接线: 打开读续播、退出写续播
└── app/src/
    ├── playlist_panel.rs       # 新增: 右侧播放列表 UI
    ├── subtitle_overlay.rs     # 新增: 字幕叠加绘制
    └── enhance.rs              # 新增: 倍速/截图 UI 与命令
```

---
## Task 1: 音量增益与暂停(audio crate)

音量作用于喂入 cpal 的样本(乘增益);暂停时 cpal 流 pause。

**Files:**
- Modify: `crates/audio/src/output.rs`
- Modify: `crates/audio/src/lib.rs`

- [ ] **Step 1: 写失败测试(增益纯函数)**

`crates/audio/src/output.rs` 追加测试:
```rust
#[cfg(test)]
mod tests {
    use super::apply_gain;

    #[test]
    fn gain_100_is_unchanged() {
        let mut s = [0.5f32, -0.5];
        apply_gain(&mut s, 100);
        assert!((s[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn gain_50_halves() {
        let mut s = [0.8f32, -0.8];
        apply_gain(&mut s, 50);
        assert!((s[0] - 0.4).abs() < 1e-6);
        assert!((s[1] + 0.4).abs() < 1e-6);
    }

    #[test]
    fn gain_0_is_silence() {
        let mut s = [0.9f32, -0.9];
        apply_gain(&mut s, 0);
        assert_eq!(s, [0.0, 0.0]);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p audio output`
Expected: 编译失败, "cannot find function `apply_gain`"。

- [ ] **Step 3: 实现 apply_gain 与暂停控制**

`crates/audio/src/output.rs` 追加(顶层函数):
```rust
/// 把 0..=100 的音量作为线性增益作用于交错样本(就地)。
pub fn apply_gain(samples: &mut [f32], volume: u8) {
    let g = volume.min(100) as f32 / 100.0;
    if (g - 1.0).abs() < f32::EPSILON {
        return;
    }
    for s in samples.iter_mut() {
        *s *= g;
    }
}
```

给 `AudioHandle` 加暂停/恢复(操作 cpal 流):
```rust
impl AudioHandle {
    pub fn pause(&self) {
        use cpal::traits::StreamTrait;
        let _ = self.stream.pause();
    }
    pub fn resume(&self) {
        use cpal::traits::StreamTrait;
        let _ = self.stream.play();
    }
}
```

- [ ] **Step 4: 导出 apply_gain**

`crates/audio/src/lib.rs` 追加: `pub use output::apply_gain;`

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p audio output`
Expected: PASS, 3 个测试通过。

- [ ] **Step 6: Commit**

```bash
git add crates/audio
git commit -m "feat(audio): 音量增益与流暂停/恢复"
```

---

## Task 2: Player 接线 pause/volume

把计划 3 留空的 pause/volume 真正作用到音频。倍速喂样本时应用增益。

**Files:**
- Modify: `crates/engine/src/player.rs`

- [ ] **Step 1: pause/resume 作用于音频流**

`player.rs` 的 `handle`: `Command::Pause` 分支在切状态机后调用 `audio_out.pause()`;`Command::Play` 分支调 `resume()`:
```rust
Command::Pause => {
    if self.machine.apply(player_core::Transition::Pause).is_ok() {
        if let Some(a) = &self.audio_out { a.pause(); }
    }
}
Command::Play => {
    if self.video.is_some()
        && self.machine.apply(player_core::Transition::Play).is_ok()
    {
        if let Some(a) = &self.audio_out { a.resume(); }
    }
}
```

- [ ] **Step 2: 音量增益作用于喂样本线程**

音频解码线程喂样本前调 `apply_gain`。音量需在线程间共享 → 用 `Arc<AtomicU8>`。

`player.rs`: 加字段 `volume_shared: Arc<std::sync::atomic::AtomicU8>,`,`new()` 初始化为 100。`Command::SetVolume(v)` 同时更新 `self.volume` 和 `self.volume_shared.store(v.min(100), Relaxed)`。`open()` 的音频线程闭包捕获 `volume_shared.clone()`,在 `producer.try_push` 前对整块应用增益:
```rust
// 音频线程内, 取到 chunk 后, push 之前:
let mut buf = chunk.samples;
audio::apply_gain(&mut buf, vol_shared.load(Ordering::Relaxed));
let mut i = 0;
while i < buf.len() { /* try_push buf[i] ... */ }
```

- [ ] **Step 3: 验证**

Run: `cargo test -p engine`
Expected: 既有测试仍 PASS。

Run: `cargo run -p app`(人工)
Expected: 拖入有声视频,拖动音量条音量实时变化;点暂停声音停止、画面停住,再播放恢复。

- [ ] **Step 4: Commit**

```bash
git add crates/engine
git commit -m "feat(engine): 暂停作用于音频流, 音量实时生效"
```

---

## Task 3: seek

seek 通过"重启视频/音频解码到目标时间 + 重置主时钟"实现。

**Files:**
- Create: `crates/engine/src/seek.rs`
- Modify: `crates/media/src/decoder.rs` (加 seek 到时间)
- Modify: `crates/media/src/audio_decoder.rs` (加 seek)
- Modify: `crates/engine/src/player.rs`

- [ ] **Step 1: media 解码器支持 seek 到毫秒**

`decoder.rs` 给 `VideoDecoder` 加方法(用 ffmpeg 的 seek + flush 解码器):
```rust
impl VideoDecoder {
    /// seek 到目标毫秒(跳到不晚于该时间的最近关键帧)。
    pub fn seek_ms(&mut self, ms: u64) -> Result<(), MediaError> {
        let ts = (ms as f64 / 1000.0 / self.time_base) as i64;
        // SAFETY: 对有效输入上下文按流时间基 seek。
        unsafe {
            let ret = ffmpeg_sys_next::av_seek_frame(
                self.ictx.as_mut_ptr(),
                self.stream_index as i32,
                ts,
                ffmpeg_sys_next::AVSEEK_FLAG_BACKWARD,
            );
            if ret < 0 {
                return Err(MediaError::NoStream("seek failed"));
            }
        }
        self.decoder.flush(); // 清解码器内部缓冲
        self.eof = false;
        Ok(())
    }
}
```

`audio_decoder.rs` 加对称的 `seek_ms`(结构相同,用其 `ictx`/`stream_index`/`time_base`/`decoder.flush()`)。

注: 实现时确认 ffmpeg-next 的 `decoder::Video` 是否暴露 `flush()`;若无,用 `unsafe { ffmpeg_sys_next::avcodec_flush_buffers(self.decoder.as_mut_ptr()) }`。

- [ ] **Step 2: Player 实现 SeekTo**

seek 时需重启解码线程(因为线程持有 decoder)。简化方案: `SeekTo` 重新 `open` 当前文件并定位。更高效的方案是给 `DecodeThread` 发 seek 信号让其线程内 seek——采用后者:

`decode_thread.rs`: 加 seek 命令通道。`DecodeThread` 加 `seek_tx: Sender<u64>`;解码线程循环每轮先 `try_recv` seek 请求,有则 `decoder.seek_ms()` 并清空发送队列:
```rust
// spawn 内, 解码循环顶部:
if let Ok(target) = seek_rx.try_recv() {
    let _ = decoder.seek_ms(target);
    // 排空旧帧由接收端(Player)负责重置
}
```
`DecodeThread` 加方法 `pub fn request_seek(&self, ms: u64) { let _ = self.seek_tx.send(ms); }`。

`player.rs` 的 `Command::SeekTo(ms)`:
```rust
Command::SeekTo(ms) => {
    if let Some(v) = &self.video { v.request_seek(ms); }
    // 音频: 简化为重启音频线程到 ms(或同样发 seek 信号); 同时重置主时钟
    if let Some(a) = &self.audio_out { a.clock.reset_to(ms); }
    // 音频解码线程的 seek 在 Task 3 Step 3 处理
}
```

- [ ] **Step 3: 音频线程响应 seek**

为简洁与正确(音频是主时钟),音频解码线程也需 seek。给音频线程同样的 seek 通道(`Arc<AtomicU64>` 存目标,`u64::MAX` 表示无请求):音频线程每轮检查,有请求则 `adec.seek_ms(target)` 并清空已推入 ringbuf 的旧样本(通过重建 producer 较复杂,简化: 接受 seek 后短暂的旧样本播放,主时钟已 reset 故视频会快速追上)。在 `player.rs` 的 `SeekTo` 中设置该原子值。

注: 音频 seek 的精确清空缓冲较复杂,本任务采用"重置主时钟 + 音频解码跳转,容忍 ringbuf 中 <1s 旧样本"的简化策略,人工验证 seek 体感可接受即可;若卡顿明显,后续优化为带 flush 的 producer 重建。

- [ ] **Step 4: 验证**

Run: `cargo test`
Expected: 既有测试 PASS。

Run: `cargo run -p app`(人工)
Expected: 拖入较长视频,拖动 seek 条 → 画面跳转到目标位置附近(关键帧),继续播放。前后拖动多次不崩溃。

- [ ] **Step 5: Commit**

```bash
git add crates/media crates/engine
git commit -m "feat: seek 到关键帧并重置主时钟"
```

---
## Task 4: 播放列表 Next/Prev + 右侧面板 UI

`player-core::Playlist` 已就绪;接线 Player 的 Next/Prev 与一个右侧列表 UI。

**Files:**
- Modify: `crates/engine/src/player.rs`
- Create: `crates/app/src/playlist_panel.rs`
- Modify: `crates/app/src/app.rs`, `crates/app/src/main.rs`

- [ ] **Step 1: Player 处理 Open/Next/Prev 与播放列表**

先在 `player-core` 增加 `Command::PlayIndex(usize)` 变体(点击列表项用,比 Open 干净,不会重复入列):
`crates/player-core/src/command.rs` 的 `Command` 枚举加 `PlayIndex(usize),`,并在其测试加一行 `assert_eq!(Command::PlayIndex(2), Command::PlayIndex(2));`。

`player.rs`: `Command::Open(path)` 除打开外,先 `self.playlist.add(path.clone())`。`Next`/`Prev`/`PlayIndex`:
```rust
Command::Next => {
    if let Some(p) = self.playlist.next().map(|p| p.to_path_buf()) {
        self.open(&p);
    }
}
Command::Prev => {
    if let Some(p) = self.playlist.prev().map(|p| p.to_path_buf()) {
        self.open(&p);
    }
}
Command::PlayIndex(i) => {
    self.playlist.set_cursor(i);
    if let Some(p) = self.playlist.current().map(|p| p.to_path_buf()) {
        self.open(&p);
    }
}
```
注: `open` 内**不应**再 add(否则 Next/PlayIndex 触发的 open 会重复添加)。"添加到列表"只放在 `Command::Open` 分支,`open()` 仅负责加载文件。

加只读访问供 UI 渲染列表:
```rust
pub fn playlist_paths(&self) -> Vec<std::path::PathBuf> {
    self.playlist.iter().map(|p| p.to_path_buf()).collect()
}
pub fn current_index(&self) -> Option<usize> { self.playlist.current_index() }
```
这需要 `player-core::Playlist` 暴露 `iter()` 与 `current_index()`——在 `crates/player-core/src/playlist.rs` 加:
```rust
pub fn iter(&self) -> std::slice::Iter<'_, std::path::PathBuf> { self.items.iter() }
pub fn current_index(&self) -> Option<usize> {
    if self.items.is_empty() { None } else { Some(self.cursor) }
}
pub fn set_cursor(&mut self, i: usize) { if i < self.items.len() { self.cursor = i; } }
```
并加单测验证 `iter`/`current_index`/`set_cursor` 行为(3 个断言)。

- [ ] **Step 2: 右侧列表 UI**

`crates/app/src/playlist_panel.rs`:
```rust
use eframe::egui;
use player_core::Command;

/// 绘制右侧播放列表, 返回点击某项产生的命令。
pub fn playlist_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    ui.heading("播放列表");
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, p) in paths.iter().enumerate() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let selected = current == Some(i);
            if ui.selectable_label(selected, name).clicked() {
                cmds.push(Command::PlayIndex(i));
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button("⏮ 上一个").clicked() { cmds.push(Command::Prev); }
        if ui.button("下一个 ⏭").clicked() { cmds.push(Command::Next); }
    });
    cmds
}
```
注: `Command::PlayIndex(usize)` 是本任务新增的变体(下方 Step 说明)。

- [ ] **Step 3: app 接入右侧面板**

`main.rs` 加 `mod playlist_panel;`。`app.rs` 的 `ui()` 在中央面板前加:
```rust
egui::SidePanel::right("playlist").default_width(200.0).show_inside(ui, |ui| {
    let paths = self.player.playlist_paths();
    let cur = self.player.current_index();
    for cmd in crate::playlist_panel::playlist_panel(ui, &paths, cur) {
        self.player.handle(cmd);
    }
});
```

- [ ] **Step 4: 验证**

Run: `cargo test`
Expected: PASS(含新增 playlist / command 测试)。

Run: `cargo run -p app`(人工)
Expected: 拖入多个文件依次出现在右侧列表;点击列表项切换播放;上一个/下一个按钮工作,当前项高亮。

- [ ] **Step 5: Commit**

```bash
git add crates/player-core crates/engine crates/app
git commit -m "feat: 播放列表 Next/Prev/PlayIndex 与右侧面板 UI"
```

---

## Task 5: 字幕 — .ass 解析

补齐 spec 要求的 .ass 字幕(只取时间与文本,忽略高级样式定位,首版够用)。

**Files:**
- Create: `crates/subtitle/src/ass.rs`
- Modify: `crates/subtitle/src/lib.rs`

- [ ] **Step 1: 写失败测试**

`crates/subtitle/src/ass.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dialogue_lines() {
        let input = "\
[Script Info]
Title: test

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Hello there
Dialogue: 0,0:00:03.00,0:00:04.50,Default,,0,0,0,,{\\i1}Styled{\\i0} text
";
        let subs = parse_ass(input);
        assert_eq!(subs.len(), 2);
        assert_eq!(subs.text_at(1500), Some("Hello there"));
        // 样式标签 {..} 应被剥离
        assert_eq!(subs.text_at(3500), Some("Styled text"));
    }

    #[test]
    fn ass_timestamp_to_ms() {
        assert_eq!(parse_ass_time("0:00:01.00"), Some(1000));
        assert_eq!(parse_ass_time("1:02:03.50"), Some(3_723_500));
        assert_eq!(parse_ass_time("bad"), None);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p subtitle ass`
Expected: 编译失败, "cannot find function `parse_ass`"。

- [ ] **Step 3: 实现 .ass 解析**

`crates/subtitle/src/ass.rs` 顶部:
```rust
use crate::model::{Cue, Subtitles};

/// 解析 .ass 时间 "H:MM:SS.cc"(centiseconds)为毫秒。
pub fn parse_ass_time(s: &str) -> Option<u64> {
    let (hms, cs) = s.trim().split_once('.')?;
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let sec: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() { return None; }
    let cs: u64 = cs.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + cs * 10)
}

/// 剥离 .ass 行内样式覆盖标签 {\...}。
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '{' => in_tag = true,
            '}' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // .ass 用 \N 表示换行
    out.replace("\\N", "\n")
}

/// 解析 .ass 文本, 只提取 [Events] 段的 Dialogue 行。
pub fn parse_ass(input: &str) -> Subtitles {
    let mut cues = Vec::new();
    let mut format_fields: Vec<String> = Vec::new();
    let mut in_events = false;

    for line in input.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_events = line.eq_ignore_ascii_case("[Events]");
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Format:") {
            format_fields = rest.split(',').map(|s| s.trim().to_lowercase()).collect();
            continue;
        }
        if let Some(rest) = line.strip_prefix("Dialogue:") {
            // 按 Format 字段数切分; Text 是最后一个且可能含逗号 → 限制 splitn
            let n = format_fields.len().max(10);
            let parts: Vec<&str> = rest.splitn(n, ',').collect();
            if parts.len() < n {
                continue;
            }
            let idx = |name: &str| format_fields.iter().position(|f| f == name);
            let start = idx("start").and_then(|i| parts.get(i)).and_then(|s| parse_ass_time(s.trim()));
            let end = idx("end").and_then(|i| parts.get(i)).and_then(|s| parse_ass_time(s.trim()));
            let text_idx = idx("text").unwrap_or(parts.len() - 1);
            let (Some(start_ms), Some(end_ms)) = (start, end) else { continue };
            let text = strip_tags(parts.get(text_idx).unwrap_or(&"").trim());
            if text.is_empty() { continue; }
            cues.push(Cue { start_ms, end_ms, text });
        }
    }
    cues.sort_by_key(|c| c.start_ms);
    Subtitles::from_cues(cues)
}
```

- [ ] **Step 4: 挂载并导出**

`crates/subtitle/src/lib.rs` 改为:
```rust
//! 字幕解析与查询: .srt 与 .ass。纯逻辑, 无系统依赖。
mod ass;
mod model;
mod srt;
pub use ass::{parse_ass, parse_ass_time};
pub use model::{Cue, Subtitles};
pub use srt::{parse_srt, parse_timestamp};
```

加一个按扩展名分发的便捷函数(可选,放 lib.rs):
```rust
use std::path::Path;
/// 按文件扩展名解析字幕文件。
pub fn load_file(path: &Path) -> std::io::Result<Subtitles> {
    let content = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    Ok(match ext.as_str() {
        "ass" | "ssa" => parse_ass(&content),
        _ => parse_srt(&content),
    })
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p subtitle`
Expected: PASS(srt + ass + model 全部)。

- [ ] **Step 6: Commit**

```bash
git add crates/subtitle
git commit -m "feat(subtitle): .ass 解析与按扩展名分发"
```

---
## Task 6: 字幕叠加显示

加载外挂字幕(拖入 .srt/.ass,或与视频同名自动加载),按当前位置在画面底部叠加文本。

**Files:**
- Modify: `crates/engine/src/player.rs` (持有 Subtitles, 暴露 current_subtitle)
- Create: `crates/app/src/subtitle_overlay.rs`
- Modify: `crates/app/src/app.rs`, `main.rs`

- [ ] **Step 1: Player 持有字幕并按位置查询**

`player.rs`: 加字段 `subtitles: Option<subtitle::Subtitles>,`(`engine/Cargo.toml` 加 `subtitle = { path = "../subtitle" }`)。`open()` 时尝试同名 `.srt`/`.ass` 自动加载:
```rust
// open() 中加载视频后:
self.subtitles = sidecar_subtitle(path);
// 辅助函数:
fn sidecar_subtitle(video: &Path) -> Option<subtitle::Subtitles> {
    for ext in ["srt", "ass", "ssa"] {
        let p = video.with_extension(ext);
        if p.exists() {
            if let Ok(s) = subtitle::load_file(&p) { return Some(s); }
        }
    }
    None
}
```
加查询方法:
```rust
pub fn current_subtitle(&self) -> Option<String> {
    let pos = self.timeline().position_ms;
    self.subtitles.as_ref().and_then(|s| s.text_at(pos)).map(|t| t.to_string())
}
/// 手动加载字幕文件(拖入 .srt/.ass 时)。
pub fn load_subtitle(&mut self, path: &Path) {
    if let Ok(s) = subtitle::load_file(path) { self.subtitles = Some(s); }
}
```

- [ ] **Step 2: 拖放区分视频与字幕**

`controls.rs` 的 `dropped_file_command` 当前只产生 Open。改为按扩展名区分: 字幕扩展名 → 不产生 Open(由 app 单独处理加载字幕)。更简单: 在 `app.rs` 直接处理 dropped_files,按扩展名分派:
```rust
// app.rs ui() 顶部, 替换原 dropped_file_command 调用:
let dropped = ctx.input(|i| i.raw.dropped_files.clone());
for f in dropped {
    if let Some(path) = f.path {
        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
            Some("srt") | Some("ass") | Some("ssa") => self.player.load_subtitle(&path),
            _ => self.player.handle(player_core::Command::Open(path)),
        }
    }
}
```
(移除 `controls::dropped_file_command`,或保留但不再调用。)

- [ ] **Step 3: 字幕叠加绘制**

`crates/app/src/subtitle_overlay.rs`:
```rust
use eframe::egui;

/// 在给定区域底部居中绘制字幕文本(带半透明描边背景)。
pub fn draw_subtitle(ui: &mut egui::Ui, area: egui::Rect, text: &str) {
    if text.is_empty() {
        return;
    }
    let painter = ui.painter_at(area);
    let font = egui::FontId::proportional(24.0);
    let pos = egui::pos2(area.center().x, area.max.y - 40.0);
    // 简单描边: 先画黑色偏移文本, 再画白色主体
    for off in [egui::vec2(1.0, 1.0), egui::vec2(-1.0, 1.0), egui::vec2(1.0, -1.0), egui::vec2(-1.0, -1.0)] {
        painter.text(pos + off, egui::Align2::CENTER_BOTTOM, text,
            font.clone(), egui::Color32::BLACK);
    }
    painter.text(pos, egui::Align2::CENTER_BOTTOM, text,
        font, egui::Color32::WHITE);
}
```

- [ ] **Step 4: 在视频画面上叠加**

`main.rs` 加 `mod subtitle_overlay;`。`video_view.rs` 的 `show` 在画完 `ui.image` 后,取画面 rect 调用叠加:
```rust
// VideoView::show 显示 image 后:
let rect = ui.min_rect();
if let Some(text) = player.current_subtitle() {
    crate::subtitle_overlay::draw_subtitle(ui, rect, &text);
}
```

- [ ] **Step 5: 验证**

Run: `cargo test`
Expected: PASS。

Run: `cargo run -p app`(人工)
Expected: 播放有同名 .srt 的视频 → 字幕在底部按时间出现/消失;拖入一个 .ass 文件 → 切换为该字幕。

- [ ] **Step 6: Commit**

```bash
git add crates/engine crates/app
git commit -m "feat: 字幕叠加显示(自动加载同名 + 拖入)"
```

---

## Task 7: 播放增强(倍速/截图)

**Files:**
- Modify: `crates/engine/src/player.rs`
- Create: `crates/app/src/enhance.rs`
- Modify: `crates/app/src/app.rs`, `main.rs`, `crates/app/Cargo.toml`

- [ ] **Step 1: 倍速 — Player 处理 SetRate**

倍速影响主时钟前进速度。最简实现: 倍速改变音频重采样输出速率不现实,改为"按倍速缩放主时钟读数"。给 `MasterClock` 加倍速因子:
`crates/audio/src/clock.rs` 加字段 `rate: Arc<AtomicU32>`(百分比, 100=1x),`position_ms` 计算时乘 `rate/100`:
```rust
// position_ms 改为:
let base = f * 1000 / self.sample_rate as u64;
base * self.rate.load(Ordering::Relaxed) as u64 / 100
```
加 `set_rate(&self, pct: u16)`。`Player::handle` 的 `SetRate(pct)` 调 `self.audio_out.clock.set_rate(pct)`。
注: 真实倍速还需音频变速(变调或不变调),首版仅做时钟与视频帧节奏的倍速,音频倍速标注为后续优化。给 clock 加 2 个倍速单测(1x 不变、200% 翻倍)。

- [ ] **Step 2: 截图 — 保存当前帧**

`enhance.rs` + `image` crate 把当前显示的 RGBA 帧写 PNG。`VideoView` 缓存最后一帧的 `(rgba, w, h)`;截图按钮调用保存:
`crates/app/Cargo.toml` 加 `image = "0.25"`。
```rust
// enhance.rs
use std::path::PathBuf;
pub fn save_screenshot(rgba: &[u8], w: u32, h: u32) -> std::io::Result<PathBuf> {
    let dir = dirs_screenshot();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("morn-shot-{}.png", now_stamp()));
    image::save_buffer(&path, rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(std::io::Error::other)?;
    Ok(path)
}
fn dirs_screenshot() -> PathBuf { std::env::temp_dir().join("morn-shots") }
fn now_stamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
```
`VideoView` 加 `last_frame: Option<(Vec<u8>, u32, u32)>`,在 `upload` 时缓存。截图按钮从 video_view 取最后一帧调 `save_screenshot`。

- [ ] **Step 5: 增强控件 UI**

`enhance.rs` 加一个绘制函数返回命令: 倍速下拉(0.5/1.0/1.5/2.0)、截图按钮。`controls.rs` 或单独面板调用。`main.rs` 加 `mod enhance;`。

- [ ] **Step 6: 验证**

Run: `cargo test`
Expected: PASS(含新增 clock 倍速、command 变体测试)。

Run: `cargo run -p app`(人工)
Expected: 切换倍速播放速度变化;截图生成 PNG(打印路径,打开确认是当前画面)。

- [ ] **Step 7: Commit**

```bash
git add crates/audio crates/player-core crates/engine crates/app
git commit -m "feat: 倍速/截图"
```

---

## Task 8: 续播与偏好记忆

打开时读 `persist` 的 resume_point 续播;退出/切换时写回。窗口大小/音量持久化。

**Files:**
- Modify: `crates/engine/src/player.rs`
- Modify: `crates/app/src/app.rs`

- [ ] **Step 1: Player 集成 persist**

`engine/Cargo.toml` 加 `persist = { path = "../persist" }`。`Player` 加字段 `prefs: persist::Preferences` 与 `prefs_path: PathBuf`。`new()` 改为 `with_prefs(path)`: `load` 偏好,音量初始化为 `prefs.volume`。
```rust
pub fn with_prefs(prefs_path: PathBuf) -> Self {
    let prefs = persist::Preferences::load(&prefs_path).unwrap_or_default();
    let mut p = Self::new();
    p.volume = prefs.volume;
    p.volume_shared.store(prefs.volume, Ordering::Relaxed);
    p.prefs = prefs;
    p.prefs_path = prefs_path;
    p
}
```

- [ ] **Step 2: 打开时续播**

`open()` 成功后,查该文件 resume_point,若有则 `SeekTo`:
```rust
let key = path.to_string_lossy().to_string();
if let Some(ms) = self.prefs.resume_point(&key) {
    if ms > 3000 { // 太靠前不续播
        // 解码线程已启动, 直接 seek
        if let Some(v) = &self.video { v.request_seek(ms); }
        if let Some(a) = &self.audio_out { a.clock.reset_to(ms); }
    }
}
```

- [ ] **Step 3: 保存续播位置**

加 `pub fn save_state(&mut self)`: 把当前文件与位置写入 prefs 并保存:
```rust
pub fn save_state(&mut self) {
    if let Some(path) = self.playlist.current() {
        let key = path.to_string_lossy().to_string();
        let pos = self.timeline().position_ms;
        if self.duration_ms > 0 && pos + 5000 < self.duration_ms {
            self.prefs.set_resume_point(&key, pos);
        } else {
            // 接近结尾: 清除续播点
            self.prefs.set_resume_point(&key, 0);
        }
    }
    self.prefs.volume = self.volume;
    let _ = self.prefs.save(&self.prefs_path);
}
```

- [ ] **Step 4: app 退出时保存**

`app.rs`: `PlayerApp::new` 用 `Player::with_prefs(prefs_path())`(prefs 路径用 `dirs` crate 或 `std::env` 下的配置目录;简化用 `std::env::temp_dir().join("morn-prefs.json")`,实现时可换标准配置目录)。实现 `eframe::App::on_exit` 或在每次 `Stop`/窗口关闭时调 `save_state`。eframe 0.34 用 `fn save(&mut self, _storage: &mut dyn eframe::Storage)` 周期性调用,可在其中 `self.player.save_state()`:
```rust
fn save(&mut self, _storage: &mut dyn eframe::Storage) {
    self.player.save_state();
}
```

- [ ] **Step 5: 验证**

Run: `cargo test`
Expected: PASS。

Run: `cargo run -p app`(人工)
Expected: 播放视频到中段,关闭程序;重新打开同一文件 → 从上次位置附近续播。音量设置在重启后保留。

- [ ] **Step 6: Commit**

```bash
git add crates/engine crates/app
git commit -m "feat: 续播位置与偏好持久化"
```

---

## Task 9: 文件打开对话框 + 静音

补齐 spec §5 的"文件对话框"打开方式与"静音"。

**Files:**
- Modify: `crates/app/Cargo.toml`, `crates/app/src/controls.rs`, `crates/app/src/app.rs`
- Modify: `crates/engine/src/player.rs`
- Modify: `crates/player-core/src/command.rs`

- [ ] **Step 1: 加 Command::ToggleMute 与 OpenDialog**

`crates/player-core/src/command.rs` 的 `Command` 加两个变体: `ToggleMute,` 和 `OpenDialog,`。command 测试加 `assert_eq!(Command::ToggleMute, Command::ToggleMute);`。

- [ ] **Step 2: Player 处理静音**

`player.rs`: 加字段 `muted: bool` 与 `volume_before_mute: u8`。`ToggleMute`:
```rust
Command::ToggleMute => {
    if self.muted {
        self.muted = false;
        self.handle_set_volume(self.volume_before_mute);
    } else {
        self.muted = true;
        self.volume_before_mute = self.volume;
        self.handle_set_volume(0);
    }
}
```
把现有 `SetVolume` 的逻辑抽到 `fn handle_set_volume(&mut self, v: u8)`(更新 `self.volume` 与 `self.volume_shared`),`SetVolume(v)` 分支调用它并清除 muted 标记。`Timeline` 加 `pub muted: bool` 字段(更新所有 Timeline 构造测试),`timeline()` 填入 `self.muted`。

- [ ] **Step 3: 文件对话框**

`crates/app/Cargo.toml` 加 `rfd = "0.15"`。`OpenDialog` 在 app 层处理(需主线程弹原生对话框):
```rust
// app.rs ui() 中, 处理 controls 返回的命令时, 拦截 OpenDialog:
Command::OpenDialog => {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("视频", &["mp4", "mkv", "webm", "mov", "avi"])
        .pick_file()
    {
        self.player.handle(Command::Open(path));
    }
}
```
(其余命令仍转发给 `self.player.handle`。)

- [ ] **Step 4: 控制栏加按钮**

`controls.rs` 的 `controls_bar`: 加"打开"按钮发 `Command::OpenDialog`;音量旁加静音按钮:
```rust
if ui.button("📂").clicked() { cmds.push(Command::OpenDialog); }
// 音量区:
let mute_icon = if t.muted { "🔇" } else { "🔊" };
if ui.button(mute_icon).clicked() { cmds.push(Command::ToggleMute); }
```

- [ ] **Step 5: 验证**

Run: `cargo test`
Expected: PASS(含新增 command 与更新的 Timeline 测试)。

Run: `cargo run -p app`(人工)
Expected: 点"打开"弹出原生文件选择框,选中视频开始播放;点静音按钮声音切断、图标变化,再点恢复到原音量。

- [ ] **Step 6: Commit**

```bash
git add crates/player-core crates/engine crates/app
git commit -m "feat: 文件打开对话框与静音"
```

---

## Task 10: 内嵌字幕轨道切换

spec §5 要求"内嵌字幕轨道切换"。枚举容器内的文本字幕流(mov_text/ass/subrip),解码选中轨道为 `Subtitles`。位图字幕(PGS/VOBSUB)标注为已知缺口。

**Files:**
- Create: `crates/media/src/subtitle_streams.rs`
- Modify: `crates/media/src/lib.rs`
- Modify: `crates/engine/src/player.rs`, `crates/app/src/controls.rs`
- Modify: `crates/player-core/src/command.rs`

- [ ] **Step 1: 写失败测试(枚举字幕轨道)**

`crates/media/tests/subtitle_streams.rs`:
```rust
use media::list_subtitle_tracks;
use std::path::Path;

#[test]
fn lists_tracks_without_error() {
    // 样本无字幕轨, 应返回空 Vec 而非报错。
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4");
    if !path.exists() { return; }
    let tracks = list_subtitle_tracks(&path).unwrap();
    assert!(tracks.is_empty() || tracks.iter().all(|t| !t.label.is_empty()));
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p media --test subtitle_streams`
Expected: 编译失败, "cannot find function `list_subtitle_tracks`"。

- [ ] **Step 3: 实现轨道枚举**

`crates/media/src/subtitle_streams.rs`:
```rust
use crate::error::MediaError;
use ffmpeg_next as ff;
use ff::media::Type;
use std::path::Path;

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
            let lang = stream
                .metadata()
                .get("language")
                .unwrap_or("und")
                .to_string();
            out.push(SubtitleTrack {
                stream_index: stream.index(),
                label: format!("轨道 {} ({})", stream.index(), lang),
            });
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: 实现文本字幕轨道解码**

在 `subtitle_streams.rs` 追加。解码指定字幕流为 `subtitle::Subtitles`。注: 文本字幕通过 ffmpeg 的 subtitle decoder 得到 `AVSubtitle`,提取 rect 的 `ass`/`text` 字段。这部分涉及 FFI(`ffmpeg-next` 的 subtitle 解码支持有限),实现要点:
```rust
/// 解码指定字幕流为文本 cues。位图字幕返回空(不支持)。
pub fn decode_text_subtitle(
    path: &Path,
    stream_index: usize,
) -> Result<subtitle::Subtitles, MediaError> {
    use subtitle::Cue;
    ff::init()?;
    let mut ictx = ff::format::input(&path)?;
    let params = ictx
        .stream(stream_index)
        .ok_or(MediaError::NoStream("subtitle"))?
        .parameters();
    let time_base = f64::from(
        ictx.stream(stream_index).unwrap().time_base()
    );
    let mut decoder = ff::codec::context::Context::from_parameters(params)?
        .decoder()
        .subtitle()?;

    let mut cues = Vec::new();
    let mut packet = ff::codec::packet::Packet::empty();
    while packet.read(&mut ictx).is_ok() {
        // SAFETY: 读取已初始化包的 stream_index。
        if unsafe { (*packet.as_ptr()).stream_index } as usize != stream_index {
            continue;
        }
        let mut sub = ff::codec::subtitle::Subtitle::default();
        if decoder.decode(&packet, &mut sub).unwrap_or(false) {
            let pts = packet.pts().unwrap_or(0);
            let start_ms = (pts as f64 * time_base * 1000.0).max(0.0) as u64;
            // 持续时间: 用 packet.duration 或 sub 的 end_display_time
            let dur_ms = (packet.duration() as f64 * time_base * 1000.0).max(0.0) as u64;
            let mut text = String::new();
            for rect in sub.rects() {
                if let ff::codec::subtitle::Rect::Text(t) = rect {
                    text.push_str(t.get());
                } else if let ff::codec::subtitle::Rect::Ass(a) = rect {
                    // ASS dialogue 行: 取最后一个逗号后的文本
                    let line = a.get();
                    if let Some(idx) = line.rfind(',') {
                        text.push_str(&line[idx + 1..]);
                    }
                }
                // Rect::Bitmap 不支持(位图字幕), 跳过
            }
            let text = text.trim().to_string();
            if !text.is_empty() && dur_ms > 0 {
                cues.push(Cue { start_ms, end_ms: start_ms + dur_ms, text });
            }
        }
    }
    cues.sort_by_key(|c| c.start_ms);
    Ok(subtitle::Subtitles::from_cues(cues))
}
```
`crates/media/Cargo.toml` 加 `subtitle = { path = "../subtitle" }`。注: `ffmpeg-next` 的 subtitle 解码 API(`decoder().subtitle()`、`Subtitle::rects()`、`Rect` 枚举)需对照 8.1 实际导出验证;若 `decode` 方法签名不同,以实际为准。位图字幕(PGS/VOBSUB)本任务不支持,返回的 cues 为空。

`crates/media/src/lib.rs` 导出:
```rust
mod subtitle_streams;
pub use subtitle_streams::{decode_text_subtitle, list_subtitle_tracks, SubtitleTrack};
```

- [ ] **Step 5: Player + UI 接线轨道切换**

`crates/player-core/src/command.rs` 加 `Command::SelectSubtitleTrack(usize),`(测试加一行)。
`player.rs`: `open()` 后调 `list_subtitle_tracks` 存入 `self.sub_tracks: Vec<media::SubtitleTrack>`;`SelectSubtitleTrack(i)` 调 `decode_text_subtitle(current_path, i)` 存入 `self.subtitles`。暴露 `pub fn subtitle_tracks(&self) -> &[media::SubtitleTrack]`。
`controls.rs`: 若 `subtitle_tracks` 非空,加一个下拉菜单(`egui::ComboBox`)列出轨道,选中发 `Command::SelectSubtitleTrack(stream_index)`。`engine/Cargo.toml` 已含 `media`。

- [ ] **Step 6: 验证**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test`
Expected: PASS(样本无字幕轨,`list_subtitle_tracks` 返回空)。

Run: `cargo run -p app`(人工)
Expected: 打开一个含内嵌文本字幕的 mkv → 控制栏出现字幕轨道下拉 → 切换轨道字幕内容随之改变。位图字幕轨道选中后无文本显示(已知限制)。

- [ ] **Step 7: Commit**

```bash
git add crates/media crates/player-core crates/engine crates/app
git commit -m "feat: 内嵌文本字幕轨道枚举与切换"
```

---

## Task 11: 全量验证与项目收尾

- [ ] **Step 1: 全量测试**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test`
Expected: 计划 1/2/3/4 所有自动化测试 PASS。

- [ ] **Step 2: Clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: 无警告、已格式化。

- [ ] **Step 3: Release 构建与体积检查**

Run: `cargo build --release -p app && ls -lh target/release/morn`
Expected(人工): 记录可执行文件大小。注: 此为开发期动态链接系统 FFmpeg 的体积;spec 的 ~25MB 目标需配合裁剪版 FFmpeg 静态链接 + strip,属于打包阶段(独立于本计划)。在此记录基线大小。

- [ ] **Step 4: spec 首版功能走查(人工)**

对照 spec 第 5 节逐项确认: 打开(对话框/拖拽)、播放/暂停/停止、seek、音量/静音、全屏、播放列表(队列/上下个/面板)、字幕(srt/ass/切换)、倍速、截图、续播、记忆偏好。逐项打勾。

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: 计划4 收尾, spec 首版功能完成"
```

---

## 已知缺口 / 后续(spec 第 9 节, 非首版)

- 音频倍速变速(当前倍速仅作用于时钟与视频节奏, 音频未变速)。
- 位图内嵌字幕(PGS/VOBSUB)显示(Task 10 仅支持文本字幕轨道)。
- 字幕样式自定义、.ass 高级定位/特效。
- 网络流、播放历史界面、缩略图预览、多语言、主题系统。
- 体积优化(裁剪 FFmpeg 静态链接 + strip)作为独立的打包计划。
