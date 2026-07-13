# Windows 原生风格窗口控制按钮 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Windows(非-macOS)标题栏的最小化/最大化/关闭三个按钮在 hover 时呈 Windows 10/11 原生风格——满标题栏高、约 46px 宽、彼此无缝、贴齐右上角、直角背景覆盖到窗口边缘, 最小化/最大化 hover 淡灰、关闭 hover 红底白字。

**Architecture:** 把三个窗口控制按钮从 `custom_titlebar` 的 `egui::Frame` 中拆到一个独立的、`fixed_pos` 到窗口右上角的 `Foreground` Area(绕开 Frame 内边距, 得以贴边满高)。在该 Area 内用不复用 `beveled_button_frame_at` 的新绘制函数画直角 hover 背景; 关闭按钮右上角在窗口浮动时跟随 `WINDOW_CORNER_RADIUS`。`custom_titlebar` Frame 仍负责 拖拽区 + 标题 + app 按钮(☰⚙), 并在右侧预留 caption 簇宽度。

**Tech Stack:** Rust, egui/eframe 0.34, 现有 `crates/app/src/titlebar.rs`。

## Global Constraints

- 仅改 `crates/app/src/titlebar.rs`, 且所有新代码在 `#[cfg(not(target_os = "macos"))]` 下; 不动 macOS 分支、不动 app 按钮(☰⚙)、不引入新依赖。
- caption 按钮宽度常量 `WINDOW_CAPTION_BUTTON_WIDTH: f32 = 46.0`; 按钮高度 = `TITLEBAR_BOTTOM_OFFSET`(28.0)。
- 配色 verbatim: 最小化/最大化 hover 填充 `Color32::from_rgb(0x3A, 0x3A, 0x3A)`, 按下 `Color32::from_rgb(0x2F, 0x2F, 0x2F)`; 关闭 hover 填充 `Color32::from_rgb(196, 43, 28)`(= `#C42B1C`), 按下 `Color32::from_rgb(0xB2, 0x27, 0x19)`, 图标白色。
- 关闭按钮 hover 背景 `ne` 角: 窗口浮动时 = `WINDOW_CORNER_RADIUS`(现有常量, 值 8), 否则 0; 其余三角恒 0。最小化/最大化四角恒 0。
- 所有填充与图标颜色乘 `opacity`(`gamma_multiply(opacity)`)以随标题栏淡入淡出。
- 测试沿用现有风格: `include_str!("titlebar.rs")` 源码断言 + 纯函数单元测试。项目用 `make test`(等价 `cargo test --locked --workspace`); 单包可用 `cargo test -p app`。
- 提交前跑 `make lint`(clippy, CI 门禁) 保持零告警。

---

## 文件结构

- Modify: `crates/app/src/titlebar.rs` — 全部改动集中于此。
  - 新增常量 `WINDOW_CAPTION_BUTTON_WIDTH`。
  - 新增纯函数 `window_caption_cluster_rect(screen_rect) -> egui::Rect` 与 `close_button_corner_radius(floating: bool) -> egui::CornerRadius`(便于单元测试几何/圆角逻辑)。
  - 改 `window_buttons_width` 计算; 新增右上角 caption Area 绘制入口 `show_window_caption_buttons(ctx, opacity, actions)`; 新增 `caption_button(...)` 与 `paint_caption_hover(...)` 绘制函数; 从 `titlebar_contents` 移除行内 `window_buttons(...)` 调用。
  - 更新受影响的现有测试。

本改动是单文件视觉调整, 不拆分文件。

---

### Task 1: caption 簇几何与关闭按钮圆角(纯函数 + 常量)

先落地可独立测试的纯几何/圆角逻辑, 再在后续任务接入绘制。

**Files:**
- Modify: `crates/app/src/titlebar.rs`(常量区 ~L11-26; 测试模块尾部)

**Interfaces:**
- Consumes: 现有 `TITLEBAR_BOTTOM_OFFSET: f32`(28.0)、`WINDOW_CORNER_RADIUS: u8`(8, 已在 `#[cfg(not(target_os = "macos"))]` 下定义 L23-24)。
- Produces:
  - `const WINDOW_CAPTION_BUTTON_WIDTH: f32 = 46.0;`(`#[cfg(not(target_os = "macos"))]`)
  - `fn window_caption_cluster_rect(screen_rect: egui::Rect) -> egui::Rect`(`#[cfg(not(target_os = "macos"))]`) — 返回三按钮簇的整体矩形: 宽 `3*WINDOW_CAPTION_BUTTON_WIDTH`、高 `TITLEBAR_BOTTOM_OFFSET`, 贴 `screen_rect` 右上角。
  - `fn close_button_corner_radius(floating: bool) -> egui::CornerRadius`(`#[cfg(not(target_os = "macos"))]`) — `ne = if floating { WINDOW_CORNER_RADIUS } else { 0 }`, 其余角 0。

