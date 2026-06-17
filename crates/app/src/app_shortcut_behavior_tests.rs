//! Keyboard shortcut behavior tests.
//!
//! These assertions cover the pure shortcut rules shared by the runtime dispatcher:
//! platform-specific modifier chords, playlist candidate movement, escape priority,
//! rate stepping, and media seek/volume commands.  Keeping these cases separate
//! from source-string tests makes shortcut refactors safer without constructing a
//! full egui frame.

use super::app_source;
use crate::app;
use eframe::egui;

fn command_modifiers() -> egui::Modifiers {
    egui::Modifiers {
        command: true,
        ..Default::default()
    }
}

struct PlaylistToggleHarness {
    show_playlist: bool,
    playlist_auto_hidden: bool,
    candidate: Option<usize>,
}

impl PlaylistToggleHarness {
    fn closed() -> Self {
        Self {
            show_playlist: false,
            playlist_auto_hidden: false,
            candidate: None,
        }
    }

    fn auto_hidden() -> Self {
        Self {
            show_playlist: true,
            playlist_auto_hidden: true,
            candidate: None,
        }
    }

    fn toggle(&mut self, current_index: Option<usize>, playlist_len: usize) -> bool {
        app::toggle_playlist_with_shortcut(app::PlaylistShortcutToggleInput {
            show_playlist: &mut self.show_playlist,
            playlist_auto_hidden: &mut self.playlist_auto_hidden,
            playlist_candidate: &mut self.candidate,
            modifiers: command_modifiers(),
            p_pressed: true,
            current_index,
            playlist_len,
        })
    }
}

#[test]
fn command_or_ctrl_arrows_navigate_playlist() {
    let command = command_modifiers();
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    assert_eq!(
        app::navigation_shortcut_command(
            crate::shortcuts::ShortcutPlatform::Macos,
            command,
            true,
            false
        ),
        Some(player_core::Command::Prev)
    );
    assert_eq!(
        app::navigation_shortcut_command(
            crate::shortcuts::ShortcutPlatform::Macos,
            command,
            false,
            true
        ),
        Some(player_core::Command::Next)
    );
    assert_eq!(
        app::navigation_shortcut_command(
            crate::shortcuts::ShortcutPlatform::Windows,
            alt,
            false,
            true
        ),
        Some(player_core::Command::Next)
    );
    assert_eq!(
        app::navigation_shortcut_command(
            crate::shortcuts::ShortcutPlatform::Windows,
            command,
            false,
            true
        ),
        None
    );
}

#[test]
fn command_or_ctrl_comma_toggles_settings() {
    let command = command_modifiers();

    assert!(app::settings_shortcut_pressed(command, true));
    assert!(!app::settings_shortcut_pressed(command, false));
    assert!(!app::settings_shortcut_pressed(Default::default(), true));

    let mut show_settings = false;
    assert!(app::toggle_settings_with_shortcut(
        &mut show_settings,
        command,
        true
    ));
    assert!(show_settings);

    assert!(app::toggle_settings_with_shortcut(
        &mut show_settings,
        command,
        true
    ));
    assert!(!show_settings);
}

#[test]
fn command_or_ctrl_p_toggles_playlist_and_initializes_candidate() {
    let mut toggle = PlaylistToggleHarness::closed();

    assert!(toggle.toggle(Some(2), 4));
    assert!(toggle.show_playlist);
    assert!(!toggle.playlist_auto_hidden);
    assert_eq!(toggle.candidate, Some(2));

    assert!(toggle.toggle(Some(2), 4));
    assert!(!toggle.show_playlist);
    assert!(!toggle.playlist_auto_hidden);
    assert_eq!(toggle.candidate, None);
}

#[test]
fn command_or_ctrl_p_restores_auto_hidden_playlist_instead_of_closing_it() {
    let mut toggle = PlaylistToggleHarness::auto_hidden();

    assert!(toggle.toggle(Some(1), 3));

    assert!(toggle.show_playlist);
    assert!(!toggle.playlist_auto_hidden);
    assert_eq!(toggle.candidate, Some(1));
}

#[test]
fn command_or_ctrl_p_can_start_from_chord_down_edge() {
    assert!(app::command_key_chord_started(true, false));
    assert!(!app::command_key_chord_started(true, true));
    assert!(!app::command_key_chord_started(false, false));
}

