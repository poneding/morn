use eframe::egui;
use player_core::Command;
use rust_i18n::t;

pub const OPEN_FILE_ICON: &str = "➕";
pub const CLEAR_ALL_ICON: &str = "🗑";
pub const PLAYLIST_MIN_WIDTH: f32 = 220.0;
const ROW_CORNER_RADIUS: u8 = 6;
const ROW_EXTRA_HEIGHT: f32 = 4.0;
const ROW_TEXT_PADDING_X: f32 = 4.0;
const ROW_ACTION_MARGIN: f32 = 4.0;
const REMOVE_ICON_STROKE_WIDTH: f32 = 1.5;
const REMOVE_ICON_INSET_FACTOR: f32 = 0.25;

fn icon_button_size(ui: &egui::Ui, icon: &str) -> egui::Vec2 {
    let text_width = ui
        .painter()
        .layout_no_wrap(
            icon.to_owned(),
            egui::TextStyle::Button.resolve(ui.style()),
            ui.visuals().text_color(),
        )
        .size()
        .x;
    egui::vec2(
        text_width + ui.spacing().button_padding.x * 2.0,
        ui.spacing().interact_size.y,
    )
}

fn adaptive_icon_button(ui: &egui::Ui, icon: &'static str) -> egui::Button<'static> {
    egui::Button::new(icon).min_size(icon_button_size(ui, icon))
}

fn centered_icon_button_at(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: &str,
    color: egui::Color32,
) -> egui::Response {
    let response = ui.allocate_rect(rect, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        ui.painter().rect_stroke(
            rect,
            visuals.corner_radius,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );

        let galley = ui.painter().layout_no_wrap(
            icon.to_owned(),
            egui::TextStyle::Button.resolve(ui.style()),
            color,
        );
        let text_pos = egui::pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(text_pos, galley, color);
    }
    response
}

pub fn open_file_button_size(ui: &egui::Ui) -> egui::Vec2 {
    icon_button_size(ui, OPEN_FILE_ICON)
}

pub fn clear_all_button_size(ui: &egui::Ui) -> egui::Vec2 {
    icon_button_size(ui, CLEAR_ALL_ICON)
}

pub fn open_file_button(ui: &mut egui::Ui) -> Vec<Command> {
    let mut cmds = Vec::new();
    let button_response = ui
        .add(adaptive_icon_button(ui, OPEN_FILE_ICON))
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("open_file"),
            crate::shortcuts::open_shortcut_label(),
        ));
    if button_response.clicked() {
        cmds.push(Command::OpenDialog);
    }
    cmds
}

pub fn open_file_button_at(ui: &mut egui::Ui, rect: egui::Rect) -> Vec<Command> {
    let mut cmds = Vec::new();
    let button_response = ui
        .put(rect, adaptive_icon_button(ui, OPEN_FILE_ICON))
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("open_file"),
            crate::shortcuts::open_shortcut_label(),
        ));
    if button_response.clicked() {
        cmds.push(Command::OpenDialog);
    }
    cmds
}

pub fn clear_all_button_at(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    cmd: Command,
    enabled: bool,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    let response = if enabled {
        centered_icon_button_at(ui, rect, CLEAR_ALL_ICON, ui.visuals().text_color())
    } else {
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect).disabled(), |ui| {
            centered_icon_button_at(ui, rect, CLEAR_ALL_ICON, ui.visuals().weak_text_color())
        })
        .inner
    }
    .on_hover_text(t!("clear_all").to_string());
    if enabled && response.clicked() {
        cmds.push(cmd);
    }
    cmds
}

fn paint_centered_x_icon(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
    let half = rect.width().min(rect.height()) * REMOVE_ICON_INSET_FACTOR;
    let center = rect.center();
    let stroke = egui::Stroke::new(REMOVE_ICON_STROKE_WIDTH, color);
    ui.painter().line_segment(
        [
            egui::pos2(center.x - half, center.y - half),
            egui::pos2(center.x + half, center.y + half),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            egui::pos2(center.x + half, center.y - half),
            egui::pos2(center.x - half, center.y + half),
        ],
        stroke,
    );
}

fn remove_button_at(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    remove_id: egui::Id,
    visible: bool,
) -> egui::Response {
    let response = ui
        .interact(rect, remove_id, egui::Sense::click())
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("remove"),
            "Delete/Backspace",
        ));
    if visible && ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        ui.painter().rect_stroke(
            rect,
            visuals.corner_radius,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        paint_centered_x_icon(ui, rect, ui.visuals().error_fg_color);
    }
    response
}

fn row_action_button_size(row_height: f32) -> egui::Vec2 {
    let size = (row_height - ROW_ACTION_MARGIN * 2.0).max(0.0);
    egui::vec2(size, size)
}

