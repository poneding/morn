use super::app_source;

#[test]
fn app_handles_playlist_context_file_commands() {
    let source = app_source();

    assert!(source.contains("player_core::Command::RevealFile(path) => reveal_file(&path)"));
    assert!(source.contains("player_core::Command::OpenSiblingVideos(_)"));
    assert!(source.contains("self.sidebar_tab = SidebarTab::Playlist"));
    assert!(source.contains("self.history_candidate = None"));
    assert!(source.contains("self.playlist_candidate = playlist_candidate_for_open"));
}

#[test]
fn video_panel_uses_no_frame_to_touch_window_edges() {
    let source = app_source();

    assert!(source.contains("CentralPanel::no_frame()"));
    assert!(!source.contains("CentralPanel::default().show_inside"));
}

#[test]
fn screenshot_notice_is_top_centered_and_includes_path() {
    let source = app_source();
    assert!(!source.contains("Align2::RIGHT_BOTTOM"));
    assert!(source.contains("Align2::CENTER_TOP"));
    assert!(source.contains("SCREENSHOT_NOTICE_TOP_OFFSET"));
    assert!(!source.contains("screenshot_notice_pos"));
    assert!(source.contains("p.display()"));
}

#[test]
fn screenshots_are_saved_under_configured_directory() {
    let source = app_source();

    assert!(source.contains("self.player.screenshot_dir()"));
    assert!(!source.contains("engine::resolve_screenshot_dir"));
    assert!(source.contains("save_screenshot"));
}

#[test]
fn bottom_controls_float_over_video_and_auto_hide() {
    let source = app_source();

    assert!(source.contains("Area::new(egui::Id::new(\"floating_controls\"))"));
    assert!(source.contains("egui::Order::Foreground"));
    assert!(source.contains("bottom_controls_visible"));
    assert!(!source.contains("Panel::bottom(\"controls\")"));
}

#[test]
fn bottom_controls_are_content_width_centered_without_app_actions() {
    let source = app_source();

    assert!(source.contains("Layout::left_to_right(egui::Align::Center)"));
    // 自适应宽度: 用 max_width 作上限而非强制撑满, 不再 set_width(outer)/set_min_width。
    assert!(source.contains("control_bar_max_width(state.screen_rect.width())"));
    assert!(source.contains("control_bar_slider_width(state.screen_rect.width())"));
    assert!(source.contains("ui.set_max_width(data.max_width)"));
    assert!(!source.contains("ui.set_min_width(data.outer_width)"));
    // ⚙/☰ live in the custom titlebar, so the bottom bar stays playback-focused.
    assert!(!source.contains("ui.add_space(ui.available_width().max(0.0))"));
    assert!(!source.contains("show_bottom_app_actions"));
}

#[test]
fn bottom_controls_use_content_height_and_idle_auto_hide() {
    let source = app_source();

    assert!(!source.contains("CONTROL_BAR_HEIGHT"));
    assert!(!source.contains("CONTROL_BAR_HOLD"));
    assert!(!source.contains("controls_keep_visible_until"));
    assert!(!source.contains("set_min_height(CONTROL_BAR_HEIGHT)"));
    assert!(!source.contains("is_pointer_over_egui"));
    assert!(source.contains("CONTROLS_IDLE_HIDE_AFTER"));
    assert!(source.contains("last_pointer_move"));
    assert!(source.contains("OVERLAY_HOVER_RECHECK_GRACE"));
    assert!(source.contains("pointer_activity_repaint_delay"));
}

#[test]
fn bottom_controls_are_centered_on_window_not_constrained_by_playlist() {
    let source = app_source();

    assert!(source.contains("egui::vec2(0.0, -crate::visuals::FLOATING_PANEL_MARGIN)"));
    assert!(!source.contains("sheet_reserved_width"));
    assert!(!source.contains("control_rect"));
}

#[test]
fn overlay_hover_uses_current_area_response_not_stored_rects() {
    let source = app_source();

    assert!(!source.contains("playlist_sheet_rect"));
    assert!(!source.contains("floating_controls_rect"));
    assert!(source.contains("playlist_area.inner || playlist_area.response.hovered()"));
    assert!(source
        .contains("floating_controls_area.inner || floating_controls_area.response.hovered()"));
    assert!(source.matches(".fade_in(false)").count() >= 2);
}

