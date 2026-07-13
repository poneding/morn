//! Keyboard shortcut dispatch for `PlayerApp`.
//!
//! Shortcut handling is split by priority rather than by key name.  Global chords
//! such as Escape, settings, open-file, and playlist toggle run even when egui has
//! text focus, matching normal desktop app behavior.  Playback/navigation keys run
//! only when egui does not want keyboard input, so combo boxes and text fields can
//! keep their native editing semantics.
//!
//! Inside the focused path the order is intentional:
//! modified up/down adjusts rate, modified left/right navigates playlist items,
//! visible sidebar keys edit the active list, and plain media keys handle
//! fullscreen, play/pause, mute, screenshot, volume, and seek.  Each helper returns
//! whether it consumed the frame, preserving the previous short-circuit behavior
//! without one very large nested function.
//!
//! The `ShortcutFrame` snapshot captures playlist lengths and the current index at
//! the start of the frame.  That mirrors the old in-place logic and avoids mixed
//! state if a shortcut opens files and another key is also pressed in the same
//! input frame.
//!
//! User-facing shortcut notices are emitted only after the command path mutates
//! state, so the text reflects the actual result instead of the requested key.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct ShortcutKeys {
    // Key state is captured once so every handler in this frame sees the same
    // pressed/released snapshot.
    comma: bool,
    o: bool,
    p: bool,
    p_down: bool,
    escape: bool,
    pub(super) enter: bool,
    space: bool,
    f: bool,
    m: bool,
    s: bool,
    pub(super) up: bool,
    pub(super) down: bool,
    left: bool,
    right: bool,
    left_down: bool,
    right_down: bool,
    pub(super) delete: bool,
    pub(super) backspace: bool,
}

impl ShortcutKeys {
    fn capture(ctx: &egui::Context) -> Self {
        ctx.input(|i| Self {
            comma: i.key_pressed(egui::Key::Comma),
            o: i.key_pressed(egui::Key::O),
            p: i.key_pressed(egui::Key::P),
            p_down: i.key_down(egui::Key::P),
            escape: i.key_pressed(egui::Key::Escape),
            enter: i.key_pressed(egui::Key::Enter),
            space: i.key_pressed(egui::Key::Space),
            f: i.key_pressed(egui::Key::F),
            m: i.key_pressed(egui::Key::M),
            s: i.key_pressed(egui::Key::S),
            up: i.key_pressed(egui::Key::ArrowUp),
            down: i.key_pressed(egui::Key::ArrowDown),
            left: i.key_pressed(egui::Key::ArrowLeft),
            right: i.key_pressed(egui::Key::ArrowRight),
            left_down: i.key_down(egui::Key::ArrowLeft),
            right_down: i.key_down(egui::Key::ArrowRight),
            delete: i.key_pressed(egui::Key::Delete),
            backspace: i.key_pressed(egui::Key::Backspace),
        })
    }
}

fn playlist_shortcut_chord_started(
    ctx: &egui::Context,
    modifiers: egui::Modifiers,
    p_down: bool,
) -> bool {
    let chord_down = modifiers.command && p_down;
    let id = egui::Id::new("playlist_shortcut_chord_down");
    ctx.data_mut(|data| {
        let was_down = data.get_temp::<bool>(id).unwrap_or(false);
        data.insert_temp(id, chord_down);
        command_key_chord_started(chord_down, was_down)
    })
}

#[derive(Clone, Copy)]
pub(super) struct ShortcutFrame {
    modifiers: egui::Modifiers,
    pub(super) keys: ShortcutKeys,
    current_index: Option<usize>,
    pub(super) playlist_len: usize,
    pub(super) history_len: usize,
    platform: crate::shortcuts::ShortcutPlatform,
    playlist_chord_started: bool,
}

impl ShortcutFrame {
    fn snapshot(ctx: &egui::Context, app: &PlayerApp) -> Self {
        // Playlist/history lengths are read before handling any shortcut because a
        // command may open or remove media later in the same dispatch.
        let modifiers = ctx.input(|i| i.modifiers);
        let keys = ShortcutKeys::capture(ctx);
        let playlist_chord_started = playlist_shortcut_chord_started(ctx, modifiers, keys.p_down);
        Self {
            modifiers,
            keys,
            current_index: app.player.current_index(),
            playlist_len: app.player.playlist_paths().len(),
            history_len: app.player.history().len(),
            platform: crate::shortcuts::ShortcutPlatform::current(),
            playlist_chord_started,
        }
    }

    pub(super) fn playlist_visible(self, app: &PlayerApp) -> bool {
        app.show_playlist && !app.playlist_auto_hidden
    }
}

impl PlayerApp {
    pub(super) fn handle_keyboard_shortcuts(
        &mut self,
        ctx: &egui::Context,
        t: engine::Timeline,
    ) -> KeyboardShortcutOutcome {
        let frame = ShortcutFrame::snapshot(ctx, self);
        let mut outcome = KeyboardShortcutOutcome::default();

        self.handle_global_shortcuts(ctx, frame, &mut outcome);
        if !ctx.egui_wants_keyboard_input() {
            self.handle_focused_shortcuts(ctx, t, frame, &mut outcome);
        } else {
            self.handle_arrow_hold_playback(ctx, t, frame, false);
        }

        outcome
    }

