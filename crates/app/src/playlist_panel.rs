use eframe::egui;
use player_core::Command;
use rust_i18n::t;

pub const OPEN_MENU_ICON: &str = "➕";
pub const PLAYLIST_MIN_WIDTH: f32 = 220.0;
const ROW_ACTION_BUTTONS: f32 = 2.0;

pub fn open_menu_commands() -> Vec<Command> {
    vec![Command::OpenDialog, Command::OpenFolder]
}

pub fn open_menu_button(ui: &mut egui::Ui) -> Vec<Command> {
    let mut cmds = Vec::new();
    ui.horizontal(|ui| {
        let button_response = ui.add(egui::Button::new(OPEN_MENU_ICON)).on_hover_text(
            crate::shortcuts::shortcut_tooltip(
                t!("open_file"),
                crate::shortcuts::open_shortcut_label(),
            ),
        );
        if button_response.clicked() {
            cmds.push(Command::OpenDialog);
        }

        let menu_button = egui::containers::menu::MenuButton::from_button(egui::Button::new("▾"))
            .config(
                egui::containers::menu::MenuConfig::new()
                    .style(crate::visuals::frosted_popup_style()),
            );
        let (menu_response, menu) = menu_button.ui(ui, |ui| {
            let mut selected = Vec::new();
            for cmd in open_menu_commands() {
                let tooltip = match cmd {
                    Command::OpenDialog => crate::shortcuts::shortcut_tooltip(
                        t!("open_file"),
                        crate::shortcuts::open_shortcut_label(),
                    ),
                    Command::OpenFolder => t!("open_folder").to_string(),
                    _ => continue,
                };
                if ui.button(tooltip).clicked() {
                    selected.push(cmd);
                    ui.close();
                }
            }
            selected
        });
        menu_response.on_hover_text(t!("open_folder").to_string());
        if let Some(menu) = menu {
            cmds.extend(menu.inner);
        }
    });
    cmds
}

fn row_actions_width(ui: &egui::Ui) -> f32 {
    ui.spacing().interact_size.x * ROW_ACTION_BUTTONS
        + ui.spacing().item_spacing.x * ROW_ACTION_BUTTONS
}

fn playlist_item_button(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    actions_width: f32,
) -> egui::Response {
    let width = (ui.available_width() - actions_width).max(0.0);
    ui.add(
        egui::Button::selectable(selected, title)
            .truncate()
            .min_size(egui::vec2(width, ui.spacing().interact_size.y)),
    )
}

fn remove_button(ui: &mut egui::Ui) -> bool {
    ui.add(egui::Button::new("×"))
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("remove"),
            "Delete/Backspace",
        ))
        .clicked()
}

fn more_actions_button(ui: &mut egui::Ui, delete_cmd: Command) -> Vec<Command> {
    let mut cmds = Vec::new();
    let button = egui::containers::menu::MenuButton::from_button(egui::Button::new("⋯")).config(
        egui::containers::menu::MenuConfig::new().style(crate::visuals::frosted_popup_style()),
    );
    let (response, menu) = button.ui(ui, |ui| {
        let mut selected = Vec::new();
        if ui.button(t!("delete_file").to_string()).clicked() {
            selected.push(delete_cmd);
            ui.close();
        }
        selected
    });
    response.on_hover_text(t!("more_actions").to_string());
    if let Some(menu) = menu {
        cmds.extend(menu.inner);
    }
    cmds
}

fn row_actions(ui: &mut egui::Ui, remove_cmd: Command, delete_cmd: Command) -> Vec<Command> {
    let mut cmds = Vec::new();
    if remove_button(ui) {
        cmds.push(remove_cmd);
    }
    cmds.extend(more_actions_button(ui, delete_cmd));
    cmds
}

fn playlist_row(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    tooltip: String,
    play_cmd: Command,
    remove_cmd: Command,
    delete_cmd: Command,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    let actions_width = row_actions_width(ui);
    let row_height = ui.spacing().interact_size.y;
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), row_height),
    );
    let row_response = ui.allocate_rect(row_rect, egui::Sense::hover());
    ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
        ui.horizontal(|ui| {
            let response =
                playlist_item_button(ui, selected, title, actions_width).on_hover_text(tooltip);
            if response.clicked() {
                cmds.push(play_cmd);
            }
            if row_response.hovered() || row_response.contains_pointer() {
                cmds.extend(row_actions(ui, remove_cmd, delete_cmd));
            } else {
                ui.add_space(actions_width);
            }
        });
    });
    cmds
}

