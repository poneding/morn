# UI 现代化美化设计（不改布局）

日期：2026-06-13
状态：已实现，待用户实机视觉验收

## 目标

把 `morn` 的整体观感做得更现代、更精致 —— **布局完全不动**（控件位置、面板结构、控制栏排布、设置窗口分区都保持现状）。只改：

- 全局配色（背景层级、文本层级）
- 强调色（amber 琥珀橙）应用在进度填充、选中项、激活态
- 圆角、描边、阴影的统一与精修
- 控件 hover / active 质感

## 约束与背景

- 框架：eframe / egui 0.34.3（wgpu）。无 CSS，全部走 `egui::Style` / `Visuals`。
- 当前状态：应用直接用 egui 自带 `Visuals::dark()/light()`（`ctx.set_theme` in `app.rs`），即「原味」灰。
- 关键发现：**全应用几乎不硬编码颜色**，控制栏 / 浮层 / 侧栏 / 设置窗口全部从 `ctx` 的 `visuals` 派生（`panel_fill`、`widgets.*`、`text_color`、`selection.*`）。唯一硬编码且应保留的：视频纹理 tint=`WHITE`（不染色）、字幕黑底白字描边、macOS 红黄绿交通灯。
- 因此：**一套自定义 `Visuals`（暗 + 亮）即可重塑整个应用**，零散组件自动继承。

## egui 0.34.3 着色映射（设计依据，已查源码核实）

| 视觉元素 | 取色字段 |
| --- | --- |
| 窗口/面板底 | `visuals.panel_fill` |
| 浮层/菜单底、设置窗口 | `visuals.window_fill`（+ `visuals.rs` 的 frosted shift） |
| 进度条/时间轴**已播填充** | `visuals.selection.bg_fill` |
| 时间轴**轨道**、滑块（静止）、复选框 | `widgets.inactive.bg_fill` |
| 滑块（hover/拖动） | `widgets.{hovered,active}.bg_fill` |
| **按钮**底（idle/hover/active） | `widgets.{state}.weak_bg_fill`（与上面 `bg_fill` **解耦**） |
| 按钮/图标**文字**色 | `widgets.{state}.fg_stroke.color` |
| 正文/标签色 | `widgets.noninteractive.fg_stroke.color` |
| 选中项（☰/⚙ 选中、当前列表行、选中下拉项）底 | `selection.bg_fill` |
| 选中项**文字** | `selection.stroke.color`（`interact_selectable` 用它当 fg） |
| 分隔线 | `widgets.noninteractive.bg_stroke` |

设计上的两个关键解耦点：

1. 按钮底走 `weak_bg_fill`、滑条/滑块走 `bg_fill` —— 可以让时间轴滑块 hover 变亮而不波及按钮 hover。
2. 选中底=`selection.bg_fill`、选中字=`selection.stroke.color` —— 于是「鲜亮 amber 进度条」与「amber 选中块上的深墨文字」可由同一组令牌一致产出，对比度可控。

## 方案：集中化 theme 模块 + 语义令牌

新增 `crates/app/src/theme.rs`：

- `struct Palette { ... }` —— 一套语义颜色令牌（base/surface/sunken/faint、text/text_muted/text_strong、surface_idle/hover/active、rail/knob_hover、border/border_strong、accent/on_accent、warn/error，外加 `is_dark`）。
- `PALETTE_DARK`（石墨暖橙）、`PALETTE_LIGHT`（暖白暖橙）两个常量。
- `Palette::apply(&self, &mut egui::Style)` —— 把令牌写进 `style.visuals` 各字段 + 统一圆角（控件 8 / 窗口 10）、描边、阴影（更柔更大 blur）、`slider_rail_height` 收薄、`handle_shape=Circle`。
- `pub fn install(ctx)` —— `ctx.style_mut_of(Theme::Dark, |s| PALETTE_DARK.apply(s))` 与 `Theme::Light` 同理。

接入点：

- `main.rs` 加 `mod theme;`
- `PlayerApp::new` 在 `install_fonts` 之后调用一次 `crate::theme::install(&cc.egui_ctx)`（每主题 Style 存于 `Options`，持久跨帧）。
- `sync_runtime_preferences` 维持 `ctx.set_theme(preference)` 不变（只切「用哪套已存 Style」，不重置我们的自定义）。

**换方向 = 改一处**：把 `PALETTE_DARK/LIGHT` 的 `accent`/`on_accent` 与几个基色换成青 / 紫 / 绿即可，结构不动。

## 配色（石墨暖橙 Graphite × Amber）

暗：base `#1B1A18`、surface `#211F1C`、text `#E6E2DB`/muted `#B8B3AA`/strong `#F5F2EC`、
按钮 idle `#2C2925`→hover `#38342E`→active `#443F37`、rail `#46423B`、knob_hover `#ECE8E1`、
border `#34312C`/strong `#4A463F`、**accent `#FF9F0A`**、on_accent `#1B1410`。

亮：base `#F3F1EC`、surface `#FBFAF7`、text `#2A2723`/muted `#6B665E`、
rail `#D7D2C9`、knob_hover `#2A2723`、border `#DAD5CC`、**accent `#E0890A`**、on_accent `#2A2723`。

## 测试

`theme.rs` 单测锁定设计意图：

- 选中令牌：`selection.bg_fill == accent`、`selection.stroke.color == on_accent`（两主题）。
- 时间轴进度色=accent；轨道=rail；二者不相等。
- 控件圆角=8、窗口圆角=10。
- `is_dark` 正确映射 `visuals.dark_mode`。
- accent 与 on_accent 亮度差足够大（对比度保障）。

工作区现有大重构（app/player 模块拆分）与本次美化解耦：美化只新增 `theme.rs` 并改 `main.rs`/`app.rs` 两个接入点。

## 验收

GUI 无法由我直接观察 —— 实现 + `cargo test`/`clippy` 通过后，交用户实机查看暗 / 亮两主题与控制栏、时间轴、选中态、设置窗口的观感，再定是否换强调色方向。
