//! Titlebar app-action overlay.
//!
//! On macOS this mirrors Translator's native overlay titlebar: the OS keeps the
//! real traffic lights and window frame while egui paints the app actions in the
//! titlebar content area. Other platforms keep the existing custom borderless
//! titlebar path.

use eframe::egui;
use rust_i18n::t;

const TITLEBAR_HEIGHT: f32 = 26.0;
const TITLEBAR_BUTTON_SIZE: f32 = crate::visuals::ICON_BUTTON_SIZE;
const TITLEBAR_INNER_MARGIN_X: i8 = 8;
const TITLEBAR_INNER_MARGIN_Y: i8 = 1;
#[cfg(not(target_os = "macos"))]
const TITLEBAR_FADE_TIME: f32 = 0.12;
#[cfg(target_os = "macos")]
const NATIVE_TRAFFIC_LIGHT_SPACER_WIDTH: f32 = 72.0;
const TITLEBAR_TRAILING_MARGIN: f32 = 4.0;
/// 标题栏这个 UI 元素的底部位置。浮层与顶部 Title 的视觉间距应从这里开始算,
/// 而不是从窗口顶部边框开始算。
pub const TITLEBAR_BOTTOM_OFFSET: f32 = TITLEBAR_HEIGHT + (TITLEBAR_INNER_MARGIN_Y as f32) * 2.0;
#[cfg(not(target_os = "macos"))]
const WINDOW_CORNER_RADIUS: u8 = crate::visuals::PANEL_CORNER_RADIUS;
#[cfg(not(target_os = "macos"))]
const WINDOW_RESIZE_HANDLE: f32 = 6.0;
#[cfg(not(target_os = "macos"))]
const WINDOW_CAPTION_BUTTON_WIDTH: f32 = 46.0;

#[derive(Default)]
pub struct TitlebarActions {
    // The titlebar returns app-level intents instead of mutating PlayerApp while
    // the overlay is being painted.
    pub toggle_playlist: bool,
    pub toggle_settings: bool,
    // 非 macOS 无边框窗口下自绘的窗口控制按钮触发的意图, 由 app 转发为
    // ViewportCommand。macOS 走原生交通灯, 这三个字段恒为 false。
    pub minimize: bool,
    pub maximize: bool,
    pub close: bool,
}

/// 窗口是否处于"浮动"状态(非全屏、非最大化)。圆角、描边、边缘 resize handles
/// 只在浮动窗口上有意义: 全屏/最大化时窗口铺满屏幕(或工作区)。
#[cfg(not(target_os = "macos"))]
fn window_is_floating(ctx: &egui::Context) -> bool {
    let (fullscreen, maximized) = ctx.input(|i| {
        let viewport = i.viewport();
        (
            viewport.fullscreen.unwrap_or(false),
            viewport.maximized.unwrap_or(false),
        )
    });
    !fullscreen && !maximized
}

#[cfg(not(target_os = "macos"))]
fn paint_window_background(ctx: &egui::Context) {
    // Transparent native windows need an explicit rounded background; otherwise
    // the video surface can leave square transparent corners during resize.
    let screen_rect = ctx.content_rect();
    let visuals = &ctx.global_style().visuals;
    let painter = ctx.layer_painter(egui::LayerId::background());
    if !window_is_floating(ctx) {
        // 全屏/最大化: 圆角会在屏幕四角把桌面透出来, 描边贴着屏幕边缘也很突兀,
        // 退化为直角、无描边的整幅实底。
        painter.rect_filled(screen_rect, 0, visuals.panel_fill);
        return;
    }
    painter.rect_filled(screen_rect, WINDOW_CORNER_RADIUS, visuals.panel_fill);
    painter.rect_stroke(
        screen_rect.shrink(0.5),
        WINDOW_CORNER_RADIUS,
        visuals.widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
}

#[cfg(not(target_os = "macos"))]
fn show_resize_handles(ctx: &egui::Context, screen_rect: egui::Rect) {
    // 8 个边/角 handle 合并到 1 个 Foreground Area, 8 倍的合成/布局/
    // 命中测试缩成 1 份, resize 拖动时收益最明显。
    egui::Area::new(egui::Id::new("window_resize_handles"))
        .fixed_pos(screen_rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let origin = screen_rect.min.to_vec2();
            for (tag, direction, screen_handle_rect) in resize_handles(screen_rect) {
                let local_rect = screen_handle_rect.translate(-origin);
                let id = ui.id().with(("resize_handle", tag));
                // Handles are transparent hit regions; BeginResize delegates the
                // actual drag to the platform window.
                let response = ui.interact(local_rect, id, egui::Sense::click_and_drag());
                if response.drag_started() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            }
        });
}

