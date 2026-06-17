use eframe::egui;

pub const FLOATING_PANEL_MARGIN: f32 = 6.0;
pub const ICON_BUTTON_SIZE: f32 = 28.0;
pub const COMPACT_ICON_BUTTON_SIZE: f32 = 24.0;
pub const CONTROL_CORNER_RADIUS: u8 = 6;
pub const FLOATING_CONTROL_BAR_INNER_MARGIN_Y: i8 = 6;
pub const FLOATING_PANEL_INNER_MARGIN_X: i8 = 14;
pub const FLOATING_PANEL_INNER_MARGIN_Y: i8 = 10;
/// 浮动面板圆角(底部控制栏、播放列表、设置、弹出层)。参考 flashot 标注工具栏。
pub const PANEL_CORNER_RADIUS: u8 = 10;
/// 面板投影: 比默认更沉, 参考 flashot 的 `0 4px 24px rgba(0,0,0,.4)`。
const PANEL_SHADOW_OFFSET: [i8; 2] = [0, 4];
const PANEL_SHADOW_BLUR: u8 = 24;
const PANEL_SHADOW_ALPHA: u8 = 110;

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

/// 浮动面板填充: 取主题的浮层底色并强制不透明——按用户要求**不做磨砂**, 用干净的
/// 实色深色面板(flashot 风格), 视频不再从面板后透出。
fn panel_fill_from_visuals(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.window_fill.to_opaque()
}

fn panel_border_stroke() -> egui::Stroke {
    // 不要边框: flashot 面板也没有描边，只有投影。
    egui::Stroke::NONE
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

/// 下拉/菜单弹出层样式: 同样的实色深色面板, 去掉 hover 描边与膨胀, 保持扁平。
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
    fn panel_fill_is_opaque_solid_dark_surface() {
        // 不做磨砂: 面板底必须完全不透明, 视频不从后面透出。
        let dark = super::panel_fill_from_visuals(&egui::Visuals::dark());
        assert_eq!(dark.a(), 255, "面板应为实色不透明");
    }

    #[test]
    fn panel_border_is_none_for_clean_look() {
        // flashot 风格: 面板只有投影，无描边。
        let stroke = super::panel_border_stroke();
        assert_eq!(stroke.width, 0.0);
    }

    #[test]
    fn floating_panel_margin_is_shared_six_pixels() {
        assert_eq!(super::FLOATING_PANEL_MARGIN, 6.0);
        assert_eq!(super::FLOATING_CONTROL_BAR_INNER_MARGIN_Y, 6);
    }

    #[test]
    fn shared_control_tokens_keep_overlays_consistent() {
        assert_eq!(super::ICON_BUTTON_SIZE, 28.0);
        assert_eq!(super::COMPACT_ICON_BUTTON_SIZE, 24.0);
        assert_eq!(super::CONTROL_CORNER_RADIUS, 6);
        assert_eq!(super::FLOATING_PANEL_INNER_MARGIN_X, 14);
        assert_eq!(super::FLOATING_PANEL_INNER_MARGIN_Y, 10);
        assert_eq!(super::PANEL_CORNER_RADIUS, 10);
    }

    #[test]
    fn popup_menu_widget_states_do_not_add_hover_borders_or_expansion() {
        let mut style = egui::Style::default();
        super::panel_popup_style().apply(&mut style);

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