fn clear_all_toolbar(ui: &mut egui::Ui, cmd: Command, cmds: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        let clear_width = ui.spacing().interact_size.x;
        ui.add_space((ui.available_width() - clear_width).max(0.0));
        if ui
            .add(egui::Button::new("🗑"))
            .on_hover_text(t!("clear_all").to_string())
            .clicked()
        {
            cmds.push(cmd);
        }
    });
}

/// 绘制左侧播放列表, 返回点击产生的命令。
pub fn playlist_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
    candidate: Option<usize>,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    if !paths.is_empty() {
        clear_all_toolbar(ui, Command::ClearPlaylist, &mut cmds);
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            for (i, p) in paths.iter().enumerate() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let selected = candidate.or(current) == Some(i);
                cmds.extend(playlist_row(
                    ui,
                    selected,
                    name,
                    p.to_string_lossy().to_string(),
                    Command::PlayIndex(i),
                    Command::RemovePlaylistIndex(i),
                    Command::DeletePlaylistFileIndex(i),
                ));
            }
        });
    });
    cmds
}

/// 绘制历史列表, 点击某项返回 Open 命令。
pub fn history_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    candidate: Option<usize>,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    if !paths.is_empty() {
        clear_all_toolbar(ui, Command::ClearHistory, &mut cmds);
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            for (i, p) in paths.iter().enumerate() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                cmds.extend(playlist_row(
                    ui,
                    candidate == Some(i),
                    name,
                    p.to_string_lossy().to_string(),
                    Command::Open(p.clone()),
                    Command::RemoveHistoryIndex(i),
                    Command::DeleteHistoryFileIndex(i),
                ));
            }
        });
    });
    cmds
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

    #[test]
    fn playlist_panel_accepts_keyboard_candidate_selection() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("candidate: Option<usize>"));
        assert!(source.contains("candidate.or(current) == Some(i)"));
    }

    #[test]
    fn playlist_and_history_panels_expose_remove_and_clear_commands() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("Command::RemovePlaylistIndex(i)"));
        assert!(source.contains("Command::ClearPlaylist"));
        assert!(source.contains("Command::RemoveHistoryIndex(i)"));
        assert!(source.contains("Command::ClearHistory"));
        assert!(source.contains("t!(\"remove\")"));
        assert!(source.contains("t!(\"clear_all\")"));
    }

    #[test]
    fn playlist_row_actions_are_trailing_hover_actions_with_more_menu() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("row_response.hovered()"));
        assert!(source.contains("ui.add_space(actions_width)"));
        assert!(source.contains("MenuButton::from_button(egui::Button::new(\"⋯\"))"));
        assert!(source.contains("Command::DeletePlaylistFileIndex(i)"));
        assert!(source.contains("Command::DeleteHistoryFileIndex(i)"));
        assert!(source.contains("t!(\"more_actions\")"));
        assert!(source.contains("t!(\"delete_file\")"));
    }

    #[test]
    fn clear_all_action_stays_in_compact_toolbar_before_scroll_area() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let before_scroll = source.split("egui::ScrollArea::vertical()").next().unwrap();

        assert!(before_scroll.contains("clear_all_toolbar"));
        assert!(!before_scroll.contains("Layout::right_to_left(egui::Align::Center)"));
        assert!(source.contains("ui.add_space((ui.available_width() - clear_width).max(0.0))"));
    }

    #[test]
    fn open_entry_is_single_menu_with_file_and_folder_options() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("OPEN_MENU_ICON"));
        assert!(source.contains("Command::OpenDialog"));
        assert!(source.contains("Command::OpenFolder"));
        assert!(source.contains("shortcut_tooltip"));
        assert!(source.contains("t!(\"open_file\")"));
        assert!(source.contains("open_shortcut_label"));
        assert!(source.contains("t!(\"open_folder\")"));
    }
}
