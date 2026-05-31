# 设计: 计划 6 — 字体/布局/播放列表/会话记忆 (运行反馈修订)

> 状态: 关键决策已与用户对齐, 待评审。完成后转 writing-plans。
> 来源: 用户实机运行计划 5 成果后的反馈。

## 目标(7 项)

**Bug**
1. 主 UI 字体改 **HelveticaNeue**(SF Pro 是可变字体, egui/ab_glyph 渲染不了, 已回退 → 放弃 SFNS.ttf)。
2. 窗口**顶边拉伸卡顿**(左/右/中拉伸正常)— 调查并尽力缓解。

**功能**
3. **会话记忆**: 持久化整个播放列表(及当前项), 启动恢复。
4+5. **播放列表自动化(两者都做)**: ① 打开/拖入任一视频 → 自动扫描其所在目录的所有视频按文件名排序作为播放列表, 当前项定位到打开的那个; ② 另加"打开文件夹"入口, 选目录后导入其中全部视频。

**UI/UX**
6. 播放控制**单行 + 窗口过窄自动换行**。
7. **设置 ⚙ 移到控制栏最右**。
8. **音量改为点击弹出竖向滑块**。

## 已确认的关键决策

- **A. 启动行为**: 恢复"当前播放列表(上次的目录)+ 选中上次的视频", **不自动打开/播放**。视频区仍显示拖入提示, 用户点列表项或按播放才打开并续播到上次位置。
- **B. 播放列表 vs 历史(用户明确)**: 引入**两个独立概念** —
  - **当前播放列表 (Playlist)**: 始终代表"当前视频所在目录"。打开/拖入一个视频 → 自动替换为该目录的视频; "打开文件夹" → 设为该目录。是临时工作队列。
  - **播放历史 (History)**: **独立持久化**的"最近播放过的视频"列表(最近优先、去重、上限 ~50), 与播放列表隔离。每次打开一个视频就记入历史(续播位置复用已有 resume_points)。点击历史项 → 打开它(于是播放列表变成它的目录)。
- **历史 UI**: 右侧栏顶部加 **"列表 / 历史" 切换(tab)**, 同一区域切换显示当前播放列表或历史。

## 关键事实(已对照本机/已装 crate 验证)

- `/System/Library/Fonts/HelveticaNeue.ttc`(4.3M, 静态 .ttc, index 0 可加载)。SFNS.ttf 是可变字体, 放弃。
- egui 0.34: `Slider::vertical()`(slider.rs:215); `Ui::horizontal_wrapped`(ui.rs:2611); 弹出可用 `ui.menu_button(label, |ui| {...})`(点击展开, 内放竖向 slider)或 `egui::Popup`/`popup_below_widget`。实现时以能编译且行为正确者为准。
- rfd 0.15: `FileDialog::new().pick_folder() -> Option<PathBuf>`。
- `persist::Preferences` 已 `#[serde(default)]`, 加字段不破坏旧文件。

## 模块设计

### 1. 字体 (`crates/app/src/font.rs`)
macOS 的 `UI_FONTS` 候选改为 `["/System/Library/Fonts/HelveticaNeue.ttc", "/System/Library/Fonts/Helvetica.ttc"]`(去掉 SFNS.ttf)。其余(CJK 候选、Windows/Linux)不变。其它逻辑不变。

### 2. 顶边拉伸卡顿调查 (`crates/app`)
先定位: 是否 `ctx.request_repaint_after(16ms)` 持续重绘 + macOS 顶边 live-resize 交互所致。尝试缓解(按优先级, 取有效者):
- 仅在"播放中"或有动画时才 `request_repaint_after`, 否则按需重绘(`ctx.request_repaint()` 由事件驱动);
- 检查是否每帧无谓重建纹理/克隆大数据(video_view 的 `last_frame` 每帧 clone RGBA 已知偏重 — 评估是否参与卡顿)。
**诚实声明**: 顶边 live-resize 卡顿可能是 winit/eframe/macOS 上游限制; 若缓解无效, 记录为已知限制, 不强行 hack。此项**不阻塞**其它任务。

### 3. 会话记忆 + 播放历史 (`crates/persist`, `crates/player-core`, `crates/engine`, `crates/app`)
- `Preferences` 加: `last_playlist: Vec<String>`(默认空)、`last_index: usize`(默认 0)、`history: Vec<String>`(默认空, 最近优先、去重、上限 50)。round-trip 单测。
- `player-core::Playlist` 加 `set_items(items: Vec<PathBuf>, cursor: usize)`(供恢复/目录导入)+ 单测。
- `player-core` 加纯函数 `push_history(history: &mut Vec<String>, path: &str, cap: usize)`: 去重(移除已存在)、插到队首、截断到 cap。单测(去重、置顶、上限)。
- `engine::Player::with_prefs`: 若 `last_playlist` 非空, `playlist.set_items(...)`, cursor=`last_index.min(len-1)`。**不自动 open**(决策 A)。历史从 `prefs.history` 载入(只读副本供 UI)。
- `engine::Player`: 每次 `open(path)` 成功后调 `push_history(&mut self.prefs.history, key, 50)`。
- `engine::Player::save_state`: 写回 `last_playlist`(当前 playlist items)、`last_index`、`history`(连同已有 volume/resume/settings)。
- 暴露 `pub fn history(&self) -> &[String]` 供 UI。

