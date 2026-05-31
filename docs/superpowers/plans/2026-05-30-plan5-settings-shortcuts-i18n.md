# 计划 5: 设置窗口 / 快捷键 / 多语言 / 中文字体 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复中文显示, 增加设置窗口(语言/主题/快进步长/字幕字号)、rust-i18n 多语言(简/繁/英)与键盘快捷键(空格/上下/左右)。

**Architecture:** 设置持久化进 `persist::Preferences`, 由 `engine::Player` 持有并暴露读写; `app` 层负责字体安装、i18n、设置窗口、主题应用与键盘处理。多数为 GUI, 仅 persist 字段与音量吸附为纯逻辑可单测。

**Tech Stack:** eframe/egui 0.34(`set_fonts`/`set_theme`/`egui::Window`), rust-i18n 4.1(`i18n!`/`t!`/`set_locale`, `_version: 2` YAML), 复用 persist/engine。

**前置依赖:** 计划 1–4 已完成(HEAD 在 master)。

## 已验证的外部事实(实现时若不符以实际为准并上报, 勿臆造)

- 系统中文字体(本机真实存在): `/System/Library/Fonts/Supplemental/Arial Unicode.ttf`(单一 TTF, 首选)、`/System/Library/Fonts/Hiragino Sans GB.ttc`、`/System/Library/Fonts/STHeiti Light.ttc`。`/Library/Fonts/Arial Unicode.ttf` 是 52B 占位符**不可用**。
- epaint `FontData` 有 `index: u32` 字段 + `from_owned(Vec<u8>)`; `.ttc` 用 index 0 可加载。
- egui: `ctx.set_theme(impl Into<egui::ThemePreference>)`; `ThemePreference::{Dark,Light,System}`。
- rust-i18n **4.1.0**(非 3.x): crate 根 `rust_i18n::i18n!("locales", fallback = "en");`; `t!("key")`(经 `use rust_i18n::t;`, 返回 `Cow<str>`, 传给 egui 用 `.to_string()`); `rust_i18n::set_locale("zh-CN")`; 语言文件放 `crates/app/locales/`(`_version: 2` 单文件多语言)。

## 文件结构

```
crates/persist/src/prefs.rs        # 修改: +language/seek_step_secs/theme/subtitle_font_size
crates/engine/src/player.rs        # 修改: prefs() 读 + 4 setter
crates/app/Cargo.toml              # 修改: +rust-i18n
crates/app/locales/app.yml         # 新增: 简/繁/英 文案
crates/app/src/main.rs             # 修改: mod font/shortcuts/settings; i18n!
crates/app/src/font.rs             # 新增: 安装系统 CJK 字体
crates/app/src/shortcuts.rs        # 新增: 音量吸附纯函数(+单测)
crates/app/src/settings.rs         # 新增: 设置窗口 UI
crates/app/src/app.rs              # 修改: 字体安装/每帧应用 locale+theme/键盘/⚙/字幕字号
crates/app/src/controls.rs         # 修改: t!() + ⚙ 按钮
crates/app/src/playlist_panel.rs   # 修改: t!()
crates/app/src/enhance.rs          # 修改: t!()
crates/app/src/video_view.rs       # 修改: t!() + 字幕字号传参
crates/app/src/subtitle_overlay.rs # 修改: draw_subtitle 加 size 参数
```

---

## Task 1: 中文字体加载

**Files:**
- Create: `crates/app/src/font.rs`
- Modify: `crates/app/src/main.rs`(加 `mod font;`)
- Modify: `crates/app/src/app.rs`(`PlayerApp::new` 调用)

- [ ] **Step 1: 写 font.rs**

