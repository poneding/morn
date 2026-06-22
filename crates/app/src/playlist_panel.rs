//! Playlist and history panel rendering.
//!
//! The panel is intentionally drawn with explicit rectangles for the pieces that
//! must stay visually aligned: header actions, rows, and per-row remove buttons.
//! egui's normal layout helpers are still used for text and spacing, but the
//! clickable row surface owns the full width so hover, selection, drag, and
//! context-menu hit testing all agree on the same bounds.
//!
//! Keep row actions independent from the row label layout.  The filename may be
//! clipped, the row may be dragged, and the remove affordance may be hidden until
//! hover, but the right-side action slot must keep a stable square footprint.
//! This prevents playlist rows from shifting when the pointer enters a row or
//! when localized text changes width.
//!
//! Commands are returned to the app layer instead of executed here.  That keeps
//! this module limited to immediate-mode UI state and leaves playlist mutation,
//! file dialogs, and playback side effects in `PlayerApp`/`Player`.

use eframe::egui;
use player_core::Command;
use rust_i18n::t;

pub const PLAYLIST_MIN_WIDTH: f32 = 220.0;
const ROW_CORNER_RADIUS: u8 = crate::visuals::CONTROL_CORNER_RADIUS;
const ROW_EXTRA_HEIGHT: f32 = 10.0;
const ROW_TEXT_PADDING_X: f32 = crate::visuals::FLOATING_PANEL_INNER_MARGIN_Y as f32;
const ROW_ACTION_MARGIN: f32 = crate::visuals::FLOATING_PANEL_MARGIN;
const REMOVE_BUTTON_SIZE: f32 = 22.0;
const REMOVE_ICON_STROKE_WIDTH: f32 = 1.35;
const REMOVE_ICON_INSET_FACTOR: f32 = 0.19;
const HEADER_ACTION_BUTTON_SIZE: f32 = crate::visuals::ICON_BUTTON_SIZE;

fn header_icon_button_size(_ui: &egui::Ui) -> egui::Vec2 {
    egui::Vec2::splat(HEADER_ACTION_BUTTON_SIZE)
}

fn header_action_button_at(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    icon: &'static str,
) -> egui::Response {
    // Header actions are positioned by the sidebar header, so this draws a button
    // directly into the caller-provided rectangle instead of asking layout to
    // allocate another cell.
    let response = ui.allocate_rect(rect, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        crate::visuals::beveled_button_frame_at(ui, rect, &response, false, 1.0);
        let visuals = ui.style().interact(&response);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(13.5),
            visuals.fg_stroke.color,
        );
    }
    response
}

pub fn open_file_button_size(ui: &egui::Ui) -> egui::Vec2 {
    header_icon_button_size(ui)
}

pub fn clear_all_button_size(ui: &egui::Ui) -> egui::Vec2 {
    header_icon_button_size(ui)
}

pub fn open_file_button_at(ui: &mut egui::Ui, rect: egui::Rect) -> Vec<Command> {
    let mut cmds = Vec::new();
    let response = header_action_button_at(ui, rect, "+");
    let button_response = response.on_hover_text(crate::shortcuts::shortcut_tooltip(
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
        header_action_button_at(ui, rect, "⌫")
    } else {
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect).disabled(), |ui| {
            header_action_button_at(ui, rect, "⌫")
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
    // The remove target is always interactable, but it is only painted on hover.
    // Keeping the hit box stable avoids row text shifting when the icon appears.
    let response = ui
        .interact(rect, remove_id, egui::Sense::click())
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("remove"),
            "Delete/Backspace",
        ));
    if visible && ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let button_size = rect.width().min(rect.height()).min(REMOVE_BUTTON_SIZE);
        let button_rect =
            egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(button_size));
        let radius = (button_size * 0.5).round() as u8;
        ui.painter().rect_filled(
            button_rect,
            egui::CornerRadius::same(radius),
            visuals.weak_bg_fill,
        );
        ui.painter().rect_stroke(
            button_rect,
            egui::CornerRadius::same(radius),
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        paint_centered_x_icon(ui, button_rect, ui.visuals().error_fg_color);
    }
    response
}

fn row_action_button_size(row_height: f32) -> egui::Vec2 {
    // The action slot derives from row height so touch targets scale with egui's
    // configured interaction size.
    let size = (row_height - ROW_ACTION_MARGIN * 2.0).max(0.0);
    egui::vec2(size, size)
}

