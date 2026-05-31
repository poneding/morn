# 计划 6: 字体/布局/播放列表/历史/会话记忆 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`).

**Goal:** 落实用户实机反馈: 主字体改 HelveticaNeue; 播放列表=当前目录(打开视频/文件夹自动导入); 独立持久化的播放历史(右侧栏 列表/历史 切换); 启动恢复上次列表+选中(不自动播放); 控制栏单行自动换行; 设置与播放列表开关移到最右; 音量改为点击弹出竖向滑块; 调查顶边拉伸卡顿。

**Architecture:** 设置/历史/列表持久化在 `persist::Preferences`; 纯逻辑(set_items/push_history/is_video_ext)在 `player-core`/`engine` 可单测; 目录扫描在 `engine`; UI 全在 `app`。

**Tech Stack:** eframe/egui 0.34(`horizontal_wrapped`/`menu_button`/`Slider::vertical`/`SidePanel`), rfd 0.15(`pick_folder`), rust-i18n。

**前置依赖:** 计划 1–5 已完成(master)。

## 已验证事实
- `/System/Library/Fonts/HelveticaNeue.ttc`(4.3M, 静态 .ttc, index 0 可加载)。SF Pro(SFNS.ttf)是可变字体, egui 渲染不了, 放弃。
- egui 0.34: `Slider::new(&mut v, lo..=hi).vertical()`; `Ui::horizontal_wrapped(|ui| {})`; `ui.menu_button(label, |ui| {})`(点击弹出菜单, 内放控件)。
- rfd 0.15: `FileDialog::new().pick_folder() -> Option<PathBuf>`。
- `persist::Preferences` 现有字段: volume/window_size/language/seek_step_secs/theme/subtitle_font_size + 私有 resume_points; `#[serde(default)]`。
- `player-core::Playlist`: items/cursor + new/len/is_empty/add/current/next/prev/iter/current_index/set_cursor。
- `player-core::Command` 现有变体 … SelectSubtitleTrack(usize)。
- `engine::Player`: prefs()/set_*; open(path) 私有(teardown+spawn, 末尾 resume+清 loop); handle(Open) 现为 `playlist.add + set_cursor(last) + open`; with_prefs/save_state 已有。

---

## Task 1: 主字体改 HelveticaNeue

**Files:** Modify `crates/app/src/font.rs`

- [ ] **Step 1: 改 macOS UI 候选**

把 `#[cfg(target_os = "macos")]` 的 `UI_FONTS` 改为:
```rust
const UI_FONTS: &[&str] = &[
    "/System/Library/Fonts/HelveticaNeue.ttc",
    "/System/Library/Fonts/Helvetica.ttc",
];
```
(去掉 `SFNS.ttf`。CJK 候选与 Windows/Linux 不变。)

- [ ] **Step 2: 验证**

Run: `cargo build -p app && cargo clippy -p app --all-targets -- -D warnings`
Expected: clean。(字体观感人工验证。)

- [ ] **Step 3: Commit**
```bash
git add crates/app/src/font.rs
git commit -m "fix(app): 主 UI 字体改用 HelveticaNeue(SF Pro 可变字体无法加载)"
```

---

## Task 2: persist 增加 last_playlist/last_index/history

**Files:** Modify `crates/persist/src/prefs.rs`

- [ ] **Step 1: 写失败测试**

追加到 `#[cfg(test)] mod tests`:
```rust
#[test]
#[allow(clippy::field_reassign_with_default)]
fn playlist_history_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    let mut p = Preferences::default();
    p.last_playlist = vec!["/a.mp4".into(), "/b.mp4".into()];
    p.last_index = 1;
    p.history = vec!["/b.mp4".into(), "/a.mp4".into()];
    p.save(&path).unwrap();
    let loaded = Preferences::load(&path).unwrap();
    assert_eq!(loaded.last_playlist, vec!["/a.mp4".to_string(), "/b.mp4".to_string()]);
    assert_eq!(loaded.last_index, 1);
    assert_eq!(loaded.history, vec!["/b.mp4".to_string(), "/a.mp4".to_string()]);
}

#[test]
fn playlist_history_defaults_empty() {
    let p = Preferences::default();
    assert!(p.last_playlist.is_empty());
    assert_eq!(p.last_index, 0);
    assert!(p.history.is_empty());
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p persist` → 编译失败(no field `last_playlist`)。