`crates/app/src/font.rs`:
```rust
use eframe::egui;

/// 把系统中文字体安装为 egui 的回退字体, 使中文(简/繁)能显示。
/// 拉丁字符仍用 egui 默认字体, CJK 回退到系统字体。读不到则降级(仅警告)。
pub fn install_cjk_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
    ];
    let Some(bytes) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) else {
        eprintln!("警告: 未找到系统中文字体, 中文可能无法显示");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
    // 追加到两个 family 末尾作为回退
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".to_owned());
    }
    ctx.set_fonts(fonts);
}
```
注: egui 0.34 的 `FontData::from_owned` 返回 `FontData`; `font_data` 的值类型是 `Arc<FontData>`, 故用 `.into()`。若类型不符, 以实际签名为准(可能直接 `from_owned(...)` 无需 `.into()`)——上报你用的形式。`.ttc` 默认 index 0 即可。

- [ ] **Step 2: 挂模块并在启动时安装**

`crates/app/src/main.rs` 加 `mod font;`(与其它 `mod` 并列)。
`crates/app/src/app.rs` 的 `PlayerApp::new(cc)` 改为先安装字体(把 `_cc` 改为 `cc`):
```rust
pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
    crate::font::install_cjk_font(&cc.egui_ctx);
    Self {
        player: Player::with_prefs(prefs_path()),
        video_view: VideoView::new(),
        rate_pct: 100,
    }
}
```

- [ ] **Step 3: 验证**

Run: `cargo build -p app`
Expected: 编译通过。(中文显示需人工 `cargo run -p app` 确认。)

Run: `cargo clippy -p app --all-targets -- -D warnings`
Expected: clean。

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/font.rs crates/app/src/main.rs crates/app/src/app.rs
git commit -m "fix(app): 加载系统中文字体, 修复中文显示"
```

---

## Task 2: persist 增加 4 个设置字段

**Files:**
- Modify: `crates/persist/src/prefs.rs`

- [ ] **Step 1: 写失败测试**

`crates/persist/src/prefs.rs` 的 `#[cfg(test)] mod tests` 追加:
```rust
#[test]
#[allow(clippy::field_reassign_with_default)]
fn new_settings_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    let mut p = Preferences::default();
    p.language = "zh-TW".into();
    p.seek_step_secs = 20;
    p.theme = "dark".into();
    p.subtitle_font_size = 30.0;
    p.save(&path).unwrap();
    let loaded = Preferences::load(&path).unwrap();
    assert_eq!(loaded.language, "zh-TW");
    assert_eq!(loaded.seek_step_secs, 20);
    assert_eq!(loaded.theme, "dark");
    assert_eq!(loaded.subtitle_font_size, 30.0);
}

#[test]
fn settings_defaults() {
    let p = Preferences::default();
    assert_eq!(p.language, "zh-CN");
    assert_eq!(p.seek_step_secs, 10);
    assert_eq!(p.theme, "system");
    assert_eq!(p.subtitle_font_size, 24.0);
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p persist`
Expected: 编译失败(no field `language` 等)。

- [ ] **Step 3: 加字段与默认值**

`Preferences` 结构体加(放在 `window_size` 后, `resume_points` 前):
```rust
    pub language: String,
    pub seek_step_secs: u64,
    pub theme: String,
    pub subtitle_font_size: f32,
```
`Default` 实现里加:
```rust
            language: "zh-CN".to_string(),
            seek_step_secs: 10,
            theme: "system".to_string(),
            subtitle_font_size: 24.0,
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p persist`
Expected: PASS(原有 + 2 新测试)。

- [ ] **Step 5: Commit**

```bash
git add crates/persist
git commit -m "feat(persist): 增加 语言/快进步长/主题/字幕字号 偏好"
```

---

## Task 3: Player 暴露设置读写

**Files:**
- Modify: `crates/engine/src/player.rs`

- [ ] **Step 1: 写失败测试**

