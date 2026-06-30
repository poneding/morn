//! Sidebar-specific keyboard shortcut handlers.
//!
//! The playlist/history sheet has its own focused key handling: arrow keys move a
//! candidate, Enter opens it, and Delete/Backspace remove it.  Keeping these
//! methods outside the general shortcut dispatcher preserves the priority order
//! while keeping list mutation details near each other.
//!
//! Candidate indices are repaired after each removal so repeated Delete/Backspace
//! acts on the next visible row instead of falling off the list or reusing stale
//! indices from the previous frame.

use super::app_shortcuts::ShortcutFrame;
use super::*;

impl PlayerApp {
    pub(super) fn handle_sidebar_shortcut(&mut self, frame: ShortcutFrame) -> bool {
        if !frame.playlist_visible(self) {
            return false;
        }

        match self.sidebar_tab {
            SidebarTab::Playlist => self.handle_playlist_sidebar_shortcut(frame),
            SidebarTab::History => self.handle_history_sidebar_shortcut(frame),
        }
    }

    fn handle_playlist_sidebar_shortcut(&mut self, frame: ShortcutFrame) -> bool {
        if move_sidebar_candidate(&mut self.playlist_candidate, frame.playlist_len, frame) {
            return true;
        }
        if frame.keys.enter {
            self.play_playlist_candidate();
            return true;
        }
        if frame.keys.delete || frame.keys.backspace {
            self.remove_playlist_candidate(frame.playlist_len);
            return true;
        }
        false
    }

    fn handle_history_sidebar_shortcut(&mut self, frame: ShortcutFrame) -> bool {
        if move_sidebar_candidate(&mut self.history_candidate, frame.history_len, frame) {
            return true;
        }
        if frame.keys.enter {
            self.open_history_candidate();
            return true;
        }
        if frame.keys.delete || frame.keys.backspace {
            self.remove_history_candidate(frame.history_len);
            return true;
        }
        false
    }

    fn play_playlist_candidate(&mut self) {
        let Some(candidate) = self.playlist_candidate else {
            return;
        };

        self.handle_command(player_core::Command::PlayIndex(candidate));
        self.close_playlist();
        if let Some(name) = self.current_playlist_name() {
            self.set_shortcut_notice(format!("{}：{}", t!("current_playing"), name));
        }
    }

    fn remove_playlist_candidate(&mut self, playlist_len: usize) {
        let Some(candidate) = playlist_candidate_for_open(self.playlist_candidate, playlist_len)
        else {
            return;
        };

        let deleting_current = self.player.current_index() == Some(candidate);
        let removed_name = self
            .player
            .playlist_paths()
            .get(candidate)
            .map(path_file_name);
        self.handle_command(player_core::Command::RemovePlaylistIndex(candidate));
        self.playlist_candidate = candidate_after_remove(Some(candidate), playlist_len);
        self.set_playlist_remove_notice(deleting_current, removed_name);
    }

    fn set_playlist_remove_notice(&mut self, deleting_current: bool, removed_name: Option<String>) {
        if deleting_current {
            if let Some(name) = self.current_playlist_name() {
                self.set_shortcut_notice(format!(
                    "{}：{}；{}",
                    t!("current_playing"),
                    name,
                    t!("shortcut_paused")
                ));
                return;
            }
        }
        if let Some(name) = removed_name {
            self.set_shortcut_notice(format!("{}：{}", t!("removed"), name));
        }
    }

    fn open_history_candidate(&mut self) {
        let Some(candidate) = self.history_candidate else {
            return;
        };
        let Some(path) = self.player.history().get(candidate).cloned() else {
            return;
        };

        self.handle_command(player_core::Command::Open(std::path::PathBuf::from(path)));
        self.close_playlist();
        if let Some(name) = self.current_playlist_name() {
            self.set_shortcut_notice(format!("{}：{}", t!("current_playing"), name));
        }
    }

    fn remove_history_candidate(&mut self, history_len: usize) {
        let Some(candidate) = playlist_candidate_for_open(self.history_candidate, history_len)
        else {
            return;
        };

        let removed_name = history_candidate_name(self.player.history(), candidate);
        self.handle_command(player_core::Command::RemoveHistoryIndex(candidate));
        self.history_candidate = candidate_after_remove(Some(candidate), history_len);
        self.set_removed_history_notice(removed_name);
    }

    fn set_removed_history_notice(&mut self, removed_name: Option<String>) {
        if let Some(name) = removed_name {
            self.set_shortcut_notice(format!("{}：{}", t!("removed"), name));
        }
    }

    pub(super) fn close_playlist(&mut self) {
        self.show_playlist = false;
        self.playlist_auto_hidden = false;
        self.playlist_candidate = None;
        self.history_candidate = None;
    }
}

fn move_sidebar_candidate(candidate: &mut Option<usize>, len: usize, frame: ShortcutFrame) -> bool {
    if frame.keys.up {
        *candidate = move_playlist_candidate(*candidate, len, -1);
        return true;
    }
    if frame.keys.down {
        *candidate = move_playlist_candidate(*candidate, len, 1);
        return true;
    }
    false
}

fn history_candidate_name(history: &[String], candidate: usize) -> Option<String> {
    history
        .get(candidate)
        .map(|path| path_file_name(std::path::Path::new(path)))
}
