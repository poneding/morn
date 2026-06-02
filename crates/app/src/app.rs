use crate::controls;
use crate::video_view::VideoView;
use eframe::egui;
use engine::Player;
use rust_i18n::t;

#[derive(PartialEq)]
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

struct ScreenshotNotice {
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

    fn handle_command(&mut self, cmd: player_core::Command) {
        match cmd {
            player_core::Command::OpenDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        t!("video_filter").to_string(),
                        &["mp4", "mkv", "webm", "mov", "avi"],
                    )
                    .pick_file()
                {
                    self.player.handle(player_core::Command::Open(path));
                }
            }
            player_core::Command::OpenFolder => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.player.open_folder(&dir);
                }
            }
            _ => self.player.handle(cmd),
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
    modifiers: egui::Modifiers,
    left_pressed: bool,
    right_pressed: bool,
) -> Option<player_core::Command> {
    if !modifiers.command {
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

fn settings_shortcut_pressed(modifiers: egui::Modifiers, comma_pressed: bool) -> bool {
    modifiers.command && comma_pressed
}

fn open_settings_with_shortcut(
    show_settings: &mut bool,
    modifiers: egui::Modifiers,
    comma_pressed: bool,
) {
    if settings_shortcut_pressed(modifiers, comma_pressed) {
        *show_settings = true;
    }
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
                match path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .as_deref()
                {
                    Some("srt") | Some("ass") | Some("ssa") => self.player.load_subtitle(&path),
                    _ => self.player.handle(player_core::Command::Open(path)),
                }
            }
        }

        self.player.tick();
        self.update_check.poll();
        let t = self.player.timeline();

        let modifiers = ctx.input(|i| i.modifiers);
        let comma = ctx.input(|i| i.key_pressed(egui::Key::Comma));
        open_settings_with_shortcut(&mut self.show_settings, modifiers, comma);

        // 键盘快捷键: Cmd/Ctrl+,=设置, Enter=全屏切换, 空格=播放/暂停, ↑↓=音量(吸附5), ←→=按步长 seek。
        if !ctx.egui_wants_keyboard_input() {
            let step_ms = self.player.prefs().seek_step_secs * 1000;
            let pos = t.position_ms;
            let dur = t.duration_ms;
            let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
            let space = ctx.input(|i| i.key_pressed(egui::Key::Space));
            let up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let down = ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));
            let left = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft));
            let right = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight));
            if enter {
                controls::toggle_fullscreen(&ctx);
            }
            if space {
                self.player
                    .handle(if t.state == player_core::PlaybackState::Playing {
                        player_core::Command::Pause
                    } else {
                        player_core::Command::Play
                    });
            }
            if up {
                self.player.handle(player_core::Command::SetVolume(
                    crate::shortcuts::snap_volume_up(t.volume),
                ));
            }
            if down {
                self.player.handle(player_core::Command::SetVolume(
                    crate::shortcuts::snap_volume_down(t.volume),
                ));
            }
            if let Some(cmd) = navigation_shortcut_command(modifiers, left, right) {
                self.player.handle(cmd);
            } else if left {
                self.player
                    .handle(player_core::Command::SeekTo(pos.saturating_sub(step_ms)));
            } else if right {
                let target = if dur > 0 {
                    (pos + step_ms).min(dur)
                } else {
                    pos + step_ms
                };
                self.player.handle(player_core::Command::SeekTo(target));
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
        let pointer_active =
            pointer_active_inside_window(pointer_pos, screen_rect, self.last_pointer_move, now);
        let pointer_overlay_recheck = pointer_visible_for_overlay_recheck(
            pointer_pos,
            screen_rect,
            self.last_pointer_move,
            now,
        );

        let playlist_paths = self.player.playlist_paths();
        let current_index = self.player.current_index();
        let has_prev = playlist_has_prev(playlist_paths.len(), current_index);
        let has_next = playlist_has_next(playlist_paths.len(), current_index);
        if self.show_playlist && (pointer_active || pointer_overlay_recheck) {
            let sheet_width = playlist_sheet_width(screen_rect.width());
            let playlist_sheet_pos = egui::pos2(
                screen_rect.max.x - sheet_width - crate::visuals::FLOATING_PANEL_MARGIN,
                screen_rect.min.y + crate::visuals::FLOATING_PANEL_MARGIN,
            );
            let playlist_sheet_height = (screen_rect.max.y
                - playlist_sheet_pos.y
                - PLAYLIST_BOTTOM_CONTROLS_RESERVED_HEIGHT)
                .max(0.0);
            let hist: Vec<std::path::PathBuf> =
                self.player.history().iter().map(Into::into).collect();
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
                            });
                            ui.separator();
                            match self.sidebar_tab {
                                SidebarTab::Playlist => {
                                    playlist_commands.extend(
                                        crate::playlist_panel::playlist_panel(
                                            ui,
                                            &playlist_paths,
                                            current_index,
                                        ),
                                    );
                                }
                                SidebarTab::History => {
                                    if let Some(cmd) =
                                        crate::playlist_panel::history_panel(ui, &hist)
                                    {
                                        playlist_commands.push(cmd);
                                    }
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
                self.handle_command(cmd);
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
        let mut screenshot_requested = false;
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
                                        .on_hover_text(t!("settings").to_string())
                                        .clicked()
                                    {
                                        self.show_settings = !self.show_settings;
                                    }
                                    if ui
                                        .add(egui::Button::new("☰").selected(self.show_playlist))
                                        .on_hover_text(t!("playlist").to_string())
                                        .clicked()
                                    {
                                        self.show_playlist = !self.show_playlist;
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

        if self.update_check.is_checking() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // 拖窗口边缘/拖滑块时让出 60Hz 主动重绘, 改由 OS Resized/Moved
        // 事件驱动, 避免 macOS 顶/左拉伸时 Moved 事件与 60Hz 重绘
        // 争抢合成带宽, 造成顶/左比底/右明显卡顿。
        let interacting = ctx.input(|i| i.pointer.any_down());
        if should_request_continuous_repaint(self.screenshot_notice.is_some(), interacting) {
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
        assert!(source.contains("i.pointer.any_down()"));
        assert!(source.contains("take_frame_pending"));
    }

    #[test]
    fn command_or_ctrl_arrows_navigate_playlist() {
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        assert_eq!(
            super::navigation_shortcut_command(command, true, false),
            Some(player_core::Command::Prev)
        );
        assert_eq!(
            super::navigation_shortcut_command(command, false, true),
            Some(player_core::Command::Next)
        );
        assert_eq!(
            super::navigation_shortcut_command(Default::default(), false, true),
            None
        );
    }

    #[test]
    fn command_or_ctrl_comma_opens_settings() {
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };

        assert!(super::settings_shortcut_pressed(command, true));
        assert!(!super::settings_shortcut_pressed(command, false));
        assert!(!super::settings_shortcut_pressed(Default::default(), true));

        let mut show_settings = false;
        super::open_settings_with_shortcut(&mut show_settings, command, true);
        assert!(show_settings);

        super::open_settings_with_shortcut(&mut show_settings, command, true);
        assert!(
            show_settings,
            "shortcut opens settings instead of toggling it"
        );
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
