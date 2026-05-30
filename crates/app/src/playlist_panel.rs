use eframe::egui;
use player_core::Command;
use rust_i18n::t;

/// 绘制左侧播放列表, 返回点击产生的命令。
pub fn playlist_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
) -> Vec<Command> {
    let mut cmds = Vec::new();
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
        if ui.button(format!("⏮ {}", t!("prev"))).clicked() {
            cmds.push(Command::Prev);
        }
        if ui.button(format!("{} ⏭", t!("next"))).clicked() {
            cmds.push(Command::Next);
        }
    });
    cmds
}

/// 绘制历史列表, 点击某项返回 Open 命令。
pub fn history_panel(ui: &mut egui::Ui, paths: &[std::path::PathBuf]) -> Option<Command> {
    let mut cmd = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in paths {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            if ui
                .selectable_label(false, name)
                .on_hover_text(p.to_string_lossy().to_string())
                .clicked()
            {
                cmd = Some(Command::Open(p.clone()));
            }
        }
    });
    cmd
}