- [ ] **Step 3: 加字段**

`Preferences` 结构体加(`subtitle_font_size` 后, `resume_points` 前):
```rust
    pub last_playlist: Vec<String>,
    pub last_index: usize,
    pub history: Vec<String>,
```
`Default` 加:
```rust
            last_playlist: Vec::new(),
            last_index: 0,
            history: Vec::new(),
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p persist` → PASS。

- [ ] **Step 5: Commit**
```bash
git add crates/persist
git commit -m "feat(persist): 持久化播放列表/索引/播放历史"
```

---

## Task 3: player-core — set_items / push_history / Command::OpenFolder

**Files:** Modify `crates/player-core/src/playlist.rs`, `crates/player-core/src/command.rs`, `crates/player-core/src/lib.rs`

- [ ] **Step 1: 写失败测试**

`playlist.rs` 测试模块加:
```rust
#[test]
fn set_items_replaces_and_sets_cursor() {
    let mut pl = Playlist::new();
    pl.add(p("/old.mp4"));
    pl.set_items(vec![p("/a.mp4"), p("/b.mp4"), p("/c.mp4")], 2);
    assert_eq!(pl.len(), 3);
    assert_eq!(pl.current(), Some(&p("/c.mp4")));
    // 越界 cursor 收敛到末尾
    pl.set_items(vec![p("/x.mp4")], 99);
    assert_eq!(pl.current(), Some(&p("/x.mp4")));
}
```
新建 `crates/player-core/src/history.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::push_history;

    #[test]
    fn pushes_to_front_dedups_and_caps() {
        let mut h: Vec<String> = vec![];
        push_history(&mut h, "/a", 3);
        push_history(&mut h, "/b", 3);
        push_history(&mut h, "/a", 3); // 重复 → 置顶去重
        assert_eq!(h, vec!["/a".to_string(), "/b".to_string()]);
        push_history(&mut h, "/c", 3);
        push_history(&mut h, "/d", 3); // 超上限 3 → 截断
        assert_eq!(h, vec!["/d".to_string(), "/c".to_string(), "/a".to_string()]);
    }
}
```
`command.rs` 测试加: `assert_eq!(Command::OpenFolder, Command::OpenFolder);`

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p player-core` → 编译失败。

- [ ] **Step 3: 实现**

`playlist.rs` 加方法:
```rust
/// 用新条目替换整个列表, cursor 收敛到 [0, len)。
pub fn set_items(&mut self, items: Vec<std::path::PathBuf>, cursor: usize) {
    self.cursor = if items.is_empty() { 0 } else { cursor.min(items.len() - 1) };
    self.items = items;
}
```
`history.rs` 顶部(测试模块之上):
```rust
/// 把 `path` 记入历史: 去重(移除已存在)、插到队首、截断到 `cap`。
pub fn push_history(history: &mut Vec<String>, path: &str, cap: usize) {
    history.retain(|p| p != path);
    history.insert(0, path.to_string());
    history.truncate(cap);
}
```
`command.rs` 的 `Command` 枚举加 `OpenFolder,`。
`lib.rs` 加 `mod history; pub use history::push_history;`(确认 `Playlist`/`Command` 已 `pub use`)。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p player-core` → PASS。

- [ ] **Step 5: Commit**
```bash
git add crates/player-core
git commit -m "feat(player-core): Playlist::set_items / push_history / OpenFolder 命令"
```

---

## Task 4: engine — 目录扫描 + 播放列表/历史/会话接线

**Files:** Modify `crates/engine/src/player.rs`

- [ ] **Step 1: 写失败测试(目录扫描)**

`player.rs` 测试模块加:
```rust
#[test]
fn is_video_ext_filters() {
    assert!(super::is_video_ext(std::path::Path::new("/x/a.mp4")));
    assert!(super::is_video_ext(std::path::Path::new("/x/a.MKV")));
    assert!(!super::is_video_ext(std::path::Path::new("/x/a.txt")));
    assert!(!super::is_video_ext(std::path::Path::new("/x/a")));
}

#[test]
fn sibling_videos_lists_sorted() {
    let dir = std::env::temp_dir().join(format!("morn_sib_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["b.mp4", "a.mp4", "note.txt", "c.mkv"] {
        std::fs::write(dir.join(n), b"x").unwrap();
    }
    let target = dir.join("a.mp4");
    let got = super::sibling_videos(&target);
    let names: Vec<_> = got.iter().map(|p| p.file_name().unwrap().to_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["a.mp4", "b.mp4", "c.mkv"]); // 排序, 排除 .txt
    std::fs::remove_dir_all(&dir).ok();
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p engine is_video_ext` → 编译失败。

