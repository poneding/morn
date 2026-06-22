use eframe::egui;

pub const FLOATING_PANEL_MARGIN: f32 = 6.0;
pub const ICON_BUTTON_SIZE: f32 = 26.0;
pub const CONTROL_CORNER_RADIUS: u8 = 5;
pub const FLOATING_CONTROL_BAR_INNER_MARGIN_Y: i8 = 5;
pub const FLOATING_PANEL_INNER_MARGIN_X: i8 = 12;
pub const FLOATING_PANEL_INNER_MARGIN_Y: i8 = 8;
/// 浮动面板圆角(底部控制栏、播放列表、设置、弹出层)。参考 flashot 标注工具栏。
pub const PANEL_CORNER_RADIUS: u8 = 8;
const PANEL_SHADOW_OFFSET: [i8; 2] = [0, 5];
const PANEL_SHADOW_BLUR: u8 = 18;
const PANEL_SHADOW_ALPHA: u8 = 130;
const BEVEL_HIGHLIGHT_ALPHA: u8 = 38;
const BEVEL_SHADOW_ALPHA: u8 = 105;

pub fn popup_anchor_above_floating_control_bar(response: &egui::Response) -> egui::PopupAnchor {
    let mut rect = response.interact_rect;
    if let Some(to_global) = response.ctx.layer_transform_to_global(response.layer_id) {
        rect = to_global * rect;
    }
    let control_bar_top = rect.top() - f32::from(FLOATING_CONTROL_BAR_INNER_MARGIN_Y);
    egui::PopupAnchor::ParentRect(egui::Rect::from_min_max(
        egui::pos2(rect.left(), control_bar_top),
        egui::pos2(rect.right(), control_bar_top),
    ))
}

/// 浮动面板填充: 取主题的浮层底色并强制不透明，视频不从面板后透出。
fn panel_fill_from_visuals(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.window_fill.to_opaque()
}

fn panel_border_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0, egui::Color32::from_black_alpha(155))
}

fn panel_shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: PANEL_SHADOW_OFFSET,
        blur: PANEL_SHADOW_BLUR,
        spread: 0,
        color: egui::Color32::from_black_alpha(PANEL_SHADOW_ALPHA),
    }
}

pub fn panel_frame(ui: &egui::Ui, inner_margin: egui::Margin) -> egui::Frame {
    panel_frame_for_style(ui.style(), inner_margin)
}

/// 干净的实色深色面板(flashot 标注工具栏风格): 不透明底 + 白色细边 + 沉稳投影。
pub fn panel_frame_for_style(style: &egui::Style, inner_margin: egui::Margin) -> egui::Frame {
    egui::Frame::NONE
        .fill(panel_fill_from_visuals(&style.visuals))
        .stroke(panel_border_stroke())
        .corner_radius(PANEL_CORNER_RADIUS)
        .shadow(panel_shadow())
        .inner_margin(inner_margin)
}

pub fn paint_panel_bevel(ui: &egui::Ui, rect: egui::Rect, _radius: u8) {
    // 仅保留底部阴影线营造轻微下沉感; 顶部高光(原 from_white_alpha)在深色面板上呈
    // 灰白色, 看起来像碍眼的"灰色 border", 故移除。
    ui.painter().line_segment(
        [
            egui::pos2(rect.left() + 1.0, rect.bottom() - 0.5),
            egui::pos2(rect.right() - 1.0, rect.bottom() - 0.5),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(BEVEL_SHADOW_ALPHA)),
    );
}

pub fn beveled_button_frame_at(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    selected: bool,
    opacity: f32,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let visuals = ui.style().interact(response);
    let radius = egui::CornerRadius::same(CONTROL_CORNER_RADIUS);
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else {
        visuals.weak_bg_fill
    }
    .gamma_multiply(opacity);
    let border_color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke.color
    }
    .gamma_multiply(opacity);
    let frame_rect = rect.shrink(0.5);

    ui.painter().rect_filled(frame_rect, radius, fill);
    ui.painter().rect_stroke(
        frame_rect,
        radius,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );

    let pressed = response.is_pointer_button_down_on() || selected;
    // 顶部高光线移除: from_white_alpha 在深色按钮上呈灰白, 像碍眼的"灰色 border"。
    let bottom_color = if pressed {
        egui::Color32::from_white_alpha(BEVEL_HIGHLIGHT_ALPHA / 2)
    } else {
        egui::Color32::from_black_alpha(BEVEL_SHADOW_ALPHA)
    }
    .gamma_multiply(opacity);

    ui.painter().line_segment(
        [
            egui::pos2(frame_rect.left() + 1.5, frame_rect.bottom() - 1.0),
            egui::pos2(frame_rect.right() - 1.5, frame_rect.bottom() - 1.0),
        ],
        egui::Stroke::new(1.0, bottom_color),
    );
}

/// 下拉/菜单弹出层样式: 同样的实色深色面板，保留轻微边线和紧凑间距。
pub fn panel_popup_style() -> egui::style::StyleModifier {
    egui::style::StyleModifier::new(|style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        let fill = panel_fill_from_visuals(&style.visuals);

        style.visuals.window_fill = fill;
        style.visuals.panel_fill = fill;
        style.visuals.window_stroke = panel_border_stroke();
        style.visuals.popup_shadow = panel_shadow();
        style.visuals.menu_corner_radius = egui::CornerRadius::same(PANEL_CORNER_RADIUS);
        style.visuals.window_corner_radius = egui::CornerRadius::same(PANEL_CORNER_RADIUS);
        style.visuals.widgets.inactive.expansion = 0.0;
        style.visuals.widgets.hovered.expansion = 0.0;
        style.visuals.widgets.active.expansion = 0.0;
        style.visuals.widgets.open.expansion = 0.0;
        style.spacing.button_padding = egui::vec2(5.0, 2.0);
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn panel_fill_is_opaque_solid_dark_surface() {
        // 不做磨砂: 面板底必须完全不透明, 视频不从后面透出。
        let dark = super::panel_fill_from_visuals(&egui::Visuals::dark());
        assert_eq!(dark.a(), 255, "面板应为实色不透明");
    }

    #[test]
    fn panel_border_is_subtle_for_skeuomorphic_edge() {
        let stroke = super::panel_border_stroke();
        assert_eq!(stroke.width, 1.0);
    }

    #[test]
    fn floating_panel_margin_stays_six_and_control_bar_is_tighter() {
        assert_eq!(super::FLOATING_PANEL_MARGIN, 6.0);
        assert_eq!(super::FLOATING_CONTROL_BAR_INNER_MARGIN_Y, 5);
    }

    #[test]
    fn shared_control_tokens_keep_overlays_consistent() {
        assert_eq!(super::ICON_BUTTON_SIZE, 26.0);
        assert_eq!(super::CONTROL_CORNER_RADIUS, 5);
        assert_eq!(super::FLOATING_PANEL_INNER_MARGIN_X, 12);
        assert_eq!(super::FLOATING_PANEL_INNER_MARGIN_Y, 8);
        assert_eq!(super::PANEL_CORNER_RADIUS, 8);
    }

    #[test]
    fn popup_menu_widget_states_do_not_add_expansion() {
        let mut style = egui::Style::default();
        super::panel_popup_style().apply(&mut style);

        assert_eq!(style.visuals.widgets.inactive.expansion, 0.0);
        assert_eq!(style.visuals.widgets.hovered.expansion, 0.0);
        assert_eq!(style.visuals.widgets.active.expansion, 0.0);
        assert_eq!(style.visuals.widgets.open.expansion, 0.0);
    }
}
