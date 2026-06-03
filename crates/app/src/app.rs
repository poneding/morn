use crate::controls;
use crate::video_view::VideoView;
use eframe::egui;
use engine::Player;
use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarTab {
    Playlist,
    History,
}

pub const APP_MIN_WIDTH: f32 = 920.0;
pub const APP_MIN_HEIGHT: f32 = 560.0;
pub const VIDEO_MIN_WIDTH: f32 = 640.0;
const CONTROL_BAR_INNER_PADDING_X: i8 = 14;
const CONTROLS_IDLE_HIDE_AFTER: std::time::Duration = std::time::Duration::from_secs(3);
const OVERLAY_HOVER_RECHECK_GRACE: std::time::Duration = std::time::Duration::from_millis(150);
const PLAYLIST_BOTTOM_CONTROLS_RESERVED_HEIGHT: f32 = 60.0;
const SCREENSHOT_NOTICE_TOP_OFFSET: f32 = 14.0;
const SHORTCUT_NOTICE_TOP_OFFSET: f32 = 14.0;
const SHORTCUT_NOTICE_DURATION: std::time::Duration = std::time::Duration::from_millis(1400);

struct ScreenshotNotice {
    message: String,
    created: std::time::Instant,
}

struct ShortcutNotice {
    message: String,
    created: std::time::Instant,
}

pub struct PlayerApp {
    player: Player,
    video_view: VideoView,
    show_settings: bool,
    show_playlist: bool,
    sidebar_tab: SidebarTab,
    screenshot_notice: Option<ScreenshotNotice>,
    shortcut_notice: Option<ShortcutNotice>,
    playlist_candidate: Option<usize>,
    history_candidate: Option<usize>,
    last_pointer_pos: Option<egui::Pos2>,
    last_pointer_move: std::time::Instant,
    font_locale: String,
    update_check: crate::updater::UpdateChecker,
}

impl PlayerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut player = Player::with_prefs(prefs_path());
        player.restore_last_session_paused();
        let font_locale = player.prefs().language.clone();
        rust_i18n::set_locale(&font_locale);
        crate::font::install_fonts(&cc.egui_ctx, &font_locale);
        let mut update_check = crate::updater::UpdateChecker::new(env!("CARGO_PKG_VERSION"));
        let check_updates_on_startup = player.prefs().check_updates_on_startup;
        if check_updates_on_startup {
            update_check.begin(player.prefs().check_beta_updates);
        }
        Self {
            player,
            video_view: VideoView::new(),
            show_settings: false,
            show_playlist: false,
            sidebar_tab: SidebarTab::Playlist,
            screenshot_notice: None,
            shortcut_notice: None,
            playlist_candidate: None,
            history_candidate: None,
            last_pointer_pos: None,
            last_pointer_move: std::time::Instant::now(),
            font_locale,
            update_check,
        }
    }

    fn show_screenshot_notice(&mut self, ctx: &egui::Context) {
        let Some(notice) = &self.screenshot_notice else {
            return;
        };
        if notice.created.elapsed() > std::time::Duration::from_secs(3) {
            self.screenshot_notice = None;
            return;
        }
        let message = notice.message.clone();
        egui::Area::new(egui::Id::new("screenshot_notice"))
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, SCREENSHOT_NOTICE_TOP_OFFSET),
            )
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.label(message);
                });
            });
    }

    fn set_screenshot_notice(&mut self, message: String) {
        self.screenshot_notice = Some(ScreenshotNotice {
            message,
            created: std::time::Instant::now(),
        });
    }

    fn show_shortcut_notice(&mut self, ctx: &egui::Context) {
        let Some(notice) = &self.shortcut_notice else {
            return;
        };
        if notice.created.elapsed() > SHORTCUT_NOTICE_DURATION {
            self.shortcut_notice = None;
            return;
        }
        let message = notice.message.clone();
        egui::Area::new(egui::Id::new("shortcut_notice"))
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, SHORTCUT_NOTICE_TOP_OFFSET),
            )
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.label(message);
                });
            });
    }

    fn set_shortcut_notice(&mut self, message: String) {
        self.shortcut_notice = Some(ShortcutNotice {
            message,
            created: std::time::Instant::now(),
        });
    }

    fn current_playlist_name(&self) -> Option<String> {
        self.current_playlist_path()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
    }

    fn current_playlist_path(&self) -> Option<std::path::PathBuf> {
        self.player
            .current_index()
            .and_then(|i| self.player.playlist_paths().get(i).cloned())
    }

    fn handle_command(&mut self, cmd: player_core::Command) -> bool {
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
            _ => {
                self.player.handle(cmd);
                true
            }
        }
    }

    fn update_candidates_after_sidebar_command(
        &mut self,
        cmd: &player_core::Command,
        playlist_len_before: usize,
        history_len_before: usize,
    ) {
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
            _ => {}
        }
    }
}

fn prefs_path() -> std::path::PathBuf {
    std::env::temp_dir().join("morn-prefs.json")
}

fn theme_preference(s: &str) -> egui::ThemePreference {
    match s {
        "dark" => egui::ThemePreference::Dark,
        "light" => egui::ThemePreference::Light,
        _ => egui::ThemePreference::System,
    }
}

fn playlist_sheet_width(available_width: f32) -> f32 {
    (available_width * 0.42)
        .clamp(crate::playlist_panel::PLAYLIST_MIN_WIDTH, 300.0)
        .min(available_width)
}

fn bottom_controls_visible(
    has_media: bool,
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
    screenshot_notice_visible: bool,
    last_pointer_move: std::time::Instant,
    now: std::time::Instant,
) -> bool {
    !has_media
        || screenshot_notice_visible
        || pointer_visible_for_overlay_recheck(pointer_pos, screen_rect, last_pointer_move, now)
}

fn pointer_active_inside_window(
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
    last_pointer_move: std::time::Instant,
    now: std::time::Instant,
) -> bool {
    pointer_inside_window(pointer_pos, screen_rect)
        && now.duration_since(last_pointer_move) <= CONTROLS_IDLE_HIDE_AFTER
}

