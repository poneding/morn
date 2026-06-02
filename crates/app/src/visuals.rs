use eframe::egui;

pub const FLOATING_PANEL_MARGIN: f32 = 6.0;
pub const FLOATING_CONTROL_BAR_INNER_MARGIN_Y: i8 = 6;
pub const FROSTED_PANEL_RADIUS: u8 = 8;
pub const FROSTED_PANEL_DARK_SHIFT: u8 = 10;
pub const FROSTED_PANEL_LIGHT_SHIFT: u8 = 6;

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

fn frosted_fill_from_visuals(visuals: &egui::Visuals) -> egui::Color32 {
    let panel_fill = visuals.panel_fill;
    let shift = if visuals.dark_mode {
        FROSTED_PANEL_DARK_SHIFT
    } else {
        FROSTED_PANEL_LIGHT_SHIFT
    };
    egui::Color32::from_rgba_unmultiplied(
        panel_fill.r().saturating_add(shift),
        panel_fill.g().saturating_add(shift),
        panel_fill.b().saturating_add(shift),
        255,
    )
}

fn frosted_stroke_from_visuals(visuals: &egui::Visuals) -> egui::Stroke {
    let color = visuals.widgets.noninteractive.bg_stroke.color;
    egui::Stroke::new(
        1.0,
        egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 255),
    )
}

pub fn frosted_frame(ui: &egui::Ui, inner_margin: egui::Margin) -> egui::Frame {
    frosted_frame_for_style(ui.style(), inner_margin)
}

pub fn frosted_frame_for_style(style: &egui::Style, inner_margin: egui::Margin) -> egui::Frame {
    egui::Frame::NONE
        .fill(frosted_fill_from_visuals(&style.visuals))
        .stroke(frosted_stroke_from_visuals(&style.visuals))
        .corner_radius(FROSTED_PANEL_RADIUS)
        .shadow(style.visuals.popup_shadow)
        .inner_margin(inner_margin)
}

pub fn frosted_popup_style() -> egui::style::StyleModifier {
    egui::style::StyleModifier::new(|style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        let fill = frosted_fill_from_visuals(&style.visuals);
        let stroke = frosted_stroke_from_visuals(&style.visuals);

        style.visuals.window_fill = fill;
        style.visuals.panel_fill = fill;
        style.visuals.window_stroke = stroke;
        style.visuals.popup_shadow = style.visuals.window_shadow;
        style.visuals.menu_corner_radius = egui::CornerRadius::same(FROSTED_PANEL_RADIUS);
        style.visuals.window_corner_radius = egui::CornerRadius::same(FROSTED_PANEL_RADIUS);
        style.visuals.widgets.inactive.weak_bg_fill =
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0);
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        style.visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
        style.visuals.widgets.inactive.expansion = 0.0;
        style.visuals.widgets.hovered.expansion = 0.0;
        style.visuals.widgets.active.expansion = 0.0;
        style.visuals.widgets.open.expansion = 0.0;
        style.spacing.button_padding = egui::vec2(4.0, 2.0);
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn frosted_colors_are_opaque() {
        let visuals = egui::Visuals::dark();
        let fill = super::frosted_fill_from_visuals(&visuals);
        let stroke = super::frosted_stroke_from_visuals(&visuals);

        assert_eq!(fill.a(), 255);
        assert_eq!(stroke.color.a(), 255);
    }

    #[test]
    fn floating_panel_margin_is_shared_six_pixels() {
        assert_eq!(super::FLOATING_PANEL_MARGIN, 6.0);
        assert_eq!(super::FLOATING_CONTROL_BAR_INNER_MARGIN_Y, 6);
    }

    #[test]
    fn popup_menu_widget_states_do_not_add_hover_borders_or_expansion() {
        let mut style = egui::Style::default();
        super::frosted_popup_style().apply(&mut style);

        assert_eq!(style.visuals.widgets.inactive.bg_stroke, egui::Stroke::NONE);
        assert_eq!(style.visuals.widgets.hovered.bg_stroke, egui::Stroke::NONE);
        assert_eq!(style.visuals.widgets.active.bg_stroke, egui::Stroke::NONE);
        assert_eq!(style.visuals.widgets.open.bg_stroke, egui::Stroke::NONE);
        assert_eq!(style.visuals.widgets.inactive.expansion, 0.0);
        assert_eq!(style.visuals.widgets.hovered.expansion, 0.0);
        assert_eq!(style.visuals.widgets.active.expansion, 0.0);
        assert_eq!(style.visuals.widgets.open.expansion, 0.0);
    }
}