fn row_actions_at(
    ui: &mut egui::Ui,
    row_rect: egui::Rect,
    remove_id: egui::Id,
    visible: bool,
    remove_cmd: Command,
) -> (Vec<Command>, bool) {
    let mut cmds = Vec::new();
    let remove_size = row_action_button_size(row_rect.height());
    let remove_rect = egui::Rect::from_min_max(
        egui::pos2(
            row_rect.right() - ROW_ACTION_MARGIN - remove_size.x,
            row_rect.top() + ROW_ACTION_MARGIN,
        ),
        egui::pos2(
            row_rect.right() - ROW_ACTION_MARGIN,
            row_rect.top() + ROW_ACTION_MARGIN + remove_size.y,
        ),
    );

    let remove_response = remove_button_at(ui, remove_rect, remove_id, visible);
    let remove_clicked = remove_response.clicked();
    if remove_clicked {
        cmds.push(remove_cmd);
    }
    (cmds, remove_clicked)
}

fn row_actions_width(row_height: f32) -> f32 {
    row_action_button_size(row_height).x + ROW_ACTION_MARGIN
}

fn row_fill(
    ui: &egui::Ui,
    response: &egui::Response,
    current: bool,
    candidate: bool,
) -> Option<egui::Color32> {
    if current {
        Some(ui.visuals().selection.bg_fill)
    } else if candidate || response.hovered() {
        Some(ui.visuals().widgets.hovered.weak_bg_fill)
    } else {
        None
    }
}

fn paint_row_title(ui: &egui::Ui, rect: egui::Rect, title: &str, color: egui::Color32) {
    let galley = egui::WidgetText::from(title.to_owned()).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        rect.width().max(0.0),
        egui::TextStyle::Body,
    );
    let text_pos = egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(text_pos, galley, color);
}

fn sidebar_scroll_area<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::ScrollArea::vertical()
        .max_width(ui.available_width())
        .max_height(ui.available_height())
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

fn playlist_row(
    ui: &mut egui::Ui,
    current: bool,
    candidate: bool,
    title: &str,
    path: &std::path::Path,
    tooltip: String,
    remove_id: egui::Id,
    play_cmd: Command,
    remove_cmd: Command,
    delete_cmd: Command,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    let row_height = ui.spacing().interact_size.y;
    let row_height = row_height + ROW_EXTRA_HEIGHT;
    let row_width = ui.available_width().max(0.0);
    let (row_rect, row_response) =
        ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());
    let row_response = row_response.on_hover_text(tooltip);
    let show_actions = row_response.hovered() || row_response.contains_pointer();

    if let Some(fill) = row_fill(ui, &row_response, current, candidate) {
        ui.painter().rect_filled(row_rect, ROW_CORNER_RADIUS, fill);
    }

    let actions_width = row_actions_width(row_height) + ROW_TEXT_PADDING_X;
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(row_rect.left() + ROW_TEXT_PADDING_X, row_rect.top()),
        egui::pos2(
            (row_rect.right() - ROW_TEXT_PADDING_X - actions_width).max(row_rect.left()),
            row_rect.bottom(),
        ),
    );
    let text_color = if current {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    };
    paint_row_title(ui, text_rect, title, text_color);

    let (action_cmds, remove_clicked) =
        row_actions_at(ui, row_rect, remove_id, show_actions, remove_cmd);

    if row_response.clicked() && !remove_clicked {
        cmds.push(play_cmd);
    }
    if let Some(menu) = egui::Popup::context_menu(&row_response)
        .style(crate::visuals::frosted_popup_style())
        .show(|ui| {
            let mut selected = Vec::new();
            let path = path.to_path_buf();
            if ui.button(t!("reveal_file").to_string()).clicked() {
                selected.push(Command::RevealFile(path.clone()));
                ui.close();
            }
            if ui.button(t!("open_sibling_videos").to_string()).clicked() {
                selected.push(Command::OpenSiblingVideos(path.clone()));
                ui.close();
            }
            if ui.button(t!("delete_file").to_string()).clicked() {
                selected.push(delete_cmd);
                ui.close();
            }
            selected
        })
    {
        cmds.extend(menu.inner);
    }
    cmds.extend(action_cmds);
    cmds
}