- [ ] **Step 1: Write the failing tests**

在 `crates/app/src/titlebar.rs` 的 `#[cfg(test)] mod tests` 内追加:

```rust
    #[test]
    fn caption_cluster_sits_flush_to_top_right_corner() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1000.0, 700.0));
        let cluster = super::window_caption_cluster_rect(screen);
        // 宽 = 3 个按钮, 高 = 满标题栏高。
        assert_eq!(cluster.width(), super::WINDOW_CAPTION_BUTTON_WIDTH * 3.0);
        assert_eq!(cluster.height(), super::TITLEBAR_BOTTOM_OFFSET);
        // 顶到边、右到边。
        assert_eq!(cluster.right(), screen.right());
        assert_eq!(cluster.top(), screen.top());
    }

    #[test]
    fn caption_button_width_is_windows_native_46() {
        assert_eq!(super::WINDOW_CAPTION_BUTTON_WIDTH, 46.0);
    }

    #[test]
    fn close_button_rounds_ne_only_when_floating() {
        let floating = super::close_button_corner_radius(true);
        assert_eq!(floating.ne, super::WINDOW_CORNER_RADIUS);
        assert_eq!(floating.nw, 0);
        assert_eq!(floating.se, 0);
        assert_eq!(floating.sw, 0);

        let maximized = super::close_button_corner_radius(false);
        assert_eq!(maximized.ne, 0);
        assert_eq!(maximized.nw, 0);
        assert_eq!(maximized.se, 0);
        assert_eq!(maximized.sw, 0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p app --lib titlebar::tests::caption 2>&1 | tail -20; cargo test -p app --lib titlebar::tests::close_button_rounds 2>&1 | tail -20`
Expected: 编译失败, 提示 `cannot find function window_caption_cluster_rect` / `close_button_corner_radius` / `WINDOW_CAPTION_BUTTON_WIDTH`。

- [ ] **Step 3: Add constant and pure functions**

在常量区(现有 `WINDOW_CORNER_RADIUS` / `WINDOW_RESIZE_HANDLE` 附近, 均在 `#[cfg(not(target_os = "macos"))]` 下)新增:

```rust
#[cfg(not(target_os = "macos"))]
const WINDOW_CAPTION_BUTTON_WIDTH: f32 = 46.0;
```

在文件内(建议紧邻 `resize_handles` 之后, 保持 `#[cfg(not(target_os = "macos"))]` 分组)新增:

```rust
/// 右上角三个 Windows 控制按钮(–▢✕)组成的簇矩形: 宽 = 3 个按钮, 高 = 满标题栏高,
/// 贴齐窗口右上角。caption Area 依此定位, 使按钮顶到边、右到边。
#[cfg(not(target_os = "macos"))]
fn window_caption_cluster_rect(screen_rect: egui::Rect) -> egui::Rect {
    let width = WINDOW_CAPTION_BUTTON_WIDTH * 3.0;
    egui::Rect::from_min_size(
        egui::pos2(screen_rect.right() - width, screen_rect.top()),
        egui::vec2(width, TITLEBAR_BOTTOM_OFFSET),
    )
}

/// 关闭按钮 hover 背景的圆角: 它贴窗口右上角, 浮动窗口该角为圆角(Win11 DWM 裁剪 /
/// Win10 自绘圆角背景, 半径 WINDOW_CORNER_RADIUS), 故 hover 填充 ne 角需跟随; 最大化/
/// 全屏时窗口为直角, ne 也取 0。其余三角恒直角。
#[cfg(not(target_os = "macos"))]
fn close_button_corner_radius(floating: bool) -> egui::CornerRadius {
    egui::CornerRadius {
        nw: 0,
        ne: if floating { WINDOW_CORNER_RADIUS } else { 0 },
        sw: 0,
        se: 0,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p app --lib titlebar::tests::caption; cargo test -p app --lib titlebar::tests::close_button_rounds`