- [ ] **Step 3: 实现目录扫描(模块级自由函数)**

`player.rs` 加(与 `probe_duration_ms` 并列):
```rust
const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "m4v", "flv", "ts"];

fn is_video_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// 返回 `video` 所在目录下所有视频(按文件名排序)。读失败/无目录则返回 [video]。
fn sibling_videos(video: &Path) -> Vec<std::path::PathBuf> {
    let Some(dir) = video.parent() else { return vec![video.to_path_buf()] };
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_ext(p))
        .collect();
    if out.is_empty() {
        return vec![video.to_path_buf()];
    }
    out.sort();
    out
}

fn dir_videos(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_video_ext(p))
        .collect();
    out.sort();
    out
}
```

- [ ] **Step 4: Open 改为目录导入 + 记历史**

`handle(Command::Open(path))` 分支改为:
```rust
Command::Open(path) => {
    let items = sibling_videos(&path);
    let idx = items.iter().position(|p| *p == path).unwrap_or(0);
    self.playlist.set_items(items, idx);
    self.open(&path);
}
```
在 `open(&mut self, path: &Path)` 成功加载后(`self.video = Some(video);` 附近)记历史:
```rust
player_core::push_history(&mut self.prefs.history, &path.to_string_lossy(), 50);
```
(`open` 被 Next/Prev/PlayIndex/Open 共用, 都会记历史 — 合理。)

- [ ] **Step 5: OpenFolder + 启动恢复 + save_state + accessor**

新增公开方法处理打开文件夹:
```rust
pub fn open_folder(&mut self, dir: &Path) {
    let items = dir_videos(dir);
    if let Some(first) = items.first().cloned() {
        self.playlist.set_items(items, 0);
        self.open(&first);
    }
}
```
`handle` 加 `Command::OpenFolder => {}`(空 — 由 app 层弹目录对话框后调 `open_folder`, 保持 match 穷尽)。
`with_prefs` 中 `p.prefs = prefs;` 之后, 恢复列表(不自动 open):
```rust
if !p.prefs.last_playlist.is_empty() {
    let items: Vec<std::path::PathBuf> = p.prefs.last_playlist.iter().map(Into::into).collect();
    let idx = p.prefs.last_index;
    p.playlist.set_items(items, idx);
}
```
`save_state` 中(写 prefs 前)加:
```rust
self.prefs.last_playlist = self.playlist.iter().map(|p| p.to_string_lossy().to_string()).collect();
self.prefs.last_index = self.playlist.current_index().unwrap_or(0);
```
加历史 accessor:
```rust
pub fn history(&self) -> &[String] {
    &self.prefs.history
}
```

- [ ] **Step 6: 运行验证**

Run: `cargo test -p engine` → PASS(含 is_video_ext/sibling_videos)。
Run: `cargo clippy -p engine --all-targets -- -D warnings` → clean。

- [ ] **Step 7: Commit**
```bash
git add crates/engine
git commit -m "feat(engine): 目录导入播放列表 + 播放历史 + 启动恢复列表"
```

---

## Task 5: app 控制栏重构(单行换行 / 音量弹出 / 设置最右 / 打开文件夹)

**Files:** Modify `crates/app/src/controls.rs`, `crates/app/src/enhance.rs`(若需), `crates/app/src/app.rs`, `crates/app/locales/app.yml`

- [ ] **Step 1: 加 i18n key**

`crates/app/locales/app.yml` 加:
```yaml
open_folder:
  en: Open Folder
  zh-CN: 打开文件夹
  zh-TW: 開啟資料夾
history:
  en: History
  zh-CN: 历史
  zh-TW: 歷史
```

- [ ] **Step 2: 音量改为点击弹出竖向滑块**

