use eframe::egui;
use rust_i18n::t;

const TITLEBAR_HEIGHT: f32 = 28.0;
const TITLEBAR_OPACITY: f32 = 0.86;
const TITLEBAR_BUTTON_SIZE: f32 = 22.0;
const WINDOW_CORNER_RADIUS: u8 = 10;
const WINDOW_RESIZE_HANDLE: f32 = 6.0;
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHT_SYMBOL_COLOR: egui::Color32 = egui::Color32::from_black_alpha(150);

#[derive(Default)]
pub struct TitlebarActions {
    pub toggle_playlist: bool,
    pub toggle_settings: bool,
}

pub fn titlebar_visible(pointer_pos: Option<egui::Pos2>, screen_rect: egui::Rect) -> bool {
    pointer_pos.is_some_and(|pos| screen_rect.contains(pos))
}

pub fn paint_window_background(ctx: &egui::Context) {
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

pub fn show_custom_titlebar(ctx: &egui::Context, show_playlist: bool) -> TitlebarActions {
    let mut actions = TitlebarActions::default();
    let screen_rect = ctx.content_rect();
    show_resize_handles(ctx, screen_rect);

    let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
    if !titlebar_visible(pointer_pos, screen_rect) {
        return actions;
    }

    egui::Area::new(egui::Id::new("custom_titlebar"))
        .fixed_pos(screen_rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_width(screen_rect.width());
            egui::Frame::NONE
                .fill(ui.visuals().panel_fill)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(egui::CornerRadius {
                    nw: WINDOW_CORNER_RADIUS,
                    ne: WINDOW_CORNER_RADIUS,
                    sw: 0,
                    se: 0,
                })
                .inner_margin(egui::Margin::symmetric(10, 3))
                .multiply_with_opacity(TITLEBAR_OPACITY)
                .show(ui, |ui| {
                    ui.set_height(TITLEBAR_HEIGHT);
                    titlebar_contents(ui, show_playlist, &mut actions);
                });
        });

    actions
}

#[cfg(target_os = "macos")]
fn titlebar_contents(ui: &mut egui::Ui, show_playlist: bool, actions: &mut TitlebarActions) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let close = traffic_light_button(ui, egui::Color32::from_rgb(255, 95, 87), "×", "Close");
        let minimize =
            traffic_light_button(ui, egui::Color32::from_rgb(255, 189, 46), "−", "Minimize");
        let maximize =
            traffic_light_button(ui, egui::Color32::from_rgb(39, 201, 63), "+", "Maximize");

        if close.clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if minimize.clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if maximize.clicked() {
            toggle_maximized(ui.ctx());
        }

        ui.add_space(8.0);
        let drag_width = (ui.available_width() - titlebar_app_buttons_width(ui)).max(0.0);
        drag_region_with_width(ui, drag_width);
        titlebar_app_buttons(ui, show_playlist, actions);
    });
}

#[cfg(not(target_os = "macos"))]
fn titlebar_contents(ui: &mut egui::Ui, show_playlist: bool, actions: &mut TitlebarActions) {
    ui.horizontal(|ui| {
        let button_width = titlebar_app_buttons_width(ui)
            + ui.spacing().item_spacing.x
            + ui.spacing().interact_size.x * 3.0
            + ui.spacing().item_spacing.x * 2.0;
        let drag_width = (ui.available_width() - button_width).max(0.0);
        drag_region_with_width(ui, drag_width);
        titlebar_app_buttons(ui, show_playlist, actions);
        if ui.button("−").on_hover_text("Minimize").clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if ui.button("□").on_hover_text("Maximize").clicked() {
            toggle_maximized(ui.ctx());
        }
        if ui.button("×").on_hover_text("Close").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

fn titlebar_app_buttons_width(ui: &egui::Ui) -> f32 {
    TITLEBAR_BUTTON_SIZE * 2.0 + ui.spacing().item_spacing.x
}

fn titlebar_app_buttons(ui: &mut egui::Ui, show_playlist: bool, actions: &mut TitlebarActions) {
    if ui
        .add_sized(
            [TITLEBAR_BUTTON_SIZE, TITLEBAR_BUTTON_SIZE],
            egui::Button::new("☰").selected(show_playlist),
        )
        .on_hover_text(t!("playlist").to_string())
        .clicked()
    {
        actions.toggle_playlist = true;
    }
    if ui
        .add_sized(
            [TITLEBAR_BUTTON_SIZE, TITLEBAR_BUTTON_SIZE],
            egui::Button::new("⚙"),
        )
        .on_hover_text(t!("settings").to_string())
        .clicked()
    {
        actions.toggle_settings = true;
    }
}

#[cfg(target_os = "macos")]
fn traffic_light_button(
    ui: &mut egui::Ui,
    color: egui::Color32,
    symbol: &'static str,
    hover_text: &'static str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
    let radius = if response.hovered() { 6.2 } else { 5.8 };
    ui.painter()
        .circle_filled(rect.center(), radius, color.gamma_multiply(0.95));
    if response.hovered() {
        draw_traffic_light_symbol(ui.painter(), rect, symbol);
    }
    response.on_hover_text(hover_text)
}

#[cfg(target_os = "macos")]
fn draw_traffic_light_symbol(painter: &egui::Painter, rect: egui::Rect, symbol: &str) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        symbol,
        egui::FontId::proportional(9.0),
        TRAFFIC_LIGHT_SYMBOL_COLOR,
    );
}

fn drag_region_with_width(ui: &mut egui::Ui, width: f32) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, TITLEBAR_HEIGHT - 6.0),
        egui::Sense::drag(),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Morn",
        egui::FontId::proportional(13.0),
        ui.visuals().text_color(),
    );
    if response.double_clicked() {
        toggle_maximized(ui.ctx());
    } else if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