/// 绘制左侧播放列表, 返回点击产生的命令。
pub fn playlist_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
    candidate: Option<usize>,
) -> Vec<Command> {
    let mut cmds = Vec::new();
    sidebar_scroll_area(ui, |ui| {
        for (i, p) in paths.iter().enumerate() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let is_current = current == Some(i);
            let is_candidate = candidate == Some(i);
            cmds.extend(playlist_row(
                ui,
                is_current,
                is_candidate,
                name,
                p,
                p.to_string_lossy().to_string(),
                ui.make_persistent_id(("playlist_remove", i)),
                Command::PlayIndex(i),
                Command::RemovePlaylistIndex(i),
                Command::DeletePlaylistFileIndex(i),
            ));
        }
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
    sidebar_scroll_area(ui, |ui| {
        for (i, p) in paths.iter().enumerate() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            cmds.extend(playlist_row(
                ui,
                false,
                candidate == Some(i),
                name,
                p,
                p.to_string_lossy().to_string(),
                ui.make_persistent_id(("history_remove", i)),
                Command::Open(p.clone()),
                Command::RemoveHistoryIndex(i),
                Command::DeleteHistoryFileIndex(i),
            ));
        }
    });
    cmds
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_file_entry_is_a_single_default_sized_button() {
        assert_eq!(super::OPEN_FILE_ICON, "➕");

        let source = include_str!("playlist_panel.rs")
            .split("fn remove_button_at")
            .next()
            .unwrap();
        assert!(!source.contains("menu_button(\"+\")"));
        assert!(!source.contains("OPEN_MENU"));
        assert!(!source.contains("MenuButton"));
        assert!(!source.contains("OpenFolder"));
        assert!(source.contains("adaptive_icon_button"));
        assert!(source.contains("Command::OpenDialog"));
    }

    #[test]
    fn icon_buttons_use_content_width_and_stable_height() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("fn adaptive_icon_button"));
        assert!(source.contains("min_size(icon_button_size(ui, icon))"));
        assert!(source.contains("fn icon_button_size"));
        assert!(source.contains("layout_no_wrap"));
        assert!(source.contains("ui.spacing().button_padding.x * 2.0"));
        assert!(source.contains("open_file_button_size"));
        assert!(source.contains("clear_all_button_size"));
    }

    #[test]
    fn playlist_items_are_single_line_truncated() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("egui::TextWrapMode::Truncate"));
        assert!(source.contains("rect.width().max(0.0)"));
    }

    #[test]
    fn playlist_items_use_fixed_rect_rows_instead_of_buttons() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("allocate_exact_size(egui::vec2(row_width, row_height)"));
        assert!(source.contains("paint_row_title"));
        assert!(!source.contains("Button::selectable"));
        assert!(!source.contains("add_sized("));
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
        assert!(source.contains("let is_current = current == Some(i)"));
        assert!(source.contains("let is_candidate = candidate == Some(i)"));
        assert!(!source.contains("candidate.or(current) == Some(i)"));
    }

    #[test]
    fn playlist_uses_blue_only_for_current_and_gray_for_candidate_or_hover() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let row_fill_source = source
            .split("fn row_fill")
            .nth(1)
            .unwrap()
            .split("fn paint_row_title")
            .next()
            .unwrap();

        assert!(row_fill_source.contains("current: bool"));
        assert!(row_fill_source.contains("candidate: bool"));
        assert!(row_fill_source.contains("if current"));
        assert!(row_fill_source.contains("candidate || response.hovered()"));
        assert!(row_fill_source.contains("ui.visuals().widgets.hovered.weak_bg_fill"));
        assert!(source.contains("let text_color = if current"));
        assert!(!source.contains("let text_color = if selected"));
    }

    #[test]
    fn playlist_and_history_panels_expose_remove_and_clear_commands() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let app_source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("Command::RemovePlaylistIndex(i)"));
        assert!(source.contains("Command::RemoveHistoryIndex(i)"));
        assert!(app_source.contains("player_core::Command::ClearPlaylist"));
        assert!(app_source.contains("player_core::Command::ClearHistory"));
        assert!(source.contains("t!(\"remove\")"));
        assert!(source.contains("t!(\"clear_all\")"));
    }

    #[test]
    fn playlist_row_actions_only_paint_remove_on_hover() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("row_response.hovered()"));
        assert!(source.contains("row_actions_at"));
        assert!(source.contains("visible && ui.is_rect_visible(rect)"));
        assert!(source.contains("paint_centered_x_icon(ui, rect, ui.visuals().error_fg_color)"));
        assert!(source.contains("ui.visuals().error_fg_color"));
        assert!(source.contains("row_actions_width(row_height) + ROW_TEXT_PADDING_X"));
        assert!(
            source.contains("row_actions_at(ui, row_rect, remove_id, show_actions, remove_cmd)")
        );
        assert!(!source.contains("ui.add_space(actions_width)"));
        assert!(!source.contains("MenuButton::from_button"));
        assert!(!source.contains("MORE_ACTIONS_ICON"));
        assert!(!source.contains("more_actions_button_at"));
        assert!(!source.contains("t!(\"more_actions\")"));
    }

    #[test]
    fn playlist_remove_button_is_square_inset_and_does_not_shift_text_on_hover() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("const ROW_ACTION_MARGIN: f32 = 4.0"));
        assert!(source.contains("fn row_action_button_size(row_height: f32) -> egui::Vec2"));
        assert!(source.contains("egui::vec2(size, size)"));
        assert!(source.contains("row_rect.top() + ROW_ACTION_MARGIN"));
        assert!(source.contains("row_rect.right() - ROW_ACTION_MARGIN"));
        assert!(source.contains("row_actions_width(row_height) + ROW_TEXT_PADDING_X"));
        assert!(!source.contains("let actions_width = if show_actions"));
        assert!(!source.contains("icon_button_size(ui, REMOVE_ICON)"));
    }

    #[test]
    fn playlist_remove_button_draws_centered_x_without_text_glyph() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("fn paint_centered_x_icon"));
        assert!(source.contains("line_segment"));
        assert!(source.contains("const REMOVE_ICON_STROKE_WIDTH: f32 = 1.5"));
        assert!(source.contains("const REMOVE_ICON_INSET_FACTOR: f32 = 0.25"));
        assert!(!source.contains("centered_icon_button_at(ui, rect, REMOVE_ICON"));
    }

    #[test]
    fn playlist_remove_action_does_not_allocate_extra_layout_on_hover() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let action_source = source
            .split("fn row_actions_at")
            .nth(1)
            .unwrap()
            .split("fn row_actions_width")
            .next()
            .unwrap();
        let remove_button_source = source
            .split("fn remove_button_at")
            .nth(1)
            .unwrap()
            .split("fn row_action_button_size")
            .next()
            .unwrap();

        assert!(remove_button_source.contains(".interact("));
        assert!(action_source.contains("remove_id"));
        assert!(!action_source.contains("allocate_rect"));
        assert!(!source.contains("if show_actions {\n        cmds.extend(row_actions_at"));
    }

    #[test]
    fn playlist_rows_use_rounded_fill_extra_height_and_tighter_padding() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("const ROW_TEXT_PADDING_X: f32 = 4.0"));
        assert!(source.contains("const ROW_EXTRA_HEIGHT: f32 = 4.0"));
        assert!(source.contains("const ROW_CORNER_RADIUS: u8 = 6"));
        assert!(source.contains("let row_height = row_height + ROW_EXTRA_HEIGHT"));
        assert!(source.contains("rect_filled(row_rect, ROW_CORNER_RADIUS, fill)"));
        assert!(source.contains("row_rect.top() + ROW_ACTION_MARGIN"));
        assert!(source.contains("row_rect.right() - ROW_ACTION_MARGIN"));
    }

    #[test]
    fn playlist_rows_expose_file_actions_in_context_menu() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("egui::Popup::context_menu(&row_response)"));
        assert!(source.contains("frosted_popup_style"));
        assert!(source.contains("t!(\"reveal_file\")"));
        assert!(source.contains("Command::RevealFile(path.clone())"));
        assert!(source.contains("t!(\"open_sibling_videos\")"));
        assert!(source.contains("Command::OpenSiblingVideos(path.clone())"));
        assert!(source.contains("Command::DeletePlaylistFileIndex(i)"));
        assert!(source.contains("Command::DeleteHistoryFileIndex(i)"));
        assert!(source.contains("t!(\"delete_file\")"));
    }

    #[test]
    fn clear_all_action_lives_in_header_and_disables_without_items() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let app_source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(!source.contains("clear_all_toolbar"));
        assert!(!source.contains("Layout::right_to_left(egui::Align::Center)"));
        assert!(source.contains("clear_all_button_at"));
        assert!(source.contains("egui::UiBuilder::new().max_rect(rect).disabled()"));
        assert!(app_source.contains("clear_all_button_at"));
        assert!(app_source.contains("!playlist_paths.is_empty()"));
        assert!(app_source.contains("!hist.is_empty()"));
    }

    #[test]
    fn open_entry_only_exposes_file_dialog() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("OPEN_FILE_ICON"));
        assert!(source.contains("Command::OpenDialog"));
        assert!(source.contains("shortcut_tooltip"));
        assert!(source.contains("t!(\"open_file\")"));
        assert!(source.contains("open_shortcut_label"));
        assert!(!source.contains("Command::OpenFolder"));
        assert!(!source.contains("t!(\"open_folder\")"));
    }

    #[test]
    fn sidebar_scroll_area_fills_fixed_panel_instead_of_content_height() {
        let source = include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("fn sidebar_scroll_area"));
        assert!(source.contains(".max_width(ui.available_width())"));
        assert!(source.contains(".max_height(ui.available_height())"));
        assert!(source.contains(".auto_shrink([false, false])"));
    }
}