fn pointer_visible_for_overlay_recheck(
    pointer_pos: Option<egui::Pos2>,
    screen_rect: egui::Rect,
    last_pointer_move: std::time::Instant,
    now: std::time::Instant,
) -> bool {
    pointer_inside_window(pointer_pos, screen_rect)
        && now.duration_since(last_pointer_move)
            <= CONTROLS_IDLE_HIDE_AFTER + OVERLAY_HOVER_RECHECK_GRACE
}

fn refresh_pointer_activity_for_current_overlay_hover(
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

fn pointer_activity_repaint_delay(
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

fn pointer_moved_since_last_frame(
    last_pointer_pos: Option<egui::Pos2>,
    pointer_pos: Option<egui::Pos2>,
) -> bool {
    match (last_pointer_pos, pointer_pos) {
        (Some(last), Some(current)) => last.distance_sq(current) > 0.25,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn should_request_continuous_repaint(screenshot_notice_visible: bool, interacting: bool) -> bool {
    !interacting && screenshot_notice_visible
}

fn navigation_shortcut_command(
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

fn rate_shortcut_command(
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

fn settings_shortcut_pressed(modifiers: egui::Modifiers, comma_pressed: bool) -> bool {
    modifiers.command && comma_pressed
}

fn open_shortcut_pressed(modifiers: egui::Modifiers, o_pressed: bool) -> bool {
    modifiers.command && o_pressed
}

fn opened_playlist_name_after_shortcut(
    opened: bool,
    after: Option<std::path::PathBuf>,
) -> Option<String> {
    opened.then_some(after).flatten().map(path_file_name)
}

fn toggle_settings_with_shortcut(
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

fn playlist_candidate_for_open(current_index: Option<usize>, playlist_len: usize) -> Option<usize> {
    if playlist_len == 0 {
        None
    } else {
        Some(current_index.unwrap_or(0).min(playlist_len - 1))
    }
}

fn move_playlist_candidate(
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

fn candidate_after_remove(current_candidate: Option<usize>, previous_len: usize) -> Option<usize> {
    if previous_len <= 1 {
        None
    } else {
        Some(current_candidate.unwrap_or(0).min(previous_len - 2))
    }
}

fn candidate_after_index_remove(
    current_candidate: Option<usize>,
    removed_index: usize,
    previous_len: usize,
) -> Option<usize> {
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

fn path_file_name(path: impl AsRef<std::path::Path>) -> String {
    path.as_ref()
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.as_ref().to_string_lossy().to_string())
}

fn toggle_playlist_with_shortcut(
    show_playlist: &mut bool,
    playlist_candidate: &mut Option<usize>,
    modifiers: egui::Modifiers,
    p_pressed: bool,
    current_index: Option<usize>,
    playlist_len: usize,
) -> bool {
    if !playlist_shortcut_pressed(modifiers, p_pressed) {
        return false;
    }
    *show_playlist = !*show_playlist;
    *playlist_candidate = if *show_playlist {
        playlist_candidate_for_open(current_index, playlist_len)
    } else {
        None
    };
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeShortcutAction {
    CloseSettings,
    ClosePlaylist,
    ExitFullscreen,
    None,
}

fn escape_shortcut_action(
    show_settings: bool,
    show_playlist: bool,
    is_fullscreen: bool,
) -> EscapeShortcutAction {
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

fn format_ms_label(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

fn playlist_has_prev(_playlist_len: usize, current_index: Option<usize>) -> bool {
    current_index.is_some_and(|index| index > 0)
}

fn playlist_has_next(playlist_len: usize, current_index: Option<usize>) -> bool {
    current_index.is_some_and(|index| index + 1 < playlist_len)
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 每帧应用语言与主题偏好(幂等), 保证设置窗口切换后立即生效。
        let language = self.player.prefs().language.clone();
        rust_i18n::set_locale(&language);
        if self.font_locale != language {
            crate::font::install_fonts(&ctx, &language);
            self.font_locale = language;
        }
        ctx.set_theme(theme_preference(&self.player.prefs().theme));

        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(path) = f.path {
                if path.is_dir() {
                    self.player.open_folder(&path);
                } else {
                    match path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .as_deref()
                    {
                        Some("srt") | Some("ass") | Some("ssa") => self.player.load_subtitle(&path),
                        _ => self
                            .player
                            .handle(player_core::Command::OpenFiles(vec![path])),
                    }
                }
            }
        }

        self.player.tick();
        self.update_check.poll();
        let t = self.player.timeline();

        let modifiers = ctx.input(|i| i.modifiers);
        let current_index_for_shortcuts = self.player.current_index();
        let playlist_len_for_shortcuts = self.player.playlist_paths().len();
        let history_len_for_shortcuts = self.player.history().len();
        let comma = ctx.input(|i| i.key_pressed(egui::Key::Comma));
        let o_key = ctx.input(|i| i.key_pressed(egui::Key::O));
        let p_key = ctx.input(|i| i.key_pressed(egui::Key::P));
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut keyboard_screenshot_requested = false;

        if escape {
            let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            match escape_shortcut_action(self.show_settings, self.show_playlist, fullscreen) {
                EscapeShortcutAction::CloseSettings => {
                    self.show_settings = false;
                    self.set_shortcut_notice(format!("{}：{}", t!("settings"), t!("closed")));
                }
                EscapeShortcutAction::ClosePlaylist => {
                    self.show_playlist = false;
                    self.playlist_candidate = None;
                    self.history_candidate = None;
                    self.set_shortcut_notice(format!("{}：{}", t!("playlist"), t!("closed")));
                }
                EscapeShortcutAction::ExitFullscreen => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    self.set_shortcut_notice(t!("fullscreen_exited").to_string());
                }
                EscapeShortcutAction::None => {}
            }
        }

        if toggle_settings_with_shortcut(&mut self.show_settings, modifiers, comma) {
            let status = if self.show_settings {
                t!("opened")
            } else {
                t!("closed")
            };
            self.set_shortcut_notice(format!("{}：{}", t!("settings"), status));
        }

        if open_shortcut_pressed(modifiers, o_key) {
            let opened = self.handle_command(player_core::Command::OpenDialog);
            if let Some(name) =
                opened_playlist_name_after_shortcut(opened, self.current_playlist_path())
            {
                self.set_shortcut_notice(format!("{}：{}", t!("current_playing"), name));
            }
        }

        if toggle_playlist_with_shortcut(
            &mut self.show_playlist,
            &mut self.playlist_candidate,
            modifiers,
            p_key,
            current_index_for_shortcuts,
            playlist_len_for_shortcuts,
        ) {
            self.sidebar_tab = SidebarTab::Playlist;
            if self.show_playlist {
                self.history_candidate = None;
            } else {
                self.playlist_candidate = None;
                self.history_candidate = None;
            }
            let status = if self.show_playlist {
                t!("opened")
            } else {
                t!("closed")
            };
            self.set_shortcut_notice(format!("{}：{}", t!("playlist"), status));
        }

        // 键盘快捷键: Cmd/Ctrl+,=设置, Cmd/Ctrl+P=播放列表, F/Enter=全屏, 空格=播放/暂停,
        // M=静音, ↑↓=音量(吸附5), ←→=按步长 seek, macOS Cmd+方向键/Windows/Linux Alt+方向键控制列表与倍速。
        if !ctx.egui_wants_keyboard_input() {
            let platform = crate::shortcuts::ShortcutPlatform::current();
            let step_ms = self.player.prefs().seek_step_secs * 1000;
            let pos = t.position_ms;
            let dur = t.duration_ms;
            let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
            let space = ctx.input(|i| i.key_pressed(egui::Key::Space));
            let f_key = ctx.input(|i| i.key_pressed(egui::Key::F));
            let m_key = ctx.input(|i| i.key_pressed(egui::Key::M));
            let s_key = ctx.input(|i| i.key_pressed(egui::Key::S));
            let up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let down = ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));
            let left = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft));
            let right = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight));
            let delete = ctx.input(|i| i.key_pressed(egui::Key::Delete));
            let backspace = ctx.input(|i| i.key_pressed(egui::Key::Backspace));

            let mut shortcut_handled = false;
            if let Some(cmd) = rate_shortcut_command(platform, modifiers, up, down, t.rate_pct) {
                let rate_pct = match cmd {
                    player_core::Command::SetRate(pct) => pct,
                    _ => t.rate_pct,
                };
                self.player.handle(cmd);
                self.set_shortcut_notice(format!(
                    "{}：{}",
                    t!("rate"),
                    crate::shortcuts::format_rate_label(rate_pct)
                ));
                shortcut_handled = true;
            }

            if !shortcut_handled {
                if let Some(cmd) = navigation_shortcut_command(platform, modifiers, left, right) {
                    self.player.handle(cmd);
                    if let Some(name) = self.current_playlist_name() {
                        self.set_shortcut_notice(format!("{}：{}", t!("current_playing"), name));
                    }
                    shortcut_handled = true;
                }
            }

            if !shortcut_handled && self.show_playlist {
                match self.sidebar_tab {
                    SidebarTab::Playlist => {
                        if up {
                            self.playlist_candidate = move_playlist_candidate(
                                self.playlist_candidate,
                                playlist_len_for_shortcuts,
                                -1,
                            );
                            shortcut_handled = true;
                        } else if down {
                            self.playlist_candidate = move_playlist_candidate(
                                self.playlist_candidate,
                                playlist_len_for_shortcuts,
                                1,
                            );
                            shortcut_handled = true;
                        } else if enter {
                            if let Some(candidate) = self.playlist_candidate {
                                self.handle_command(player_core::Command::PlayIndex(candidate));
                                self.show_playlist = false;
                                self.playlist_candidate = None;
                                self.history_candidate = None;
                                if let Some(name) = self.current_playlist_name() {
                                    self.set_shortcut_notice(format!(
                                        "{}：{}",
                                        t!("current_playing"),
                                        name
                                    ));
                                }
                            }
                            shortcut_handled = true;
                        } else if delete || backspace {
                            if let Some(candidate) = playlist_candidate_for_open(
                                self.playlist_candidate,
                                playlist_len_for_shortcuts,
                            ) {
                                let deleting_current =
                                    self.player.current_index() == Some(candidate);
                                let removed_name = self
                                    .player
                                    .playlist_paths()
                                    .get(candidate)
                                    .map(path_file_name);
                                let cmd = player_core::Command::RemovePlaylistIndex(candidate);
                                self.handle_command(cmd);
                                self.playlist_candidate = candidate_after_remove(
                                    Some(candidate),
                                    playlist_len_for_shortcuts,
                                );
                                if deleting_current {
                                    if let Some(name) = self.current_playlist_name() {
                                        self.set_shortcut_notice(format!(
                                            "{}：{}；{}",
                                            t!("current_playing"),
                                            name,
                                            t!("shortcut_paused")
                                        ));
                                    } else if let Some(name) = removed_name {
                                        self.set_shortcut_notice(format!(
                                            "{}：{}",
                                            t!("removed"),
                                            name
                                        ));
                                    }
                                } else if let Some(name) = removed_name {
                                    self.set_shortcut_notice(format!(
                                        "{}：{}",
                                        t!("removed"),
                                        name
                                    ));
                                }
                            }
                            shortcut_handled = true;
                        }
                    }
                    SidebarTab::History => {
                        if up {
                            self.history_candidate = move_playlist_candidate(
                                self.history_candidate,
                                history_len_for_shortcuts,
                                -1,
                            );
                            shortcut_handled = true;
                        } else if down {
                            self.history_candidate = move_playlist_candidate(
                                self.history_candidate,
                                history_len_for_shortcuts,
                                1,
                            );
                            shortcut_handled = true;
                        } else if enter {
                            if let Some(candidate) = self.history_candidate {
                                if let Some(path) = self.player.history().get(candidate).cloned() {
                                    self.handle_command(player_core::Command::Open(
                                        std::path::PathBuf::from(path),
                                    ));
                                    self.show_playlist = false;
                                    self.playlist_candidate = None;
                                    self.history_candidate = None;
                                    if let Some(name) = self.current_playlist_name() {
                                        self.set_shortcut_notice(format!(
                                            "{}：{}",
                                            t!("current_playing"),
                                            name
                                        ));
                                    }
                                }
                            }
                            shortcut_handled = true;
                        } else if delete || backspace {
                            if let Some(candidate) = playlist_candidate_for_open(
                                self.history_candidate,
                                history_len_for_shortcuts,
                            ) {
                                let removed_name = self
                                    .player
                                    .history()
                                    .get(candidate)
                                    .map(|path| path_file_name(std::path::Path::new(path)));
                                let cmd = player_core::Command::RemoveHistoryIndex(candidate);
                                self.handle_command(cmd);
                                self.history_candidate = candidate_after_remove(
                                    Some(candidate),
                                    history_len_for_shortcuts,
                                );
                                if let Some(name) = removed_name {
                                    self.set_shortcut_notice(format!(
                                        "{}：{}",
                                        t!("removed"),
                                        name
                                    ));
                                }
                            }
                            shortcut_handled = true;
                        }
                    }
                }
            }

            if !shortcut_handled && (f_key || enter) {
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                controls::toggle_fullscreen(&ctx);
                self.set_shortcut_notice(if fullscreen {
                    t!("fullscreen_exited").to_string()
                } else {
                    t!("fullscreen_entered").to_string()
                });
                shortcut_handled = true;
            }
            if !shortcut_handled && space {
                let cmd = if t.state == player_core::PlaybackState::Playing {
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
                shortcut_handled = true;
            }
            if !shortcut_handled && m_key {
                self.player.handle(player_core::Command::ToggleMute);
                let volume = self.player.timeline().volume;
                self.set_shortcut_notice(format!("{}：{}", t!("volume"), volume));
                shortcut_handled = true;
            }
            if !shortcut_handled && s_key {
                keyboard_screenshot_requested = true;
                shortcut_handled = true;
            }
            if !shortcut_handled && up {
                let volume = crate::shortcuts::snap_volume_up(t.volume);
                self.player.handle(player_core::Command::SetVolume(volume));
                self.set_shortcut_notice(format!("{}：{}", t!("volume"), volume));
                shortcut_handled = true;
            }
            if !shortcut_handled && down {
                let volume = crate::shortcuts::snap_volume_down(t.volume);
                self.player.handle(player_core::Command::SetVolume(volume));
                self.set_shortcut_notice(format!("{}：{}", t!("volume"), volume));
                shortcut_handled = true;
            }
            if !shortcut_handled && left {
                let target = pos.saturating_sub(step_ms);
                self.player.handle(player_core::Command::SeekTo(target));
                self.set_shortcut_notice(format!(
                    "{}：{}",
                    t!("position"),
                    format_ms_label(target)
                ));
                shortcut_handled = true;
            }
            if !shortcut_handled && right {
                let target = if dur > 0 {
                    (pos + step_ms).min(dur)
                } else {
                    pos + step_ms
                };
                self.player.handle(player_core::Command::SeekTo(target));
                self.set_shortcut_notice(format!(
                    "{}：{}",
                    t!("position"),
                    format_ms_label(target)
                ));
            }
        }

        let mut video_commands = Vec::new();
        egui::CentralPanel::no_frame().show_inside(ui, |ui| {
            ui.set_min_width(VIDEO_MIN_WIDTH);
            video_commands = self.video_view.show(ui, frame, &self.player);
        });
        for cmd in video_commands {
            self.handle_command(cmd);
        }

        let screen_rect = ctx.content_rect();
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let now = std::time::Instant::now();
        if pointer_moved_since_last_frame(self.last_pointer_pos, pointer_pos) {
            self.last_pointer_move = now;
        }
        self.last_pointer_pos = pointer_pos;
        let playlist_paths = self.player.playlist_paths();
        let current_index = self.player.current_index();
        let has_prev = playlist_has_prev(playlist_paths.len(), current_index);
        let has_next = playlist_has_next(playlist_paths.len(), current_index);
        if self.show_playlist {
            let hist: Vec<std::path::PathBuf> =
                self.player.history().iter().map(Into::into).collect();
            match self.sidebar_tab {
                SidebarTab::Playlist => {
                    self.playlist_candidate = playlist_candidate_for_open(
                        self.playlist_candidate.or(current_index),
                        playlist_paths.len(),
                    );
                }
                SidebarTab::History => {
                    self.history_candidate =
                        playlist_candidate_for_open(self.history_candidate, hist.len());
                }
            }
            let sheet_width = playlist_sheet_width(screen_rect.width());
            let playlist_sheet_pos = egui::pos2(
                screen_rect.max.x - sheet_width - crate::visuals::FLOATING_PANEL_MARGIN,
                screen_rect.min.y + crate::visuals::FLOATING_PANEL_MARGIN,
            );
            let playlist_sheet_height = (screen_rect.max.y
                - playlist_sheet_pos.y
                - PLAYLIST_BOTTOM_CONTROLS_RESERVED_HEIGHT)
                .max(0.0);
            let mut playlist_commands = Vec::new();

            let playlist_area = egui::Area::new(egui::Id::new("playlist_sheet"))
                .fixed_pos(playlist_sheet_pos)
                .order(egui::Order::Foreground)
                .fade_in(false)
                .show(&ctx, |ui| {
                    ui.set_width(sheet_width);
                    ui.set_height(playlist_sheet_height);
                    let frame = crate::visuals::frosted_frame(ui, egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_min_height(playlist_sheet_height);
                            ui.horizontal(|ui| {
                                playlist_commands
                                    .extend(crate::playlist_panel::open_menu_button(ui));
                                let tab_before = self.sidebar_tab;
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.selectable_value(
                                            &mut self.sidebar_tab,
                                            SidebarTab::History,
                                            t!("history").to_string(),
                                        );
                                        ui.selectable_value(
                                            &mut self.sidebar_tab,
                                            SidebarTab::Playlist,
                                            t!("playlist").to_string(),
                                        );
                                    },
                                );
                                if self.sidebar_tab != tab_before {
                                    match self.sidebar_tab {
                                        SidebarTab::Playlist => {
                                            self.playlist_candidate = playlist_candidate_for_open(
                                                self.playlist_candidate.or(current_index),
                                                playlist_paths.len(),
                                            );
                                            self.history_candidate = None;
                                        }
                                        SidebarTab::History => {
                                            self.history_candidate = playlist_candidate_for_open(
                                                self.history_candidate,
                                                hist.len(),
                                            );
                                            self.playlist_candidate = None;
                                        }
                                    }
                                }
                            });
                            ui.separator();
                            match self.sidebar_tab {
                                SidebarTab::Playlist => {
                                    playlist_commands.extend(
                                        crate::playlist_panel::playlist_panel(
                                            ui,
                                            &playlist_paths,
                                            current_index,
                                            self.playlist_candidate,
                                        ),
                                    );
                                }
                                SidebarTab::History => {
                                    playlist_commands.extend(crate::playlist_panel::history_panel(
                                        ui,
                                        &hist,
                                        self.history_candidate,
                                    ));
                                }
                            }
                        });
                    frame.response.hovered() || frame.response.contains_pointer()
                });
            refresh_pointer_activity_for_current_overlay_hover(
                playlist_area.inner || playlist_area.response.hovered(),
                &mut self.last_pointer_move,
                now,
            );

            for cmd in playlist_commands {
                let playlist_len_before = self.player.playlist_paths().len();
                let history_len_before = self.player.history().len();
                self.handle_command(cmd.clone());
                self.update_candidates_after_sidebar_command(
                    &cmd,
                    playlist_len_before,
                    history_len_before,
                );
            }
        }

        let has_media = self.player.video().is_some() || t.duration_ms > 0;
        let controls_visible = bottom_controls_visible(
            has_media,
            pointer_pos,
            screen_rect,
            self.screenshot_notice.is_some(),
            self.last_pointer_move,
            now,
        );

        let mut bottom_commands = Vec::new();
        let mut screenshot_requested = keyboard_screenshot_requested;
        let tracks = self.player.subtitle_tracks().to_vec();
        if controls_visible {
            let outer_width =
                (screen_rect.width() - crate::visuals::FLOATING_PANEL_MARGIN * 2.0).max(0.0);
            let content_width =
                (outer_width - f32::from(CONTROL_BAR_INNER_PADDING_X) * 2.0).max(0.0);
            let floating_controls_area = egui::Area::new(egui::Id::new("floating_controls"))
                .anchor(
                    egui::Align2::CENTER_BOTTOM,
                    egui::vec2(0.0, -crate::visuals::FLOATING_PANEL_MARGIN),
                )
                .order(egui::Order::Foreground)
                .fade_in(false)
                .show(&ctx, |ui| {
                    ui.set_width(outer_width);
                    ui.set_min_width(outer_width);
                    let frame = crate::visuals::frosted_frame(
                        ui,
                        egui::Margin::symmetric(
                            CONTROL_BAR_INNER_PADDING_X,
                            crate::visuals::FLOATING_CONTROL_BAR_INNER_MARGIN_Y,
                        ),
                    )
                    .show(ui, |ui| {
                        ui.set_width(content_width);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            bottom_commands
                                .extend(controls::controls_bar(ui, &t, has_prev, has_next));

                            let actions = crate::enhance::enhance_bar(ui, t.rate_pct);
                            bottom_commands.extend(actions.commands);
                            screenshot_requested |= actions.screenshot;

                            if !tracks.is_empty() {
                                if let Some(cmd) = controls::subtitle_track_combo(ui, &tracks) {
                                    bottom_commands.push(cmd);
                                }
                            }

                            ui.add_space(ui.available_width().max(0.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(egui::Button::new("⚙").selected(self.show_settings))
                                        .on_hover_text(crate::shortcuts::shortcut_tooltip(
                                            t!("settings"),
                                            crate::shortcuts::settings_shortcut_label(),
                                        ))
                                        .clicked()
                                    {
                                        self.show_settings = !self.show_settings;
                                    }
                                    if ui
                                        .add(egui::Button::new("☰").selected(self.show_playlist))
                                        .on_hover_text(crate::shortcuts::shortcut_tooltip(
                                            t!("playlist"),
                                            crate::shortcuts::playlist_shortcut_label(),
                                        ))
                                        .clicked()
                                    {
                                        self.show_playlist = !self.show_playlist;
                                        if self.show_playlist {
                                            self.sidebar_tab = SidebarTab::Playlist;
                                            self.playlist_candidate = playlist_candidate_for_open(
                                                current_index,
                                                playlist_paths.len(),
                                            );
                                            self.history_candidate = None;
                                        } else {
                                            self.playlist_candidate = None;
                                            self.history_candidate = None;
                                        }
                                    }
                                },
                            );
                        });
                    });
                    frame.response.hovered() || frame.response.contains_pointer()
                });
            refresh_pointer_activity_for_current_overlay_hover(
                floating_controls_area.inner || floating_controls_area.response.hovered(),
                &mut self.last_pointer_move,
                now,
            );
        }
        for cmd in bottom_commands {
            self.handle_command(cmd);
        }
        if screenshot_requested {
            if let Some((rgba, w, h)) = self.video_view.last_frame() {
                let screenshot_dir = self.player.screenshot_dir();
                match crate::enhance::save_screenshot(rgba, w, h, &screenshot_dir) {
                    Ok(p) => {
                        eprintln!("截图已保存: {}", p.display());
                        self.set_screenshot_notice(format!(
                            "{}: {}",
                            t!("screenshot_saved"),
                            p.display()
                        ));
                    }
                    Err(e) => {
                        eprintln!("截图失败: {e}");
                        self.set_screenshot_notice(format!("{}: {e}", t!("screenshot_failed")));
                    }
                }
            } else {
                self.set_screenshot_notice(t!("screenshot_no_frame").to_string());
            }
        }

        crate::settings::settings_window(
            &ctx,
            &mut self.show_settings,
            &mut self.player,
            &mut self.update_check,
        );
        self.show_screenshot_notice(&ctx);
        self.show_shortcut_notice(&ctx);

        if self.update_check.is_checking() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // 拖窗口边缘/拖滑块时让出 60Hz 主动重绘, 改由 OS Resized/Moved
        // 事件驱动, 避免 macOS 顶/左拉伸时 Moved 事件与 60Hz 重绘
        // 争抢合成带宽, 造成顶/左比底/右明显卡顿。
        let interacting = ctx.input(|i| i.pointer.any_down());
        if should_request_continuous_repaint(
            self.screenshot_notice.is_some() || self.shortcut_notice.is_some(),
            interacting,
        ) {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        if let Some(delay) =
            pointer_activity_repaint_delay(pointer_pos, screen_rect, self.last_pointer_move, now)
        {
            ctx.request_repaint_after(delay);
        }
        if !interacting
            && self.player.timeline().state == player_core::PlaybackState::Playing
            && self.player.video().is_some_and(|v| v.take_frame_pending())
        {
            ctx.request_repaint();
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.player.save_state();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        _visuals.panel_fill.to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
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
    fn playlist_sheet_width_stays_inside_current_window() {
        assert_eq!(super::playlist_sheet_width(super::APP_MIN_WIDTH), 300.0);
        assert_eq!(super::playlist_sheet_width(160.0), 160.0);

        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        assert!(source.contains("playlist_sheet_width"));
        assert!(!source.contains(".max_size(playlist_max_width"));
    }

    #[test]
    fn video_panel_uses_no_frame_to_touch_window_edges() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("CentralPanel::no_frame()"));
        assert!(!source.contains("CentralPanel::default().show_inside"));
    }

    #[test]
    fn screenshot_notice_is_top_centered_and_includes_path() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        assert!(!source.contains("Align2::RIGHT_BOTTOM"));
        assert!(source.contains("Align2::CENTER_TOP"));
        assert!(source.contains("SCREENSHOT_NOTICE_TOP_OFFSET"));
        assert!(!source.contains("screenshot_notice_pos"));
        assert!(source.contains("p.display()"));
    }

    #[test]
    fn screenshots_are_saved_under_configured_directory() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("self.player.screenshot_dir()"));
        assert!(!source.contains("engine::resolve_screenshot_dir"));
        assert!(source.contains("save_screenshot"));
    }

    #[test]
    fn bottom_controls_float_over_video_and_auto_hide() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("Area::new(egui::Id::new(\"floating_controls\"))"));
        assert!(source.contains("egui::Order::Foreground"));
        assert!(source.contains("bottom_controls_visible"));
        assert!(!source.contains("Panel::bottom(\"controls\")"));
    }

    #[test]
    fn bottom_controls_are_full_width_and_host_app_actions_on_the_right() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("Layout::left_to_right(egui::Align::Center)"));
        assert!(
            source.contains("screen_rect.width() - crate::visuals::FLOATING_PANEL_MARGIN * 2.0")
        );
        assert!(source.contains("let outer_width"));
        assert!(source.contains("let content_width"));
        assert!(source.contains("ui.set_width(outer_width)"));
        assert!(source.contains("ui.set_width(content_width)"));
        assert!(source.contains("Layout::right_to_left(egui::Align::Center)"));
        assert!(source.contains("Button::new(\"⚙\")"));
        assert!(source.contains("Button::new(\"☰\")"));
    }

    #[test]
    fn bottom_controls_use_content_height_and_idle_auto_hide() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

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
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("egui::vec2(0.0, -crate::visuals::FLOATING_PANEL_MARGIN)"));
        assert!(!source.contains("sheet_reserved_width"));
        assert!(!source.contains("control_rect"));
    }

    #[test]
    fn bottom_controls_visibility_rule_uses_whole_window() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0));
        let now = std::time::Instant::now();
        let recent_move = now - std::time::Duration::from_secs(1);
        let recheck_move =
            now - super::CONTROLS_IDLE_HIDE_AFTER - std::time::Duration::from_millis(1);
        let expired_move = now
            - super::CONTROLS_IDLE_HIDE_AFTER
            - super::OVERLAY_HOVER_RECHECK_GRACE
            - std::time::Duration::from_millis(1);

        assert!(super::bottom_controls_visible(
            false,
            None,
            screen,
            false,
            expired_move,
            now
        ));
        assert!(super::bottom_controls_visible(
            true,
            None,
            screen,
            true,
            expired_move,
            now
        ));
        assert!(super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 700.0)),
            screen,
            false,
            recent_move,
            now
        ));
        assert!(super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 500.0)),
            screen,
            false,
            recent_move,
            now
        ));
        assert!(super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 500.0)),
            screen,
            false,
            recheck_move,
            now
        ));
        assert!(!super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 500.0)),
            screen,
            false,
            expired_move,
            now
        ));
        assert!(!super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 721.0)),
            screen,
            false,
            recent_move,
            now
        ));
    }

    #[test]
    fn pointer_active_inside_window_tracks_hover_and_idle_timeout() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0));
        let now = std::time::Instant::now();
        let recent_move = now - std::time::Duration::from_secs(1);
        let idle_move = now - super::CONTROLS_IDLE_HIDE_AFTER - std::time::Duration::from_millis(1);

        assert!(super::pointer_active_inside_window(
            Some(egui::pos2(640.0, 360.0)),
            screen,
            recent_move,
            now
        ));
        assert!(!super::pointer_active_inside_window(
            Some(egui::pos2(640.0, 360.0)),
            screen,
            idle_move,
            now
        ));
        assert!(!super::pointer_active_inside_window(
            Some(egui::pos2(640.0, 721.0)),
            screen,
            recent_move,
            now
        ));
        assert!(!super::pointer_active_inside_window(
            None,
            screen,
            recent_move,
            now
        ));
    }

    #[test]
    fn current_overlay_hover_refreshes_pointer_activity_after_idle_timeout() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0));
        let now = std::time::Instant::now();
        let mut last_pointer_move =
            now - super::CONTROLS_IDLE_HIDE_AFTER - std::time::Duration::from_millis(1);

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
            true,
            Some(egui::pos2(640.0, 680.0)),
            screen,
            false,
            last_pointer_move,
            now
        ));
    }

    #[test]
    fn current_overlay_non_hover_does_not_refresh_pointer_activity() {
        let now = std::time::Instant::now();
        let idle_move = now - super::CONTROLS_IDLE_HIDE_AFTER - std::time::Duration::from_millis(1);
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
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0));
        let now = std::time::Instant::now();
        let recheck_move =
            now - super::CONTROLS_IDLE_HIDE_AFTER - std::time::Duration::from_millis(1);
        let expired_move = now
            - super::CONTROLS_IDLE_HIDE_AFTER
            - super::OVERLAY_HOVER_RECHECK_GRACE
            - std::time::Duration::from_millis(1);

        assert!(super::pointer_visible_for_overlay_recheck(
            Some(egui::pos2(640.0, 680.0)),
            screen,
            recheck_move,
            now
        ));
        assert!(!super::pointer_visible_for_overlay_recheck(
            None,
            screen,
            recheck_move,
            now
        ));
        assert!(!super::pointer_visible_for_overlay_recheck(
            Some(egui::pos2(640.0, 721.0)),
            screen,
            recheck_move,
            now
        ));
        assert!(!super::pointer_visible_for_overlay_recheck(
            Some(egui::pos2(640.0, 680.0)),
            screen,
            expired_move,
            now
        ));
    }

    #[test]
    fn overlay_hover_uses_current_area_response_not_stored_rects() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(!source.contains("playlist_sheet_rect"));
        assert!(!source.contains("floating_controls_rect"));
        assert!(source.contains("playlist_area.inner || playlist_area.response.hovered()"));
        assert!(source
            .contains("floating_controls_area.inner || floating_controls_area.response.hovered()"));
        assert!(source.matches(".fade_in(false)").count() >= 2);
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
    fn playlist_sheet_overlays_without_resizing_video() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("show_playlist"));
        assert!(source.contains("Area::new(egui::Id::new(\"playlist_sheet\"))"));
        assert!(source.contains(".fixed_pos(playlist_sheet_pos"));
        assert!(source.contains("egui::Order::Foreground"));
        assert!(!source.contains("Panel::right(\"playlist\")"));
        assert!(!source.contains("Panel::left(\"playlist\")"));
        assert!(source.contains("self.show_playlist = !self.show_playlist"));
    }

    #[test]
    fn playlist_is_closed_by_default_on_startup() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("show_playlist: false"));
        assert!(!source.contains("show_playlist: true"));
    }

    #[test]
    fn playlist_sheet_floats_as_rounded_card_above_controls() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        let playlist_source = source
            .split("egui::Area::new(egui::Id::new(\"playlist_sheet\"))")
            .nth(1)
            .unwrap()
            .split("let pointer_pos")
            .next()
            .unwrap();

        assert!(source.contains("screen_rect.min.y + crate::visuals::FLOATING_PANEL_MARGIN"));
        assert!(source
            .contains("screen_rect.max.x - sheet_width - crate::visuals::FLOATING_PANEL_MARGIN"));
        assert!(playlist_source.contains("crate::visuals::frosted_frame"));
    }

    #[test]
    fn overlay_panels_use_frosted_opaque_frame() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        let visuals_source = include_str!("visuals.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("crate::visuals::frosted_frame"));
        assert!(visuals_source.contains("frosted_frame"));
        assert!(visuals_source.contains("frosted_popup_style"));
        assert!(visuals_source.contains("from_rgba_unmultiplied"));
        assert!(visuals_source.contains(", 255"));
        assert!(!source.contains("multiply_with_opacity(0.9)"));
    }

    #[test]
    fn app_uses_native_titlebar_and_bottom_actions() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(!source.contains("crate::titlebar"));
        assert!(!source.contains("show_custom_titlebar"));
        assert!(source.contains("self.show_playlist = !self.show_playlist"));
        assert!(source.contains("self.show_settings = !self.show_settings"));
    }

    #[test]
    fn native_window_uses_panel_clear_color() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("fn clear_color"));
        assert!(source.contains("_visuals.panel_fill"));
    }

    #[test]
    fn settings_and_playlist_buttons_live_in_bottom_controls() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("Button::new(\"⚙\")"));
        assert!(source.contains("selected(self.show_settings)"));
        assert!(source.contains("Button::new(\"☰\")"));
        assert!(source.contains(".selected(self.show_playlist)"));
        assert!(source.contains("Layout::right_to_left(egui::Align::Center)"));
    }

    #[test]
    fn continuous_repaint_is_only_requested_when_needed() {
        assert!(!super::should_request_continuous_repaint(false, false));
        assert!(super::should_request_continuous_repaint(true, false));
        assert!(!super::should_request_continuous_repaint(true, true));
        assert!(!super::should_request_continuous_repaint(false, true));

        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        assert!(source.contains("if should_request_continuous_repaint"));
        assert!(source.contains("ctx.request_repaint_after"));
        assert!(source.contains("should_request_continuous_repaint"));
        assert!(source.contains("self.shortcut_notice.is_some()"));
        assert!(source.contains("i.pointer.any_down()"));
        assert!(source.contains("take_frame_pending"));
    }

    #[test]
    fn command_or_ctrl_arrows_navigate_playlist() {
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let alt = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            super::navigation_shortcut_command(
                crate::shortcuts::ShortcutPlatform::Macos,
                command,
                true,
                false
            ),
            Some(player_core::Command::Prev)
        );
        assert_eq!(
            super::navigation_shortcut_command(
                crate::shortcuts::ShortcutPlatform::Macos,
                command,
                false,
                true
            ),
            Some(player_core::Command::Next)
        );
        assert_eq!(
            super::navigation_shortcut_command(
                crate::shortcuts::ShortcutPlatform::Windows,
                alt,
                false,
                true
            ),
            Some(player_core::Command::Next)
        );
        assert_eq!(
            super::navigation_shortcut_command(
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
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };

        assert!(super::settings_shortcut_pressed(command, true));
        assert!(!super::settings_shortcut_pressed(command, false));
        assert!(!super::settings_shortcut_pressed(Default::default(), true));

        let mut show_settings = false;
        assert!(super::toggle_settings_with_shortcut(
            &mut show_settings,
            command,
            true
        ));
        assert!(show_settings);

        assert!(super::toggle_settings_with_shortcut(
            &mut show_settings,
            command,
            true
        ));
        assert!(!show_settings);
    }

    #[test]
    fn command_or_ctrl_p_toggles_playlist_and_initializes_candidate() {
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let mut show_playlist = false;
        let mut candidate = None;

        assert!(super::toggle_playlist_with_shortcut(
            &mut show_playlist,
            &mut candidate,
            command,
            true,
            Some(2),
            4
        ));
        assert!(show_playlist);
        assert_eq!(candidate, Some(2));

        assert!(super::toggle_playlist_with_shortcut(
            &mut show_playlist,
            &mut candidate,
            command,
            true,
            Some(2),
            4
        ));
        assert!(!show_playlist);
        assert_eq!(candidate, None);
    }

    #[test]
    fn playlist_candidate_moves_with_bounds() {
        assert_eq!(super::playlist_candidate_for_open(Some(2), 4), Some(2));
        assert_eq!(super::playlist_candidate_for_open(None, 4), Some(0));
        assert_eq!(super::playlist_candidate_for_open(Some(9), 4), Some(3));
        assert_eq!(super::playlist_candidate_for_open(None, 0), None);

        assert_eq!(super::move_playlist_candidate(Some(1), 4, -1), Some(0));
        assert_eq!(super::move_playlist_candidate(Some(1), 4, 1), Some(2));
        assert_eq!(super::move_playlist_candidate(Some(0), 4, -1), Some(0));
        assert_eq!(super::move_playlist_candidate(Some(3), 4, 1), Some(3));
        assert_eq!(super::move_playlist_candidate(None, 4, 1), Some(0));
        assert_eq!(super::move_playlist_candidate(None, 0, 1), None);
    }

    #[test]
    fn deleting_candidate_clamps_to_remaining_items() {
        assert_eq!(super::candidate_after_remove(Some(1), 4), Some(1));
        assert_eq!(super::candidate_after_remove(Some(3), 4), Some(2));
        assert_eq!(super::candidate_after_remove(Some(0), 1), None);
        assert_eq!(super::candidate_after_remove(None, 4), Some(0));
        assert_eq!(super::candidate_after_remove(None, 0), None);
    }

    #[test]
    fn escape_priority_closes_panels_before_exiting_fullscreen() {
        assert_eq!(
            super::escape_shortcut_action(true, true, true),
            super::EscapeShortcutAction::CloseSettings
        );
        assert_eq!(
            super::escape_shortcut_action(false, true, true),
            super::EscapeShortcutAction::ClosePlaylist
        );
        assert_eq!(
            super::escape_shortcut_action(false, false, true),
            super::EscapeShortcutAction::ExitFullscreen
        );
        assert_eq!(
            super::escape_shortcut_action(false, false, false),
            super::EscapeShortcutAction::None
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
            super::rate_shortcut_command(
                crate::shortcuts::ShortcutPlatform::Macos,
                command,
                true,
                false,
                100
            ),
            Some(player_core::Command::SetRate(125))
        );
        assert_eq!(
            super::rate_shortcut_command(
                crate::shortcuts::ShortcutPlatform::Linux,
                alt,
                false,
                true,
                100
            ),
            Some(player_core::Command::SetRate(75))
        );
        assert_eq!(
            super::rate_shortcut_command(
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
    fn app_handles_escape_playlist_enter_and_extra_single_key_shortcuts() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("key_pressed(egui::Key::Escape)"));
        assert!(source.contains("key_pressed(egui::Key::P)"));
        assert!(source.contains("key_pressed(egui::Key::F)"));
        assert!(source.contains("key_pressed(egui::Key::M)"));
        assert!(source.contains("player_core::Command::ToggleMute"));
        assert!(source.contains("player_core::Command::PlayIndex(candidate)"));
        assert!(source.contains("self.show_playlist = false"));
    }

    #[test]
    fn app_handles_delete_and_backspace_for_playlist_and_history_candidates() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("key_pressed(egui::Key::Delete)"));
        assert!(source.contains("key_pressed(egui::Key::Backspace)"));
        assert!(source.contains("player_core::Command::RemovePlaylistIndex(candidate)"));
        assert!(source.contains("player_core::Command::RemoveHistoryIndex(candidate)"));
        assert!(source.contains("self.history_candidate"));
        assert!(source.contains("candidate_after_remove"));
    }

    #[test]
    fn s_key_requests_screenshot_from_keyboard_shortcut_path() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("key_pressed(egui::Key::S)"));
        assert!(source.contains("keyboard_screenshot_requested = true"));
        assert!(source.contains("let mut screenshot_requested = keyboard_screenshot_requested"));
    }

    #[test]
    fn command_or_ctrl_o_opens_multi_file_picker() {
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };

        assert!(super::open_shortcut_pressed(command, true));
        assert!(!super::open_shortcut_pressed(command, false));
        assert!(!super::open_shortcut_pressed(Default::default(), true));

        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        assert!(source.contains("key_pressed(egui::Key::O)"));
        assert!(source.contains("open_shortcut_pressed(modifiers, o_key)"));
        assert!(source.contains(".pick_files()"));
        assert!(source.contains("player_core::Command::OpenFiles(paths)"));
        assert!(!source.contains(".pick_file()"));
        assert!(
            source.contains("let opened = self.handle_command(player_core::Command::OpenDialog)")
        );
        assert!(source.contains("opened_playlist_name_after_shortcut"));
    }

    #[test]
    fn open_shortcut_notice_requires_dialog_selection() {
        assert_eq!(
            super::opened_playlist_name_after_shortcut(
                true,
                Some(std::path::PathBuf::from("/tmp/a.mp4"))
            ),
            Some("a.mp4".to_string())
        );
        assert_eq!(
            super::opened_playlist_name_after_shortcut(
                false,
                Some(std::path::PathBuf::from("/tmp/a.mp4"))
            ),
            None
        );
        assert_eq!(super::opened_playlist_name_after_shortcut(true, None), None);
    }

    #[test]
    fn app_tooltips_include_shortcut_descriptions_for_panel_actions() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

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
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("key_pressed(egui::Key::Enter)"));
        assert!(source.contains("controls::toggle_fullscreen(&ctx)"));
    }

    #[test]
    fn playlist_navigation_availability_tracks_current_index() {
        assert!(!super::playlist_has_prev(0, None));
        assert!(!super::playlist_has_next(0, None));
        assert!(!super::playlist_has_prev(3, Some(0)));
        assert!(super::playlist_has_next(3, Some(0)));
        assert!(super::playlist_has_prev(3, Some(1)));
        assert!(super::playlist_has_next(3, Some(1)));
        assert!(super::playlist_has_prev(3, Some(2)));
        assert!(!super::playlist_has_next(3, Some(2)));
    }

    #[test]
    fn app_keeps_update_flow_inside_settings() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

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
}
