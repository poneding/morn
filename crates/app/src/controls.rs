use eframe::egui;
use engine::Timeline;
use player_core::Command;

/// 在底部面板绘制控制栏, 返回本帧产生的命令(若有)。
pub fn controls_bar(ui: &mut egui::Ui, t: &Timeline) -> Vec<Command> {
    use player_core::PlaybackState;
    let mut cmds = Vec::new();

    ui.horizontal(|ui| {
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

        if ui.button("⛶").clicked() {
            let fs = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        }

        ui.label(if t.hardware_decode { "HW" } else { "SW" });
    });

    cmds
}
