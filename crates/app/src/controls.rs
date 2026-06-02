use eframe::egui;
use engine::Timeline;
use player_core::Command;
use rust_i18n::t;

pub const VOLUME_MENU_ICON: &str = "🔊";
pub const MUTE_MENU_ICON: &str = "🔇";
const VOLUME_POPUP_SLIDER_HEIGHT: f32 = 96.0;
const VOLUME_POPUP_SLIDER_WIDTH: f32 = 24.0;
const VOLUME_POPUP_INNER_MARGIN_X: i8 = 10;
const VOLUME_POPUP_INNER_MARGIN_Y: i8 = 8;

pub fn mute_icon(muted: bool, volume: u8) -> &'static str {
    if muted || volume == 0 {
        MUTE_MENU_ICON
    } else {
        VOLUME_MENU_ICON
    }
}

/// 在底部面板绘制控制栏, 返回本帧产生的命令(若有)。
pub fn controls_bar(
    ui: &mut egui::Ui,
    t: &Timeline,
    has_prev: bool,
    has_next: bool,
) -> Vec<Command> {
    use player_core::PlaybackState;
    let mut cmds = Vec::new();

    if ui
        .add_enabled(has_prev, egui::Button::new("⏮"))
        .on_hover_text(t!("prev").to_string())
        .clicked()
    {
        cmds.push(Command::Prev);
    }
    let playing = t.state == PlaybackState::Playing;
    if ui.button(if playing { "⏸" } else { "▶" }).clicked() {
        cmds.push(if playing {
            Command::Pause
        } else {
            Command::Play
        });
    }
    if ui
        .add_enabled(has_next, egui::Button::new("⏭"))
        .on_hover_text(t!("next").to_string())
        .clicked()
    {
        cmds.push(Command::Next);
    }
    ui.label(t.position_label());

    let mut pos = t.position_ms as f64;
    let dur = t.duration_ms.max(1) as f64;
    let resp = ui.add(
        egui::Slider::new(&mut pos, 0.0..=dur)
            .show_value(false)
            .trailing_fill(true),
    );
    if resp.changed() {
        cmds.push(Command::SeekTo(pos as u64));
    }

    ui.label(t.duration_label());

    let volume_response = ui
        .button(mute_icon(t.muted, t.volume))
        .on_hover_text(t!("volume").to_string());
    volume_popup(ui, &volume_response, t.volume, &mut cmds);

    if ui.button("⛶").clicked() {
        toggle_fullscreen(ui.ctx());
    }

    cmds
}

fn volume_popup(
    ui: &mut egui::Ui,
    volume_response: &egui::Response,
    volume: u8,
    cmds: &mut Vec<Command>,
) {
    egui::Popup::from_toggle_button_response(volume_response)
        .kind(egui::PopupKind::Popup)
        .anchor(crate::visuals::popup_anchor_above_floating_control_bar(
            volume_response,
        ))
        .align(egui::RectAlign::TOP)
        .align_alternatives(&[])
        .gap(crate::visuals::FLOATING_PANEL_MARGIN)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(crate::visuals::frosted_frame_for_style(
            ui.style(),
            egui::Margin::symmetric(VOLUME_POPUP_INNER_MARGIN_X, VOLUME_POPUP_INNER_MARGIN_Y),
        ))
        .show(|ui| {
            let value_text = volume.to_string();
            let value_width = ui
                .painter()
                .layout_no_wrap(
                    value_text.clone(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().text_color(),
                )
                .size()
                .x;
            let content_width = VOLUME_POPUP_SLIDER_WIDTH.max(value_width);
            ui.set_min_width(content_width);
            ui.vertical_centered(|ui| {
                let mut vol = f32::from(volume);
                let slider_response = ui
                    .allocate_ui_with_layout(
                        egui::vec2(content_width, VOLUME_POPUP_SLIDER_HEIGHT),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().slider_width = VOLUME_POPUP_SLIDER_HEIGHT;
                            ui.add(
                                egui::Slider::new(&mut vol, 0.0..=100.0)
                                    .vertical()
                                    .show_value(false),
                            )
                        },
                    )
                    .inner;
                if slider_response.changed() {
                    cmds.push(Command::SetVolume(vol.round() as u8));
                }
                ui.label(value_text);
            });
        });
}