fn row_actions_at(ui: &mut egui::Ui, input: RowActionInput) -> (Vec<Command>, bool) {
    let RowActionInput {
        row_rect,
        remove_id,
        visible,
        remove_cmd,
    } = input;
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
    // Current playback gets the strong selection color; keyboard candidates and
    // hover use the weaker fill so they read as previews, not active media.
    if current {
        Some(ui.visuals().selection.bg_fill)
    } else if candidate || response.hovered() || response.contains_pointer() {
        Some(ui.visuals().widgets.hovered.weak_bg_fill)
    } else {
        None
    }
}

struct RowTitlePaint<'a> {
    rect: egui::Rect,
    title: &'a str,
    color: egui::Color32,
    strong: bool,
}

fn paint_row_title(ui: &egui::Ui, input: RowTitlePaint<'_>) {
    // Titles are painted manually because the row has a fixed action gutter and
    // should truncate instead of reflowing under the remove button.  The current
    // track is bolded so the now-playing row reads as the anchor of the list.
    let galley = egui::WidgetText::from(row_title_text(input.title, input.strong)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        input.rect.width().max(0.0),
        egui::TextStyle::Body,
    );
    let text_pos = egui::pos2(
        input.rect.left(),
        input.rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, input.color);
}

fn row_title_text(title: &str, strong: bool) -> egui::RichText {
    if strong {
        egui::RichText::new(title).strong()
    } else {
        egui::RichText::new(title)
    }
}

fn sidebar_scroll_area<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // The sidebar owns its width; disabling horizontal auto-shrink prevents long
    // filenames from changing the overlay size.
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

struct PlaylistRow<'a> {
    // Row identity and visual state are passed in explicitly so playlist and
    // history rows can share the same painter without deriving behavior from
    // command variants or path equality.
    current: bool,
    candidate: bool,
    title: &'a str,
    path: &'a std::path::Path,
    tooltip: String,
    remove_id: egui::Id,
    play_cmd: Command,
    remove_cmd: Command,
    delete_cmd: Command,
}

struct RowActionInput {
    row_rect: egui::Rect,
    remove_id: egui::Id,
    visible: bool,
    remove_cmd: Command,
}

fn playlist_row(ui: &mut egui::Ui, row: PlaylistRow<'_>) -> Vec<Command> {
    let PlaylistRow {
        current,
        candidate,
        title,
        path,
        tooltip,
        remove_id,
        play_cmd,
        remove_cmd,
        delete_cmd,
    } = row;
    let mut cmds = Vec::new();
    let row_height = ui.spacing().interact_size.y;
    let row_height = row_height + ROW_EXTRA_HEIGHT;
    let row_width = ui.available_width().max(0.0);
    let (row_rect, row_response) =
        ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());
    let row_response = row_response.on_hover_text(tooltip);
    let show_actions = row_response.hovered() || row_response.contains_pointer();

    // Painting is separated from input collection so the row can remain a single
    // click target while the remove button consumes its own click.
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
    paint_row_title(
        ui,
        RowTitlePaint {
            rect: text_rect,
            title,
            color: text_color,
            strong: current,
        },
    );

    let (action_cmds, remove_clicked) = row_actions_at(
        ui,
        RowActionInput {
            row_rect,
            remove_id,
            visible: show_actions,
            remove_cmd,
        },
    );

    if row_response.clicked() && !remove_clicked {
        cmds.push(play_cmd);
    }
    cmds.extend(playlist_row_context_commands(
        row_response,
        path,
        &delete_cmd,
    ));
    cmds.extend(action_cmds);
    cmds
}

fn playlist_row_context_commands(
    row_response: egui::Response,
    path: &std::path::Path,
    delete_cmd: &Command,
) -> Vec<Command> {
    // Context-menu commands are appended after row/remove clicks.  The app layer
    // still serializes command execution, so a menu action cannot mutate the list
    // while this row is being painted.
    let mut cmds = Vec::new();
    if let Some(menu) = egui::Popup::context_menu(&row_response)
        .style(crate::visuals::panel_popup_style())
        .show(|ui| playlist_row_context_menu(ui, path, delete_cmd))
    {
        cmds.extend(menu.inner);
    }
    cmds
}