#[cfg(not(target_os = "macos"))]
fn resize_handles(
    screen_rect: egui::Rect,
) -> [(&'static str, egui::ResizeDirection, egui::Rect); 8] {
    // Edges exclude the corner squares so diagonal resize targets win at corners.
    let h = WINDOW_RESIZE_HANDLE;
    let left = screen_rect.left();
    let right = screen_rect.right();
    let top = screen_rect.top();
    let bottom = screen_rect.bottom();

    [
        (
            "n",
            egui::ResizeDirection::North,
            egui::Rect::from_min_max(egui::pos2(left + h, top), egui::pos2(right - h, top + h)),
        ),
        (
            "s",
            egui::ResizeDirection::South,
            egui::Rect::from_min_max(
                egui::pos2(left + h, bottom - h),
                egui::pos2(right - h, bottom),
            ),
        ),
        (
            "w",
            egui::ResizeDirection::West,
            egui::Rect::from_min_max(egui::pos2(left, top + h), egui::pos2(left + h, bottom - h)),
        ),
        (
            "e",
            egui::ResizeDirection::East,
            egui::Rect::from_min_max(
                egui::pos2(right - h, top + h),
                egui::pos2(right, bottom - h),
            ),
        ),
        (
            "nw",
            egui::ResizeDirection::NorthWest,
            egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + h, top + h)),
        ),
        (
            "ne",
            egui::ResizeDirection::NorthEast,
            egui::Rect::from_min_max(egui::pos2(right - h, top), egui::pos2(right, top + h)),
        ),
        (
            "sw",
            egui::ResizeDirection::SouthWest,
            egui::Rect::from_min_max(egui::pos2(left, bottom - h), egui::pos2(left + h, bottom)),
        ),
        (
            "se",
            egui::ResizeDirection::SouthEast,
            egui::Rect::from_min_max(egui::pos2(right - h, bottom - h), egui::pos2(right, bottom)),
        ),
    ]
}

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