pub fn toggle_fullscreen(ctx: &egui::Context) {
    let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
}

/// 字幕轨道下拉; 选中某轨返回 SelectSubtitleTrack(stream_index)。
pub fn subtitle_track_combo(ui: &mut egui::Ui, tracks: &[media::SubtitleTrack]) -> Option<Command> {
    let mut chosen = None;
    egui::ComboBox::from_label(t!("subtitle_track").to_string())
        .selected_text(t!("select").to_string())
        .popup_style(crate::visuals::frosted_popup_style())
        .show_ui(ui, |ui| {
            for tr in tracks {
                if ui.selectable_label(false, &tr.label).clicked() {
                    chosen = Some(Command::SelectSubtitleTrack(tr.stream_index));
                }
            }
        });
    chosen
}

#[cfg(test)]
mod tests {
    #[test]
    fn mute_icon_reflects_mute_and_zero_volume() {
        assert_eq!(super::mute_icon(false, 50), "🔊");
        assert_eq!(super::mute_icon(true, 50), "🔇");
        assert_eq!(super::mute_icon(false, 0), "🔇");
    }

    #[test]
    fn controls_bar_no_longer_contains_open_entry() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        for removed in [concat!("Open", "Dialog"), concat!("open", "_file")] {
            assert!(
                !source.contains(removed),
                "bottom controls still contain open entry: {removed}"
            );
        }
    }

    #[test]
    fn controls_bar_excludes_removed_decode_status() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        for removed in [
            concat!("H", "W"),
            concat!("S", "W"),
            concat!("hardware", "_decode"),
        ] {
            assert!(
                !source.contains(removed),
                "bottom controls still expose decode status: {removed}"
            );
        }
    }

    #[test]
    fn controls_bar_excludes_removed_stop_button() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        for removed in [concat!("Command", "::", "Stop"), "\u{23F9}"] {
            assert!(
                !source.contains(removed),
                "bottom controls still expose stop button: {removed}"
            );
        }
    }

    #[test]
    fn volume_popup_is_aligned_above_icon_and_omits_inner_mute_button() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("Popup::from_toggle_button_response(volume_response)"));
        assert!(source.contains("popup_anchor_above_floating_control_bar"));
        assert!(source.contains("RectAlign::TOP"));
        assert!(source.contains("FLOATING_PANEL_MARGIN"));
        assert!(source.contains("show_value(false)"));
        assert!(source.contains("CloseOnClickOutside"));
        assert!(source.contains("layout_no_wrap"));
        assert!(source.contains("let content_width = VOLUME_POPUP_SLIDER_WIDTH.max(value_width)"));
        assert!(source.contains("allocate_ui_with_layout"));
        assert!(source.contains("egui::Layout::top_down(egui::Align::Center)"));
        assert!(source.contains("ui.spacing_mut().slider_width = VOLUME_POPUP_SLIDER_HEIGHT"));
        assert!(source.contains("vertical_centered"));
        assert!(!source.contains("Command::ToggleMute"));
        assert!(!source.contains("MenuButton::from_button(egui::Button::new"));
        assert!(!source.contains("volume_panel_width"));
        assert!(!source.contains("set_min_width(panel_width)"));
        assert!(!source.contains("set_max_width(panel_width)"));
    }

    #[test]
    fn controls_bar_exposes_disabled_playlist_navigation_buttons() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("has_prev"));
        assert!(source.contains("has_next"));
        assert!(source.contains("add_enabled(has_prev"));
        assert!(source.contains("add_enabled(has_next"));
        assert!(source.contains("Command::Prev"));
        assert!(source.contains("Command::Next"));
    }

    #[test]
    fn fullscreen_button_and_enter_share_toggle_helper() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("pub fn toggle_fullscreen"));
        assert!(source.contains("ViewportCommand::Fullscreen(!fs)"));
    }
}