### 4. 播放列表自动化 (`crates/engine`, `crates/player-core`, `crates/app`)
- 新增 `engine::sibling_videos(path: &Path) -> Vec<PathBuf>`: 读取 `path` 父目录, 过滤视频扩展名(mp4/mkv/webm/mov/avi/m4v/flv/ts 等, 抽 `is_video_ext` 纯函数 + 单测), 按文件名排序。空/读失败返回仅含 `path` 自身的 Vec(降级)。
- `Player::handle(Command::Open(path))` 改为: `let vids = sibling_videos(&path); playlist.set_items(vids, index_of(path)); self.open(&path);`(替换 — 判断点 B 默认)。
- 新增 `player_core::Command::OpenFolder`(单元变体 + 测试)。`app` 层拦截(像 OpenDialog 一样): `rfd::FileDialog::new().pick_folder()` → 若选中, 扫描该目录视频 → `player.handle_open_folder(dir)`(engine 加方法: set_items(scan(dir), 0) 并 open 第一个; 空目录则忽略)。
- 控制栏/设置区加"打开文件夹"按钮(📁)发 `OpenFolder`。
- **右侧栏 列表/历史 切换**: `PlayerApp` 加 `sidebar_tab: SidebarTab { Playlist, History }`(默认 Playlist)。右侧 `SidePanel` 顶部用 `ui.selectable_value` 画两个 tab(`t!("playlist")` / `t!("history")`); 选中 Playlist 时画现有 `playlist_panel`, 选中 History 时画历史列表(`player.history()`, 每项 `selectable_label(false, 文件名)`, 点击发 `Command::Open(path)` → 打开并把播放列表设为其目录)。历史项可显示完整路径作 hover。

### 5. 控制栏单行自动换行 (`crates/app/src/controls.rs`, `enhance.rs`, `app.rs`)
- 把底部面板内的 `controls_bar` + `enhance_bar` + 字幕轨 combo + ⚙ 合并到**一个 `ui.horizontal_wrapped(|ui| {...})`** 中, 窗口窄时自动换行。
- 现有 `controls_bar`/`enhance_bar` 改为接收同一个 `ui`(在 wrapped 容器内顺序绘制), 或 app.rs 直接在 wrapped 容器内调用两者。保持返回命令的方式不变。

### 6. ⚙ 移到最右 (`crates/app/src/app.rs`)
- 在 wrapped 控制行中, ⚙ 放到**最后**绘制(配合换行, 它会落在行尾/最右)。或用 `ui.with_layout(Layout::right_to_left, ...)` 把 ⚙ 钉到最右。实现时取视觉合理者; 默认: 顺序上放最后。

### 7. 音量竖向滑块弹出 (`crates/app/src/controls.rs`)
- 把当前内联的水平音量 `Slider` 换成一个 🔊/🔇 按钮; 点击用 `ui.menu_button("🔊…", |ui| { ui.add(Slider::new(&mut vol, 0..=100).vertical()); })` 弹出竖向滑块。滑块改变发 `SetVolume`。静音按钮(已存在的 ToggleMute)可并入此弹出或保留。
- 图标按 `t.muted`/音量值显示(如 0=🔇)。

## 测试
- `persist`: `last_playlist/last_index` round-trip。
- `player-core`: `Playlist::set_items` 行为; `Command::OpenFolder` 相等性。
- `engine`: `is_video_ext` 纯函数; `sibling_videos` 用 tempfile 建几个 .mp4/.txt 验证过滤+排序。
- 字体/布局/弹出/换行/拉伸为 GUI, 编译 + clippy + 人工验证。

## i18n
新增 UI 串走 `t!()`: `open_folder`(打开文件夹/開啟資料夾/Open Folder)、`history`(历史/歷史/History)。其余复用已有 key。

## 已知限制 / 验证项
- HelveticaNeue 非 SF Pro(SF Pro 技术上无法用); 若仍不满意可考虑内嵌 Inter(后续)。
- 顶边拉伸卡顿或为上游限制(见 2)。
- 人工验证: 字体观感; 顶边拉伸; 重启恢复播放列表+选中项; 打开视频→同目录入列; 打开文件夹→整目录入列; 控制栏窄窗换行; ⚙ 在最右; 音量点击弹竖向滑块。
