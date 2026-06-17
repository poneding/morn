//! Pure app behavior helpers.
//!
//! This module contains geometry, visibility, shortcut, and candidate-selection
//! decisions that are easier to test without constructing egui windows or a real
//! player.  The app shell passes frame snapshots in, and these functions return
//! deterministic answers or mutate only the small state handles supplied to them.
//!
//! Keeping these rules out of `app_ui` protects the frame scheduler from growing
//! policy branches, and it gives regression tests a stable target for edge cases
//! like pointer idle expiry, playlist auto-hide arming, and configured seek steps.

use super::*;

const CONTROL_BAR_SLIDER_MIN: f32 = 160.0;
const CONTROL_BAR_SLIDER_MAX: f32 = 460.0;
const CONTROL_BAR_SLIDER_FRACTION: f32 = 0.34;

/// 底部控制栏的最大可用宽度（窗口宽减去两侧浮层留白）。控制栏自适应内容
/// 宽度并居中, 不再横跨整个窗口底部; 这个值只作为「再宽也不能超过窗口」的上限。
pub(super) fn control_bar_max_width(available_width: f32) -> f32 {
    (available_width - crate::visuals::FLOATING_PANEL_MARGIN * 2.0).max(0.0)
}

/// 进度条(时间轴)宽度: 随窗口宽度成比例放大, 让它成为控制栏里最显眼的元素,
/// 同时夹在 \[最小, 上限\] 之间, 避免窄窗溢出或宽窗里长到失控。
pub(super) fn control_bar_slider_width(available_width: f32) -> f32 {
    // 窄窗下上限收紧到「最大可用宽度的一半」, 给两侧按钮留地方, 不至于撑破。
    let ceiling = CONTROL_BAR_SLIDER_MAX
        .min((control_bar_max_width(available_width) * 0.5).max(CONTROL_BAR_SLIDER_MIN));
    (available_width * CONTROL_BAR_SLIDER_FRACTION).clamp(CONTROL_BAR_SLIDER_MIN, ceiling)
}

pub(super) fn playlist_sheet_width(available_width: f32) -> f32 {
    (available_width * 0.42)
        .clamp(crate::playlist_panel::PLAYLIST_MIN_WIDTH, 300.0)
        .min(available_width)
}

fn floating_controls_outer_height(style: &egui::Style) -> f32 {
    let frame = crate::visuals::panel_frame_for_style(
        style,
        egui::Margin::symmetric(
            CONTROL_BAR_INNER_PADDING_X,
            crate::visuals::FLOATING_CONTROL_BAR_INNER_MARGIN_Y,
        ),
    );
    let content_height = style
        .spacing
        .interact_size
        .y
        .max(crate::controls::CONTROL_BUTTON_SIZE);
    content_height + frame.total_margin().sum().y
}

fn playlist_sheet_gap() -> f32 {
    crate::visuals::FLOATING_PANEL_MARGIN * 2.0
}

pub(super) fn playlist_sheet_top_offset() -> f32 {
    crate::titlebar::TITLEBAR_BOTTOM_OFFSET + playlist_sheet_gap()
}

pub(super) fn floating_controls_top_offset(screen_rect: egui::Rect, style: &egui::Style) -> f32 {
    screen_rect.height()
        - crate::visuals::FLOATING_PANEL_MARGIN
        - floating_controls_outer_height(style)
}

pub(super) fn playlist_sheet_height(screen_rect: egui::Rect, style: &egui::Style) -> f32 {
    // Use the same explicit gap above and below the playlist sheet:
    // top titlebar -> sheet top == sheet bottom -> bottom playback controls.
    (floating_controls_top_offset(screen_rect, style)
        - playlist_sheet_gap()
        - playlist_sheet_top_offset())
    .max(0.0)
}

#[derive(Clone, Copy)]
pub(super) struct BottomControlsVisibilityInput {
    pub(super) has_media: bool,
    pub(super) pointer_pos: Option<egui::Pos2>,
    pub(super) screen_rect: egui::Rect,
    pub(super) screenshot_notice_visible: bool,
    pub(super) active_overlay_hovered: bool,
    pub(super) last_pointer_move: std::time::Instant,
    pub(super) now: std::time::Instant,
}

pub(super) fn bottom_controls_visible(input: BottomControlsVisibilityInput) -> bool {
    !input.has_media
        || input.screenshot_notice_visible
        || input.active_overlay_hovered
        || pointer_visible_for_overlay_recheck(
            input.pointer_pos,
            input.screen_rect,
            input.last_pointer_move,
            input.now,
        )
}

