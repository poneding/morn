# 设计: 键盘快捷键 + 设置窗口 + 多语言 + 中文字体修复

> 状态: 已与用户对齐, 待评审。本设计完成后转 writing-plans 生成实现计划。

## 目标

1. **修复中文显示**(bug): 当前 egui 自带字体不含 CJK 字形, 所有中文 UI(播放列表/字幕轨/打开文件…)显示为缺字方块。
2. **设置窗口**: 一个浮动设置窗口, 含外观/播放/字幕三组配置, 持久化。
3. **多语言**: 简体中文 / 繁体中文 / English 实时切换(rust-i18n)。
4. **键盘快捷键**: 空格播放/暂停, 上下调音量(吸附到 5 的倍数), 左右按可配置步长 seek。

非目标(YAGNI, 后续可扩): 自定义主题色、字幕样式/描边/位置、快捷键自定义映射、更多语言。

## 关键事实(已对照本机/已装 crate 验证)

- **可用系统中文字体**(macOS, 本机已确认存在且为真实文件):
  - `/System/Library/Fonts/Supplemental/Arial Unicode.ttf`(22M, 单一 TTF, Unicode 覆盖广 — 首选, 无 .ttc 索引问题)
  - `/System/Library/Fonts/Hiragino Sans GB.ttc`(25M)
  - `/System/Library/Fonts/STHeiti Light.ttc`(53M)
  - 注: `/Library/Fonts/Arial Unicode.ttf` 是 52B 的云字体占位符, **不可用**, 不要选它。
- **epaint `FontData`**(egui 0.34 重导出)有 `index: u32` 字段(`epaint/src/text/fonts.rs:124`)+ `from_owned(Vec<u8>)`, 故 `.ttc` 集合可加载(index 0)。
- **egui 主题 API**: `ctx.set_theme(impl Into<ThemePreference>)`(`egui/src/context.rs:2091`); `ThemePreference` 含 `Dark/Light/System`。
- **rust-i18n**: 用最新 3.x。`i18n!("locales")` 宏 + `t!("key")` + `rust_i18n::set_locale("zh-CN")`。具体宏/YAML 格式在实现时对照 3.x 文档再确认。
- 现有 `persist::Preferences` 已支持任意字段增删(`#[serde(default)]`), 加字段不破坏旧文件。
- `PlayerApp::new(cc: &eframe::CreationContext)` 可用 `cc.egui_ctx.set_fonts(...)` 安装字体(当前 `_cc` 未用)。

## 模块设计

### 1. 中文字体修复 (`crates/app`)

在 `PlayerApp::new` 里加 `install_cjk_font(&cc.egui_ctx)`:
- 候选路径按序尝试, 第一个能 `std::fs::read` 成功的即用:
  1. `/System/Library/Fonts/Supplemental/Arial Unicode.ttf`
  2. `/System/Library/Fonts/Hiragino Sans GB.ttc`
  3. `/System/Library/Fonts/STHeiti Light.ttc`
- 取 `egui::FontDefinitions::default()`, 插入该字体(`FontData::from_owned(bytes)`, ttc 用 index 0), 把它的 key **追加到** `Proportional` 与 `Monospace` family 列表**末尾**(作为回退): 拉丁字符仍用 egui 默认字体渲染, 中文回退到系统字体。一种字体即覆盖简/繁/拉丁。
- 读不到任何候选时不 panic, 仅 `eprintln` 警告(降级到原行为)。

### 2. 多语言 (`crates/app`, rust-i18n)

- `crates/app/Cargo.toml` 加 `rust-i18n = "3"`。
- `crates/app/src/main.rs` 顶部 `rust_i18n::i18n!("locales");`(默认 fallback locale = `en` 或 `zh-CN`, 取 zh-CN)。
- 语言文件 `crates/app/locales/`(rust-i18n v2 多语言单文件或分文件, 实现时定): 每个 key 给 `en`/`zh-CN`/`zh-TW` 三个值。
- 现有所有硬编码中文 UI 串改为 `t!("key")`。需翻译的串清单(约 30 条), 分布于:
  - `controls.rs`: 打开文件、静音切换、字幕轨、选择、HW/SW(保留英文)
  - `playlist_panel.rs`: 播放列表、上一个、下一个
  - `enhance.rs`: 倍速、逐帧(暂停时)、设循环起点、设循环终点、清除 AB 循环、截图
  - `video_view.rs`: 拖入视频文件开始播放
  - `app.rs`: rfd 文件过滤器 "视频"
  - 设置窗口新串: 设置、外观、语言、主题、深色、浅色、跟随系统、播放、快进步长、字幕、字幕字号、秒、关闭
- 启动时按 prefs 调 `set_locale`; 设置窗口切换语言时实时 `set_locale`。图标(emoji)不翻译。