`player.rs` 的 `#[cfg(test)] mod tests` 追加:
```rust
#[test]
fn setters_update_prefs() {
    let mut p = Player::new();
    p.set_seek_step(20);
    p.set_language("en");
    p.set_theme("dark");
    p.set_subtitle_font_size(32.0);
    assert_eq!(p.prefs().seek_step_secs, 20);
    assert_eq!(p.prefs().language, "en");
    assert_eq!(p.prefs().theme, "dark");
    assert_eq!(p.prefs().subtitle_font_size, 32.0);
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p engine setters_update_prefs`
Expected: 编译失败(no method `set_seek_step` 等)。

- [ ] **Step 3: 加 accessor 与 setter**

`player.rs` 的 `impl Player` 内加(`prefs` 字段已存在):
```rust
pub fn prefs(&self) -> &persist::Preferences {
    &self.prefs
}
pub fn set_language(&mut self, v: &str) {
    self.prefs.language = v.to_string();
}
pub fn set_seek_step(&mut self, secs: u64) {
    self.prefs.seek_step_secs = secs;
}
pub fn set_theme(&mut self, v: &str) {
    self.prefs.theme = v.to_string();
}
pub fn set_subtitle_font_size(&mut self, size: f32) {
    self.prefs.subtitle_font_size = size;
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p engine`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/engine
git commit -m "feat(engine): Player 暴露设置读写接口"
```

---

## Task 4: 音量吸附纯函数

**Files:**
- Create: `crates/app/src/shortcuts.rs`
- Modify: `crates/app/src/main.rs`(加 `mod shortcuts;`)

- [ ] **Step 1: 写失败测试**

`crates/app/src/shortcuts.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::{snap_volume_down, snap_volume_up};

    #[test]
    fn up_snaps_to_next_multiple_of_5() {
        assert_eq!(snap_volume_up(43), 45);
        assert_eq!(snap_volume_up(45), 50);
        assert_eq!(snap_volume_up(40), 45);
        assert_eq!(snap_volume_up(98), 100);
        assert_eq!(snap_volume_up(100), 100);
    }

    #[test]
    fn down_snaps_to_prev_multiple_of_5() {
        assert_eq!(snap_volume_down(43), 40);
        assert_eq!(snap_volume_down(45), 40);
        assert_eq!(snap_volume_down(40), 35);
        assert_eq!(snap_volume_down(3), 0);
        assert_eq!(snap_volume_down(0), 0);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p app snap_volume`
Expected: 编译失败(cannot find function `snap_volume_up`)。

- [ ] **Step 3: 实现**

`shortcuts.rs` 顶部(测试模块之上):
```rust
/// 音量上调到下一个 5 的倍数(已是倍数则 +5), clamp 100。例: 43→45, 45→50。
pub fn snap_volume_up(vol: u8) -> u8 {
    (((vol / 5) + 1) * 5).min(100)
}

/// 音量下调到上一个 5 的倍数(已是倍数则 -5), clamp 0。例: 43→40, 45→40, 40→35。
pub fn snap_volume_down(vol: u8) -> u8 {
    (vol.saturating_sub(1) / 5) * 5
}
```

- [ ] **Step 4: 挂模块并运行测试**

`crates/app/src/main.rs` 加 `mod shortcuts;`。
Run: `cargo test -p app snap_volume`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/shortcuts.rs crates/app/src/main.rs
git commit -m "feat(app): 音量吸附到5的倍数 纯函数"
```

---

## Task 5: rust-i18n 接入 + 语言文件

**Files:**
- Modify: `crates/app/Cargo.toml`
- Create: `crates/app/locales/app.yml`
- Modify: `crates/app/src/main.rs`

- [ ] **Step 1: 加依赖**

`crates/app/Cargo.toml` 的 `[dependencies]` 加:
```
rust-i18n = "4"
```

- [ ] **Step 2: 写语言文件**

`crates/app/locales/app.yml`(`_version: 2`, 一个 key 三种语言):
```yaml
_version: 2
open_file:
  en: Open
  zh-CN: 打开文件
  zh-TW: 開啟檔案
play:
  en: Play
  zh-CN: 播放
  zh-TW: 播放
pause:
  en: Pause
  zh-CN: 暂停
  zh-TW: 暫停
stop:
  en: Stop
  zh-CN: 停止
  zh-TW: 停止
mute_toggle:
  en: Mute
  zh-CN: 静音切换
  zh-TW: 靜音切換
fullscreen:
  en: Fullscreen
  zh-CN: 全屏
  zh-TW: 全螢幕
subtitle_track:
  en: Subtitle Track
  zh-CN: 字幕轨
  zh-TW: 字幕軌
select:
  en: Select
  zh-CN: 选择
  zh-TW: 選擇
playlist:
  en: Playlist
  zh-CN: 播放列表
  zh-TW: 播放清單
prev:
  en: Previous
  zh-CN: 上一个
  zh-TW: 上一個
next:
  en: Next
  zh-CN: 下一个
  zh-TW: 下一個
rate:
  en: Speed
  zh-CN: 倍速
  zh-TW: 倍速
screenshot:
  en: Screenshot
  zh-CN: 截图
  zh-TW: 截圖
drop_hint:
  en: Drop a video file to start
  zh-CN: 拖入视频文件开始播放
  zh-TW: 拖入影片檔開始播放
video_filter:
  en: Video
  zh-CN: 视频
  zh-TW: 影片
settings:
  en: Settings
  zh-CN: 设置
  zh-TW: 設定
appearance:
  en: Appearance
  zh-CN: 外观
  zh-TW: 外觀
language:
  en: Language
  zh-CN: 语言
  zh-TW: 語言
theme:
  en: Theme
  zh-CN: 主题
  zh-TW: 主題
theme_dark:
  en: Dark
  zh-CN: 深色
  zh-TW: 深色
theme_light:
  en: Light
  zh-CN: 浅色
  zh-TW: 淺色
theme_system:
  en: Follow System
  zh-CN: 跟随系统
  zh-TW: 跟隨系統
playback:
  en: Playback
  zh-CN: 播放
  zh-TW: 播放
seek_step:
  en: Seek Step
  zh-CN: 快进步长
  zh-TW: 快進步長
subtitle:
  en: Subtitle
  zh-CN: 字幕
  zh-TW: 字幕
subtitle_size:
  en: Subtitle Size
  zh-CN: 字幕字号
  zh-TW: 字幕字號
seconds:
  en: s
  zh-CN: 秒
  zh-TW: 秒
```

- [ ] **Step 3: 初始化 i18n**

`crates/app/src/main.rs` 顶部(`mod` 声明附近, crate 根作用域)加:
```rust
rust_i18n::i18n!("locales", fallback = "en");
```

- [ ] **Step 4: 验证**

Run: `cargo build -p app`
Expected: 编译通过(i18n! 宏在编译期加载 locales/app.yml)。若宏路径/格式报错, 对照 rust-i18n 4.1 文档修正并上报实际用法。

- [ ] **Step 5: Commit**

```bash
git add crates/app/Cargo.toml crates/app/locales crates/app/src/main.rs
git commit -m "feat(app): 接入 rust-i18n 与简繁英语言文件"
```

---

## Task 6: UI 文案改用 t!()

**Files:**
- Modify: `crates/app/src/controls.rs`, `playlist_panel.rs`, `enhance.rs`, `video_view.rs`, `app.rs`

每个文件顶部加 `use rust_i18n::t;`。把硬编码中文串替换为 `t!("key").to_string()`(egui widget 取 `String`)。图标(emoji)、倍速 "{:.2}x"、语言名("简体中文" 等)**不改**。

- [ ] **Step 1: controls.rs**

替换:
- 打开按钮 tooltip: `.on_hover_text("打开文件")` → `.on_hover_text(t!("open_file").to_string())`
- 静音 tooltip: `.on_hover_text("静音切换")` → `.on_hover_text(t!("mute_toggle").to_string())`
- 字幕轨 combo: `egui::ComboBox::from_label("字幕轨")` → `from_label(t!("subtitle_track").to_string())`; `.selected_text("选择")` → `.selected_text(t!("select").to_string())`
(播放/暂停/停止/全屏为 emoji 图标, 不改。)

- [ ] **Step 2: playlist_panel.rs**

- `ui.heading("播放列表")` → `ui.heading(t!("playlist").to_string())`
- `"⏮ 上一个"` → `format!("⏮ {}", t!("prev"))`
- `"下一个 ⏭"` → `format!("{} ⏭", t!("next"))`

- [ ] **Step 3: enhance.rs**

- `from_label("倍速")` → `from_label(t!("rate").to_string())`
- `.on_hover_text("截图")` → `t!("screenshot").to_string()`

- [ ] **Step 4: video_view.rs**

- `ui.label("拖入视频文件开始播放")` → `ui.label(t!("drop_hint").to_string())`

- [ ] **Step 5: app.rs**

- rfd: `.add_filter("视频", &[...])` → `.add_filter(t!("video_filter").to_string(), &[...])`

- [ ] **Step 6: 验证**

Run: `cargo build -p app && cargo clippy -p app --all-targets -- -D warnings`
Expected: 编译 + clippy clean。

- [ ] **Step 7: Commit**

```bash
git add crates/app
git commit -m "feat(app): UI 文案改用 t!() 支持多语言"
```

---

## Task 7: 每帧应用 locale 与主题 + 启动设 locale

**Files:**
- Modify: `crates/app/src/app.rs`

- [ ] **Step 1: 解析主题字符串的辅助函数**

`app.rs` 加自由函数:
```rust
fn theme_preference(s: &str) -> egui::ThemePreference {
    match s {
        "dark" => egui::ThemePreference::Dark,
        "light" => egui::ThemePreference::Light,
        _ => egui::ThemePreference::System,
    }
}
```

- [ ] **Step 2: ui() 开头应用 locale + theme**

`ui()` 最开始(取 `ctx` 后)加:
```rust
rust_i18n::set_locale(&self.player.prefs().language);
ctx.set_theme(theme_preference(&self.player.prefs().theme));
```
(每帧设置, 幂等; 保证设置窗口切换后立即生效。)

- [ ] **Step 3: 验证**

Run: `cargo build -p app && cargo clippy -p app --all-targets -- -D warnings`
Expected: clean。

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/app.rs
git commit -m "feat(app): 每帧应用语言与主题偏好"
```

---

## Task 8: 设置窗口

**Files:**
- Create: `crates/app/src/settings.rs`
- Modify: `crates/app/src/main.rs`(`mod settings;`), `crates/app/src/app.rs`, `crates/app/src/controls.rs`

- [ ] **Step 1: 写设置窗口**

`crates/app/src/settings.rs`:
```rust
use eframe::egui;
use engine::Player;
use rust_i18n::t;

/// 绘制设置窗口。`open` 控制显隐, 直接读写 player 的偏好。
pub fn settings_window(ctx: &egui::Context, open: &mut bool, player: &mut Player) {
    egui::Window::new(t!("settings").to_string())
        .open(open)
        .resizable(false)
        .show(ctx, |ui| {
            // 外观
            ui.heading(t!("appearance").to_string());
            ui.horizontal(|ui| {
                ui.label(t!("language").to_string());
                let mut lang = player.prefs().language.clone();
                egui::ComboBox::from_id_salt("lang")
                    .selected_text(lang_label(&lang))
                    .show_ui(ui, |ui| {
                        for (code, label) in
                            [("zh-CN", "简体中文"), ("zh-TW", "繁体中文"), ("en", "English")]
                        {
                            ui.selectable_value(&mut lang, code.to_string(), label);
                        }
                    });
                if lang != player.prefs().language {
                    player.set_language(&lang);
                }
            });
            ui.horizontal(|ui| {
                ui.label(t!("theme").to_string());
                let mut theme = player.prefs().theme.clone();
                egui::ComboBox::from_id_salt("theme")
                    .selected_text(theme_label(&theme))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut theme, "dark".to_string(), t!("theme_dark").to_string());
                        ui.selectable_value(&mut theme, "light".to_string(), t!("theme_light").to_string());
                        ui.selectable_value(&mut theme, "system".to_string(), t!("theme_system").to_string());
                    });
                if theme != player.prefs().theme {
                    player.set_theme(&theme);
                }
            });
            ui.separator();
            // 播放
            ui.heading(t!("playback").to_string());
            ui.horizontal(|ui| {
                ui.label(t!("seek_step").to_string());
                let mut step = player.prefs().seek_step_secs;
                egui::ComboBox::from_id_salt("seek_step")
                    .selected_text(format!("{} {}", step, t!("seconds")))
                    .show_ui(ui, |ui| {
                        for s in [5u64, 10, 20, 30] {
                            ui.selectable_value(&mut step, s, format!("{} {}", s, t!("seconds")));
                        }
                    });
                if step != player.prefs().seek_step_secs {
                    player.set_seek_step(step);
                }
            });
            ui.separator();
            // 字幕
            ui.heading(t!("subtitle").to_string());
            ui.horizontal(|ui| {
                ui.label(t!("subtitle_size").to_string());
                let mut size = player.prefs().subtitle_font_size;
                if ui.add(egui::Slider::new(&mut size, 12.0..=48.0)).changed() {
                    player.set_subtitle_font_size(size);
                }
            });
        });
}

