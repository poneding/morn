use eframe::egui;
use player_core::Command;
use rust_i18n::t;

pub const OPEN_MENU_ICON: &str = "➕";
pub const PLAYLIST_MIN_WIDTH: f32 = 220.0;

pub fn open_menu_commands() -> Vec<Command> {
    vec![Command::OpenDialog, Command::OpenFolder]
}

pub fn open_menu_button(ui: &mut egui::Ui) -> Vec<Command> {
    let mut cmds = Vec::new();
    let button = egui::containers::menu::MenuButton::from_button(egui::Button::new(OPEN_MENU_ICON))
        .config(
            egui::containers::menu::MenuConfig::new().style(crate::visuals::frosted_popup_style()),
        );
    let (_, menu) = button.ui(ui, |ui| {
        let mut selected = Vec::new();
        for cmd in open_menu_commands() {
            let label = match cmd {
                Command::OpenDialog => t!("open_file").to_string(),
                Command::OpenFolder => t!("open_folder").to_string(),
                _ => continue,
            };
            if ui.button(label).clicked() {
                selected.push(cmd);
                ui.close();
            }
        }
        selected
    });
    if let Some(menu) = menu {
        cmds.extend(menu.inner);
    }
    cmds
}

fn playlist_item_button(ui: &mut egui::Ui, selected: bool, title: &str) -> egui::Response {
    ui.add(
        egui::Button::selectable(selected, title)
            .truncate()
            .min_size(egui::vec2(
                ui.available_width(),
                ui.spacing().interact_size.y,
            )),
    )
}

/// 绘制左侧播放列表, 返回点击产生的命令。
pub fn playlist_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            for (i, p) in paths.iter().enumerate() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let selected = current == Some(i);
                let response = playlist_item_button(ui, selected, name)
                    .on_hover_text(p.to_string_lossy().to_string());
                if response.clicked() {
                    cmds.push(Command::PlayIndex(i));
                }
            }
        });
    });
    cmds
}

/// 绘制历史列表, 点击某项返回 Open 命令。
pub fn history_panel(ui: &mut egui::Ui, paths: &[std::path::PathBuf]) -> Option<Command> {
    let mut cmd = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            for p in paths {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let response = playlist_item_button(ui, false, name)
                    .on_hover_text(p.to_string_lossy().to_string());
                if response.clicked() {
                    cmd = Some(Command::Open(p.clone()));
                }
            }
        });
    });
    cmd
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_menu_commands_are_file_then_folder() {
        assert_eq!(
            super::open_menu_commands(),
            vec![
                player_core::Command::OpenDialog,
                player_core::Command::OpenFolder
            ]
        );
    }

    #[test]
    fn open_menu_uses_default_sized_icon_button() {
        assert_eq!(super::OPEN_MENU_ICON, "➕");

        let source = include_str!("playlist_panel.rs")
            .split("fn playlist_item_button")
            .next()
            .unwrap();
        assert!(!source.contains("menu_button(\"+\")"));
        assert!(!source.contains("OPEN_MENU_BUTTON_SIZE"));
        assert!(!source.contains("min_size"));
        assert!(source.contains("MenuButton"));
        assert!(source.contains("frosted_popup_style"));
    }

    #[test]
    fn playlist_items_are_single_line_truncated() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("truncate()"));
        assert!(source.contains("available_width()"));
    }

    #[test]
    fn playlist_items_use_left_aligned_full_width_buttons() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(!source.contains("add_sized("));
        assert!(source.contains("min_size(egui::vec2"));
    }

    #[test]
    fn playlist_panel_does_not_duplicate_bottom_navigation_controls() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        for removed in [
            concat!("Command", "::", "Prev"),
            concat!("Command", "::", "Next"),
        ] {
            assert!(
                !source.contains(removed),
                "playlist panel still exposes navigation command: {removed}"
            );
        }
    }
}
