//! Bottom media control widgets.
//!
//! This module is intentionally command-only: buttons and sliders collect
//! `player_core::Command` values and leave mutation to the app after egui finishes
//! layout.  That keeps immediate-mode borrowing simple and ensures playlist or
//! playback changes happen at one point in the frame.
//!
//! Icon buttons use measured minimum widths so play/pause and volume/mute do not
//! shift neighboring controls when their glyph changes.  The volume popup owns a
//! fixed content width; the slider is centered inside that panel independently of
//! the current numeric volume.

use eframe::egui;
use engine::Timeline;
use player_core::Command;
use rust_i18n::t;

pub const VOLUME_MENU_ICON: &str = crate::symbols::VOLUME;
pub const MUTE_MENU_ICON: &str = crate::symbols::MUTE;
const PLAY_ICON: &str = crate::symbols::PLAY;
const PAUSE_ICON: &str = crate::symbols::PAUSE;
/// 控制栏所有图标按钮的固定大小（正方形），保持视觉统一。
pub(crate) const CONTROL_BUTTON_SIZE: f32 = crate::visuals::ICON_BUTTON_SIZE;
const VOLUME_POPUP_SLIDER_HEIGHT: f32 = 96.0;
const VOLUME_POPUP_SLIDER_WIDTH: f32 = 24.0;
const VOLUME_POPUP_CONTENT_WIDTH: f32 = 40.0;
const VOLUME_POPUP_INNER_MARGIN_X: i8 = 10;
const VOLUME_POPUP_INNER_MARGIN_Y: i8 = 8;

pub struct ControlsBarInput<'a> {
    // Bundle frame-local control data so the call site cannot accidentally pass
    // mismatched timeline and playlist-navigation state.
    pub timeline: &'a Timeline,
    pub has_prev: bool,
    pub has_next: bool,
    pub slider_width: f32,
}

pub fn mute_icon(muted: bool, volume: u8) -> &'static str {
    if muted || volume == 0 {
        MUTE_MENU_ICON
    } else {
        VOLUME_MENU_ICON
    }
}

