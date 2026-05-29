use eframe::egui;
use player_core::Command;

/// 绘制左侧播放列表, 返回点击产生的命令。
pub fn playlist_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    ui.heading("播放列表");
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, p) in paths.iter().enumerate() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let selected = current == Some(i);
            if ui.selectable_label(selected, name).clicked() {
                cmds.push(Command::PlayIndex(i));
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button("⏮ 上一个").clicked() {
            cmds.push(Command::Prev);
        }
        if ui.button("下一个 ⏭").clicked() {
            cmds.push(Command::Next);
        }
    });
    cmds
}
