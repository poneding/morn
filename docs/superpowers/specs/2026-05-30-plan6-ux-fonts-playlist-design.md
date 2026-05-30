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

## 待用户在评审时确认的判断点(我已选默认, 可改)

- **A. 启动恢复后是否自动打开/播放上次的视频?** 默认: **恢复列表 + 选中上次的视频, 但不自动打开**(避免启动即加载大文件/突然出声)。视频区仍显示"拖入提示", 用户点列表项或按播放才打开并续播到上次位置。若你要"启动即续播播放", 告诉我。
- **B. 打开一个视频是否替换整个播放列表为该目录的视频?** 默认: **是**(替换)。即播放列表始终代表"当前视频所在目录"。这会改变此前"拖多个文件累积成列表"的行为 — 符合你 #5 的诉求。若想保留累积模式, 告诉我。

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

### 3. 会话记忆 — 播放列表持久化 (`crates/persist`, `crates/player-core`, `crates/engine`)
- `Preferences` 加 `last_playlist: Vec<String>`(默认空)与 `last_index: usize`(默认 0)。round-trip 单测。
- `player-core::Playlist` 加 `set_items(items: Vec<PathBuf>, cursor: usize)`(供恢复)与现有 `iter()/current_index()` 配合; 加单测。
- `engine::Player::with_prefs`: 若 `prefs.last_playlist` 非空, 用其重建 `playlist`(set_items), cursor=`last_index.min(len-1)`。**不自动 open**(判断点 A 默认)。
- `engine::Player::save_state`: 把当前 `playlist` 的 items 与 cursor 写入 `prefs.last_playlist/last_index`(连同已有的 volume/resume/settings)。

### 4. 播放列表自动化 (`crates/engine`, `crates/player-core`, `crates/app`)
- 新增 `engine::sibling_videos(path: &Path) -> Vec<PathBuf>`: 读取 `path` 父目录, 过滤视频扩展名(mp4/mkv/webm/mov/avi/m4v/flv/ts 等, 抽 `is_video_ext` 纯函数 + 单测), 按文件名排序。空/读失败返回仅含 `path` 自身的 Vec(降级)。
- `Player::handle(Command::Open(path))` 改为: `let vids = sibling_videos(&path); playlist.set_items(vids, index_of(path)); self.open(&path);`(替换 — 判断点 B 默认)。
- 新增 `player_core::Command::OpenFolder`(单元变体 + 测试)。`app` 层拦截(像 OpenDialog 一样): `rfd::FileDialog::new().pick_folder()` → 若选中, 扫描该目录视频 → `player.handle_open_folder(dir)`(engine 加方法: set_items(scan(dir), 0) 并 open 第一个; 空目录则忽略)。
- 控制栏/设置区加"打开文件夹"按钮(📁)发 `OpenFolder`。

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
新增 UI 串走 `t!()`: `open_folder`(打开文件夹/開啟資料夾/Open Folder)。其余复用已有 key。

## 已知限制 / 验证项
- HelveticaNeue 非 SF Pro(SF Pro 技术上无法用); 若仍不满意可考虑内嵌 Inter(后续)。
- 顶边拉伸卡顿或为上游限制(见 2)。
- 人工验证: 字体观感; 顶边拉伸; 重启恢复播放列表+选中项; 打开视频→同目录入列; 打开文件夹→整目录入列; 控制栏窄窗换行; ⚙ 在最右; 音量点击弹竖向滑块。
