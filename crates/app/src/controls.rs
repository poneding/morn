use eframe::egui;
use engine::Timeline;
use player_core::Command;
use rust_i18n::t;

pub const VOLUME_MENU_ICON: &str = "🔊";
pub const MUTE_MENU_ICON: &str = "🔇";

pub fn mute_icon(muted: bool, volume: u8) -> &'static str {
    if muted || volume == 0 {
        MUTE_MENU_ICON
    } else {
        VOLUME_MENU_ICON
    }
}

/// 在底部面板绘制控制栏, 返回本帧产生的命令(若有)。
pub fn controls_bar(ui: &mut egui::Ui, t: &Timeline) -> Vec<Command> {
    use player_core::PlaybackState;
    let mut cmds = Vec::new();

    let playing = t.state == PlaybackState::Playing;
    if ui.button(if playing { "⏸" } else { "▶" }).clicked() {
        cmds.push(if playing {
            Command::Pause
        } else {
            Command::Play
        });
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

    ui.menu_button(mute_icon(t.muted, t.volume), |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            let mut vol = t.volume as f32;
            if ui
                .add(egui::Slider::new(&mut vol, 0.0..=100.0).vertical())
                .changed()
            {
                cmds.push(Command::SetVolume(vol as u8));
            }
            if ui
                .button(mute_icon(t.muted, t.volume))
                .on_hover_text(t!("mute_toggle").to_string())
                .clicked()
            {
                cmds.push(Command::ToggleMute);
            }
        });
    });

    if ui.button("⛶").clicked() {
        let fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
    }

    cmds
}

/// 字幕轨道下拉; 选中某轨返回 SelectSubtitleTrack(stream_index)。
pub fn subtitle_track_combo(ui: &mut egui::Ui, tracks: &[media::SubtitleTrack]) -> Option<Command> {
    let mut chosen = None;
    egui::ComboBox::from_label(t!("subtitle_track").to_string())
        .selected_text(t!("select").to_string())
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
    fn volume_menu_uses_centered_adaptive_layout() {
        let source = include_str!("controls.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(!source.contains("set_min_width(64.0)"));
        assert!(source.contains("top_down(egui::Align::Center)"));
    }
}
