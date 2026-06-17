//! Command handling for `Player`.
//!
//! This module keeps user-initiated commands separate from playback timing and
//! decoding.  Public UI commands are translated into small private helpers that
//! save state at the same boundaries as the original monolithic match: before
//! replacing media, after playlist mutations, and after destructive operations.
//!
//! The helpers deliberately avoid opening dialogs or revealing files; those are
//! app-shell responsibilities.  Engine commands only mutate player state, playlist
//! contents, history, playback rate/volume, subtitles, and on-disk media deletion.
//! Keeping that boundary explicit makes it easier to reason about tests that call
//! `Player::handle` directly without an egui context.
//!
//! Handlers that tolerate invalid/idempotent transitions document that choice at
//! the call site.  Commands that can replace media save state before and after the
//! mutation so resume data is not lost if opening the next item fails.

use super::*;

impl Player {
    pub fn handle(&mut self, cmd: Command) {
        // Each handler either consumes the command or returns it for the next
        // category.  This keeps the public dispatcher flat while preserving the
        // old match ordering that tests rely on.
        let Some(cmd) = self.handle_app_shell_command(cmd) else {
            return;
        };
        let Some(cmd) = self.handle_media_source_command(cmd) else {
            return;
        };
        let Some(cmd) = self.handle_playlist_command(cmd) else {
            return;
        };
        let Some(cmd) = self.handle_history_command(cmd) else {
            return;
        };
        if let Some(cmd) = self.handle_playback_command(cmd) {
            debug_assert!(false, "unhandled player command: {cmd:?}");
        }
    }

    fn handle_app_shell_command(&mut self, cmd: Command) -> Option<Command> {
        // Shell commands are intentionally ignored here because the app layer owns
        // native dialogs and file-manager integration.
        match cmd {
            Command::OpenDialog | Command::OpenFolder | Command::RevealFile(_) => None,
            cmd => Some(cmd),
        }
    }

    fn handle_media_source_command(&mut self, cmd: Command) -> Option<Command> {
        // Opening commands are state boundaries: persist before changing the
        // playlist, then persist again after the selected media is known.
        match cmd {
            Command::Open(path) => {
                self.handle_open_command(path);
                None
            }
            Command::OpenFiles(paths) => {
                self.handle_open_files_command(paths);
                None
            }
            Command::OpenSiblingVideos(path) => {
                self.open_sibling_videos(&path);
                None
            }
            cmd => Some(cmd),
        }
    }

    fn handle_playlist_command(&mut self, cmd: Command) -> Option<Command> {
        // Playlist mutations may change the active item, so the concrete helper
        // owns any follow-up open/pause/teardown work.
        match cmd {
            Command::Next => {
                self.open_adjacent_playlist_item(PlaylistStep::Next);
                None
            }
            Command::Prev => {
                self.open_adjacent_playlist_item(PlaylistStep::Prev);
                None
            }
            Command::PlayIndex(i) => {
                self.play_index_command(i);
                None
            }
            Command::RemovePlaylistIndex(i) => {
                self.remove_playlist_index_command(i);
                None
            }
            Command::ClearPlaylist => {
                self.clear_playlist_command();
                None
            }
            Command::DeletePlaylistFileIndex(i) => {
                self.delete_playlist_file_index_command(i);
                None
            }
            cmd => Some(cmd),
        }
    }

    fn handle_history_command(&mut self, cmd: Command) -> Option<Command> {
        // History commands operate on persisted path strings and should not touch
        // playback unless a deleted file is also present in the playlist.
        if let Command::RemoveHistoryIndex(i) = &cmd {
            self.remove_history_index_command(*i);
            return None;
        }
        if matches!(&cmd, Command::ClearHistory) {
            self.clear_history_command();
            return None;
        }
        if let Command::DeleteHistoryFileIndex(i) = &cmd {
            self.delete_history_file_index_command(*i);
            return None;
        }
        Some(cmd)
    }

    fn handle_playback_command(&mut self, cmd: Command) -> Option<Command> {
        // Playback commands are the only commands that directly touch the state
        // machine, clock, shared audio controls, or subtitle selection.
        match cmd {
            Command::Play => self.play_command(),
            Command::Pause => self.pause_playback(),
            Command::Stop => self.stop_playback(),
            Command::SetVolume(v) => self.set_volume_command(v),
            Command::ToggleMute => self.toggle_mute_command(),
            Command::SeekTo(ms) => self.seek_to_user_target(ms),
            Command::SetRate(pct) => self.set_rate_command(pct),
            Command::SelectSubtitleTrack(idx) => self.select_subtitle_track(idx),
            cmd => return Some(cmd),
        }
        None
    }

    pub(super) fn pause_playback(&mut self) {
        if self.machine.apply(player_core::Transition::Pause).is_ok() {
            if let Some(a) = &self.audio_out {
                a.pause();
            }
            self.clock.pause();
        }
    }

    fn stop_playback(&mut self) {
        if self.machine.apply(player_core::Transition::Stop).is_err() {
            // Stop is idempotent at the Player layer; teardown still owns resource cleanup.
        }
        self.teardown();
    }

    fn handle_open_command(&mut self, path: std::path::PathBuf) {
        self.save_state();
        self.playlist.append_or_select(path.clone());
        self.open_path_and_save(&path);
    }

    fn handle_open_files_command(&mut self, paths: Vec<std::path::PathBuf>) {
        let items = selected_video_files(paths);
        if items.is_empty() {
            return;
        }

        self.open_selected_files(items);
    }

    fn open_selected_files(&mut self, items: Vec<std::path::PathBuf>) {
        self.save_state();
        let selected = self
            .playlist
            .append_or_select_many(items)
            .and_then(|index| self.playlist.as_slice().iter().nth(index).cloned());
        if let Some(selected) = selected {
            self.open_path_and_save(&selected);
        }
    }