fn toggle_maximized(ctx: &egui::Context) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
}

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
                let response = ui.interact(local_rect, id, egui::Sense::click_and_drag());
                if response.drag_started() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            }
        });
}

fn resize_handles(
    screen_rect: egui::Rect,
) -> [(&'static str, egui::ResizeDirection, egui::Rect); 8] {
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

#[cfg(test)]
mod tests {
    #[test]
    fn titlebar_is_visible_only_while_pointer_is_inside_window() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));

        assert!(super::titlebar_visible(
            Some(egui::pos2(20.0, 20.0)),
            screen
        ));
        assert!(!super::titlebar_visible(
            Some(egui::pos2(20.0, -1.0)),
            screen
        ));
        assert!(!super::titlebar_visible(None, screen));
    }

    #[test]
    fn titlebar_supports_window_drag_buttons_and_resize_handles() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("ViewportCommand::StartDrag"));
        assert!(source.contains("ViewportCommand::Close"));
        assert!(source.contains("ViewportCommand::Minimized(true)"));
        assert!(source.contains("ViewportCommand::Maximized"));
        assert!(source.contains("ViewportCommand::BeginResize"));
        assert!(source.contains("TITLEBAR_OPACITY"));
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
        assert!(source.contains("Button::new(\"☰\")"));
        assert!(source.contains("Button::new(\"⚙\")"));
    }

    #[test]
    fn resize_handles_cover_all_eight_directions_at_window_edges() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));
        let h = super::WINDOW_RESIZE_HANDLE;
        let handles = super::resize_handles(screen);
        let mut seen = [false; 8];
        for (_tag, d, _) in handles {
            let idx = match d {
                egui::ResizeDirection::North => 0,
                egui::ResizeDirection::South => 1,
                egui::ResizeDirection::East => 2,
                egui::ResizeDirection::West => 3,
                egui::ResizeDirection::NorthWest => 4,
                egui::ResizeDirection::NorthEast => 5,
                egui::ResizeDirection::SouthWest => 6,
                egui::ResizeDirection::SouthEast => 7,
            };
            assert!(!seen[idx], "duplicate direction {d:?}");
            seen[idx] = true;
        }
        assert!(seen.iter().all(|s| *s), "missing direction(s)");

        let find = |dir: egui::ResizeDirection| -> egui::Rect {
            handles
                .iter()
                .find(|(_, d, _)| *d == dir)
                .map(|(_, _, r)| *r)
                .unwrap()
        };
        assert_eq!(
            find(egui::ResizeDirection::NorthWest),
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(h, h))
        );
        assert_eq!(
            find(egui::ResizeDirection::NorthEast),
            egui::Rect::from_min_max(egui::pos2(800.0 - h, 0.0), egui::pos2(800.0, h))
        );
        assert_eq!(
            find(egui::ResizeDirection::SouthWest),
            egui::Rect::from_min_max(egui::pos2(0.0, 600.0 - h), egui::pos2(h, 600.0))
        );
        assert_eq!(
            find(egui::ResizeDirection::SouthEast),
            egui::Rect::from_min_max(egui::pos2(800.0 - h, 600.0 - h), egui::pos2(800.0, 600.0))
        );
        let n = find(egui::ResizeDirection::North);
        assert_eq!(n.min, egui::pos2(h, 0.0));
        assert_eq!(n.max, egui::pos2(800.0 - h, h));
        let w = find(egui::ResizeDirection::West);
        assert_eq!(w.min, egui::pos2(0.0, h));
        assert_eq!(w.max, egui::pos2(h, 600.0 - h));
    }

    #[test]
    fn show_resize_handles_uses_a_single_foreground_area() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert_eq!(
            source.matches("\"window_resize_handle\"").count(),
            0,
            "old per-handle Area id string is still present"
        );
        assert!(
            source.contains("Id::new(\"window_resize_handles\")"),
            "merged Area id missing"
        );
        assert_eq!(
            source.matches("ui.interact(").count(),
            1,
            "expected exactly one ui.interact call site (in a loop)"
        );
    }

    #[test]
    fn titlebar_is_compact_centered_and_shows_hover_symbols() {
        assert!(std::hint::black_box(super::TITLEBAR_HEIGHT) <= 28.0);

        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("left_to_right(egui::Align::Center)"));
        assert!(source.contains("draw_traffic_light_symbol"));
        assert!(source.contains("\"×\""));
        assert!(source.contains("\"−\""));
    }

    #[test]
    fn transparent_window_paints_rounded_background() {
        let source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("paint_window_background"));
        assert!(source.contains("WINDOW_CORNER_RADIUS"));
        assert!(source.contains("LayerId::background"));
        assert!(source.contains("rect_filled"));
    }
}