### 3. 设置持久化 (`crates/persist`)

`Preferences` 增 4 字段(均 `#[serde(default)]` 友好, Default 给默认值):
- `language: String`(默认 `"zh-CN"`)
- `seek_step_secs: u64`(默认 `10`)
- `theme: String`(默认 `"system"`; 取值 dark/light/system)
- `subtitle_font_size: f32`(默认 `24.0`)

加对应 getter/setter 或直接公开字段(沿用现有 `pub volume` 风格 — language/seek_step/theme/subtitle_font_size 设为 `pub`)。`resume_points` 仍私有。补单测: 4 字段 save→load round-trip。

### 4. Player 暴露设置 (`crates/engine`)

Player 已持有 `prefs`。新增:
- `pub fn prefs(&self) -> &persist::Preferences`(只读, 供 app 读 seek_step/subtitle_font_size/language/theme)
- setter(更新 prefs 字段, 由设置窗口调用; save_state 已负责落盘):
  `set_language(&mut self, &str)`, `set_seek_step(&mut self, u64)`, `set_theme(&mut self, &str)`, `set_subtitle_font_size(&mut self, f32)`
- Player 只存储字符串, 不解释 language/theme 语义(解释在 app 层)。

### 5. 设置窗口 UI (`crates/app/src/settings.rs` 新增)

- `PlayerApp` 加 `show_settings: bool`(默认 false)。控制栏加 ⚙ 按钮切换。
- `pub fn settings_window(ctx, open: &mut bool, player: &mut Player)`: 用 `egui::Window::new(t!("settings")).open(open)`:
  - **外观**: 语言 ComboBox(简体中文/繁体中文/English → zh-CN/zh-TW/en); 主题 ComboBox(深色/浅色/跟随系统 → dark/light/system)
  - **播放**: 快进步长 ComboBox(5/10/20/30 秒)
  - **字幕**: 字幕字号 Slider(12..=48)
- 任一项变更 → 调对应 Player setter; 语言/主题的 UI 生效在 app 每帧应用(见下)。

### 6. app 每帧应用语言/主题 (`crates/app/src/app.rs`)

`ui()` 开头(读 prefs):
- `rust_i18n::set_locale(player.prefs().language)`(thread-local, 幂等, 每帧设无妨)
- `ctx.set_theme(parse_theme(player.prefs().theme))`(映射 dark/light/system → ThemePreference)

### 7. 键盘快捷键 (`crates/app/src/app.rs`)

`ui()` 中用 `ctx.input(|i| ...)`, 仅在无文本框获得焦点时处理(`!ctx.wants_keyboard_input()`; 当前无文本输入框, 但加此守卫以防设置窗口未来加输入框); 每键 `key_pressed`(每次按下触发一次):
- `Space` → 据当前 state 发 `Play`/`Pause`
- `↑` → `vol = ((vol/5)+1)*5`(吸附到下一个 5 的倍数), clamp 100, 发 `SetVolume`。例: 43→45→50
- `↓` → `vol = ((vol.saturating_sub(1))/5)*5`(吸附到上一个 5 的倍数), clamp 0, 发 `SetVolume`。例: 43→40→35
- `←` → `SeekTo(pos.saturating_sub(step_ms))`, `step_ms = prefs().seek_step_secs*1000`
- `→` → `SeekTo((pos+step_ms).min(duration))`
- pos/duration 取自 `player.timeline()`。

### 8. 字幕字号接线 (`crates/app`)

`subtitle_overlay::draw_subtitle` 加 `size: f32` 参数(替换硬编码 24.0); `video_view::show` 把 `player.prefs().subtitle_font_size` 传入。

## 数据流

设置窗口 → Player setter(写 prefs) → app 每帧读 prefs 应用(locale/theme) + 传参(字幕字号/seek 步长) → 退出/周期 save_state 落盘 → 下次 with_prefs 恢复。

## 测试

- `persist`: 新 4 字段 save→load round-trip 单测。
- 纯逻辑可测的音量吸附函数(`snap_volume_up/down`)抽成纯函数 + 单测(43→45, 45→50, 43→40, 40→35, 边界 0/100)。
- i18n/字体/主题/窗口/键盘为 GUI/集成, 编译 + clippy + 人工验证。

## 已知限制 / 验证项

- 字体路径为 macOS 专用; 非 macOS 暂不处理(本项目当前仅 macOS)。
- rust-i18n 3.x 的宏与 YAML 格式在实现首步对照真实 crate 确认(外部 API 风险点)。
- 主题"跟随系统"依赖 egui `ThemePreference::System` 的系统检测。
- 人工验证: 中文正常显示; 切换三种语言 UI 文案变化; 切换主题; 快捷键(空格/上下/左右)行为; 字幕字号变化; 重启后设置保留。