Expected: PASS(4 个新测试全绿)。若报 `WINDOW_CORNER_RADIUS` 类型不匹配(`u8` vs `WINDOW_CORNER_RADIUS`), 确认 `CornerRadius` 字段为 `u8` 且 `WINDOW_CORNER_RADIUS` 亦 `u8`(现值 `pub const ... : u8 = ...`), 无需转换。

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/titlebar.rs
git commit -m "feat(titlebar): caption 簇几何与关闭按钮圆角纯函数(Win)"
```

---

### Task 2: caption 按钮绘制与右上角 Area 接入

把三个按钮从 Frame 行内移到独立右上角 Area, 用新绘制函数画直角 hover 背景, 并接回 `TitlebarActions`。

**Files:**
- Modify: `crates/app/src/titlebar.rs`
  - `window_buttons_width`(L339-343)
  - `show_custom_titlebar`(L161-215)
  - `titlebar_contents`(L252-284): 移除 `window_buttons(...)` 调用
  - 替换 `window_buttons`(L345-388) 与 `window_control_button`(L415-451)/`paint_close_hover_frame`(L456-492) 为新的 caption 绘制路径

**Interfaces:**
- Consumes: Task 1 的 `WINDOW_CAPTION_BUTTON_WIDTH`、`window_caption_cluster_rect`、`close_button_corner_radius`; 现有 `TITLEBAR_BOTTOM_OFFSET`、`window_is_floating(ctx)`(L44-53)、`paint_centered_symbol(ui, rect, icon, color)`(L402-413)、`titlebar_opacity(ctx, screen_rect)`(L222-250)、`toggle_maximized`(未用)、`TitlebarActions`(L28-39)、`crate::symbols::WINDOW_{MINIMIZE,MAXIMIZE,RESTORE,CLOSE}`。
- Produces:
  - `fn show_window_caption_buttons(ctx: &egui::Context, screen_rect: egui::Rect, opacity: f32, actions: &mut TitlebarActions)` — 在右上角 `fixed_pos` 的 `Foreground` Area 内画三个按钮, 写回 `actions.{minimize,maximize,close}`。
  - `enum CaptionKind { Minimize, Maximize, Close }`(替代旧 `WindowButtonAccent`)。
  - `fn caption_button(ui, kind, icon, floating, opacity) -> egui::Response`。
  - `fn window_buttons_width() -> f32`(改为无参或忽略 `ui`, 返回 `WINDOW_CAPTION_BUTTON_WIDTH * 3.0`)。

- [ ] **Step 1: Write the failing source-assertion test**

在 `mod tests` 内追加(并将在 Step 5 更新旧测试):

```rust
    #[test]
    fn caption_buttons_render_in_dedicated_top_right_area() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        // 独立右上角 Area, Foreground, fixed_pos 到簇矩形左上角。
        assert!(source.contains("fn show_window_caption_buttons("));
        assert!(source.contains("window_caption_buttons"));
        assert!(source.contains("window_caption_cluster_rect(screen_rect)"));
        assert!(source.contains("egui::Order::Foreground"));

        // 直角 hover 背景配色 verbatim。
        assert!(source.contains("0x3A, 0x3A, 0x3A"));
        assert!(source.contains("0x2F, 0x2F, 0x2F"));
        assert!(source.contains("196, 43, 28"));
        assert!(source.contains("0xB2, 0x27, 0x19"));

        // 关闭按钮圆角走 close_button_corner_radius; 满标题栏高。
        assert!(source.contains("close_button_corner_radius("));
        assert!(source.contains("WINDOW_CAPTION_BUTTON_WIDTH"));

        // caption 按钮不再复用 beveled/close-hover 旧 frame。
        assert!(!source.contains("fn window_control_button("));
        assert!(!source.contains("fn paint_close_hover_frame("));
        assert!(!source.contains("enum WindowButtonAccent"));

        // 三个窗口意图仍暴露。
        assert!(source.contains("actions.minimize = true"));
        assert!(source.contains("actions.maximize = true"));
        assert!(source.contains("actions.close = true"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app --lib titlebar::tests::caption_buttons_render_in_dedicated_top_right_area 2>&1 | tail -20`
Expected: FAIL(断言不满足: 尚无 `show_window_caption_buttons` 等)。

- [ ] **Step 3: Implement the caption Area and buttons**

3a. 改 `window_buttons_width`(去掉 `item_spacing`, 无缝簇):

```rust
#[cfg(not(target_os = "macos"))]
fn window_buttons_width() -> f32 {
    // 三个按钮无缝相邻(Windows 原生), 簇宽 = 3 × 单按钮宽。
    WINDOW_CAPTION_BUTTON_WIDTH * 3.0
}
```

同步更新其调用处 `total_trailing_buttons_width`(L288-298)里的 `window_buttons_width(ui)` → `window_buttons_width()`(移除 `ui` 实参):

```rust
fn total_trailing_buttons_width(ui: &egui::Ui) -> f32 {
    let app_width = titlebar_app_buttons_width(ui);
    #[cfg(not(target_os = "macos"))]
    {
        app_width + window_buttons_width()
    }
    #[cfg(target_os = "macos")]
    {
        app_width
    }
}
```

3b. 从 `titlebar_contents` 移除行内窗口按钮块(删除 L279-282 的 `#[cfg(not(target_os = "macos"))] { window_buttons(ui, opacity, actions); }`)。拖拽区宽度预留不变(仍走 `total_trailing_buttons_width`)。

3c. 在 `show_custom_titlebar` 里, 在绘制 resize handles 之后、返回前, 于同一 `opacity` 门控下接入 caption Area。将现有函数尾部改为:

```rust
    let opacity = titlebar_opacity(ctx, screen_rect);
    if opacity <= 0.01 {
        return actions;
    }

    egui::Area::new(egui::Id::new("custom_titlebar"))
        .fixed_pos(screen_rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_width(screen_rect.width());
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(
                    TITLEBAR_INNER_MARGIN_X,
                    TITLEBAR_INNER_MARGIN_Y,
                ))
                .multiply_with_opacity(opacity)
                .show(ui, |ui| {
                    ui.set_height(TITLEBAR_HEIGHT);
                    titlebar_contents(
                        ui,
                        title,
                        show_playlist,
                        show_settings,
                        opacity,
                        &mut actions,
                    );
                });
        });

    // 三个 Windows 控制按钮独立成右上角贴边满高的 Area(macOS 用原生交通灯, 不画)。
    #[cfg(not(target_os = "macos"))]
    show_window_caption_buttons(ctx, screen_rect, opacity, &mut actions);

    actions
}
```

3d. 删除旧 `window_buttons`、`window_control_button`、`WindowButtonAccent`、`paint_close_hover_frame`(L345-492 的窗口按钮相关块; 保留 `paint_centered_symbol`), 替换为:

```rust
/// caption 按钮种类, 决定 hover 配色与圆角。
#[cfg(not(target_os = "macos"))]
enum CaptionKind {
    Minimize,
    Maximize,
    Close,
}

/// 右上角三个 Windows 控制按钮: 独立 Foreground Area, 贴窗口右上角、满标题栏高、
/// 彼此无缝。hover 背景为直角色块覆盖到边缘; 关闭按钮 hover 红底白字, 其右上角在
/// 浮动窗口跟随窗口圆角。随标题栏 opacity 淡入淡出。
#[cfg(not(target_os = "macos"))]
fn show_window_caption_buttons(
    ctx: &egui::Context,
    screen_rect: egui::Rect,
    opacity: f32,
    actions: &mut TitlebarActions,
) {
    let cluster = window_caption_cluster_rect(screen_rect);
    let floating = window_is_floating(ctx);
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let maximize_icon = if maximized {
        crate::symbols::WINDOW_RESTORE
    } else {
        crate::symbols::WINDOW_MAXIMIZE
    };

    egui::Area::new(egui::Id::new("window_caption_buttons"))
        .fixed_pos(cluster.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_min_size(cluster.size());
            // 按钮无缝相邻: 去掉 item_spacing。
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.horizontal(|ui| {
                if caption_button(
                    ui,
                    CaptionKind::Minimize,
                    crate::symbols::WINDOW_MINIMIZE,
                    floating,
                    opacity,
                )
                .clicked()
                {
                    actions.minimize = true;
                }
                if caption_button(
                    ui,
                    CaptionKind::Maximize,
                    maximize_icon,
                    floating,
                    opacity,
                )
                .clicked()
                {
                    actions.maximize = true;
                }
                if caption_button(
                    ui,
                    CaptionKind::Close,
                    crate::symbols::WINDOW_CLOSE,
                    floating,
                    opacity,
                )
                .clicked()
                {
                    actions.close = true;
                }
            });
        });
}

/// 画单个 caption 按钮: 满高定宽方块, hover 直角背景(关闭按钮红底、ne 角跟随窗口),
/// 图标精确居中。
#[cfg(not(target_os = "macos"))]
fn caption_button(
    ui: &mut egui::Ui,
    kind: CaptionKind,
    icon: &'static str,
    floating: bool,
    opacity: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(WINDOW_CAPTION_BUTTON_WIDTH, TITLEBAR_BOTTOM_OFFSET),
        egui::Sense::click(),
    );
    let hovered = response.hovered();
    let pressed = response.is_pointer_button_down_on();

    let is_close = matches!(kind, CaptionKind::Close);
    if hovered || pressed {
        let fill = match (is_close, pressed) {
            (true, true) => egui::Color32::from_rgb(0xB2, 0x27, 0x19),
            (true, false) => egui::Color32::from_rgb(196, 43, 28),
            (false, true) => egui::Color32::from_rgb(0x2F, 0x2F, 0x2F),
            (false, false) => egui::Color32::from_rgb(0x3A, 0x3A, 0x3A),
        }
        .gamma_multiply(opacity);
        let radius = if is_close {
            close_button_corner_radius(floating)
        } else {
            egui::CornerRadius::ZERO
        };
        paint_caption_hover(ui, rect, radius, fill);
    }

    // 关闭按钮 hover 用白字(压红底); 其余按钮恒用正文色。
    let icon_color = if is_close && (hovered || pressed) {
        egui::Color32::WHITE
    } else {
        ui.visuals().text_color()
    }
    .gamma_multiply(opacity);
    paint_centered_symbol(ui, rect, icon, icon_color);
    response
}

/// caption 按钮 hover 背景: 纯色直角(或关闭按钮的单角圆)填充, 铺满按钮矩形到边缘。
#[cfg(not(target_os = "macos"))]
fn paint_caption_hover(
    ui: &egui::Ui,
    rect: egui::Rect,
    radius: egui::CornerRadius,
    fill: egui::Color32,
) {
    ui.painter().rect_filled(rect, radius, fill);
}
```

注意: `paint_centered_symbol` 现有签名为 `(ui: &mut egui::Ui, rect, icon, color)`, 上面按此调用; 保持其定义不动。

- [ ] **Step 4: Run the new test to verify it passes**

Run: `cargo test -p app --lib titlebar::tests::caption_buttons_render_in_dedicated_top_right_area 2>&1 | tail -20`
Expected: PASS。若 clippy/编译报 `window_is_floating` 未使用于某 cfg, 确认调用在 `#[cfg(not(target_os = "macos"))]` 内。

- [ ] **Step 5: Update the pre-existing tests to match new structure**

旧测试 `titlebar_renders_window_control_buttons_on_non_macos`(引用 `fn window_buttons(`、`fn window_control_button(`、`WindowButtonAccent::Close`、`fn paint_close_hover_frame`、`TITLEBAR_BUTTON_SIZE * 3.0`、`rect_filled(rect.shrink(0.5), 2,`)会失败。替换该测试体为:

```rust
    #[test]
    fn titlebar_renders_window_control_buttons_on_non_macos() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        // 三个窗口控制按钮改由独立右上角 caption Area 渲染(仅非 macOS)。
        assert!(source.contains("fn show_window_caption_buttons("));
        assert!(source.contains("fn caption_button("));
        assert!(source.contains("enum CaptionKind"));
        assert!(source.contains("crate::symbols::WINDOW_MINIMIZE"));
        assert!(source.contains("crate::symbols::WINDOW_MAXIMIZE"));
        assert!(source.contains("crate::symbols::WINDOW_RESTORE"));
        assert!(source.contains("crate::symbols::WINDOW_CLOSE"));
        // 最大化按钮按 maximized 在 ▢/❐ 间切换。
        assert!(source.contains("i.viewport().maximized.unwrap_or(false)"));
        // TitlebarActions 暴露三个窗口控制意图。
        assert!(source.contains("pub minimize: bool"));
        assert!(source.contains("pub maximize: bool"));
        assert!(source.contains("pub close: bool"));
        // 关闭按钮 hover 红底(#C42B1C)白字; 图标按字形包围盒精确居中。
        assert!(source.contains("196, 43, 28"));
        assert!(source.contains("egui::Color32::WHITE"));
        assert!(source.contains("fn paint_centered_symbol"));
        assert!(source.contains("layout_no_wrap"));
        // caption 簇预留宽度 = 3 × 单按钮宽, 无缝无间距。
        assert!(source.contains("fn window_buttons_width("));
        assert!(source.contains("WINDOW_CAPTION_BUTTON_WIDTH * 3.0"));
        // hover 背景为直角覆盖到边缘, 不再复用旧 beveled/close-hover frame。
        assert!(source.contains("fn paint_caption_hover("));
        assert!(!source.contains("fn window_control_button("));
        assert!(!source.contains("fn paint_close_hover_frame("));
    }
```

旧测试 `titlebar_supports_window_drag_buttons_and_resize_handles` 仍应通过(它断言 `StartDrag`/`Maximized`/`BeginResize`/`show_resize_handles`/`paint_window_background`, 均保留)。若 `window_minimize_icon_uses_short_en_dash` 引用的 `crate::symbols::WINDOW_MINIMIZE` 未变(仍 `"–"`), 保持不动。

- [ ] **Step 6: Run the full titlebar test module + clippy**

Run: `cargo test -p app --lib titlebar 2>&1 | tail -30`
Expected: 该模块全部 PASS(含更新后的旧测试与新测试)。

Run: `make lint 2>&1 | tail -15`
Expected: `Finished`、退出码 0、无 clippy 告警。若出现 `function window_buttons_width has too many arguments`/未使用 `ui` 告警, 确认已改为无参版本并同步调用点。

- [ ] **Step 7: Commit**

```bash
git add crates/app/src/titlebar.rs
git commit -m "feat(titlebar): Windows 原生风格 caption 按钮(贴边满高直角 hover)"
```

---

### Task 3: 真机验证与收尾

**Files:** 无代码改动(仅验证; 如发现问题回到 Task 2)。

- [ ] **Step 1: Build & run, 目测三态**

Run: `cargo build --bin morn 2>&1 | tail -5`
Expected: `Finished`。

Run(前台跑 ~15s, 自行操作观察): `RUST_BACKTRACE=1 ./target/debug/morn.exe`

逐项确认:
- 三个按钮贴右上角、满标题栏高、彼此无缝、约 46px 宽;
- 最小化/最大化 hover 淡灰直角背景、图标原色; 覆盖到窗口顶边/按钮边缘;
- 关闭 hover 红底白字, 浮动窗口时右上角圆润贴合窗口圆角、不溢出到桌面;
- 最大化后关闭按钮右上角变直角、铺满屏幕角;
- 鼠标离开标题栏后三按钮随标题栏淡出; 三个按钮点击分别触发最小化/最大化(切换)/关闭。

- [ ] **Step 2: 全量测试门禁**

Run: `make test 2>&1 | tail -20`
Expected: 全绿, 退出码 0。

- [ ] **Step 3: Commit(若 Step 1 有任何微调)**

```bash
git add -A
git commit -m "fix(titlebar): caption 按钮真机目测微调"
```
若无改动则跳过。

---

## Self-Review

**Spec coverage:**
- Spec §1 几何(独立右上角 Area、46×28、无缝、Frame 预留 caption 宽) → Task 1(常量/簇矩形) + Task 2(Area 接入、`window_buttons_width`、移除行内 `window_buttons`)。✓
- Spec §2 直角 hover 配色(min/max #3A3A3A/#2F2F2F 原色字、close #C42B1C 白字) → Task 2 `caption_button`/`paint_caption_hover`, 配色 verbatim。✓
- Spec §3 关闭按钮 ne 角跟随窗口圆角 → Task 1 `close_button_corner_radius` + Task 2 接入。✓
- Spec §4 淡入淡出与命中(opacity 门控、Foreground、resize 之后) → Task 2 Step 3c(同 opacity 门控、caption Area 在 resize handles 之后 show)。✓
- Spec 测试节(源断言更新/新增) → Task 1 Step 1、Task 2 Step 1/Step 5。✓
- Spec 非目标(不动 macOS/app 按钮/拖拽/依赖) → 全部改动在 `#[cfg(not(target_os = "macos"))]`, 未触 app 按钮与依赖。✓

**Placeholder scan:** 无 TBD/TODO; 每个代码步给出完整代码与确切命令、预期输出。✓

**Type consistency:** `window_caption_cluster_rect(screen_rect) -> Rect`、`close_button_corner_radius(bool) -> CornerRadius`、`window_buttons_width() -> f32`、`caption_button(ui, CaptionKind, &str, bool, f32) -> Response`、`show_window_caption_buttons(ctx, Rect, f32, &mut TitlebarActions)`、`paint_caption_hover(ui, Rect, CornerRadius, Color32)` 在各任务间签名一致; `CaptionKind` 三变体全程一致; `WINDOW_CORNER_RADIUS`/`WINDOW_CAPTION_BUTTON_WIDTH` 类型与用法一致。✓