fn lang_label(code: &str) -> &'static str {
    match code {
        "zh-TW" => "繁体中文",
        "en" => "English",
        _ => "简体中文",
    }
}
fn theme_label(code: &str) -> String {
    match code {
        "dark" => t!("theme_dark").to_string(),
        "light" => t!("theme_light").to_string(),
        _ => t!("theme_system").to_string(),
    }
}
```
注: egui 0.34 用 `ComboBox::from_id_salt`(旧名 `from_id_source` 已弃用)。若 `from_id_salt` 不存在以实际 API 为准并上报。

- [ ] **Step 2: app 持有显隐状态并调用**

`crates/app/src/main.rs` 加 `mod settings;`。
`app.rs` 的 `PlayerApp` 加字段 `show_settings: bool`, `new()` 初始化 `show_settings: false`。
在 `ui()` 末尾(各面板之后)调用:
```rust
crate::settings::settings_window(&ctx, &mut self.show_settings, &mut self.player);
```

- [ ] **Step 3: 控制栏加 ⚙ 按钮**

`controls.rs` 的 `controls_bar` 返回的命令无法携带"打开设置"(那是 app 状态)。改为: 在 `app.rs` 的底部面板, controls_bar 调用之后加一个 ⚙ 按钮直接翻转 `self.show_settings`:
```rust
// app.rs 底部面板闭包内, controls_bar 处理之后:
if ui.button("⚙").on_hover_text(t!("settings").to_string()).clicked() {
    self.show_settings = !self.show_settings;
}
```
(`use rust_i18n::t;` 已在 app.rs。)

- [ ] **Step 4: 验证**

Run: `cargo build -p app && cargo clippy -p app --all-targets -- -D warnings`
Expected: clean。(窗口/切换需人工验证。)

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): 设置窗口(语言/主题/快进步长/字幕字号)"
```

