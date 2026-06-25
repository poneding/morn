//! Main app state and cross-module glue.
//!
//! `PlayerApp` owns the engine `Player`, egui view state, transient notices,
//! playlist/sidebar selection, pointer activity timestamps, font locale, update
//! checker, and the last video path used for automatic window sizing.  Rendering
//! and input-heavy code lives in sibling modules so this file remains the place
//! where long-lived state is declared and small shell-level operations are wired.
//!
//! The split is intentional:
//! `app_ui` owns per-frame egui orchestration and bottom controls,
//! `app_sidebar_ui` owns the floating playlist/history overlay,
//! `app_shortcuts` owns keyboard dispatch priority,
//! `app_behavior` owns pure UI rules that are easy to unit test, and
//! `app_platform` owns filesystem/platform helpers.  Keeping these boundaries
//! narrow helps prevent regressions where a UI refactor mutates player state while
//! egui is still borrowing layout data, or where platform setup leaks into the
//! playback engine.
//!
//! Code in this module should therefore stay focused on state construction,
//! notice presentation, command bridging between UI and engine, dropped-file
//! routing, screenshots, and one-shot window resize decisions.

use crate::controls;
use crate::video_view::VideoView;
use eframe::egui;
use engine::Player;
use rust_i18n::t;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarTab {
    Playlist,
    History,
}

pub const APP_MIN_WIDTH: f32 = 920.0;
pub const APP_MIN_HEIGHT: f32 = 560.0;
pub const VIDEO_MIN_WIDTH: f32 = 640.0;
pub(crate) const DEFAULT_INITIAL_INNER_SIZE: [f32; 2] = [960.0, 600.0];
const CONTROL_BAR_INNER_PADDING_X: i8 = crate::visuals::FLOATING_PANEL_INNER_MARGIN_X;
const CONTROLS_IDLE_HIDE_AFTER: std::time::Duration = std::time::Duration::from_secs(3);
const OVERLAY_HOVER_RECHECK_GRACE: std::time::Duration = std::time::Duration::from_millis(150);
const WINDOW_RESIZE_REPAINT_GRACE: std::time::Duration = std::time::Duration::from_millis(150);
const OCCLUDED_VIDEO_PRESENTATION_REPAINT_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(16);
const PLAYLIST_SHEET_INNER_MARGIN_X: i8 = crate::visuals::FLOATING_PANEL_INNER_MARGIN_X;
const PLAYLIST_SHEET_INNER_MARGIN_Y: i8 = crate::visuals::FLOATING_CONTROL_BAR_INNER_MARGIN_Y;
// 提示标语锚定在标题栏下方, 不遮挡标题栏(macOS 红绿灯/自绘标题)。
// 复用标题栏底部偏移 + 一个间距, 与播放列表顶部避开标题栏的逻辑一致。
const SCREENSHOT_NOTICE_TOP_OFFSET: f32 =
    crate::titlebar::TITLEBAR_BOTTOM_OFFSET + crate::visuals::FLOATING_PANEL_MARGIN;
const SHORTCUT_NOTICE_TOP_OFFSET: f32 =
    crate::titlebar::TITLEBAR_BOTTOM_OFFSET + crate::visuals::FLOATING_PANEL_MARGIN;
const SHORTCUT_NOTICE_DURATION: std::time::Duration = std::time::Duration::from_millis(1400);
const ARROW_LONG_PRESS_RATE_PCT: u16 = 200;
const ARROW_LONG_PRESS_THRESHOLD: std::time::Duration = std::time::Duration::from_millis(450);

pub(crate) fn window_size_for_video_dimensions(width: u32, height: u32) -> Option<egui::Vec2> {
    if width == 0 || height == 0 {
        return None;
    }

    Some(window_size_for_video_aspect(width as f32 / height as f32))
}

fn window_size_for_video_aspect(aspect: f32) -> egui::Vec2 {
    if !aspect.is_finite() || aspect <= 0.0 {
        return DEFAULT_INITIAL_INNER_SIZE.into();
    }

    // The inner window height includes the titlebar area; the video renders
    // below it, so derive dimensions such that the video area maintains the
    // correct aspect ratio.
    let tb_offset = crate::titlebar::TITLEBAR_BOTTOM_OFFSET;
    let mut inner_width = (DEFAULT_INITIAL_INNER_SIZE[1] - tb_offset) * aspect;
    let mut inner_height = DEFAULT_INITIAL_INNER_SIZE[1];

    if inner_width < APP_MIN_WIDTH {
        inner_width = APP_MIN_WIDTH;
        inner_height = inner_width / aspect + tb_offset;
    }
    if inner_height < APP_MIN_HEIGHT {
        inner_height = APP_MIN_HEIGHT;
        inner_width = (inner_height - tb_offset) * aspect;
    }
    egui::vec2(inner_width, inner_height)
}