    fn play_command(&mut self) {
        if self.video.is_none() {
            return;
        }
        if self.machine.apply(player_core::Transition::Play).is_err() {
            return;
        }

        if self.playback_ended {
            self.seek_to(0);
        }
        if let Some(a) = &self.audio_out {
            a.resume();
        }
        // Seek gate owns clock resume so audio/video are released on the same target frame.
        if self.seek_gate.is_none() {
            self.clock.resume();
        }
    }

    fn set_volume_command(&mut self, v: u8) {
        self.muted = false;
        self.handle_set_volume(v);
    }

    fn toggle_mute_command(&mut self) {
        let target_volume = if self.muted {
            self.volume_before_mute
        } else {
            0
        };
        self.muted = !self.muted;
        if self.muted {
            self.volume_before_mute = self.volume;
        }
        self.handle_set_volume(target_volume);
    }

    fn handle_set_volume(&mut self, v: u8) {
        self.volume = v.min(100);
        self.volume_shared.store(v.min(100), Ordering::Relaxed);
    }

    fn set_rate_command(&mut self, pct: u16) {
        let pct = clamp_rate_pct(pct);
        self.rate_pct = pct;
        self.rate_shared.store(pct as u32, Ordering::Relaxed);
        self.clock.set_rate(pct);
        if self.audio_out.is_some() {
            self.audio_flush.store(true, Ordering::Relaxed);
        }
    }

    fn open_adjacent_playlist_item(&mut self, step: PlaylistStep) {
        self.save_state();
        if let Some(path) = adjacent_playlist_path(&mut self.playlist, step) {
            self.open_path_and_save(&path);
        }
    }

    fn play_index_command(&mut self, index: usize) {
        self.save_state();
        self.playlist.set_cursor(index);
        if let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) {
            self.open_path_and_save(&path);
        }
    }

    fn remove_playlist_index_command(&mut self, index: usize) {
        self.save_state_around(|player| player.remove_playlist_index(index));
    }

    fn clear_playlist_command(&mut self) {
        self.save_state_around(|player| {
            player.playlist.clear();
            player.stop_playback();
        });
    }

    fn remove_history_index_command(&mut self, index: usize) {
        player_core::remove_history_index(&mut self.prefs.history, index);
        self.save_state();
    }

    fn clear_history_command(&mut self) {
        player_core::clear_history(&mut self.prefs.history);
        self.save_state();
    }

    fn delete_playlist_file_index_command(&mut self, index: usize) {
        self.save_state_around(|player| player.delete_playlist_file_index(index));
    }

    fn delete_history_file_index_command(&mut self, index: usize) {
        self.save_state_around(|player| player.delete_history_file_index(index));
    }

    fn select_subtitle_track(&mut self, idx: usize) {
        let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) else {
            return;
        };
        if let Ok(s) = media::decode_text_subtitle(&path, idx) {
            self.subtitles = Some(s);
        }
    }

    fn open_sibling_videos(&mut self, path: &Path) {
        let Some(dir) = path.parent() else {
            return;
        };
        let items = dir_videos(dir);
        let index = items.iter().position(|item| item == path).unwrap_or(0);
        if let Some(selected) = items.get(index).cloned() {
            self.save_state();
            self.playlist.set_items(items, index);
            self.open_path_and_save(&selected);
        }
    }

    fn remove_playlist_index(&mut self, index: usize) {
        let was_current = self.playlist.current_index() == Some(index);
        let had_media = self.video.is_some() || self.duration_ms > 0;

        if self.playlist.remove_index(index).is_none() {
            return;
        }
        if !was_current {
            return;
        }

        let Some(path) = self.playlist.current().map(|p| p.to_path_buf()) else {
            self.stop_playback();
            return;
        };
        if had_media {
            self.open_media(&path);
            self.pause_playback();
        }
    }

    fn delete_playlist_file_index(&mut self, index: usize) {
        let Some(path) = self.playlist.iter().nth(index).map(|p| p.to_path_buf()) else {
            return;
        };
        self.remove_playlist_index(index);
        remove_file_if_present(&path);
        let key = path.to_string_lossy().to_string();
        self.prefs.history.retain(|p| p != &key);
    }

    fn delete_history_file_index(&mut self, index: usize) {
        let Some(path) = self.prefs.history.get(index).cloned() else {
            return;
        };
        let path_buf = std::path::PathBuf::from(&path);
        if let Some(playlist_index) = self.playlist.iter().position(|p| p == &path_buf) {
            self.remove_playlist_index(playlist_index);
        }
        remove_file_if_present(&path_buf);
        self.prefs.history.retain(|p| p != &path);
    }

    fn save_state_around(&mut self, mutate: impl FnOnce(&mut Self)) {
        self.save_state();
        mutate(self);
        self.save_state();
    }

    fn open_path_and_save(&mut self, path: &Path) {
        self.open_media(path);
        self.save_state();
    }
}

#[derive(Clone, Copy)]
enum PlaylistStep {
    Next,
    Prev,
}

fn selected_video_files(paths: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    paths
        .into_iter()
        .filter(|path| path.is_file() && is_video_ext(path))
        .collect()
}

fn adjacent_playlist_path(
    playlist: &mut Playlist,
    step: PlaylistStep,
) -> Option<std::path::PathBuf> {
    match step {
        PlaylistStep::Next => playlist.next(),
        PlaylistStep::Prev => playlist.prev(),
    }
    .map(|path| path.to_path_buf())
}

fn remove_file_if_present(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => eprintln!("删除文件失败 {}: {err}", path.display()),
    }
}