---

## Task 9: 键盘快捷键 + 字幕字号接线

**Files:**
- Modify: `crates/app/src/app.rs`
- Modify: `crates/app/src/subtitle_overlay.rs`, `crates/app/src/video_view.rs`

- [ ] **Step 1: subtitle_overlay 加 size 参数**

`subtitle_overlay.rs` 的 `draw_subtitle` 签名加 `size: f32`, 用它替换硬编码 `24.0`:
```rust
pub fn draw_subtitle(ui: &mut egui::Ui, area: egui::Rect, text: &str, size: f32) {
    if text.is_empty() { return; }
    let painter = ui.painter_at(area);
    let font = egui::FontId::proportional(size);
    // ... 其余不变 ...
}
```

- [ ] **Step 2: video_view 传字幕字号**

`video_view.rs` 末尾调用处:
```rust
if let Some(text) = player.current_subtitle() {
    crate::subtitle_overlay::draw_subtitle(ui, rect, &text, player.prefs().subtitle_font_size);
}
```

- [ ] **Step 3: 键盘处理**

`app.rs` 的 `ui()` 中(`let t = self.player.timeline();` 之后), 加键盘处理:
```rust
if !ctx.wants_keyboard_input() {
    let step_ms = self.player.prefs().seek_step_secs * 1000;
    let dur = t.duration_ms;
    let pos = t.position_ms;
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Space) {
            self.player.handle(if t.state == player_core::PlaybackState::Playing {
                player_core::Command::Pause
            } else {
                player_core::Command::Play
            });
        }
        if i.key_pressed(egui::Key::ArrowUp) {
            self.player
                .handle(player_core::Command::SetVolume(crate::shortcuts::snap_volume_up(t.volume)));
        }
        if i.key_pressed(egui::Key::ArrowDown) {
            self.player
                .handle(player_core::Command::SetVolume(crate::shortcuts::snap_volume_down(t.volume)));
        }
        if i.key_pressed(egui::Key::ArrowLeft) {
            self.player
                .handle(player_core::Command::SeekTo(pos.saturating_sub(step_ms)));
        }
        if i.key_pressed(egui::Key::ArrowRight) {
            let target = if dur > 0 { (pos + step_ms).min(dur) } else { pos + step_ms };
            self.player.handle(player_core::Command::SeekTo(target));
        }
    });
}
```
注: 需 `use player_core::PlaybackState;` 或全限定(上面已全限定 `player_core::PlaybackState`)。`ctx.input` 闭包内调 `self.player.handle` 需 `self` 可变借用; `ctx` 是开头 clone 的独立句柄, 与 `self` 无借用冲突。若借用冲突, 先把按键状态读进局部 bool 再在闭包外 handle。