`controls.rs` 的 `controls_bar` 中, 把现有水平音量 `Slider` + 静音按钮区域替换为一个弹出:
```rust
// 替换原 `egui::Slider::new(&mut vol, 0.0..=100.0).text("🔊")` 区块:
let vol_icon = if t.muted || t.volume == 0 { "🔇" } else { "🔊" };
ui.menu_button(vol_icon, |ui| {
    let mut vol = t.volume as f32;
    if ui.add(egui::Slider::new(&mut vol, 0.0..=100.0).vertical()).changed() {
        cmds.push(Command::SetVolume(vol as u8));
    }
    if ui.button(t!("mute_toggle").to_string()).clicked() {
        cmds.push(Command::ToggleMute);
    }
});
```
(原独立静音按钮可移除, 并入弹出。`use rust_i18n::t;` 已在 controls.rs。)

- [ ] **Step 3: 控制栏单行自动换行 + 设置/打开文件夹**

在 `app.rs` 底部面板, 把现有 `controls_bar` + `enhance_bar` + 字幕轨 combo + ⚙ 全部包进一个 `ui.horizontal_wrapped`, 并把 ⚙ 放最后(最右); 加"打开文件夹"📁 按钮:
```rust
egui::Panel::bottom("controls").show_inside(ui, |ui| {
    ui.horizontal_wrapped(|ui| {
        for cmd in controls::controls_bar(ui, &t) { /* 同现有: 拦截 OpenDialog, 其余 handle */ }
        // 打开文件夹
        if ui.button("📁").on_hover_text(t!("open_folder").to_string()).clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.player.open_folder(&dir);
            }
        }
        let actions = crate::enhance::enhance_bar(ui, self.rate_pct);
        for cmd in actions.commands { /* 同现有 */ }
        if actions.screenshot { /* 同现有 */ }
        let tracks = self.player.subtitle_tracks().to_vec();
        if !tracks.is_empty() {
            if let Some(cmd) = controls::subtitle_track_combo(ui, &tracks) { self.player.handle(cmd); }
        }
        if ui.button("⚙").on_hover_text(t!("settings").to_string()).clicked() {
            self.show_settings = !self.show_settings;
        }
    });
});
```
(注: `horizontal_wrapped` 内顺序绘制, 窄窗自动换行; ⚙ 在最后, 视觉上落在行尾。若要严格"最右"可后续用右对齐布局, 本版顺序最后即可。`OpenFolder` 命令变体保留供未来键盘/菜单触发, app 这里直接调 `open_folder`。)

- [ ] **Step 4: 验证**

Run: `cargo build -p app && cargo clippy --all-targets -- -D warnings` → clean。(布局/弹出人工验证。)

- [ ] **Step 5: Commit**
```bash
git add crates/app
git commit -m "feat(app): 控制栏单行自动换行/音量竖向弹出/打开文件夹/设置最右"
```

---

## Task 6: app 右侧栏 列表/历史 切换

**Files:** Modify `crates/app/src/app.rs`, `crates/app/src/playlist_panel.rs`

- [ ] **Step 1: app 加 sidebar_tab 状态**

`app.rs` 加枚举与字段:
```rust
#[derive(PartialEq)]
enum SidebarTab { Playlist, History }
```
`PlayerApp` 加 `sidebar_tab: SidebarTab`(`new()` 初始化 `SidebarTab::Playlist`)。

- [ ] **Step 2: 右侧面板加 tab + 历史列表**

把现有 `egui::Panel::right("playlist")` 闭包改为:
```rust
egui::Panel::right("sidebar").default_size(200.0).show_inside(ui, |ui| {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut self.sidebar_tab, SidebarTab::Playlist, t!("playlist").to_string());
        ui.selectable_value(&mut self.sidebar_tab, SidebarTab::History, t!("history").to_string());
    });
    ui.separator();
    match self.sidebar_tab {
        SidebarTab::Playlist => {
            let paths = self.player.playlist_paths();
            let cur = self.player.current_index();
            for cmd in crate::playlist_panel::playlist_panel(ui, &paths, cur) {
                self.player.handle(cmd);
            }
        }
        SidebarTab::History => {
            let hist: Vec<std::path::PathBuf> = self.player.history().iter().map(Into::into).collect();
            if let Some(cmd) = crate::playlist_panel::history_panel(ui, &hist) {
                self.player.handle(cmd);
            }
        }
    }
});
```
(注: `playlist_panel` heading 现含 "播放列表" — 因为顶部已有 tab, 把 `playlist_panel` 内的 `ui.heading(...)` 去掉避免重复; 或保留也可, 实现时取整洁者。)