pub fn show_custom_titlebar(
    ctx: &egui::Context,
    title: &str,
    show_playlist: bool,
    show_settings: bool,
) -> TitlebarActions {
    let mut actions = TitlebarActions::default();
    let screen_rect = ctx.content_rect();

    #[cfg(not(target_os = "macos"))]
    {
        // Paint window background with rounded corners first (for transparent window)
        paint_window_background(ctx);

        // 全屏/最大化窗口不可拖边缩放, 只有浮动窗口显示边缘 resize handles。
        if window_is_floating(ctx) {
            show_resize_handles(ctx, screen_rect);
        }
    }

    let opacity = titlebar_opacity(ctx, screen_rect);
    if opacity <= 0.01 {
        return actions;
    }

    egui::Area::new(egui::Id::new("custom_titlebar"))
        .fixed_pos(screen_rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_width(screen_rect.width());
            // 标题栏 Frame 不画自己的 fill 背景: 窗口圆角填充已由 paint_window_background
            // 统一负责(整窗 panel_fill + 四角圆角 + 整圈描边)。若 Frame 再叠一层不透明 fill,
            // 其 ne 圆角在渲染时可能未完美对齐 background 的圆角缺角, 把右上角圆角缺角填成
            // 直角灰色。Frame 透明即消除该覆盖, 四角圆角完全由 background 单层保证。
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
    // 必须在 show_resize_handles 之后绘制: 关闭按钮 rect 覆盖了右上角 ne resize handle,
    // 同为 Foreground 时 egui 把指针优先给后添加的 Area, 故此调用顺序保证右上角命中
    // 关闭按钮而非 ne resize。切勿把本调用移到 resize handles 之前。
    #[cfg(not(target_os = "macos"))]
    show_window_caption_buttons(ctx, screen_rect, opacity, &mut actions);

    actions
}

#[cfg(not(target_os = "macos"))]
/// 全屏时露出标题栏的顶部热区高度: 鼠标进入窗口顶部这条带才提示标题栏,
/// 既给用户退出全屏/切播放列表的入口, 又不在观影时整条浮出遮挡画面。
const FULLSCREEN_TITLEBAR_REVEAL_EDGE: f32 = 4.0;

fn titlebar_opacity(ctx: &egui::Context, screen_rect: egui::Rect) -> f32 {
    #[cfg(target_os = "macos")]
    {
        let _ = screen_rect;
        if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
            0.0
        } else {
            1.0
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let visible = if fullscreen {
            // 全屏: 只在鼠标贴近屏幕顶部热区时才提示标题栏(像 YouTube/IINA),
            // 移开后淡出, 不长时间遮画面。退出全屏仍可走 Esc/回车。
            pointer_pos.is_some_and(|pos| pos.y <= FULLSCREEN_TITLEBAR_REVEAL_EDGE)
        } else {
            pointer_pos.is_some_and(|pos| screen_rect.contains(pos))
        };
        ctx.animate_bool_with_time(
            egui::Id::new("custom_titlebar_opacity"),
            visible,
            TITLEBAR_FADE_TIME,
        )
    }
}

fn titlebar_contents(
    ui: &mut egui::Ui,
    title: &str,
    show_playlist: bool,
    show_settings: bool,
    opacity: f32,
    actions: &mut TitlebarActions,
) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        #[cfg(target_os = "macos")]
        {
            // Keep egui content out of the native traffic-light cluster, matching
            // Translator's `.mac-traffic-light-spacer`.
            ui.add_space(NATIVE_TRAFFIC_LIGHT_SPACER_WIDTH);
        }

        #[cfg(not(target_os = "macos"))]
        {
            ui.add_space(0.0);
        }

        // App actions live on the right; the rest of the strip is the drag region.
        let buttons_width = total_trailing_buttons_width(ui);
        let drag_width = (ui.available_width() - buttons_width).max(0.0);
        drag_region(ui, title, drag_width, opacity);
        titlebar_app_buttons(ui, show_playlist, show_settings, opacity, actions);
    });
}

/// 标题栏右侧所有按钮的总宽度, 用于为拖拽区域预留空间。
/// macOS 仅 app 按钮; 其它平台额外加上窗口控制按钮。
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

fn toggle_maximized(ctx: &egui::Context) {
    // egui exposes maximized state as an optional viewport value; missing state is
    // treated as not maximized so the command remains a toggle.
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
}

fn titlebar_app_buttons_width(ui: &egui::Ui) -> f32 {
    TITLEBAR_BUTTON_SIZE * 2.0 + ui.spacing().item_spacing.x * 2.0 + TITLEBAR_TRAILING_MARGIN
}

fn titlebar_app_buttons(
    ui: &mut egui::Ui,
    show_playlist: bool,
    show_settings: bool,
    opacity: f32,
    actions: &mut TitlebarActions,
) {
    let playlist = titlebar_icon_button(ui, crate::symbols::PLAYLIST, show_playlist, opacity)
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("playlist"),
            crate::shortcuts::playlist_shortcut_label(),
        ));
    if playlist.clicked() {
        actions.toggle_playlist = true;
    }

    let settings = titlebar_icon_button(ui, crate::symbols::SETTINGS, show_settings, opacity)
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("settings"),
            crate::shortcuts::settings_shortcut_label(),
        ));
    if settings.clicked() {
        actions.toggle_settings = true;
    }

    ui.add_space(TITLEBAR_TRAILING_MARGIN);
}

#[cfg(not(target_os = "macos"))]
fn window_buttons_width() -> f32 {
    // 三个按钮无缝相邻(Windows 原生), 簇宽 = 3 × 单按钮宽。
    WINDOW_CAPTION_BUTTON_WIDTH * 3.0
}

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