#[derive(Clone, Copy)]
pub(super) struct PlaylistAutoHideInput {
    pub(super) playlist_visible: bool,
    pub(super) auto_hide_armed: bool,
    pub(super) pointer_pos: Option<egui::Pos2>,
    pub(super) screen_rect: egui::Rect,
    pub(super) playlist_hovered: bool,
    pub(super) bottom_controls_hovered: bool,
    pub(super) last_pointer_move: std::time::Instant,
    pub(super) now: std::time::Instant,
    pub(super) opened_this_frame: bool,
}

pub(super) fn playlist_should_auto_hide(input: PlaylistAutoHideInput) -> bool {
    if !input.playlist_visible || !input.auto_hide_armed || input.opened_this_frame {
        return false;
    }
    if !pointer_inside_window(input.pointer_pos, input.screen_rect) {
        return true;
    }
    !input.playlist_hovered
        && !input.bottom_controls_hovered
        && input.now.duration_since(input.last_pointer_move) > CONTROLS_IDLE_HIDE_AFTER
}

pub(super) fn update_playlist_auto_hide_armed(
    auto_hide_armed: &mut bool,
    show_playlist: bool,
    opened_this_frame: bool,
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
) {
    if !show_playlist {
        *auto_hide_armed = true;
        return;
    }
    if opened_this_frame {
        *auto_hide_armed = pointer_inside_window(pointer_pos, screen_rect);
        return;
    }
    if pointer_inside_window(pointer_pos, screen_rect) {
        *auto_hide_armed = true;
    }
}

pub(super) fn playlist_should_restore_from_auto_hide(
    show_playlist: bool,
    playlist_auto_hidden: bool,
    pointer_moved: bool,
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
) -> bool {
    show_playlist
        && playlist_auto_hidden
        && pointer_moved
        && pointer_inside_window(pointer_pos, screen_rect)
}

#[allow(dead_code)]
pub(super) fn pointer_active_inside_window(
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
    last_pointer_move: std::time::Instant,
    now: std::time::Instant,
) -> bool {
    pointer_inside_window(pointer_pos, screen_rect)
        && now.duration_since(last_pointer_move) <= CONTROLS_IDLE_HIDE_AFTER
}

pub(super) fn pointer_visible_for_overlay_recheck(
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
    last_pointer_move: std::time::Instant,
    now: std::time::Instant,
) -> bool {
    pointer_inside_window(pointer_pos, screen_rect)
        && now.duration_since(last_pointer_move)
            <= CONTROLS_IDLE_HIDE_AFTER + OVERLAY_HOVER_RECHECK_GRACE
}

pub(super) fn refresh_pointer_activity_for_current_overlay_hover(
    overlay_hovered: bool,
    last_pointer_move: &mut std::time::Instant,
    now: std::time::Instant,
) -> bool {
    if overlay_hovered {
        *last_pointer_move = now;
        true
    } else {
        false
    }
}

pub(super) fn pointer_activity_repaint_delay(
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
    last_pointer_move: std::time::Instant,
    now: std::time::Instant,
) -> Option<std::time::Duration> {
    if !pointer_inside_window(pointer_pos, screen_rect) {
        return None;
    }
    let idle_for = now.duration_since(last_pointer_move);
    let hide_after = CONTROLS_IDLE_HIDE_AFTER;
    let recheck_until = CONTROLS_IDLE_HIDE_AFTER + OVERLAY_HOVER_RECHECK_GRACE;
    if idle_for < hide_after {
        Some(hide_after - idle_for)
    } else if idle_for < recheck_until {
        Some(recheck_until - idle_for)
    } else {
        None
    }
}

fn pointer_inside_window(pointer_pos: Option<egui::Pos2>, screen_rect: egui::Rect) -> bool {
    pointer_pos.is_some_and(|pos| screen_rect.contains(pos))
}

pub(super) fn pointer_moved_since_last_frame(
    last_pointer_pos: Option<egui::Pos2>,
    pointer_pos: Option<egui::Pos2>,
) -> bool {
    match (last_pointer_pos, pointer_pos) {
        (Some(last), Some(current)) => last.distance_sq(current) > 0.25,
        (None, Some(_)) => true,
        _ => false,
    }
}

pub(super) fn window_content_rect_changed(
    last_screen_rect: Option<egui::Rect>,
    screen_rect: egui::Rect,
) -> bool {
    let Some(last_screen_rect) = last_screen_rect else {
        return false;
    };
    let delta = last_screen_rect.size() - screen_rect.size();
    delta.length_sq() > 0.25
}