fn playlist_row_context_menu(
    ui: &mut egui::Ui,
    path: &std::path::Path,
    delete_cmd: &Command,
) -> Vec<Command> {
    // Clone the path once for menu actions so each branch can return an owned
    // command without borrowing from egui's popup closure.
    let mut selected = Vec::new();
    let path = path.to_path_buf();
    if ui.button(t!("reveal_file").to_string()).clicked() {
        selected.push(Command::RevealFile(path.clone()));
        let _closed = ui.close();
    }
    if ui.button(t!("open_sibling_videos").to_string()).clicked() {
        selected.push(Command::OpenSiblingVideos(path.clone()));
        let _closed = ui.close();
    }
    if ui.button(t!("delete_file").to_string()).clicked() {
        selected.push(delete_cmd.clone());
        let _closed = ui.close();
    }
    selected
}

/// 绘制左侧播放列表, 返回点击产生的命令。
pub fn playlist_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
    candidate: Option<usize>,
) -> Vec<Command> {
    sidebar_scroll_area(ui, |ui| playlist_commands(ui, paths, current, candidate))
}

fn playlist_commands(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    current: Option<usize>,
    candidate: Option<usize>,
) -> Vec<Command> {
    // Playlist rows use indices for commands because the engine owns cursor
    // normalization after removals and deletions.
    let mut cmds = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let is_current = current == Some(i);
        let is_candidate = candidate == Some(i);
        cmds.extend(playlist_row(
            ui,
            PlaylistRow {
                current: is_current,
                candidate: is_candidate,
                title: name,
                path: p,
                tooltip: p.to_string_lossy().to_string(),
                remove_id: ui.make_persistent_id(("playlist_remove", i)),
                play_cmd: Command::PlayIndex(i),
                remove_cmd: Command::RemovePlaylistIndex(i),
                delete_cmd: Command::DeletePlaylistFileIndex(i),
            },
        ));
    }
    cmds
}

/// 绘制历史列表, 点击某项返回 Open 命令。
pub fn history_panel(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    candidate: Option<usize>,
) -> Vec<Command> {
    sidebar_scroll_area(ui, |ui| history_commands(ui, paths, candidate))
}

fn history_commands(
    ui: &mut egui::Ui,
    paths: &[std::path::PathBuf],
    candidate: Option<usize>,
) -> Vec<Command> {
    // History rows open by path, but remove/delete by index to match the persisted
    // history ordering shown to the user in this frame.
    let mut cmds = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        cmds.extend(playlist_row(
            ui,
            PlaylistRow {
                current: false,
                candidate: candidate == Some(i),
                title: name,
                path: p,
                tooltip: p.to_string_lossy().to_string(),
                remove_id: ui.make_persistent_id(("history_remove", i)),
                play_cmd: Command::Open(p.clone()),
                remove_cmd: Command::RemoveHistoryIndex(i),
                delete_cmd: Command::DeleteHistoryFileIndex(i),
            },
        ));
    }
    cmds
}

#[cfg(test)]
mod tests {
    fn source_before_tests() -> &'static str {
        include_str!("playlist_panel.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
    }