/// 在按钮 rect 内把单个符号字形按其 galley 包围盒精确居中。
/// `painter.text(.., Align2::CENTER_CENTER, ..)` 是按文本行盒(baseline+advance)
/// 对齐, 不同字形(–/▢/❐/✕)的 side bearing 各异, 视觉上会偏左/偏上。这里改成
/// 测量 galley 后用左上角偏移定位, 让字形包围盒严格落在 rect 中心。
#[cfg(not(target_os = "macos"))]
fn paint_centered_symbol(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: &'static str,
    color: egui::Color32,
) {
    let galley =
        ui.painter()
            .layout_no_wrap(icon.to_owned(), egui::FontId::proportional(14.0), color);
    let pos = rect.center() - galley.size() * 0.5;
    ui.painter().galley(pos, galley, color);
}

fn titlebar_icon_button(
    ui: &mut egui::Ui,
    icon: &'static str,
    selected: bool,
    opacity: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(TITLEBAR_BUTTON_SIZE, TITLEBAR_BUTTON_SIZE),
        egui::Sense::click_and_drag(),
    );
    let icon_color = if selected {
        ui.visuals().selection.stroke.color.gamma_multiply(opacity)
    } else {
        ui.visuals().text_color().gamma_multiply(opacity)
    };
    if selected || response.hovered() || response.is_pointer_button_down_on() {
        crate::visuals::beveled_button_frame_at(ui, rect, &response, selected, opacity);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(14.0),
        icon_color,
    );
    response
}