/// 在底部面板绘制控制栏, 返回本帧产生的命令(若有)。
/// `slider_width` 为时间轴期望宽度(随窗口自适应), 让进度条成为最显眼的元素。
pub fn controls_bar(ui: &mut egui::Ui, input: ControlsBarInput<'_>) -> Vec<Command> {
    use player_core::PlaybackState;
    let t = input.timeline;
    let mut cmds = Vec::new();

    if ui
        .add_enabled(
            input.has_prev,
            egui::Button::new(crate::symbols::PREV)
                .min_size(egui::vec2(CONTROL_BUTTON_SIZE, CONTROL_BUTTON_SIZE)),
        )
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("prev"),
            crate::shortcuts::prev_shortcut_label(),
        ))
        .clicked()
    {
        cmds.push(Command::Prev);
    }
    let playing = t.state == PlaybackState::Playing;
    let play_pause_tooltip = if playing {
        crate::shortcuts::shortcut_tooltip(t!("pause"), "Space")
    } else {
        crate::shortcuts::shortcut_tooltip(t!("play"), "Space")
    };
    if ui
        .add_sized(
            [CONTROL_BUTTON_SIZE, CONTROL_BUTTON_SIZE],
            egui::Button::new(if playing { PAUSE_ICON } else { PLAY_ICON }),
        )
        .on_hover_text(play_pause_tooltip)
        .clicked()
    {
        cmds.push(if playing {
            Command::Pause
        } else {
            Command::Play
        });
    }
    if ui
        .add_enabled(
            input.has_next,
            egui::Button::new(crate::symbols::NEXT)
                .min_size(egui::vec2(CONTROL_BUTTON_SIZE, CONTROL_BUTTON_SIZE)),
        )
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("next"),
            crate::shortcuts::next_shortcut_label(),
        ))
        .clicked()
    {
        cmds.push(Command::Next);
    }
    ui.add(egui::Label::new(
        egui::RichText::new(t.position_label()).family(egui::FontFamily::Monospace),
    ));

    let mut pos = t.position_ms as f64;
    let dur = t.duration_ms.max(1) as f64;
    ui.spacing_mut().slider_width = input.slider_width;
    let resp = ui.add(
        egui::Slider::new(&mut pos, 0.0..=dur)
            .show_value(false)
            .trailing_fill(true),
    );
    // 仅在"拖动松手"或"非拖动交互(键盘/滚轮微调)"时 seek, 不要在拖动过程中每帧 seek。
    // 否则一次拖动会被展开成一连串 SeekTo, 每次都触发视频/音频重定位 + seek 闸门冻结,
    // 既导致拖动抖动, 又会在按住在进度条左端(位置 0)时把播放位置反复拉回 0——
    // 时钟刚 creep 几毫秒就被下一次 SeekTo(0) 重置, 表现为"播几秒又回到开头重新播"。
    if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
        cmds.push(Command::SeekTo(pos as u64));
    }

    ui.add(egui::Label::new(
        egui::RichText::new(t.duration_label()).family(egui::FontFamily::Monospace),
    ));

    let volume_response = ui
        .add_sized(
            [CONTROL_BUTTON_SIZE, CONTROL_BUTTON_SIZE],
            egui::Button::new(mute_icon(t.muted, t.volume)),
        )
        .on_hover_text(crate::shortcuts::shortcut_tooltip(t!("volume"), "↑/↓, M"));
    // The popup is attached to the icon response so it tracks the actual button
    // position inside the floating control bar.
    volume_popup(ui, &volume_response, t.volume, &mut cmds);

    if ui
        .add_sized(
            [CONTROL_BUTTON_SIZE, CONTROL_BUTTON_SIZE],
            egui::Button::new(crate::symbols::FULLSCREEN),
        )
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("fullscreen"),
            "F/Enter",
        ))
        .clicked()
    {
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
        .frame(crate::visuals::panel_frame_for_style(
            ui.style(),
            egui::Margin::symmetric(VOLUME_POPUP_INNER_MARGIN_X, VOLUME_POPUP_INNER_MARGIN_Y),
        ))
        .show(|ui| {
            let value_text = volume.to_string();
            // The number is measured only for a debug invariant; the visual width
            // remains fixed so the slider stays centered regardless of value.
            let value_width = ui
                .painter()
                .layout_no_wrap(
                    value_text.clone(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().text_color(),
                )
                .size()
                .x;
            debug_assert!(VOLUME_POPUP_CONTENT_WIDTH >= value_width);
            debug_assert!(VOLUME_POPUP_CONTENT_WIDTH >= VOLUME_POPUP_SLIDER_WIDTH);
            ui.set_width(VOLUME_POPUP_CONTENT_WIDTH);

            let mut vol = f32::from(volume);
            // A vertical slider's cross-axis width is `thickness`
            // (= max(body text height, interact_size.y)), NOT slider_width, and
            // egui wraps it in a left-aligned `ui.vertical`.  Relying on the
            // column's center-align therefore leaves the slider visually shifted
            // left, so center it ourselves by left-padding half the leftover width.
            let thickness = ui
                .text_style_height(&egui::TextStyle::Body)
                .max(ui.spacing().interact_size.y);
            let slider_left_pad = ((VOLUME_POPUP_CONTENT_WIDTH - thickness) * 0.5).max(0.0);
            let slider_response = ui
                .allocate_ui_with_layout(
                    egui::vec2(VOLUME_POPUP_CONTENT_WIDTH, VOLUME_POPUP_SLIDER_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(slider_left_pad);
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

            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(value_text).family(egui::FontFamily::Monospace));
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
    // Track labels come from the decoder; return the stream index so the engine can
    // decode the selected embedded subtitle stream later.
    egui::ComboBox::from_label(t!("subtitle_track").to_string())
        .selected_text(t!("select").to_string())
        .popup_style(crate::visuals::panel_popup_style())
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
        assert_eq!(super::mute_icon(false, 50), crate::symbols::VOLUME);
        assert_eq!(super::mute_icon(true, 50), crate::symbols::MUTE);
        assert_eq!(super::mute_icon(false, 0), crate::symbols::MUTE);
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
        assert!(source.contains("VOLUME_POPUP_CONTENT_WIDTH"));
        assert!(source.contains("allocate_ui_with_layout"));
        // 滑轮自身居中: 左右布局 + 计算左侧 padding 把窄滑轮推到正中。
        assert!(source.contains("egui::Layout::left_to_right(egui::Align::Center)"));
        assert!(source.contains("slider_left_pad"));
        assert!(source.contains("ui.add_space(slider_left_pad)"));
        assert!(source.contains("ui.spacing_mut().slider_width = VOLUME_POPUP_SLIDER_HEIGHT"));
        assert!(source.contains("vertical_centered"));
        assert!(!source.contains("Command::ToggleMute"));
        assert!(!source.contains("MenuButton::from_button(egui::Button::new"));
        assert!(!source.contains("let content_width = VOLUME_POPUP_SLIDER_WIDTH.max(value_width)"));
    }

    #[test]
    fn controls_bar_exposes_disabled_playlist_navigation_buttons() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("has_prev"));
        assert!(source.contains("has_next"));
        assert!(source.contains("add_enabled(\n            input.has_prev"));
        assert!(source.contains("add_enabled(\n            input.has_next"));
        assert!(source.contains("Command::Prev"));
        assert!(source.contains("Command::Next"));
    }

    #[test]
    fn all_control_buttons_use_fixed_square_size() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("const CONTROL_BUTTON_SIZE"));
        // 所有图标按钮都用固定正方形尺寸，视觉统一。
        assert!(source.contains("CONTROL_BUTTON_SIZE, CONTROL_BUTTON_SIZE"));
        assert!(source.contains("min_size(egui::vec2(CONTROL_BUTTON_SIZE"));
        assert!(!source.contains("toggled_icon_button_size"));
    }

    #[test]
    fn controls_tooltips_include_keyboard_shortcuts() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        for expected in [
            "shortcut_tooltip",
            "t!(\"prev\")",
            "prev_shortcut_label",
            "t!(\"play\")",
            "t!(\"pause\")",
            "Space",
            "t!(\"next\")",
            "next_shortcut_label",
            "t!(\"volume\")",
            "↑/↓, M",
            "t!(\"fullscreen\")",
            "F/Enter",
        ] {
            assert!(source.contains(expected));
        }
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
