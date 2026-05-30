use eframe::egui;
use engine::Timeline;
use player_core::Command;
use rust_i18n::t;

/// 在底部面板绘制控制栏, 返回本帧产生的命令(若有)。
pub fn controls_bar(ui: &mut egui::Ui, t: &Timeline) -> Vec<Command> {
    use player_core::PlaybackState;
    let mut cmds = Vec::new();

    ui.horizontal(|ui| {
        if ui
            .button("📂")
            .on_hover_text(t!("open_file").to_string())
            .clicked()
        {
            cmds.push(Command::OpenDialog);
        }
        let playing = t.state == PlaybackState::Playing;
        if ui.button(if playing { "⏸" } else { "▶" }).clicked() {
            cmds.push(if playing {
                Command::Pause
            } else {
                Command::Play
            });
        }
        if ui.button("⏹").clicked() {
            cmds.push(Command::Stop);
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

        let mut vol = t.volume as f64;
        if ui
            .add(egui::Slider::new(&mut vol, 0.0..=100.0).text("🔊"))
            .changed()
        {
            cmds.push(Command::SetVolume(vol as u8));
        }

        let mute_icon = if t.muted { "🔇" } else { "🔊" };
        if ui
            .button(mute_icon)
            .on_hover_text(t!("mute_toggle").to_string())
            .clicked()
        {
            cmds.push(Command::ToggleMute);
        }

        if ui.button("⛶").clicked() {
            let fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        }

        ui.label(if t.hardware_decode { "HW" } else { "SW" });
    });

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