    fn source_before(marker: &str) -> &'static str {
        include_str!("playlist_panel.rs")
            .split(marker)
            .next()
            .unwrap_or_default()
    }

    fn source_section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        source
            .split(start)
            .nth(1)
            .unwrap_or_default()
            .split(end)
            .next()
            .unwrap_or_default()
    }

    #[test]
    fn open_file_entry_is_a_single_default_sized_button() {
        let source = source_before("fn remove_button_at");
        assert!(!source.contains("menu_button(\"+\")"));
        assert!(!source.contains("OPEN_MENU"));
        assert!(!source.contains("MenuButton"));
        assert!(!source.contains("OpenFolder"));
        assert!(source.contains("header_action_button_at(ui, rect, \"+\")"));
        assert!(source.contains("Command::OpenDialog"));
    }

    #[test]
    fn header_action_buttons_use_shared_symbol_buttons() {
        let source = source_before_tests();

        assert!(source.contains("fn header_icon_button_size"));
        assert!(source.contains("HEADER_ACTION_BUTTON_SIZE"));
        assert!(source.contains("egui::Vec2::splat(HEADER_ACTION_BUTTON_SIZE)"));
        assert!(source.contains("open_file_button_size"));
        assert!(source.contains("clear_all_button_size"));
        assert!(source.contains("beveled_button_frame_at"));
        assert!(source.contains("header_action_button_at(ui, rect, \"+\")"));
        assert!(source.contains("header_action_button_at(ui, rect, \"⌫\")"));
        assert!(!source.contains("enum HeaderActionIcon"));
        assert!(!source.contains("paint_header_add_icon"));
        assert!(!source.contains("paint_header_trash_icon"));
        assert!(!source.contains("paint_header_clear_list_icon"));
        assert!(!source.contains("fn adaptive_icon_button"));
        assert!(!source.contains("fn icon_button_size"));
    }

    #[test]
    fn playlist_items_are_single_line_truncated() {
        let source = source_before_tests();
        assert!(source.contains("egui::TextWrapMode::Truncate"));
        assert!(source.contains("rect.width().max(0.0)"));
    }

    #[test]
    fn playlist_items_use_fixed_rect_rows_instead_of_buttons() {
        let source = source_before_tests();

        assert!(source.contains("allocate_exact_size(egui::vec2(row_width, row_height)"));
        assert!(source.contains("paint_row_title"));
        assert!(!source.contains("Button::selectable"));
        assert!(!source.contains("add_sized("));
    }

    #[test]
    fn playlist_panel_does_not_duplicate_bottom_navigation_controls() {
        let source = source_before_tests();

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
        let source = source_before_tests();

        assert!(source.contains("candidate: Option<usize>"));
        assert!(source.contains("let is_current = current == Some(i)"));
        assert!(source.contains("let is_candidate = candidate == Some(i)"));
        assert!(!source.contains("candidate.or(current) == Some(i)"));
    }

    #[test]
    fn playlist_uses_blue_only_for_current_and_gray_for_candidate_or_hover() {
        let source = source_before_tests();
        let row_fill_source = source_section(source, "fn row_fill", "fn paint_row_title");

        assert!(row_fill_source.contains("current: bool"));
        assert!(row_fill_source.contains("candidate: bool"));
        assert!(row_fill_source.contains("if current"));
        assert!(row_fill_source
            .contains("candidate || response.hovered() || response.contains_pointer()"));
        assert!(row_fill_source.contains("ui.visuals().widgets.hovered.weak_bg_fill"));
        assert!(source.contains("let text_color = if current"));
        assert!(!source.contains("let text_color = if selected"));
    }

    #[test]
    fn playlist_and_history_panels_expose_remove_and_clear_commands() {
        let source = source_before_tests();
        let app_source = [
            include_str!("app.rs"),
            include_str!("app_ui.rs"),
            include_str!("app_sidebar_ui.rs"),
        ]
        .join("\n");

        assert!(source.contains("Command::RemovePlaylistIndex(i)"));
        assert!(source.contains("Command::RemoveHistoryIndex(i)"));
        assert!(app_source.contains("player_core::Command::ClearPlaylist"));
        assert!(app_source.contains("player_core::Command::ClearHistory"));
        assert!(source.contains("t!(\"remove\")"));
        assert!(source.contains("t!(\"clear_all\")"));
    }

    #[test]
    fn playlist_row_actions_only_paint_remove_on_hover() {
        let source = source_before_tests();

        assert!(source.contains("row_response.hovered()"));
        assert!(source.contains("row_actions_at"));
        assert!(source.contains("visible && ui.is_rect_visible(rect)"));
        assert!(
            source.contains("paint_centered_x_icon(ui, button_rect, ui.visuals().error_fg_color)")
        );
        assert!(source.contains("ui.visuals().error_fg_color"));
        assert!(source.contains("row_actions_width(row_height) + ROW_TEXT_PADDING_X"));
        assert!(source.contains("RowActionInput"));
        assert!(source.contains("visible: show_actions"));
        assert!(!source.contains("ui.add_space(actions_width)"));
        assert!(!source.contains("MenuButton::from_button"));
        assert!(!source.contains("MORE_ACTIONS_ICON"));
        assert!(!source.contains("more_actions_button_at"));
        assert!(!source.contains("t!(\"more_actions\")"));
    }

    #[test]
    fn playlist_remove_button_is_square_inset_and_does_not_shift_text_on_hover() {
        let source = source_before_tests();

        assert!(
            source.contains("const ROW_ACTION_MARGIN: f32 = crate::visuals::FLOATING_PANEL_MARGIN")
        );
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
        let source = source_before_tests();

        assert!(source.contains("const REMOVE_BUTTON_SIZE: f32 = 22.0"));
        assert!(source.contains("button_size"));
        assert!(source.contains("button_rect"));
        assert!(source.contains("egui::CornerRadius::same(radius)"));
        assert!(source.contains("fn paint_centered_x_icon"));
        assert!(source.contains("line_segment"));
        assert!(source.contains("const REMOVE_ICON_STROKE_WIDTH: f32 = 1.35"));
        assert!(source.contains("const REMOVE_ICON_INSET_FACTOR: f32 = 0.19"));
        assert!(!source.contains("centered_icon_button_at(ui, rect, REMOVE_ICON"));
    }

    #[test]
    fn playlist_remove_action_does_not_allocate_extra_layout_on_hover() {
        let source = source_before_tests();
        let action_source = source_section(source, "fn row_actions_at", "fn row_actions_width");
        let remove_button_source =
            source_section(source, "fn remove_button_at", "fn row_action_button_size");

        assert!(remove_button_source.contains(".interact("));
        assert!(action_source.contains("remove_id"));
        assert!(!action_source.contains("allocate_rect"));
        assert!(!source.contains("if show_actions {\n        cmds.extend(row_actions_at"));
    }

    #[test]
    fn playlist_rows_use_rounded_fill_extra_height_and_tighter_padding() {
        let source = source_before_tests();

        assert!(source.contains(
            "const ROW_TEXT_PADDING_X: f32 = crate::visuals::FLOATING_PANEL_INNER_MARGIN_Y as f32"
        ));
        assert!(source.contains("const ROW_EXTRA_HEIGHT: f32 = 10.0"));
        assert!(
            source.contains("const ROW_CORNER_RADIUS: u8 = crate::visuals::CONTROL_CORNER_RADIUS")
        );
        assert!(source.contains("let row_height = row_height + ROW_EXTRA_HEIGHT"));
        assert!(source.contains("rect_filled(row_rect, ROW_CORNER_RADIUS, fill)"));
        assert!(source.contains("row_rect.top() + ROW_ACTION_MARGIN"));
        assert!(source.contains("row_rect.right() - ROW_ACTION_MARGIN"));
    }

    #[test]
    fn playlist_rows_expose_file_actions_in_context_menu() {
        let source = source_before_tests();

        assert!(source.contains("egui::Popup::context_menu(&row_response)"));
        assert!(source.contains("panel_popup_style"));
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
        let source = source_before_tests();
        let app_source = [
            include_str!("app.rs"),
            include_str!("app_ui.rs"),
            include_str!("app_sidebar_ui.rs"),
        ]
        .join("\n");

        assert!(!source.contains("clear_all_toolbar"));
        assert!(!source.contains("Layout::right_to_left(egui::Align::Center)"));
        assert!(source.contains("clear_all_button_at"));
        assert!(source.contains("egui::UiBuilder::new().max_rect(rect).disabled()"));
        assert!(app_source.contains("clear_all_button_at"));
        assert!(app_source.contains("!data.playlist_paths.is_empty()"));
        assert!(app_source.contains("!data.history.is_empty()"));
    }

    #[test]
    fn open_entry_only_exposes_file_dialog() {
        let source = source_before_tests();

        assert!(source.contains("header_action_button_at(ui, rect, \"+\")"));
        assert!(source.contains("Command::OpenDialog"));
        assert!(source.contains("shortcut_tooltip"));
        assert!(source.contains("t!(\"open_file\")"));
        assert!(source.contains("open_shortcut_label"));
        assert!(!source.contains("Command::OpenFolder"));
        assert!(!source.contains("t!(\"open_folder\")"));
    }

    #[test]
    fn sidebar_scroll_area_fills_fixed_panel_instead_of_content_height() {
        let source = source_before_tests();

        assert!(source.contains("fn sidebar_scroll_area"));
        assert!(source.contains(".max_width(ui.available_width())"));
        assert!(source.contains(".max_height(ui.available_height())"));
        assert!(source.contains(".auto_shrink([false, false])"));
    }
}
