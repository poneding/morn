//! App-level regression tests.
//!
//! These tests intentionally mix pure function assertions with source-structure
//! assertions.  The pure assertions cover deterministic rules such as pointer
//! activity, playlist candidate movement, seek target clamping, platform config
//! paths, and window sizing.  The source assertions protect UI architecture that is
//! hard to exercise in a headless unit test: floating overlays, bottom controls,
//! native titlebar usage, stable egui area IDs, and shortcut wiring.
//!
//! Source-string checks are kept narrow and literal on purpose.  They are not a
//! substitute for behavior tests, but they catch accidental regressions where a UI
//! control moves back into the wrong panel, a repaint condition starts depending on
//! frame availability again, or a keyboard shortcut path is silently removed during
//! refactoring.  When production code is split across modules, `app_source()`
//! aggregates those modules so the assertions keep tracking the behavior surface
//! instead of a single physical file.
//!
//! The tests also document platform expectations:
//! config files live under each OS's conventional app data directory, reveal-file
//! commands use the native file manager, and restored windows use the selected
//! video's aspect ratio without fighting fullscreen or resize interactions.
//!
//! Playlist and overlay tests focus on interaction invariants that users notice
//! immediately: the playlist opens as a floating sheet, does not resize the video,
//! auto-hides only after the pointer leaves active overlays, keeps keyboard
//! candidates bounded, and preserves the current/hover/candidate visual split.
//!
//! Shortcut tests mirror the dispatch order in `app_shortcuts.rs`: global command
//! chords run before egui text-focus checks, modified arrows control rate or item
//! navigation, visible sidebars consume list keys first, and unmodified media keys
//! handle fullscreen, play/pause, mute, screenshot, volume, and seek.
//!
//! Screenshot and startup restore tests exist because those paths cross multiple
//! subsystems.  The app must use the configured screenshot directory, surface
//! save failures to the user, and repaint a restored paused video until the first
//! decoded frame is uploaded instead of leaving a blank startup surface.
//!
//! Keeping these expectations close to the app module makes UI refactors cheaper:
//! when a split is behavior-preserving, the update is usually limited to
//! `app_source()` or the smallest literal that names the new helper.

fn app_source() -> String {
    [
        include_str!("app.rs"),
        include_str!("app_shortcuts.rs"),
        include_str!("app_sidebar_shortcuts.rs"),
        include_str!("app_ui.rs"),
        include_str!("app_sidebar_ui.rs"),
        include_str!("app_platform.rs"),
        include_str!("app_behavior.rs"),
    ]
    .join("\n")
}

#[path = "app_platform_tests.rs"]
mod platform_tests;
#[path = "app_prefs_migration_tests.rs"]
mod prefs_migration_tests;
#[path = "app_shortcut_behavior_tests.rs"]
mod shortcut_behavior_tests;
#[path = "app_source_tests.rs"]
mod source_tests;

#[derive(Default)]
struct ResizeHarness {
    last: Option<std::path::PathBuf>,
}

impl ResizeHarness {
    fn request(
        &mut self,
        path: &str,
        dimensions: (u32, u32),
        fullscreen: bool,
    ) -> Option<egui::Vec2> {
        super::video_window_resize_size(
            &mut self.last,
            Some(std::path::PathBuf::from(path)),
            Some(dimensions),
            fullscreen,
        )
    }

    fn last_is(&self, path: &str) -> bool {
        self.last == Some(std::path::PathBuf::from(path))
    }
}

#[test]
fn app_width_budget_preserves_sidebar_and_video_minimums() {
    let app_min_width = std::hint::black_box(super::APP_MIN_WIDTH);
    let playlist_min_width = std::hint::black_box(crate::playlist_panel::PLAYLIST_MIN_WIDTH);
    let video_min_width = std::hint::black_box(super::VIDEO_MIN_WIDTH);
    assert!(
        app_min_width >= playlist_min_width + video_min_width,
        "app minimum width must preserve sidebar and video minimums"
    );
}