- [ ] **Step 3: history_panel**

`playlist_panel.rs` 加:
```rust
/// 绘制历史列表, 点击某项返回 Open 命令。
pub fn history_panel(ui: &mut egui::Ui, paths: &[std::path::PathBuf]) -> Option<Command> {
    let mut cmd = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in paths {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            if ui.selectable_label(false, name)
                .on_hover_text(p.to_string_lossy().to_string())
                .clicked()
            {
                cmd = Some(Command::Open(p.clone()));
            }
        }
    });
    cmd
}
```

- [ ] **Step 4: 验证**

Run: `cargo build -p app && cargo clippy --all-targets -- -D warnings` → clean。

- [ ] **Step 5: Commit**
```bash
git add crates/app
git commit -m "feat(app): 右侧栏 列表/历史 切换与历史面板"
```

---

## Task 7: 顶边拉伸卡顿调查(尽力, 不阻塞)

**Files:** 可能 Modify `crates/app/src/app.rs`

- [ ] **Step 1: 定位**

通读 `app.rs ui()` 与 `video_view.rs`。重点怀疑: ① `ctx.request_repaint_after(16ms)` 每帧重绘; ② `video_view::upload` 每帧 `vf.rgba.clone()`(大帧昂贵)。判断顶边 live-resize 时是否触发额外重绘/重建。

- [ ] **Step 2: 尝试缓解(取有效者, 任一即可)**

方案 a: 把 `ctx.request_repaint_after(Duration::from_millis(16))` 改为仅在播放中请求:
```rust
if t.state == player_core::PlaybackState::Playing {
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
}
```
(暂停/停止时不强制 60fps 重绘, 减少 live-resize 期间的重绘压力。)

方案 b(若 a 无明显改善): 评估 `last_frame` 仅在截图待触发时才 clone(用一个 `Cell<bool>` 标志), 避免每帧 clone 大 RGBA。

- [ ] **Step 3: 验证 + 诚实记录**

Run: `cargo build -p app && cargo clippy --all-targets -- -D warnings`。
人工: 顶边拉伸是否改善。**若缓解无效**: 在本任务 commit message 或 plan 备注中记录"顶边 live-resize 卡顿疑为 winit/eframe/macOS 上游限制, 应用层缓解有限", 不强行 hack。

- [ ] **Step 4: Commit(若有改动)**
```bash
git add crates/app
git commit -m "perf(app): 暂停时停止强制重绘以缓解拉伸卡顿"
```
(若结论是无法缓解, 跳过 commit, 在最终汇报里说明。)

---

## Task 8: 全量验证

- [ ] **Step 1: 全量测试**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test`
Expected: 全 PASS(含 persist round-trip、player-core set_items/push_history、engine is_video_ext/sibling_videos)。

- [ ] **Step 2: clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: clean。

- [ ] **Step 3: 人工走查(用户)**

`cargo run -p app`: ① 字体是 HelveticaNeue 观感; ② 打开一个视频 → 同目录视频进列表、当前项选中; ③ 📁 打开文件夹 → 整目录进列表; ④ 右侧栏 列表/历史 切换, 历史显示最近播放、点击重开; ⑤ 重启 → 恢复上次列表+选中(不自动播放); ⑥ 控制栏窄窗换行; ⑦ 播放列表开关与 ⚙ 在最右; ⑧ 音量点击弹竖向滑块; ⑨ 顶边拉伸是否改善(或确认仍有限)。

- [ ] **Step 4: Commit(收尾, 若有)**
```bash
git add -A && git commit -m "chore: 计划6 收尾"
```
(工作树干净则跳过。)

---

## 已知限制 / 后续
- HelveticaNeue 非 SF Pro(SF Pro 技术上无法用); 若要更接近可后续内嵌 Inter。
- 顶边拉伸卡顿或为上游限制(见 Task 7)。
- 历史无清除 UI、无大小写/软链去重之外的去重; 上限固定 50。