#[test]
fn playlist_candidate_moves_with_bounds() {
    assert_eq!(app::playlist_candidate_for_open(Some(2), 4), Some(2));
    assert_eq!(app::playlist_candidate_for_open(None, 4), Some(0));
    assert_eq!(app::playlist_candidate_for_open(Some(9), 4), Some(3));
    assert_eq!(app::playlist_candidate_for_open(None, 0), None);

    assert_eq!(app::move_playlist_candidate(Some(1), 4, -1), Some(0));
    assert_eq!(app::move_playlist_candidate(Some(1), 4, 1), Some(2));
    assert_eq!(app::move_playlist_candidate(Some(0), 4, -1), Some(0));
    assert_eq!(app::move_playlist_candidate(Some(3), 4, 1), Some(3));
    assert_eq!(app::move_playlist_candidate(None, 4, 1), Some(0));
    assert_eq!(app::move_playlist_candidate(None, 0, 1), None);
}

#[test]
fn deleting_candidate_clamps_to_remaining_items() {
    assert_eq!(app::candidate_after_remove(Some(1), 4), Some(1));
    assert_eq!(app::candidate_after_remove(Some(3), 4), Some(2));
    assert_eq!(app::candidate_after_remove(Some(0), 1), None);
    assert_eq!(app::candidate_after_remove(None, 4), Some(0));
    assert_eq!(app::candidate_after_remove(None, 0), None);
}

#[test]
fn escape_priority_closes_panels_before_exiting_fullscreen() {
    assert_eq!(
        app::escape_shortcut_action(true, true, true),
        app::EscapeShortcutAction::CloseSettings
    );
    assert_eq!(
        app::escape_shortcut_action(false, true, true),
        app::EscapeShortcutAction::ClosePlaylist
    );
    assert_eq!(
        app::escape_shortcut_action(false, false, true),
        app::EscapeShortcutAction::ExitFullscreen
    );
    assert_eq!(
        app::escape_shortcut_action(false, false, false),
        app::EscapeShortcutAction::None
    );
}

#[test]
fn modified_up_down_adjust_playback_rate() {
    let command = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };

    assert_eq!(
        app::rate_shortcut_command(
            crate::shortcuts::ShortcutPlatform::Macos,
            command,
            true,
            false,
            100
        ),
        Some(player_core::Command::SetRate(125))
    );
    assert_eq!(
        app::rate_shortcut_command(
            crate::shortcuts::ShortcutPlatform::Linux,
            alt,
            false,
            true,
            100
        ),
        Some(player_core::Command::SetRate(75))
    );
    assert_eq!(
        app::rate_shortcut_command(
            crate::shortcuts::ShortcutPlatform::Linux,
            command,
            true,
            false,
            100
        ),
        None
    );
}

#[test]
fn command_or_ctrl_o_opens_multi_file_picker() {
    let command = egui::Modifiers {
        command: true,
        ..Default::default()
    };

    assert!(app::open_shortcut_pressed(command, true));
    assert!(!app::open_shortcut_pressed(command, false));
    assert!(!app::open_shortcut_pressed(Default::default(), true));

    let source = app_source();
    assert!(source.contains("key_pressed(egui::Key::O)"));
    assert!(source.contains("open_shortcut_pressed(modifiers, o_key)"));
    assert!(source.contains(".pick_files()"));
    assert!(source.contains("player_core::Command::OpenFiles(paths)"));
    assert!(!source.contains(".pick_file()"));
    assert!(source.contains("let opened = self.handle_command(player_core::Command::OpenDialog)"));
    assert!(source.contains("opened_playlist_name_after_shortcut"));
}

#[test]
fn open_shortcut_notice_requires_dialog_selection() {
    assert_eq!(
        app::opened_playlist_name_after_shortcut(
            true,
            Some(std::path::PathBuf::from("/tmp/a.mp4"))
        ),
        Some("a.mp4".to_string())
    );
    assert_eq!(
        app::opened_playlist_name_after_shortcut(
            false,
            Some(std::path::PathBuf::from("/tmp/a.mp4"))
        ),
        None
    );
    assert_eq!(app::opened_playlist_name_after_shortcut(true, None), None);
}

#[test]
fn playlist_navigation_availability_tracks_current_index() {
    assert!(!app::playlist_has_prev(0, None));
    assert!(!app::playlist_has_next(0, None));
    assert!(!app::playlist_has_prev(3, Some(0)));
    assert!(app::playlist_has_next(3, Some(0)));
    assert!(app::playlist_has_prev(3, Some(1)));
    assert!(app::playlist_has_next(3, Some(1)));
    assert!(app::playlist_has_prev(3, Some(2)));
    assert!(!app::playlist_has_next(3, Some(2)));
}