struct TimedNotice {
    message: String,
    created: std::time::Instant,
}

type ScreenshotNotice = TimedNotice;
type ShortcutNotice = TimedNotice;

impl TimedNotice {
    fn new(message: String) -> Self {
        Self {
            message,
            created: std::time::Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrowHoldKey {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrowHoldAction {
    None,
    Activate { restore_rate_pct: u16 },
    Restore { rate_pct: u16 },
}

#[derive(Default)]
pub(super) struct ArrowHoldPlayback {
    key: Option<ArrowHoldKey>,
    pressed_at: Option<std::time::Instant>,
    restore_rate_pct: Option<u16>,
    active: bool,
}

impl ArrowHoldPlayback {
    pub(super) fn update(
        &mut self,
        input: ArrowHoldInput,
        now: std::time::Instant,
        current_rate_pct: u16,
    ) -> ArrowHoldAction {
        let held_key = input.held_key();
        let action = match (self.key, held_key) {
            (Some(key), Some(held_key)) if key == held_key => {
                self.update_held_key(input, now, current_rate_pct)
            }
            (Some(_), _) => self.reset(),
            (None, Some(key)) if input.can_start => {
                self.key = Some(key);
                self.pressed_at = Some(now);
                ArrowHoldAction::None
            }
            _ => ArrowHoldAction::None,
        };

        action
    }

    fn update_held_key(
        &mut self,
        input: ArrowHoldInput,
        now: std::time::Instant,
        current_rate_pct: u16,
    ) -> ArrowHoldAction {
        if !input.can_start {
            return self.reset();
        }
        if self.active {
            return ArrowHoldAction::None;
        }
        let Some(pressed_at) = self.pressed_at else {
            self.pressed_at = Some(now);
            return ArrowHoldAction::None;
        };
        if now.duration_since(pressed_at) < ARROW_LONG_PRESS_THRESHOLD {
            return ArrowHoldAction::None;
        }

        self.active = true;
        self.restore_rate_pct = Some(current_rate_pct);
        ArrowHoldAction::Activate {
            restore_rate_pct: current_rate_pct,
        }
    }

    fn reset(&mut self) -> ArrowHoldAction {
        let restore = self.active.then_some(self.restore_rate_pct).flatten();
        self.key = None;
        self.pressed_at = None;
        self.restore_rate_pct = None;
        self.active = false;
        restore
            .map(|rate_pct| ArrowHoldAction::Restore { rate_pct })
            .unwrap_or(ArrowHoldAction::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ArrowHoldInput {
    pub(super) left_down: bool,
    pub(super) right_down: bool,
    pub(super) can_start: bool,
}

impl ArrowHoldInput {
    fn held_key(self) -> Option<ArrowHoldKey> {
        match (self.left_down, self.right_down) {
            (true, false) => Some(ArrowHoldKey::Left),
            (false, true) => Some(ArrowHoldKey::Right),
            _ => None,
        }
    }
}

#[derive(Default)]
struct KeyboardShortcutOutcome {
    screenshot_requested: bool,
    playlist_opened_this_frame: bool,
}

pub struct PlayerApp {
    player: Player,
    video_view: VideoView,
    show_settings: bool,
    show_playlist: bool,
    playlist_auto_hidden: bool,
    playlist_auto_hide_armed: bool,
    sidebar_tab: SidebarTab,
    screenshot_notice: Option<ScreenshotNotice>,
    shortcut_notice: Option<ShortcutNotice>,
    playlist_candidate: Option<usize>,
    history_candidate: Option<usize>,
    last_pointer_pos: Option<egui::Pos2>,
    last_pointer_move: std::time::Instant,
    last_screen_rect: Option<egui::Rect>,
    last_window_resize: Option<std::time::Instant>,
    font_locale: String,
    update_check: crate::updater::UpdateChecker,
    last_window_resized_video_path: Option<std::path::PathBuf>,
    arrow_hold_playback: ArrowHoldPlayback,
    /// macOS: 标记是否已在首帧应用 resize 旧帧拉伸的掩盖(layerContentsPlacement)。
    #[cfg(target_os = "macos")]
    resize_glitch_mask_applied: bool,
}

#[path = "app_behavior.rs"]
mod app_behavior;
#[path = "app_platform.rs"]
mod app_platform;
#[path = "app_shortcuts.rs"]
mod app_shortcuts;
#[path = "app_sidebar_shortcuts.rs"]
mod app_sidebar_shortcuts;
#[path = "app_sidebar_ui.rs"]
mod app_sidebar_ui;
#[path = "app_ui.rs"]
mod app_ui;

use app_behavior::*;
pub(crate) use app_platform::prefs_path;
use app_platform::reveal_file;

fn restored_player() -> Player {
    let mut player = Player::with_prefs(prefs_path());
    // Restore the selected media immediately, but keep it paused; VideoView
    // will request repaint until the first restored frame is uploaded.
    player.restore_last_session_paused();
    player
}

fn startup_update_checker(player: &Player) -> crate::updater::UpdateChecker {
    let mut update_check = crate::updater::UpdateChecker::new(env!("CARGO_PKG_VERSION"));
    if player.prefs().check_updates_on_startup {
        // Startup checks use the same channel/beta settings as the manual
        // settings-window action.
        update_check.begin(player.prefs().check_beta_updates);
    }
    update_check
}

fn active_notice_message(
    notice: &mut Option<TimedNotice>,
    duration: std::time::Duration,
) -> Option<String> {
    let current = notice.as_ref()?;
    if current.created.elapsed() > duration {
        *notice = None;
        None
    } else {
        Some(current.message.clone())
    }
}

fn show_notice_area(ctx: &egui::Context, id: &'static str, top_offset: f32, message: String) {
    egui::Area::new(egui::Id::new(id))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, top_offset))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(420.0);
                ui.label(message);
            });
        });
}

impl PlayerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_paths: Vec<PathBuf>) -> Self {
        let player = restored_player();
        let font_locale = player.prefs().language.clone();
        rust_i18n::set_locale(&font_locale);
        crate::font::install_fonts(&cc.egui_ctx, &font_locale);
        crate::theme::install(&cc.egui_ctx);
        let update_check = startup_update_checker(&player);
        let mut this = Self {
            player,
            video_view: VideoView::new(),
            show_settings: false,
            show_playlist: false,
            playlist_auto_hidden: false,
            playlist_auto_hide_armed: true,
            sidebar_tab: SidebarTab::Playlist,
            screenshot_notice: None,
            shortcut_notice: None,
            playlist_candidate: None,
            history_candidate: None,
            last_pointer_pos: None,
            last_pointer_move: std::time::Instant::now(),
            last_screen_rect: None,
            last_window_resize: None,
            font_locale,
            update_check,
            last_window_resized_video_path: None,
            arrow_hold_playback: ArrowHoldPlayback::default(),
            #[cfg(target_os = "macos")]
            resize_glitch_mask_applied: false,
        };
        if !initial_paths.is_empty() {
            this.player
                .handle(player_core::Command::OpenFiles(initial_paths));
        }
        this
    }

    fn show_screenshot_notice(&mut self, ctx: &egui::Context) {
        let Some(message) = active_notice_message(
            &mut self.screenshot_notice,
            std::time::Duration::from_secs(3),
        ) else {
            return;
        };
        show_notice_area(
            ctx,
            "screenshot_notice",
            SCREENSHOT_NOTICE_TOP_OFFSET,
            message,
        );
    }

    fn set_screenshot_notice(&mut self, message: String) {
        self.screenshot_notice = Some(ScreenshotNotice::new(message));
    }

    fn show_shortcut_notice(&mut self, ctx: &egui::Context) {
        let Some(message) =
            active_notice_message(&mut self.shortcut_notice, SHORTCUT_NOTICE_DURATION)
        else {
            return;
        };
        show_notice_area(ctx, "shortcut_notice", SHORTCUT_NOTICE_TOP_OFFSET, message);
    }

    fn set_shortcut_notice(&mut self, message: String) {
        self.shortcut_notice = Some(ShortcutNotice::new(message));
    }

    fn current_playlist_name(&self) -> Option<String> {
        self.current_playlist_path()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
    }

    fn current_playlist_path(&self) -> Option<std::path::PathBuf> {
        self.player
            .current_index()
            .and_then(|i| self.player.playlist_paths().iter().nth(i).cloned())
    }

    fn handle_command(&mut self, cmd: player_core::Command) -> bool {
        // App-shell commands need native dialogs or platform integration; all other
        // commands are delegated to the engine so playback mutation stays in one
        // place.
        match cmd {
            player_core::Command::OpenDialog => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter(
                        t!("video_filter").to_string(),
                        &["mp4", "mkv", "webm", "mov", "avi"],
                    )
                    .pick_files()
                {
                    self.player.handle(player_core::Command::OpenFiles(paths));
                    true
                } else {
                    false
                }
            }
            player_core::Command::OpenFolder => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.player.open_folder(&dir);
                    true
                } else {
                    false
                }
            }
            player_core::Command::RevealFile(path) => reveal_file(&path),
            _ => {
                self.player.handle(cmd);
                true
            }
        }
    }

    fn resize_window_for_current_video(&mut self, ctx: &egui::Context) {
        if let Some(size) = self.current_video_resize_request(ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }

    fn current_video_resize_request(&mut self, ctx: &egui::Context) -> Option<egui::Vec2> {
        let current_path = self.current_playlist_path();
        let dimensions = self.player.current_video_dimensions();
        let current_frame_dimensions = self
            .player
            .current_frame_rgba()
            .map(|(_rgba, width, height)| (width, height));
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        video_window_resize_size(
            &mut self.last_window_resized_video_path,
            current_path,
            dimensions,
            current_frame_dimensions,
            fullscreen,
        )
    }

    fn sync_runtime_preferences(&mut self, ctx: &egui::Context) {
        let language = self.player.prefs().language.clone();
        rust_i18n::set_locale(&language);
        if self.font_locale != language {
            // Locale changes may also change CJK font fallback order, so reinstall
            // fonts only when the language actually changes.
            crate::font::install_fonts(ctx, &language);
            self.font_locale = language;
        }
        // 仅深色: 应用强制使用深色主题, 不跟随系统、也不提供切换。
        ctx.set_theme(egui::ThemePreference::Dark);
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        // Copy dropped paths out of egui input first; routing each path may mutate
        // the player and should not borrow input state.
        let dropped_paths = ctx.input(|i| dropped_file_paths(&i.raw.dropped_files));
        for path in dropped_paths {
            self.handle_dropped_path(path);
        }
    }

    fn handle_dropped_path(&mut self, path: std::path::PathBuf) {
        // Directories become playlists, subtitle files attach to the current
        // media, and all other files go through the same multi-open command path
        // as the file picker.
        if path.is_dir() {
            self.player.open_folder(&path);
            return;
        }
        if is_subtitle_path(&path) {
            self.player.load_subtitle(&path);
            return;
        }
        self.player
            .handle(player_core::Command::OpenFiles(vec![path]));
    }

    fn show_video_panel(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let mut video_commands = Vec::new();
        let fullscreen = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
        egui::CentralPanel::no_frame().show_inside(ui, |ui| {
            ui.set_min_width(VIDEO_MIN_WIDTH);
            if !fullscreen {
                ui.add_space(crate::titlebar::TITLEBAR_BOTTOM_OFFSET);
            }
            video_commands = self.video_view.show(ui, frame, &mut self.player);
        });
        for cmd in video_commands {
            self.handle_command(cmd);
        }
    }

    fn save_current_screenshot(&mut self) {
        let screenshot_dir = self.player.screenshot_dir();
        // Save against the engine's current frame snapshot so screenshot commands
        // work consistently from both the button and keyboard shortcut paths.
        let shot = self
            .player
            .current_frame_rgba()
            .map(|(rgba, w, h)| crate::enhance::save_screenshot(rgba, w, h, &screenshot_dir));
        match shot {
            Some(Ok(p)) => {
                eprintln!("截图已保存: {}", p.display());
                self.set_screenshot_notice(format!("{}: {}", t!("screenshot_saved"), p.display()));
            }
            Some(Err(err)) => {
                eprintln!("截图失败: {err}");
                self.set_screenshot_notice(format!("{}: {err}", t!("screenshot_failed")));
            }
            None => {
                self.set_screenshot_notice(t!("screenshot_no_frame").to_string());
            }
        }
    }

    fn update_candidates_after_sidebar_command(
        &mut self,
        cmd: &player_core::Command,
        playlist_len_before: usize,
        history_len_before: usize,
    ) {
        // Commands are applied after the sidebar is painted, so candidate repair
        // uses the list lengths captured immediately before mutation.
        match cmd {
            player_core::Command::RemovePlaylistIndex(index)
            | player_core::Command::DeletePlaylistFileIndex(index) => {
                self.playlist_candidate = candidate_after_index_remove(
                    self.playlist_candidate,
                    *index,
                    playlist_len_before,
                );
            }
            player_core::Command::ClearPlaylist => {
                self.playlist_candidate = None;
            }
            player_core::Command::RemoveHistoryIndex(index)
            | player_core::Command::DeleteHistoryFileIndex(index) => {
                self.history_candidate = candidate_after_index_remove(
                    self.history_candidate,
                    *index,
                    history_len_before,
                );
            }
            player_core::Command::ClearHistory => {
                self.history_candidate = None;
            }
            player_core::Command::OpenSiblingVideos(_) => {
                self.sidebar_tab = SidebarTab::Playlist;
                self.history_candidate = None;
                self.playlist_candidate = playlist_candidate_for_open(
                    self.player.current_index(),
                    self.player.playlist_paths().len(),
                );
            }
            _ => {}
        }
    }
}

fn dropped_file_paths(files: &[egui::DroppedFile]) -> Vec<std::path::PathBuf> {
    files.iter().filter_map(|file| file.path.clone()).collect()
}

fn is_subtitle_path(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("srt" | "ass" | "ssa")
    )
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