    fn handle_global_shortcuts(
        &mut self,
        ctx: &egui::Context,
        frame: ShortcutFrame,
        outcome: &mut KeyboardShortcutOutcome,
    ) {
        // Global shortcuts intentionally ignore text focus.  They control windows,
        // dialogs, and overlays rather than editing the focused widget.
        self.handle_escape_shortcut(ctx, frame.keys.escape);
        self.handle_settings_shortcut(frame.modifiers, frame.keys.comma);
        self.handle_open_shortcut(frame.modifiers, frame.keys.o);
        self.handle_playlist_toggle_shortcut(frame, outcome);
    }

    fn handle_escape_shortcut(&mut self, ctx: &egui::Context, escape: bool) {
        if !escape {
            return;
        }

        let fullscreen = viewport_is_fullscreen(ctx);
        match escape_shortcut_action(self.show_settings, self.show_playlist, fullscreen) {
            EscapeShortcutAction::CloseSettings => {
                self.show_settings = false;
                self.set_shortcut_notice(format!("{}：{}", t!("settings"), t!("closed")));
            }
            EscapeShortcutAction::ClosePlaylist => {
                self.close_playlist();
                self.set_shortcut_notice(format!("{}：{}", t!("playlist"), t!("closed")));
            }
            EscapeShortcutAction::ExitFullscreen => {
                crate::controls::set_fullscreen(ctx, false);
            }
            EscapeShortcutAction::None => {}
        }
    }

    fn handle_settings_shortcut(&mut self, modifiers: egui::Modifiers, comma: bool) {
        if !toggle_settings_with_shortcut(&mut self.show_settings, modifiers, comma) {
            return;
        }

        let status = if self.show_settings {
            t!("opened")
        } else {
            t!("closed")
        };
        self.set_shortcut_notice(format!("{}：{}", t!("settings"), status));
    }

    fn handle_open_shortcut(&mut self, modifiers: egui::Modifiers, o_key: bool) {
        if !open_shortcut_pressed(modifiers, o_key) {
            return;
        }

        let opened = self.handle_command(player_core::Command::OpenDialog);
        if let Some(name) =
            opened_playlist_name_after_shortcut(opened, self.current_playlist_path())
        {
            self.set_shortcut_notice(format!("{}：{}", t!("current_playing"), name));
        }
    }

    fn handle_playlist_toggle_shortcut(
        &mut self,
        frame: ShortcutFrame,
        outcome: &mut KeyboardShortcutOutcome,
    ) {
        let toggled = toggle_playlist_with_shortcut(PlaylistShortcutToggleInput {
            show_playlist: &mut self.show_playlist,
            playlist_auto_hidden: &mut self.playlist_auto_hidden,
            playlist_candidate: &mut self.playlist_candidate,
            modifiers: frame.modifiers,
            p_pressed: frame.keys.p || frame.playlist_chord_started,
            current_index: frame.current_index,
            playlist_len: frame.playlist_len,
        });
        if !toggled {
            return;
        }

        self.sidebar_tab = SidebarTab::Playlist;
        if self.show_playlist {
            self.playlist_auto_hidden = false;
            self.history_candidate = None;
            outcome.playlist_opened_this_frame = true;
        } else {
            self.close_playlist();
        }
        let status = if self.show_playlist {
            t!("opened")
        } else {
            t!("closed")
        };
        self.set_shortcut_notice(format!("{}：{}", t!("playlist"), status));
    }

    fn handle_focused_shortcuts(
        &mut self,
        ctx: &egui::Context,
        t: engine::Timeline,
        frame: ShortcutFrame,
        outcome: &mut KeyboardShortcutOutcome,
    ) {
        // Focused shortcuts are ordered from most specific modifier chords to plain
        // media keys so one key press maps to exactly one command.
        if self.arrow_hold_playback.active && self.handle_arrow_hold_playback(ctx, t, frame, true) {
            return;
        }
        if self.handle_rate_shortcut(t.rate_pct, frame) {
            return;
        }
        if self.handle_navigation_shortcut(frame) {
            return;
        }
        if self.handle_sidebar_shortcut(frame) {
            return;
        }
        if self.handle_arrow_hold_playback(ctx, t, frame, true) {
            return;
        }
        self.handle_media_shortcut(ctx, t, frame, outcome);
    }