#[test]
fn video_window_resize_tracks_current_video_once_and_waits_out_fullscreen() {
    let mut resize = ResizeHarness::default();

    let first = resize.request("/wide.mp4", (1920, 1080), false).unwrap();
    // 16:9: inner_width = (600 - 34) * 16/9, inner_height = 600
    assert!((first.x - 1006.2222).abs() < 0.01);
    assert!((first.y - 600.0).abs() < 0.01);
    assert!(resize.last_is("/wide.mp4"));

    assert_eq!(resize.request("/wide.mp4", (1920, 1080), false), None);

    assert_eq!(resize.request("/squareish.mp4", (160, 120), true), None);
    assert!(!resize.last_is("/squareish.mp4"));

    let second = resize.request("/squareish.mp4", (160, 120), false).unwrap();
    // 4:3 clamped to APP_MIN_WIDTH: video_height = 920 * 3/4, inner_height = 690 + 34
    assert!((second.x - super::APP_MIN_WIDTH).abs() < 0.01);
    assert!((second.y - 724.0).abs() < 0.01);
}

#[test]
fn playlist_sheet_width_stays_inside_current_window() {
    assert_eq!(super::playlist_sheet_width(super::APP_MIN_WIDTH), 300.0);
    assert_eq!(super::playlist_sheet_width(160.0), 160.0);

    let source = app_source();
    assert!(source.contains("playlist_sheet_width"));
    assert!(!source.contains(".max_size(playlist_max_width"));
}

#[test]
fn playlist_sheet_keeps_equal_gaps_to_titlebar_and_bottom_playback_controls() {
    let style = egui::Style::default();
    let screen = test_screen_rect();
    let top = super::playlist_sheet_top_offset();
    let height = super::playlist_sheet_height(screen, &style);
    let bottom_controls_top = super::floating_controls_top_offset(screen, &style);

    let gap_from_titlebar = top - crate::titlebar::TITLEBAR_BOTTOM_OFFSET;
    let gap_to_controls = bottom_controls_top - (top + height);

    assert!(
        (gap_from_titlebar - gap_to_controls).abs() < 0.01,
        "playlist gaps should match: titlebar={gap_from_titlebar}, controls={gap_to_controls}"
    );
}

#[test]
fn control_bar_slider_width_grows_with_window_but_stays_bounded() {
    // 进度条随窗口加宽而变长(成比例), 是控制栏里最显眼的元素。
    let narrow = super::control_bar_slider_width(900.0);
    let wide = super::control_bar_slider_width(1600.0);
    assert!(wide > narrow, "宽窗的进度条应更长: {narrow} -> {wide}");

    // 下限不低于 160, 上限不超过 460(也不超过窗口)。
    assert!(super::control_bar_slider_width(400.0) >= 160.0);
    assert!(super::control_bar_slider_width(4000.0) <= 460.0);

    // 极窄窗: 进度条不得超过可用宽度。
    let tiny = 360.0;
    assert!(super::control_bar_slider_width(tiny) <= super::control_bar_max_width(tiny));
}

#[test]
fn control_bar_max_width_subtracts_both_side_margins() {
    let margin = crate::visuals::FLOATING_PANEL_MARGIN;
    assert_eq!(super::control_bar_max_width(1000.0), 1000.0 - margin * 2.0);
    assert_eq!(super::control_bar_max_width(0.0), 0.0);
}

fn bottom_visibility_input(
    screen_rect: egui::Rect,
    now: std::time::Instant,
    last_pointer_move: std::time::Instant,
) -> super::BottomControlsVisibilityInput {
    super::BottomControlsVisibilityInput {
        has_media: true,
        pointer_pos: Some(egui::pos2(640.0, 500.0)),
        screen_rect,
        screenshot_notice_visible: false,
        active_overlay_hovered: false,
        last_pointer_move,
        now,
    }
}

fn test_screen_rect() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0))
}

fn recent_pointer_move(now: std::time::Instant) -> std::time::Instant {
    now - std::time::Duration::from_secs(1)
}

fn after_idle_hide(now: std::time::Instant) -> std::time::Instant {
    now - super::CONTROLS_IDLE_HIDE_AFTER - std::time::Duration::from_millis(1)
}