pub(super) fn should_request_window_resize_repaint(
    last_window_resize: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    last_window_resize
        .is_some_and(|last_resize| now.duration_since(last_resize) <= WINDOW_RESIZE_REPAINT_GRACE)
}

pub(super) fn should_request_continuous_repaint(
    screenshot_notice_visible: bool,
    interacting: bool,
) -> bool {
    !interacting && screenshot_notice_visible
}

pub(super) fn navigation_shortcut_command(
    platform: crate::shortcuts::ShortcutPlatform,
    modifiers: egui::Modifiers,
    left_pressed: bool,
    right_pressed: bool,
) -> Option<player_core::Command> {
    if !crate::shortcuts::navigation_modifier_pressed(platform, modifiers) {
        return None;
    }
    if left_pressed {
        Some(player_core::Command::Prev)
    } else if right_pressed {
        Some(player_core::Command::Next)
    } else {
        None
    }
}

pub(super) fn plain_arrow_shortcut_modifiers(modifiers: egui::Modifiers) -> bool {
    !modifiers.alt
        && !modifiers.ctrl
        && !modifiers.shift
        && !modifiers.mac_cmd
        && !modifiers.command
}

pub(super) fn rate_shortcut_command(
    platform: crate::shortcuts::ShortcutPlatform,
    modifiers: egui::Modifiers,
    up_pressed: bool,
    down_pressed: bool,
    rate_pct: u16,
) -> Option<player_core::Command> {
    if !crate::shortcuts::navigation_modifier_pressed(platform, modifiers) {
        return None;
    }
    if up_pressed {
        Some(player_core::Command::SetRate(
            crate::shortcuts::snap_rate_up(rate_pct),
        ))
    } else if down_pressed {
        Some(player_core::Command::SetRate(
            crate::shortcuts::snap_rate_down(rate_pct),
        ))
    } else {
        None
    }
}

pub(super) fn settings_shortcut_pressed(modifiers: egui::Modifiers, comma_pressed: bool) -> bool {
    modifiers.command && comma_pressed
}

pub(super) fn open_shortcut_pressed(modifiers: egui::Modifiers, o_pressed: bool) -> bool {
    modifiers.command && o_pressed
}

pub(super) fn command_key_chord_started(chord_down: bool, was_down: bool) -> bool {
    chord_down && !was_down
}

pub(super) fn opened_playlist_name_after_shortcut(
    opened: bool,
    after: Option<std::path::PathBuf>,
) -> Option<String> {
    opened.then_some(after).flatten().map(path_file_name)
}

pub(super) fn toggle_settings_with_shortcut(
    show_settings: &mut bool,
    modifiers: egui::Modifiers,
    comma_pressed: bool,
) -> bool {
    if settings_shortcut_pressed(modifiers, comma_pressed) {
        *show_settings = !*show_settings;
        true
    } else {
        false
    }
}

fn playlist_shortcut_pressed(modifiers: egui::Modifiers, p_pressed: bool) -> bool {
    modifiers.command && p_pressed
}

pub(super) fn playlist_candidate_for_open(
    current_index: Option<usize>,
    playlist_len: usize,
) -> Option<usize> {
    // Opening the sidebar should start on the current item when possible, then
    // clamp if the playlist changed since the last frame.
    if playlist_len == 0 {
        None
    } else {
        Some(current_index.unwrap_or(0).min(playlist_len - 1))
    }
}

pub(super) fn move_playlist_candidate(
    current_candidate: Option<usize>,
    playlist_len: usize,
    delta: isize,
) -> Option<usize> {
    if current_candidate.is_none() {
        return playlist_candidate_for_open(None, playlist_len);
    }
    let candidate = playlist_candidate_for_open(current_candidate, playlist_len)?;
    let max = playlist_len.saturating_sub(1) as isize;
    Some((candidate as isize + delta).clamp(0, max) as usize)
}

pub(super) fn candidate_after_remove(
    current_candidate: Option<usize>,
    previous_len: usize,
) -> Option<usize> {
    if previous_len <= 1 {
        None
    } else {
        Some(current_candidate.unwrap_or(0).min(previous_len - 2))
    }
}

pub(super) fn candidate_after_index_remove(
    current_candidate: Option<usize>,
    removed_index: usize,
    previous_len: usize,
) -> Option<usize> {
    // Keep keyboard focus on the same logical row after deletion: shift left when
    // an earlier row is removed, otherwise clamp at the end.
    if previous_len <= 1 {
        return None;
    }
    let candidate = current_candidate.unwrap_or(0).min(previous_len - 1);
    if removed_index < candidate {
        Some(candidate - 1)
    } else if removed_index == candidate {
        candidate_after_remove(Some(candidate), previous_len)
    } else {
        Some(candidate)
    }
}

