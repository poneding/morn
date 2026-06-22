//! Titlebar app-action overlay.
//!
//! On macOS this mirrors Translator's native overlay titlebar: the OS keeps the
//! real traffic lights and window frame while egui paints the app actions in the
//! titlebar content area. Other platforms keep the existing custom borderless
//! titlebar path.

use eframe::egui;
use rust_i18n::t;

const TITLEBAR_HEIGHT: f32 = 28.0;
const TITLEBAR_BUTTON_SIZE: f32 = crate::visuals::ICON_BUTTON_SIZE;
const TITLEBAR_INNER_MARGIN_X: i8 = 8;
const TITLEBAR_INNER_MARGIN_Y: i8 = 3;
#[cfg(not(target_os = "macos"))]
const TITLEBAR_FADE_TIME: f32 = 0.12;
#[cfg(target_os = "macos")]
const NATIVE_TRAFFIC_LIGHT_SPACER_WIDTH: f32 = 72.0;
const TITLEBAR_TRAILING_MARGIN: f32 = crate::visuals::FLOATING_PANEL_INNER_MARGIN_X as f32;
/// 标题栏这个 UI 元素的底部位置。浮层与顶部 Title 的视觉间距应从这里开始算,
/// 而不是从窗口顶部边框开始算。
pub const TITLEBAR_BOTTOM_OFFSET: f32 = TITLEBAR_HEIGHT + (TITLEBAR_INNER_MARGIN_Y as f32) * 2.0;
const WINDOW_CORNER_RADIUS: u8 = crate::visuals::PANEL_CORNER_RADIUS;
#[cfg(not(target_os = "macos"))]
const WINDOW_RESIZE_HANDLE: f32 = 6.0;

#[derive(Default)]
pub struct TitlebarActions {
    // The titlebar returns app-level intents instead of mutating PlayerApp while
    // the overlay is being painted.
    pub toggle_playlist: bool,
    pub toggle_settings: bool,
}

pub fn window_corner_radius() -> f32 {
    WINDOW_CORNER_RADIUS as f32
}

#[cfg(not(target_os = "macos"))]
fn paint_window_background(ctx: &egui::Context) {
    // Transparent native windows need an explicit rounded background; otherwise
    // the video surface can leave square transparent corners during resize.
    let screen_rect = ctx.content_rect();
    let visuals = &ctx.global_style().visuals;
    let painter = ctx.layer_painter(egui::LayerId::background());
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

        // Show resize handles around the edge
        show_resize_handles(ctx, screen_rect);
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
            egui::Frame::NONE
                .fill(titlebar_fill(ui))
                // 标题栏上方两个圆角与窗口一致，下方两个是直角
                .corner_radius(egui::CornerRadius {
                    nw: WINDOW_CORNER_RADIUS,
                    ne: WINDOW_CORNER_RADIUS,
                    sw: 0,
                    se: 0,
                })
                .inner_margin(egui::Margin::symmetric(
                    TITLEBAR_INNER_MARGIN_X,
                    TITLEBAR_INNER_MARGIN_Y,
                ))
                .multiply_with_opacity(opacity)
                .show(ui, |ui| {
                    ui.set_height(TITLEBAR_HEIGHT);
                    titlebar_contents(ui, title, show_playlist, show_settings, opacity, &mut actions);
                });
        });

    actions
}

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
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let visible = pointer_pos.is_some_and(|pos| screen_rect.contains(pos));
        ctx.animate_bool_with_time(
            egui::Id::new("custom_titlebar_opacity"),
            visible,
            TITLEBAR_FADE_TIME,
        )
    }
}

fn titlebar_fill(ui: &egui::Ui) -> egui::Color32 {
    ui.visuals().window_fill.to_opaque()
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
        let buttons_width = titlebar_app_buttons_width(ui);
        let drag_width = (ui.available_width() - buttons_width).max(0.0);
        drag_region(ui, title, drag_width, opacity);
        titlebar_app_buttons(ui, show_playlist, show_settings, opacity, actions);
    });
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
        assert!(source.contains(
            "TITLEBAR_TRAILING_MARGIN: f32 = crate::visuals::FLOATING_PANEL_INNER_MARGIN_X as f32"
        ));
        assert!(source.contains("ui.spacing().item_spacing.x * 2.0"));
        // Both action buttons share one fixed square size.
        assert!(source.contains("egui::vec2(TITLEBAR_BUTTON_SIZE, TITLEBAR_BUTTON_SIZE)"));
        assert!(source.contains("TITLEBAR_TRAILING_MARGIN"));
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