fn after_overlay_recheck(now: std::time::Instant) -> std::time::Instant {
    now - super::CONTROLS_IDLE_HIDE_AFTER
        - super::OVERLAY_HOVER_RECHECK_GRACE
        - std::time::Duration::from_millis(1)
}

struct OverlayTiming {
    screen: egui::Rect,
    now: std::time::Instant,
    recent_move: std::time::Instant,
    idle_move: std::time::Instant,
    recheck_move: std::time::Instant,
    expired_move: std::time::Instant,
}

fn overlay_timing() -> OverlayTiming {
    let now = std::time::Instant::now();
    OverlayTiming {
        screen: test_screen_rect(),
        now,
        recent_move: recent_pointer_move(now),
        idle_move: after_idle_hide(now),
        recheck_move: after_idle_hide(now),
        expired_move: after_overlay_recheck(now),
    }
}

#[test]
fn bottom_controls_visibility_rule_uses_whole_window() {
    let timing = overlay_timing();

    assert!(super::bottom_controls_visible(
        super::BottomControlsVisibilityInput {
            has_media: false,
            pointer_pos: None,
            ..bottom_visibility_input(timing.screen, timing.now, timing.expired_move)
        }
    ));
    assert!(super::bottom_controls_visible(
        super::BottomControlsVisibilityInput {
            pointer_pos: None,
            screenshot_notice_visible: true,
            ..bottom_visibility_input(timing.screen, timing.now, timing.expired_move)
        }
    ));
    assert!(super::bottom_controls_visible(
        super::BottomControlsVisibilityInput {
            pointer_pos: Some(egui::pos2(640.0, 700.0)),
            ..bottom_visibility_input(timing.screen, timing.now, timing.recent_move)
        }
    ));
    assert!(super::bottom_controls_visible(bottom_visibility_input(
        timing.screen,
        timing.now,
        timing.recent_move
    )));
    assert!(super::bottom_controls_visible(bottom_visibility_input(
        timing.screen,
        timing.now,
        timing.recheck_move
    )));
    assert!(super::bottom_controls_visible(
        super::BottomControlsVisibilityInput {
            pointer_pos: Some(egui::pos2(1000.0, 240.0)),
            active_overlay_hovered: true,
            ..bottom_visibility_input(timing.screen, timing.now, timing.expired_move)
        }
    ));
    assert!(!super::bottom_controls_visible(bottom_visibility_input(
        timing.screen,
        timing.now,
        timing.expired_move
    )));
    assert!(!super::bottom_controls_visible(
        super::BottomControlsVisibilityInput {
            pointer_pos: Some(egui::pos2(640.0, 721.0)),
            ..bottom_visibility_input(timing.screen, timing.now, timing.recent_move)
        }
    ));
}

#[test]
fn pointer_active_inside_window_tracks_hover_and_idle_timeout() {
    let timing = overlay_timing();

    assert!(super::pointer_active_inside_window(
        Some(egui::pos2(640.0, 360.0)),
        timing.screen,
        timing.recent_move,
        timing.now
    ));
    assert!(!super::pointer_active_inside_window(
        Some(egui::pos2(640.0, 360.0)),
        timing.screen,
        timing.idle_move,
        timing.now
    ));
    assert!(!super::pointer_active_inside_window(
        Some(egui::pos2(640.0, 721.0)),
        timing.screen,
        timing.recent_move,
        timing.now
    ));
    assert!(!super::pointer_active_inside_window(
        None,
        timing.screen,
        timing.recent_move,
        timing.now
    ));
}

#[test]
fn current_overlay_hover_refreshes_pointer_activity_after_idle_timeout() {
    let screen = test_screen_rect();
    let now = std::time::Instant::now();
    let mut last_pointer_move = after_idle_hide(now);

    assert!(super::pointer_visible_for_overlay_recheck(
        Some(egui::pos2(640.0, 680.0)),
        screen,
        last_pointer_move,
        now
    ));
    assert!(super::refresh_pointer_activity_for_current_overlay_hover(
        true,
        &mut last_pointer_move,
        now
    ));
    assert_eq!(last_pointer_move, now);
    assert!(super::bottom_controls_visible(
        super::BottomControlsVisibilityInput {
            pointer_pos: Some(egui::pos2(640.0, 680.0)),
            ..bottom_visibility_input(screen, now, last_pointer_move)
        }
    ));
}