fn drag_region(ui: &mut egui::Ui, title: &str, width: f32, opacity: f32) {
    // Native titlebar drag stops working once content extends under it, so the
    // strip itself starts a window drag; double-click mirrors the native zoom.
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, TITLEBAR_HEIGHT),
        egui::Sense::click_and_drag(),
    );
    let displayed_title = if title.is_empty() { "Morn" } else { title };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        displayed_title,
        egui::FontId::proportional(12.0),
        ui.visuals()
            .widgets
            .noninteractive
            .fg_stroke
            .color
            .gamma_multiply(opacity),
    );
    if response.double_clicked() {
        toggle_maximized(ui.ctx());
    } else if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn titlebar_supports_window_drag_buttons_and_resize_handles() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("ViewportCommand::StartDrag"));
        assert!(source.contains("ViewportCommand::Maximized"));
        assert!(source.contains("ViewportCommand::BeginResize"));
        // Non-macOS custom titlebar still owns background and resize handles.
        assert!(source.contains("show_resize_handles"));
        assert!(source.contains("paint_window_background"));
    }

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

    #[test]
    fn window_minimize_icon_uses_short_en_dash() {
        // 最小化用 en dash(短横杠 –) 而非 em dash(—), 视觉更紧凑。
        assert_eq!(crate::symbols::WINDOW_MINIMIZE, "–");
        assert_ne!(crate::symbols::WINDOW_MINIMIZE, "—");
    }

    #[test]
    fn titlebar_frame_is_transparent_to_let_window_background_own_corners() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        // 标题栏 Frame 不再画自己的 fill 背景: 四角圆角统一由 paint_window_background
        // 单层负责, 避免 Frame 不透明 fill 的 ne 圆角与 background 圆角缺角错位、把右上角
        // 填成直角。标题栏 Frame 应只剩 inner_margin, 无 fill/corner_radius。
        assert!(!source.contains("fn titlebar_fill"));
        assert!(!source.contains(".fill(titlebar_fill"));
        let titlebar_area_block = source
            .split("egui::Area::new(egui::Id::new(\"custom_titlebar\"))")
            .nth(1)
            .unwrap()
            .split("titlebar_contents")
            .next()
            .unwrap();
        assert!(
            !titlebar_area_block.contains(".fill("),
            "标题栏 Frame 不应再设 fill"
        );
        assert!(
            !titlebar_area_block.contains("corner_radius"),
            "标题栏 Frame 不应再设 corner_radius"
        );
    }

    #[test]
    fn titlebar_hosts_playlist_and_settings_actions_on_right() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("pub struct TitlebarActions"));
        assert!(source.contains("toggle_playlist"));
        assert!(source.contains("toggle_settings"));
        assert!(source.contains("titlebar_app_buttons"));
        assert!(source.contains("crate::symbols::PLAYLIST"));
        assert!(source.contains("crate::symbols::SETTINGS"));
        assert!(source.contains("titlebar_icon_button(ui, crate::symbols::SETTINGS, show_settings"));
        assert!(source.contains("beveled_button_frame_at"));
        assert!(source.contains("ui.visuals().selection.stroke.color"));
        assert_eq!(super::TITLEBAR_TRAILING_MARGIN, 4.0);
        assert_eq!(super::TITLEBAR_BOTTOM_OFFSET, 28.0);
        assert!(source.contains("ui.spacing().item_spacing.x * 2.0"));
        // Both action buttons share one fixed square size.
        assert!(source.contains("egui::vec2(TITLEBAR_BUTTON_SIZE, TITLEBAR_BUTTON_SIZE)"));
        assert!(source.contains("TITLEBAR_TRAILING_MARGIN"));
    }

    #[test]
    fn fullscreen_uses_hot_edge_reveal_off_macos_and_squares_window_chrome() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        let opacity_source = source
            .split("fn titlebar_opacity")
            .nth(1)
            .unwrap()
            .split("fn titlebar_contents")
            .next()
            .unwrap();

        // macOS 全屏仍完全隐藏标题栏。
        let macos_branch = opacity_source
            .split("#[cfg(target_os = \"macos\")]")
            .nth(1)
            .unwrap()
            .split("#[cfg(not(target_os = \"macos\"))]")
            .next()
            .unwrap();
        assert!(macos_branch.contains("viewport().fullscreen.unwrap_or(false)"));
        assert!(macos_branch.contains("0.0"));

        // 非 macOS 全屏只在顶部热区显出标题栏, 平时淡出不遮挡画面。
        let non_macos_branch = opacity_source
            .split("#[cfg(not(target_os = \"macos\"))]")
            .nth(1)
            .unwrap();
        assert!(non_macos_branch.contains("let fullscreen"));
        assert!(non_macos_branch.contains("FULLSCREEN_TITLEBAR_REVEAL_EDGE"));
        assert!(non_macos_branch.contains("pos.y <= FULLSCREEN_TITLEBAR_REVEAL_EDGE"));

        // 全屏/最大化: 窗口背景退化为直角无描边, 不再显示边缘 resize handles。
        assert!(source.contains("fn window_is_floating"));
        assert!(source.contains("if window_is_floating(ctx) {"));
        let background_source = source
            .split("fn paint_window_background")
            .nth(1)
            .unwrap()
            .split("fn show_resize_handles")
            .next()
            .unwrap();
        assert!(background_source.contains("if !window_is_floating(ctx)"));
        assert!(background_source.contains("rect_filled(screen_rect, 0,"));
    }

    #[test]
    fn macos_titlebar_uses_native_traffic_lights_with_spacer() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("NATIVE_TRAFFIC_LIGHT_SPACER_WIDTH"));
        assert!(source.contains("ui.add_space(NATIVE_TRAFFIC_LIGHT_SPACER_WIDTH)"));
        assert!(source.contains("viewport().fullscreen.unwrap_or(false)"));
        assert!(source.contains("0.0"));
        assert!(!source.contains("traffic_light_button"));
        assert!(!source.contains("TrafficLightSymbol"));
        assert!(!source.contains("draw_traffic_light_symbol"));
    }

    #[test]
    fn titlebar_action_buttons_do_not_use_default_button_frames() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let button_source = source
            .split("fn titlebar_icon_button")
            .nth(1)
            .unwrap()
            .split("fn drag_region")
            .next()
            .unwrap();
        assert!(source.contains("fn titlebar_icon_button"));
        assert!(source.contains("ui.allocate_exact_size"));
        assert!(button_source.contains("egui::Sense::click_and_drag()"));
        assert!(!source.contains("egui::Button::new"));
    }

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
}