    fn handle_arrow_hold_playback(
        &mut self,
        ctx: &egui::Context,
        t: engine::Timeline,
        frame: ShortcutFrame,
        allow_start: bool,
    ) -> bool {
        let input = ArrowHoldInput {
            left_down: frame.keys.left_down,
            right_down: frame.keys.right_down,
            can_start: allow_start
                && t.state == player_core::PlaybackState::Playing
                && plain_arrow_shortcut_modifiers(frame.modifiers),
        };
        let action = self
            .arrow_hold_playback
            .update(input, std::time::Instant::now(), t.rate_pct);
        match action {
            ArrowHoldAction::None => self.arrow_hold_playback.active,
            ArrowHoldAction::Activate { .. } => {
                self.player
                    .handle(player_core::Command::SetRate(ARROW_LONG_PRESS_RATE_PCT));
                self.set_shortcut_notice(format!(
                    "{}：{}",
                    t!("rate"),
                    crate::shortcuts::format_rate_label(ARROW_LONG_PRESS_RATE_PCT)
                ));
                ctx.request_repaint();
                true
            }
            ArrowHoldAction::Restore { rate_pct } => {
                self.player.handle(player_core::Command::SetRate(rate_pct));
                self.set_shortcut_notice(format!(
                    "{}：{}",
                    t!("rate"),
                    crate::shortcuts::format_rate_label(rate_pct)
                ));
                true
            }
        }
    }

    fn handle_rate_shortcut(&mut self, rate_pct: u16, frame: ShortcutFrame) -> bool {
        let Some(cmd) = rate_shortcut_command(
            frame.platform,
            frame.modifiers,
            frame.keys.up,
            frame.keys.down,
            rate_pct,
        ) else {
            return false;
        };

        let new_rate = match cmd {
            player_core::Command::SetRate(pct) => pct,
            _ => rate_pct,
        };
        self.player.handle(cmd);
        self.set_shortcut_notice(format!(
            "{}：{}",
            t!("rate"),
            crate::shortcuts::format_rate_label(new_rate)
        ));
        true
    }

    fn handle_navigation_shortcut(&mut self, frame: ShortcutFrame) -> bool {
        let Some(cmd) = navigation_shortcut_command(
            frame.platform,
            frame.modifiers,
            frame.keys.left,
            frame.keys.right,
        ) else {
            return false;
        };

        self.player.handle(cmd);
        if let Some(name) = self.current_playlist_name() {
            self.set_shortcut_notice(format!("{}：{}", t!("current_playing"), name));
        }
        true
    }

    fn handle_media_shortcut(
        &mut self,
        ctx: &egui::Context,
        t: engine::Timeline,
        frame: ShortcutFrame,
        outcome: &mut KeyboardShortcutOutcome,
    ) -> bool {
        // Plain keys mirror common video-player behavior: fullscreen, play/pause,
        // mute, screenshot, volume, then seek.
        if frame.keys.f || frame.keys.enter {
            return self.handle_fullscreen_shortcut(ctx);
        }
        if frame.keys.space {
            return self.handle_play_pause_shortcut(t.state);
        }
        if frame.keys.m {
            return self.handle_mute_shortcut();
        }
        if frame.keys.s {
            outcome.screenshot_requested = true;
            return true;
        }
        if frame.keys.up {
            return self.handle_volume_shortcut(crate::shortcuts::snap_volume_up(t.volume));
        }
        if frame.keys.down {
            return self.handle_volume_shortcut(crate::shortcuts::snap_volume_down(t.volume));
        }
        if frame.keys.left {
            return self.handle_seek_shortcut(t, false);
        }
        if frame.keys.right {
            return self.handle_seek_shortcut(t, true);
        }
        false
    }

    fn handle_fullscreen_shortcut(&mut self, ctx: &egui::Context) -> bool {
        controls::toggle_fullscreen(ctx);
        true
    }

    fn handle_play_pause_shortcut(&mut self, state: player_core::PlaybackState) -> bool {
        let cmd = if state == player_core::PlaybackState::Playing {
            player_core::Command::Pause
        } else {
            player_core::Command::Play
        };
        self.player.handle(cmd);
        match self.player.timeline().state {
            player_core::PlaybackState::Playing => {
                self.set_shortcut_notice(t!("shortcut_playing").to_string());
            }
            player_core::PlaybackState::Paused => {
                self.set_shortcut_notice(t!("shortcut_paused").to_string());
            }
            player_core::PlaybackState::Stopped => {}
        }
        true
    }

    fn handle_mute_shortcut(&mut self) -> bool {
        self.player.handle(player_core::Command::ToggleMute);
        let volume = self.player.timeline().volume;
        self.set_shortcut_notice(format!("{}：{}", t!("volume"), volume));
        true
    }

    fn handle_volume_shortcut(&mut self, volume: u8) -> bool {
        self.player.handle(player_core::Command::SetVolume(volume));
        self.set_shortcut_notice(format!("{}：{}", t!("volume"), volume));
        true
    }

    fn handle_seek_shortcut(&mut self, t: engine::Timeline, forward: bool) -> bool {
        let target = seek_shortcut_target(
            t.position_ms,
            t.duration_ms,
            self.player.prefs().seek_step_secs,
            forward,
        );
        self.player.handle(player_core::Command::SeekTo(target));
        self.set_shortcut_notice(format!("{}：{}", t!("position"), format_ms_label(target)));
        true
    }
}

fn viewport_is_fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| matches!(i.viewport().fullscreen, Some(true)))
}
