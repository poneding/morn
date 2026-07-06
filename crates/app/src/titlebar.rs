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

        #[cfg(not(target_os = "macos"))]
        {
            window_buttons(ui, opacity, actions);
        }
    });
}

/// 标题栏右侧所有按钮的总宽度, 用于为拖拽区域预留空间。
/// macOS 仅 app 按钮; 其它平台额外加上窗口控制按钮。
fn total_trailing_buttons_width(ui: &egui::Ui) -> f32 {
    let app_width = titlebar_app_buttons_width(ui);
    #[cfg(not(target_os = "macos"))]
    {
        app_width + window_buttons_width(ui)
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
fn window_buttons_width(ui: &egui::Ui) -> f32 {
    // 最小化 + 最大化 + 关闭三个按钮, 复用与 app 按钮相同的方形尺寸与间距。
    TITLEBAR_BUTTON_SIZE * 3.0 + ui.spacing().item_spacing.x * 2.0
}

#[cfg(not(target_os = "macos"))]
fn window_buttons(ui: &mut egui::Ui, opacity: f32, actions: &mut TitlebarActions) {
    let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));

    let minimize = window_control_button(
        ui,
        crate::symbols::WINDOW_MINIMIZE,
        false,
        WindowButtonAccent::Normal,
        opacity,
    );
    if minimize.clicked() {
        actions.minimize = true;
    }

    let maximize_icon = if maximized {
        crate::symbols::WINDOW_RESTORE
    } else {
        crate::symbols::WINDOW_MAXIMIZE
    };
    // 最大化按钮不记录"选中"状态(它不是像 ☰/⚙ 那样的开关); 只有图标在 ▢/❐ 间
    // 切换, 避免按下后整按钮常驻 selection 高亮色。
    let maximize = window_control_button(
        ui,
        maximize_icon,
        false,
        WindowButtonAccent::Normal,
        opacity,
    );
    if maximize.clicked() {
        actions.maximize = true;
    }

    let close = window_control_button(
        ui,
        crate::symbols::WINDOW_CLOSE,
        false,
        WindowButtonAccent::Close,
        opacity,
    );
    if close.clicked() {
        actions.close = true;
    }
}

/// 关闭按钮 hover 时的强调色, 区别于普通窗口按钮。
#[cfg(not(target_os = "macos"))]
enum WindowButtonAccent {
    Normal,
    Close,
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

#[cfg(not(target_os = "macos"))]
fn window_control_button(
    ui: &mut egui::Ui,
    icon: &'static str,
    active: bool,
    accent: WindowButtonAccent,
    opacity: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(TITLEBAR_BUTTON_SIZE, TITLEBAR_BUTTON_SIZE),
        egui::Sense::click_and_drag(),
    );
    let hovered = response.hovered() || response.is_pointer_button_down_on();
    let close_hover = matches!(accent, WindowButtonAccent::Close) && hovered && !active;

    // 关闭按钮 hover 时用红色 frame, 其余情况复用与其他标题栏按钮完全一致的
    // beveled frame(同尺寸/同圆角5/同底部高光), 避免"更大、圆角更小"的视觉差异。
    if close_hover {
        paint_close_hover_frame(ui, rect, &response, opacity);
        // 关闭 hover 用白色字, 与红色底对比; galley 精确居中保证 ✕ 与其它字
        // 形同高同宽, 不再显得更大或偏位。
        paint_centered_symbol(ui, rect, icon, egui::Color32::WHITE.gamma_multiply(opacity));
        return response;
    }

    if active || hovered {
        crate::visuals::beveled_button_frame_at(ui, rect, &response, active, opacity);
    }

    let icon_color = if active {
        ui.visuals().selection.stroke.color.gamma_multiply(opacity)
    } else {
        ui.visuals().text_color().gamma_multiply(opacity)
    };
    paint_centered_symbol(ui, rect, icon, icon_color);
    response
}

/// 关闭按钮 hover 时的红色 frame: 与 `beveled_button_frame_at` 几何完全一致
/// (同 rect.shrink(0.5)、同 CONTROL_CORNER_RADIUS 圆角、同底部高光线), 仅颜色改红,
/// 保证关闭按钮 hover 时尺寸/圆角与其他按钮一致, 只是配色不同。
#[cfg(not(target_os = "macos"))]
fn paint_close_hover_frame(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    opacity: f32,
) {
    use crate::visuals::{BEVEL_SHADOW_ALPHA, CONTROL_CORNER_RADIUS};
    let frame_rect = rect.shrink(0.5);
    let radius = egui::CornerRadius::same(CONTROL_CORNER_RADIUS);
    let fill = egui::Color32::from_rgb(196, 43, 28).gamma_multiply(opacity);
    ui.painter().rect_filled(frame_rect, radius, fill);
    ui.painter().rect_stroke(
        frame_rect,
        radius,
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgb(140, 30, 20).gamma_multiply(opacity),
        ),
        egui::StrokeKind::Inside,
    );
    // 与 beveled_button_frame_at 一致的底部阴影线, 保持立体感统一。
    let pressed = response.is_pointer_button_down_on();
    let bottom_color = egui::Color32::from_black_alpha(if pressed {
        BEVEL_SHADOW_ALPHA / 2
    } else {
        BEVEL_SHADOW_ALPHA
    })
    .gamma_multiply(opacity);
    ui.painter().line_segment(
        [
            egui::pos2(frame_rect.left() + 1.5, frame_rect.bottom() - 1.0),
            egui::pos2(frame_rect.right() - 1.5, frame_rect.bottom() - 1.0),
        ],
        egui::Stroke::new(1.0, bottom_color),
    );
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

        // 最小化/最大化/关闭三个窗口控制按钮仅非 macOS 渲染。
        assert!(source.contains("fn window_buttons("));
        assert!(source.contains("fn window_control_button("));
        assert!(source.contains("crate::symbols::WINDOW_MINIMIZE"));
        assert!(source.contains("crate::symbols::WINDOW_MAXIMIZE"));
        assert!(source.contains("crate::symbols::WINDOW_RESTORE"));
        assert!(source.contains("crate::symbols::WINDOW_CLOSE"));
        // 最大化按钮按当前 maximized 状态在 ▢/❐ 之间切换, 但按钮本身不常驻选中高亮。
        assert!(source.contains("i.viewport().maximized.unwrap_or(false)"));
        assert!(source.contains("let maximize = window_control_button("));
        assert!(source.contains("maximize_icon,"));
        assert!(source.contains("false,"));
        // TitlebarActions 暴露三个窗口控制意图。
        assert!(source.contains("pub minimize: bool"));
        assert!(source.contains("pub maximize: bool"));
        assert!(source.contains("pub close: bool"));
        // 关闭按钮 hover 红色高亮, 图标按字形包围盒精确居中。
        assert!(source.contains("WindowButtonAccent::Close"));
        assert!(source.contains("196, 43, 28"));
        assert!(source.contains("fn paint_centered_symbol"));
        assert!(source.contains("layout_no_wrap"));
        // 标题栏布局为窗口按钮预留宽度。
        assert!(source.contains("fn window_buttons_width("));
        assert!(source.contains("TITLEBAR_BUTTON_SIZE * 3.0"));
        // 关闭按钮 hover 用独立的 paint_close_hover_frame, 与其他按钮同圆角(CONTROL_CORNER_RADIUS),
        // 不再叠半径2的额外红色层。
        assert!(source.contains("fn paint_close_hover_frame"));
        assert!(source.contains("CONTROL_CORNER_RADIUS"));
        assert!(!source.contains("rect_filled(rect.shrink(0.5), 2,"));
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
}