#[test]
fn current_overlay_non_hover_does_not_refresh_pointer_activity() {
    let now = std::time::Instant::now();
    let idle_move = after_idle_hide(now);
    let mut last_pointer_move = idle_move;

    assert!(!super::refresh_pointer_activity_for_current_overlay_hover(
        false,
        &mut last_pointer_move,
        now
    ));
    assert_eq!(last_pointer_move, idle_move);
}

#[test]
fn overlay_recheck_requires_pointer_inside_window_and_expires() {
    let timing = overlay_timing();

    assert!(super::pointer_visible_for_overlay_recheck(
        Some(egui::pos2(640.0, 680.0)),
        timing.screen,
        timing.recheck_move,
        timing.now
    ));
    assert!(!super::pointer_visible_for_overlay_recheck(
        None,
        timing.screen,
        timing.recheck_move,
        timing.now
    ));
    assert!(!super::pointer_visible_for_overlay_recheck(
        Some(egui::pos2(640.0, 721.0)),
        timing.screen,
        timing.recheck_move,
        timing.now
    ));
    assert!(!super::pointer_visible_for_overlay_recheck(
        Some(egui::pos2(640.0, 680.0)),
        timing.screen,
        timing.expired_move,
        timing.now
    ));
}

#[test]
fn playlist_auto_hides_when_idle_outside_active_overlays() {
    let timing = overlay_timing();
    let input = |pointer_pos,
                 playlist_hovered,
                 bottom_controls_hovered,
                 last_pointer_move,
                 opened_this_frame| {
        super::PlaylistAutoHideInput {
            playlist_visible: true,
            auto_hide_armed: true,
            pointer_pos,
            screen_rect: timing.screen,
            playlist_hovered,
            bottom_controls_hovered,
            last_pointer_move,
            now: timing.now,
            opened_this_frame,
        }
    };

    assert!(super::playlist_should_auto_hide(input(
        Some(egui::pos2(640.0, 360.0)),
        false,
        false,
        timing.idle_move,
        false
    )));
    assert!(super::playlist_should_auto_hide(input(
        None,
        false,
        false,
        timing.recent_move,
        false
    )));
    assert!(!super::playlist_should_auto_hide(input(
        Some(egui::pos2(640.0, 360.0)),
        true,
        false,
        timing.idle_move,
        false
    )));
    assert!(!super::playlist_should_auto_hide(input(
        Some(egui::pos2(640.0, 680.0)),
        false,
        true,
        timing.idle_move,
        false
    )));
    assert!(!super::playlist_should_auto_hide(input(
        Some(egui::pos2(640.0, 360.0)),
        false,
        false,
        timing.idle_move,
        true
    )));
    assert!(!super::playlist_should_auto_hide(
        super::PlaylistAutoHideInput {
            playlist_visible: false,
            auto_hide_armed: true,
            pointer_pos: Some(egui::pos2(640.0, 360.0)),
            screen_rect: timing.screen,
            playlist_hovered: false,
            bottom_controls_hovered: false,
            last_pointer_move: timing.idle_move,
            now: timing.now,
            opened_this_frame: false,
        }
    ));
}

#[test]
fn playlist_opened_while_pointer_outside_waits_for_pointer_entry_before_auto_hide() {
    let screen = test_screen_rect();
    let mut armed = true;

    super::update_playlist_auto_hide_armed(&mut armed, true, true, None, screen);
    assert!(!armed);

    super::update_playlist_auto_hide_armed(&mut armed, true, false, None, screen);
    assert!(!armed);
    assert!(!super::playlist_should_auto_hide(
        super::PlaylistAutoHideInput {
            playlist_visible: true,
            auto_hide_armed: armed,
            pointer_pos: None,
            screen_rect: screen,
            playlist_hovered: false,
            bottom_controls_hovered: false,
            last_pointer_move: std::time::Instant::now(),
            now: std::time::Instant::now(),
            opened_this_frame: false,
        }
    ));

    super::update_playlist_auto_hide_armed(
        &mut armed,
        true,
        false,
        Some(egui::pos2(640.0, 360.0)),
        screen,
    );
    assert!(armed);
}

