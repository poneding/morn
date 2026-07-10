# Windows 原生风格窗口控制按钮设计

日期: 2026-07-10
范围: `crates/app/src/titlebar.rs`(仅非-macOS 分支)

## 背景与目标

当前非-macOS 平台的三个窗口控制按钮(最小化 – / 最大化 ▢❐ / 关闭 ✕)与 app 按钮
(☰ 播放列表 / ⚙ 设置)同处 `custom_titlebar` 这一个 `egui::Frame` 内, 受 Frame 的
`Margin::symmetric(8, 1)` 内边距约束, 够不到窗口的顶边与右边。hover 时它们复用
`beveled_button_frame_at`, 呈现 26×26、圆角 5、带底部高光的"浮起小方块"。

目标: 让 Windows 上的这三个按钮在 hover 时呈 **Windows 10/11 原生风格**——
- 按钮为 **满标题栏高、约 46px 宽、彼此无缝** 的一排;
- 贴齐窗口 **右上角**(顶到边、右到边);
- hover 背景为 **直角色块, 覆盖到窗口边缘**;
- 配色: 最小化/最大化 hover 淡灰、图标原色; 关闭 hover 红底白字。

macOS 走原生交通灯, 完全不受本设计影响。

## 设计

### 1. 几何: caption 按钮独立成右上角 Area

新增常量:
- `WINDOW_CAPTION_BUTTON_WIDTH: f32 = 46.0` — 单个 caption 按钮宽度;
- caption 按钮高度 = `TITLEBAR_BOTTOM_OFFSET`(28.0), 即满标题栏高。

把三个窗口按钮从 `custom_titlebar` Frame 中拆出, 放进一个独立的 Area:
- `fixed_pos` 定位到窗口右上角: `x = screen_rect.right() - 3*W`, `y = screen_rect.top()`;
- `order(Foreground)`, 且在 `show_resize_handles` **之后** 绘制, 保证右上角点击落到
  关闭按钮而非 NE resize 热区;
- Area 内用水平布局, `item_spacing.x = 0`、无 Frame 内边距 → 三个按钮相邻无缝;
- 按钮顺序: 最小化、最大化、关闭(关闭最右, 贴窗口右上角)。

`custom_titlebar` Frame 仍负责 拖拽区 + 标题 + app 按钮(☰⚙), 并在右侧预留 caption
簇宽度。为此:
- `window_buttons_width` 改为 `WINDOW_CAPTION_BUTTON_WIDTH * 3.0`(去掉原来的
  `item_spacing.x * 2.0`, 因 caption 簇内部已无间距);
- app 按钮因 `total_trailing_buttons_width` 预留了 caption 宽度, 自然落在 caption 簇
  左侧, 中间靠 Frame 的 8px 右边距形成视觉分隔(即 `☰ ⚙  – ▢ ✕` 的那段间隙)。
- 原 `window_buttons(ui, ...)` 从 `titlebar_contents` 的行内布局中移除, 不再在 Frame
  内渲染。

### 2. Hover 背景: 直角、覆盖到边缘

在 caption Area 内新增独立绘制函数(不复用 `beveled_button_frame_at`):
- 按钮命中矩形 = 整块 `W × TITLEBAR_BOTTOM_OFFSET`, 顶到边、右到边;
- **最小化 / 最大化** hover 填充 `#3A3A3A`, 按下 `#2F2F2F`, 图标用 `text_color()`;
- **关闭** hover 填充 `#C42B1C`, 图标白色;
- 非 hover: 无填充, 仅居中图标(沿用 `paint_centered_symbol` 精确居中);
- 所有填充乘以标题栏 `opacity`(`gamma_multiply`)以随淡入淡出。

### 3. 唯一例外: 关闭按钮右上角跟随窗口圆角

关闭按钮贴窗口右上角。窗口 **浮动** 时右上角为圆角(Win11 由 DWM 裁剪, Win10 由
`paint_window_background` 画的圆角背景, 半径 `WINDOW_CORNER_RADIUS = 8`)。若关闭
hover 填成纯直角, Win10 上会在圆角外的透明区涂出红色直角尖角(露到桌面)。

规则: 关闭按钮 hover 填充用 **每角不同半径**:
- `ne`(右上)角 = 窗口浮动时 `WINDOW_CORNER_RADIUS`, 最大化/全屏时 `0`;
- 其余三角恒为 `0`。

最小化、最大化按钮不在窗口角上, 四角恒直角。这与 Windows 自身行为一致: 色块铺到
边, 唯独贴窗口轮廓的那一个角跟随圆角。

### 4. 淡入淡出与命中测试

- caption Area 复用现有 `titlebar_opacity(ctx, screen_rect)`: 鼠标离开淡出, 全屏时仅
  顶部热区显出; `opacity <= 0.01` 时与标题栏一起跳过绘制(提前 return)。
- Area 为 `Foreground` 且在 resize handles 之后 `show`, 使关闭按钮在右上角赢得命中,
  代价是牺牲精确的 NE 角落 resize(与原实现相比该角本就被窗口按钮占据)。

## 测试(源断言, 沿用现有 titlebar 测试风格)

`titlebar.rs` 已有的测试全部以 `include_str!("titlebar.rs")` 做源码断言。更新/新增:
- caption 按钮几何: 断言存在 `WINDOW_CAPTION_BUTTON_WIDTH`、值为 `46.0`;
  `window_buttons_width` 使用 `WINDOW_CAPTION_BUTTON_WIDTH * 3.0` 且不含 `item_spacing`;
- caption Area 独立: 断言存在一个新的窗口按钮 Area(如 `Id::new("window_caption_buttons")`),
  `fixed_pos`、`Order::Foreground`, 且在 `show_resize_handles` 之后调用;
- hover 配色: 断言最小化/最大化 hover 用 `0x3A,0x3A,0x3A`、关闭用 `196, 43, 28`(现有断言保留)、
  按下用 `0x2F,0x2F,0x2F`;
- 直角: 断言 caption 填充默认 `CornerRadius` 各角为 0(如通过 `CornerRadius { ne, .. }` 构造),
  关闭按钮 `ne` 在浮动时取 `WINDOW_CORNER_RADIUS`;
- 更新受影响的现有测试(`titlebar_renders_window_control_buttons_on_non_macos`、
  `titlebar_supports_window_drag_buttons_and_resize_handles` 等), 使其反映新结构;
  保留三个窗口意图字段(`minimize/maximize/close`)与图标符号断言。

## 非目标

- 不改 macOS 分支;
- 不改 app 按钮(☰⚙)的样式与尺寸;
- 不改窗口拖拽、resize、全屏热区的既有行为(除右上角命中优先级);
- 不引入新依赖。