- [ ] **Step 4: 验证**

Run: `cargo test`
Expected: 既有测试 PASS。

Run: `cargo build -p app && cargo clippy --all-targets -- -D warnings`
Expected: clean。

Run: `cargo run -p app`(人工)
Expected: 空格播放/暂停; ↑↓ 音量按 5 吸附; ←→ 按设置步长 seek; 设置里改字幕字号后字幕变大/小。

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): 键盘快捷键(空格/上下/左右)与字幕字号接线"
```

---

## Task 10: 全量验证

- [ ] **Step 1: 全量测试**

Run: `bash crates/media/tests/gen_fixture.sh && cargo test`
Expected: 全部 PASS(含 persist 新 2 测、engine setter 测、app 音量吸附测)。

- [ ] **Step 2: clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: 无警告、已格式化。

- [ ] **Step 3: 人工走查(用户)**

`cargo run -p app` 确认: ① 中文正常显示(无方块); ② 设置窗口可开, 切换 简体/繁体/English UI 文案变化; ③ 切换 深色/浅色/跟随系统 主题变化; ④ 快进步长改 5/30 后 ←→ seek 幅度变化; ⑤ 字幕字号滑块改变字幕大小; ⑥ 空格/↑↓/←→ 快捷键; ⑦ 重启后语言/主题/步长/字号/音量保留。

- [ ] **Step 4: Commit(若有收尾改动)**

```bash
git add -A
git commit -m "chore: 计划5 收尾"
```
(若工作树已干净则跳过。)

---

## 已知限制 / 后续

- 字体路径为 macOS 专用; 非 macOS 需另加候选路径(后续)。
- 主题"跟随系统"依赖 egui 的系统主题检测。
- 快捷键映射暂不可自定义; 语言仅简/繁/英。