#[test]
fn playlist_auto_hide_restores_on_pointer_motion_inside_window() {
    let screen = test_screen_rect();

    assert!(super::playlist_should_restore_from_auto_hide(
        true,
        true,
        true,
        Some(egui::pos2(640.0, 360.0)),
        screen
    ));
    assert!(super::playlist_should_restore_from_auto_hide(
        true,
        true,
        true,
        Some(egui::pos2(4.0, 4.0)),
        screen
    ));
    assert!(!super::playlist_should_restore_from_auto_hide(
        false,
        true,
        true,
        Some(egui::pos2(640.0, 360.0)),
        screen
    ));
    assert!(!super::playlist_should_restore_from_auto_hide(
        true,
        false,
        true,
        Some(egui::pos2(640.0, 360.0)),
        screen
    ));
    assert!(!super::playlist_should_restore_from_auto_hide(
        true,
        true,
        false,
        Some(egui::pos2(640.0, 360.0)),
        screen
    ));
    assert!(!super::playlist_should_restore_from_auto_hide(
        true, true, true, None, screen
    ));
    assert!(!super::playlist_should_restore_from_auto_hide(
        true,
        true,
        true,
        Some(egui::pos2(640.0, 721.0)),
        screen
    ));
}

#[test]
fn pointer_movement_tracks_real_motion() {
    assert!(super::pointer_moved_since_last_frame(
        None,
        Some(egui::pos2(1.0, 1.0))
    ));
    assert!(!super::pointer_moved_since_last_frame(
        Some(egui::pos2(1.0, 1.0)),
        Some(egui::pos2(1.1, 1.1))
    ));
    assert!(super::pointer_moved_since_last_frame(
        Some(egui::pos2(1.0, 1.0)),
        Some(egui::pos2(2.0, 1.0))
    ));
    assert!(!super::pointer_moved_since_last_frame(
        Some(egui::pos2(1.0, 1.0)),
        None
    ));
}

#[test]
fn continuous_repaint_is_only_requested_when_needed() {
    assert!(!super::should_request_continuous_repaint(false, false));
    assert!(super::should_request_continuous_repaint(true, false));
    assert!(!super::should_request_continuous_repaint(true, true));
    assert!(!super::should_request_continuous_repaint(false, true));

    let source = app_source();
    assert!(source.contains("if should_request_continuous_repaint"));
    assert!(source.contains("ctx.request_repaint_after"));
    assert!(source.contains("should_request_continuous_repaint"));
    assert!(source.contains("self.shortcut_notice.is_some()"));
    assert!(source.contains("i.pointer.any_down()"));
    // 播放态连续重绘(取代旧的 frame_pending 唤醒)。
    assert!(source.contains("ctx.request_repaint()"));
    assert!(!source.contains("take_frame_pending"));
}

#[test]
fn window_resize_repaint_only_tracks_real_content_rect_changes() {
    let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(960.0, 600.0));
    let resized = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1120.0, 720.0));

    assert!(!super::window_content_rect_changed(None, rect));
    assert!(!super::window_content_rect_changed(Some(rect), rect));
    assert!(super::window_content_rect_changed(Some(rect), resized));

    let now = std::time::Instant::now();
    assert!(super::should_request_window_resize_repaint(Some(now), now));
    assert!(super::should_request_window_resize_repaint(
        Some(now),
        now + super::WINDOW_RESIZE_REPAINT_GRACE - std::time::Duration::from_millis(1),
    ));
    assert!(!super::should_request_window_resize_repaint(
        Some(now),
        now + super::WINDOW_RESIZE_REPAINT_GRACE + std::time::Duration::from_millis(1),
    ));

    let source = app_source();
    assert!(source.contains("self.track_window_resize(state);"));
    assert!(source.contains("request_window_resize_repaint"));
    assert!(source.contains("window_content_rect_changed"));
}