#[test]
fn playlist_sheet_overlays_without_resizing_video() {
    let source = app_source();

    assert!(source.contains("show_playlist"));
    assert!(source.contains("Area::new(egui::Id::new(\"playlist_sheet_fixed_v2\"))"));
    assert!(source.contains(".anchor("));
    assert!(source.contains("egui::Align2::RIGHT_TOP"));
    assert!(source.contains(".default_size(egui::vec2(data.sheet_width, data.sheet_height))"));
    assert!(source.contains("egui::Order::Foreground"));
    assert!(!source.contains("playlist_sheet_pos"));
    assert!(!source.contains("Panel::right(\"playlist\")"));
    assert!(!source.contains("Panel::left(\"playlist\")"));
    assert!(source.contains("toggle_playlist_visibility"));
}

#[test]
fn playlist_is_closed_by_default_on_startup() {
    let source = app_source();

    assert!(source.contains("show_playlist: false"));
    assert!(!source.contains("show_playlist: true"));
}

#[test]
fn playlist_sheet_floats_as_rounded_card_above_controls() {
    let source = app_source();
    let playlist_source = source
        .split("egui::Area::new(egui::Id::new(\"playlist_sheet_fixed_v2\"))")
        .nth(1)
        .unwrap()
        .split("fn show_bottom_controls_overlay")
        .next()
        .unwrap();

    assert!(playlist_source.contains(".anchor("));
    assert!(playlist_source.contains("egui::Align2::RIGHT_TOP"));
    assert!(playlist_source.contains("-crate::visuals::FLOATING_PANEL_MARGIN"));
    assert!(playlist_source.contains("playlist_sheet_top_offset()"));
    assert!(source.contains("fn playlist_sheet_gap()"));
    assert!(source.contains("TITLEBAR_BOTTOM_OFFSET"));
    assert!(source.contains("crate::controls::CONTROL_BUTTON_SIZE"));
    assert!(source.contains("crate::visuals::FLOATING_PANEL_MARGIN * 2.0"));
    assert!(source.contains("fn floating_controls_top_offset"));
    assert!(source.contains("- playlist_sheet_gap()"));
    assert!(source.contains("- playlist_sheet_top_offset()"));
    assert!(source.contains("floating_controls_outer_height(style)"));
    assert!(playlist_source.contains("crate::visuals::panel_frame"));
}

#[test]
fn playlist_sheet_content_size_deducts_frame_margin() {
    let source = app_source();
    let playlist_source = source
        .split("egui::Area::new(egui::Id::new(\"playlist_sheet_fixed_v2\"))")
        .nth(1)
        .unwrap()
        .split("fn apply_sidebar_commands")
        .next()
        .unwrap();

    assert!(playlist_source.contains("playlist_frame.total_margin().sum()"));
    assert!(source.contains("data.sheet_width - frame_margin.x"));
    assert!(source.contains("data.sheet_height - frame_margin.y"));
    assert!(playlist_source.contains("ui.set_width(content_size.x)"));
    assert!(playlist_source.contains("ui.set_height(content_size.y)"));
    assert!(!playlist_source.contains("ui.set_min_height(data.sheet_height)"));
}

#[test]
fn playlist_sidebar_tabs_use_settings_like_padding() {
    let source = app_source();

    assert!(source.contains("const SIDEBAR_TAB_PADDING_X: f32 = 14.0"));
    assert!(source.contains("ui.spacing_mut().button_padding.x = SIDEBAR_TAB_PADDING_X"));
    assert!(source.contains("let tab_padding = SIDEBAR_TAB_PADDING_X * 2.0"));
}