pub(super) fn path_file_name(path: impl AsRef<std::path::Path>) -> String {
    path.as_ref()
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.as_ref().to_string_lossy().to_string())
}

pub(super) struct PlaylistShortcutToggleInput<'a> {
    pub(super) show_playlist: &'a mut bool,
    pub(super) playlist_auto_hidden: &'a mut bool,
    pub(super) playlist_candidate: &'a mut Option<usize>,
    pub(super) modifiers: egui::Modifiers,
    pub(super) p_pressed: bool,
    pub(super) current_index: Option<usize>,
    pub(super) playlist_len: usize,
}

pub(super) fn toggle_playlist_with_shortcut(input: PlaylistShortcutToggleInput<'_>) -> bool {
    if !playlist_shortcut_pressed(input.modifiers, input.p_pressed) {
        return false;
    }
    toggle_playlist_visibility(
        input.show_playlist,
        input.playlist_auto_hidden,
        input.playlist_candidate,
        input.current_index,
        input.playlist_len,
    );
    true
}

pub(super) fn toggle_playlist_visibility(
    show_playlist: &mut bool,
    playlist_auto_hidden: &mut bool,
    playlist_candidate: &mut Option<usize>,
    current_index: Option<usize>,
    playlist_len: usize,
) {
    // A keyboard or button toggle treats an auto-hidden playlist as closed from the
    // user's perspective, so toggling it opens the sheet visibly again.
    let playlist_visible = *show_playlist && !*playlist_auto_hidden;
    *show_playlist = !playlist_visible;
    *playlist_auto_hidden = false;
    *playlist_candidate = if *show_playlist {
        playlist_candidate_for_open(current_index, playlist_len)
    } else {
        None
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EscapeShortcutAction {
    CloseSettings,
    ClosePlaylist,
    ExitFullscreen,
    None,
}

pub(super) fn escape_shortcut_action(
    show_settings: bool,
    show_playlist: bool,
    is_fullscreen: bool,
) -> EscapeShortcutAction {
    // Escape unwinds transient UI before leaving fullscreen, matching desktop
    // media-player expectations.
    if show_settings {
        EscapeShortcutAction::CloseSettings
    } else if show_playlist {
        EscapeShortcutAction::ClosePlaylist
    } else if is_fullscreen {
        EscapeShortcutAction::ExitFullscreen
    } else {
        EscapeShortcutAction::None
    }
}

pub(super) fn seek_shortcut_target(
    pos_ms: u64,
    duration_ms: u64,
    step_secs: u64,
    forward: bool,
) -> u64 {
    // Shortcut seeks use the current visible timeline position plus the configured
    // step, then clamp only when media duration is known.
    let step_ms = step_secs.saturating_mul(1000);
    let target = if forward {
        pos_ms.saturating_add(step_ms)
    } else {
        pos_ms.saturating_sub(step_ms)
    };

    if duration_ms > 0 {
        target.min(duration_ms)
    } else {
        target
    }
}

pub(super) fn format_ms_label(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

pub(super) fn playlist_has_prev(_playlist_len: usize, current_index: Option<usize>) -> bool {
    current_index.is_some_and(|index| index > 0)
}

pub(super) fn playlist_has_next(playlist_len: usize, current_index: Option<usize>) -> bool {
    current_index.is_some_and(|index| index + 1 < playlist_len)
}

pub(super) fn video_window_resize_size(
    last_resized_video_path: &mut Option<std::path::PathBuf>,
    current_path: Option<std::path::PathBuf>,
    dimensions: Option<(u32, u32)>,
    fullscreen: bool,
) -> Option<egui::Vec2> {
    // Automatic resize is one-shot per selected video and disabled in fullscreen
    // so it never fights user-driven window state.
    if fullscreen {
        return None;
    }
    let current_path = current_path?;
    if last_resized_video_path.as_ref() == Some(&current_path) {
        return None;
    }
    let (width, height) = dimensions?;
    let size = window_size_for_video_dimensions(width, height)?;
    *last_resized_video_path = Some(current_path);
    Some(size)
}

pub(super) fn player_should_request_repaint(
    interacting: bool,
    state: player_core::PlaybackState,
    seek_pending: bool,
    video_loaded: bool,
    video_texture_visible: bool,
) -> bool {
    // Paused restored media needs repaint until the first texture appears; after
    // that, only playback and seek gates should drive continuous frames.
    !interacting
        && (state == player_core::PlaybackState::Playing
            || seek_pending
            || (video_loaded
                && state == player_core::PlaybackState::Paused
                && !video_texture_visible))
}