#[test]
fn seek_shortcut_target_uses_configured_step_seconds() {
    assert_eq!(
        super::seek_shortcut_target(50_000, 120_000, 20, false),
        30_000
    );
    assert_eq!(super::seek_shortcut_target(5_000, 120_000, 20, false), 0);
    assert_eq!(
        super::seek_shortcut_target(50_000, 120_000, 20, true),
        70_000
    );
    assert_eq!(
        super::seek_shortcut_target(115_000, 120_000, 20, true),
        120_000
    );
    assert_eq!(super::seek_shortcut_target(50_000, 0, 30, true), 80_000);
}

#[test]
fn arrow_hold_playback_activates_after_threshold_and_restores_rate() {
    let now = std::time::Instant::now();
    let mut hold = super::ArrowHoldPlayback::default();
    let input = super::ArrowHoldInput {
        left_down: false,
        right_down: true,
        can_start: true,
    };

    assert_eq!(hold.update(input, now, 125), super::ArrowHoldAction::None);
    assert_eq!(
        hold.update(
            input,
            now + super::ARROW_LONG_PRESS_THRESHOLD - std::time::Duration::from_millis(1),
            125,
        ),
        super::ArrowHoldAction::None
    );
    assert_eq!(
        hold.update(input, now + super::ARROW_LONG_PRESS_THRESHOLD, 125),
        super::ArrowHoldAction::Activate {
            restore_rate_pct: 125
        }
    );
    assert_eq!(
        hold.update(input, now + super::ARROW_LONG_PRESS_THRESHOLD * 2, 200),
        super::ArrowHoldAction::None
    );
    assert_eq!(
        hold.update(
            super::ArrowHoldInput {
                right_down: false,
                ..input
            },
            now + super::ARROW_LONG_PRESS_THRESHOLD * 2,
            200,
        ),
        super::ArrowHoldAction::Restore { rate_pct: 125 }
    );
}

#[test]
fn arrow_hold_playback_requires_plain_single_arrow_key() {
    let now = std::time::Instant::now();
    let mut hold = super::ArrowHoldPlayback::default();

    assert_eq!(
        hold.update(
            super::ArrowHoldInput {
                left_down: true,
                right_down: true,
                can_start: true,
            },
            now,
            100,
        ),
        super::ArrowHoldAction::None
    );
    assert_eq!(
        hold.update(
            super::ArrowHoldInput {
                left_down: true,
                right_down: false,
                can_start: false,
            },
            now + super::ARROW_LONG_PRESS_THRESHOLD,
            100,
        ),
        super::ArrowHoldAction::None
    );

    let command = egui::Modifiers {
        command: true,
        ctrl: true,
        ..Default::default()
    };
    assert!(!super::plain_arrow_shortcut_modifiers(command));
    assert!(super::plain_arrow_shortcut_modifiers(Default::default()));
}

#[test]
fn restored_paused_media_repaints_until_first_frame_arrives() {
    let source = app_source();
    let repaint_call = source
        .split("if player_should_request_repaint(")
        .last()
        .unwrap()
        .split("ctx.request_repaint();")
        .next()
        .unwrap();

    assert!(repaint_call.contains("self.video_view.has_texture()"));
    assert!(!repaint_call.contains("self.player.current_frame_rgba().is_some()"));

    assert!(super::player_should_request_repaint(
        false,
        player_core::PlaybackState::Paused,
        false,
        true,
        false,
    ));
    assert!(!super::player_should_request_repaint(
        false,
        player_core::PlaybackState::Paused,
        false,
        true,
        true,
    ));
    assert!(!super::player_should_request_repaint(
        true,
        player_core::PlaybackState::Paused,
        false,
        true,
        false,
    ));
    assert!(super::player_should_request_repaint(
        false,
        player_core::PlaybackState::Playing,
        false,
        true,
        true,
    ));
    assert!(super::player_should_request_repaint(
        false,
        player_core::PlaybackState::Paused,
        true,
        true,
        true,
    ));
}