#[test]
fn overlay_panels_use_opaque_dark_panel_style() {
    let source = app_source();
    let visuals_source = include_str!("visuals.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap();

    assert!(source.contains("crate::visuals::panel_frame"));
    assert!(visuals_source.contains("panel_frame"));
    assert!(visuals_source.contains("panel_popup_style"));
    // 不做磨砂: 面板是不透明实色(flashot 风格), 视频不从后面透出。
    assert!(visuals_source.contains("panel_fill_from_visuals"));
    assert!(visuals_source.contains(".to_opaque()"));
    assert!(!source.contains("FrostOverlay"));
}

#[test]
fn app_uses_custom_titlebar_and_app_actions() {
    let source = app_source();

    // 原生窗口装饰: 不再画窗口背景、红绿灯、resize handles。
    assert!(!source.contains("paint_window_background"));
    assert!(source.contains("crate::titlebar::show_custom_titlebar"));
    assert!(source.contains("toggle_playlist_from_titlebar"));
    assert!(source.contains("toggle_playlist_visibility"));
    assert!(source.contains("self.show_settings = !self.show_settings"));
}

#[test]
fn clear_color_matches_native_or_custom_titlebar_mode() {
    let source = app_source();

    assert!(source.contains("fn clear_color"));
    assert!(source.contains("macOS uses the native rounded window frame"));
    assert!(source.contains("[0.0, 0.0, 0.0, 1.0]"));
    // 非 macOS 自绘窗口需要透明背景，让 egui 绘制的圆角背景可见。
    assert!(source.contains("[0.0, 0.0, 0.0, 0.0]"));
}

#[test]
fn settings_and_playlist_buttons_live_in_titlebar() {
    let source = app_source();
    let titlebar_source = include_str!("titlebar.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap();

    assert!(titlebar_source.contains("crate::symbols::PLAYLIST"));
    assert!(titlebar_source.contains("crate::symbols::SETTINGS"));
    assert!(titlebar_source.contains("fn titlebar_icon_button"));
    assert!(!titlebar_source.contains("egui::Button::new"));
    assert!(source.contains("actions.toggle_settings"));
    assert!(source.contains("actions.toggle_playlist"));
}

#[test]
fn app_handles_escape_playlist_enter_and_extra_single_key_shortcuts() {
    let source = app_source();

    assert!(source.contains("key_pressed(egui::Key::Escape)"));
    assert!(source.contains("key_pressed(egui::Key::P)"));
    assert!(source.contains("key_down(egui::Key::P)"));
    assert!(source.contains("playlist_shortcut_chord_started"));
    assert!(source.contains("key_pressed(egui::Key::F)"));
    assert!(source.contains("key_pressed(egui::Key::M)"));
    assert!(source.contains("player_core::Command::ToggleMute"));
    assert!(source.contains("player_core::Command::PlayIndex(candidate)"));
    assert!(source.contains("self.show_playlist = false"));
}

#[test]
fn app_handles_delete_and_backspace_for_playlist_and_history_candidates() {
    let source = app_source();

    assert!(source.contains("key_pressed(egui::Key::Delete)"));
    assert!(source.contains("key_pressed(egui::Key::Backspace)"));
    assert!(source.contains("player_core::Command::RemovePlaylistIndex(candidate)"));
    assert!(source.contains("player_core::Command::RemoveHistoryIndex(candidate)"));
    assert!(source.contains("self.history_candidate"));
    assert!(source.contains("candidate_after_remove"));
}

#[test]
fn s_key_requests_screenshot_from_keyboard_shortcut_path() {
    let source = app_source();

    assert!(source.contains("key_pressed(egui::Key::S)"));
    assert!(source.contains("outcome.screenshot_requested = true"));
    assert!(source.contains("shortcut_outcome.screenshot_requested"));
    assert!(source.contains("state.screenshot_requested"));
}

#[test]
fn app_tooltips_include_shortcut_descriptions_for_panel_actions() {
    let source = format!("{}\n{}", app_source(), include_str!("titlebar.rs"));

    for expected in [
        "shortcut_tooltip",
        "t!(\"settings\")",
        "settings_shortcut_label",
        "t!(\"playlist\")",
        "playlist_shortcut_label",
    ] {
        assert!(source.contains(expected));
    }
}

#[test]
fn enter_key_toggles_fullscreen() {
    let source = app_source();

    assert!(source.contains("key_pressed(egui::Key::Enter)"));
    assert!(source.contains("controls::toggle_fullscreen(ctx)"));
}

#[test]
fn app_keeps_update_flow_inside_settings() {
    let source = app_source();

    assert!(source.contains("update_check"));
    assert!(source.contains("check_updates_on_startup"));
    assert!(source.contains("check_beta_updates"));
    assert!(source.contains("settings_window"));
    assert!(source.contains("&mut self.update_check"));
    assert!(source.contains("self.update_check.poll()"));
    assert!(!source.contains("show_update_window"));
    assert!(!source.contains("take_check_update_request"));
    assert!(!source.contains("update_window"));
    assert!(!source.contains("install_check_update_menu_item"));
    assert!(!source.contains("show_update_result"));
}
