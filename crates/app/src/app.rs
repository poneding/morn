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
const CONTROL_BAR_SIDE_MARGIN: f32 = 8.0;
const PLAYLIST_TITLEBAR_HEIGHT: f32 = 28.0;
const PLAYLIST_BOTTOM_CONTROLS_RESERVED_HEIGHT: f32 = 60.0;

struct ScreenshotNotice {
    message: String,
    created: std::time::Instant,
    pos: egui::Pos2,
}

pub struct PlayerApp {
    player: Player,
    video_view: VideoView,
    show_settings: bool,
    show_playlist: bool,
    sidebar_tab: SidebarTab,
    screenshot_notice: Option<ScreenshotNotice>,
    font_locale: String,
    #[cfg(not(target_os = "macos"))]
    show_update_window: bool,
    update_check: crate::updater::UpdateChecker,
    #[cfg(target_os = "macos")]
    native_window_configured: bool,
}

impl PlayerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut player = Player::with_prefs(prefs_path());
        player.restore_last_session_paused();
        let font_locale = player.prefs().language.clone();
        rust_i18n::set_locale(&font_locale);
        crate::font::install_fonts(&cc.egui_ctx, &font_locale);
        #[cfg(target_os = "macos")]
        crate::macos::install_check_update_menu_item(t!("check_updates").as_ref());
        let mut update_check = crate::updater::UpdateChecker::new(env!("CARGO_PKG_VERSION"));
        let check_updates_on_startup = player.prefs().check_updates_on_startup;
        if check_updates_on_startup {
            update_check.begin(player.prefs().check_beta_updates);
        }
        #[cfg(not(target_os = "macos"))]
        let show_update_window = check_updates_on_startup;
        Self {
            player,
            video_view: VideoView::new(),
            show_settings: false,
            show_playlist: true,
            sidebar_tab: SidebarTab::Playlist,
            screenshot_notice: None,
            font_locale,
            #[cfg(not(target_os = "macos"))]
            show_update_window,
            update_check,
            #[cfg(target_os = "macos")]
            native_window_configured: false,
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
        let pos = notice.pos;
        egui::Area::new(egui::Id::new("screenshot_notice"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.label(message);
                });
            });
    }

    fn set_screenshot_notice(&mut self, message: String, notice_pos: egui::Pos2) {
        self.screenshot_notice = Some(ScreenshotNotice {
            message,
            created: std::time::Instant::now(),
            pos: notice_pos,
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

    fn begin_update_check(&mut self) {
        self.update_check
            .begin(self.player.prefs().check_beta_updates);
        #[cfg(target_os = "macos")]
        crate::macos::show_update_check_started();
        #[cfg(not(target_os = "macos"))]
        {
            self.show_update_window = true;
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
) -> bool {
    !has_media || screenshot_notice_visible || pointer_inside_window(pointer_pos, screen_rect)
}

fn pointer_inside_window(pointer_pos: Option<egui::Pos2>, screen_rect: egui::Rect) -> bool {
    pointer_pos.is_some_and(|pos| screen_rect.contains(pos))
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

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 每帧应用语言与主题偏好(幂等), 保证设置窗口切换后立即生效。
        let language = self.player.prefs().language.clone();
        rust_i18n::set_locale(&language);
        if self.font_locale != language {
            crate::font::install_fonts(&ctx, &language);
            self.font_locale = language;
            #[cfg(target_os = "macos")]
            crate::macos::install_check_update_menu_item(t!("check_updates").as_ref());
        }
        ctx.set_theme(theme_preference(&self.player.prefs().theme));
        crate::titlebar::paint_window_background(&ctx);

        #[cfg(target_os = "macos")]
        if !self.native_window_configured {
            self.native_window_configured = crate::macos::configure_frameless_window_appearance();
        }

        #[cfg(target_os = "macos")]
        if crate::macos::take_check_update_request() {
            self.begin_update_check();
        }

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
        let t = self.player.timeline();

        let modifiers = ctx.input(|i| i.modifiers);
        let comma = ctx.input(|i| i.key_pressed(egui::Key::Comma));
        open_settings_with_shortcut(&mut self.show_settings, modifiers, comma);

        // 键盘快捷键: Cmd/Ctrl+,=设置, 空格=播放/暂停, ↑↓=音量(吸附5), ←→=按步长 seek。
        if !ctx.egui_wants_keyboard_input() {
            let step_ms = self.player.prefs().seek_step_secs * 1000;
            let pos = t.position_ms;
            let dur = t.duration_ms;
            let space = ctx.input(|i| i.key_pressed(egui::Key::Space));
            let up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let down = ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));
            let left = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft));
            let right = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight));
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

        let mut video_rect = ctx.content_rect();
        let mut video_commands = Vec::new();
        egui::CentralPanel::no_frame().show_inside(ui, |ui| {
            ui.set_min_width(VIDEO_MIN_WIDTH);
            video_rect = ui.available_rect_before_wrap();
            video_commands = self.video_view.show(ui, frame, &self.player);
        });
        for cmd in video_commands {
            self.handle_command(cmd);
        }

        let screen_rect = ctx.content_rect();
        if self.show_playlist {
            let sheet_width = playlist_sheet_width(screen_rect.width());
            let playlist_sheet_pos = egui::pos2(
                screen_rect.max.x - sheet_width - CONTROL_BAR_SIDE_MARGIN,
                screen_rect.min.y + PLAYLIST_TITLEBAR_HEIGHT + CONTROL_BAR_SIDE_MARGIN,
            );
            let playlist_sheet_height = (screen_rect.max.y
                - playlist_sheet_pos.y
                - PLAYLIST_BOTTOM_CONTROLS_RESERVED_HEIGHT)
                .max(0.0);
            let paths = self.player.playlist_paths();
            let cur = self.player.current_index();
            let hist: Vec<std::path::PathBuf> =
                self.player.history().iter().map(Into::into).collect();
            let mut playlist_commands = Vec::new();

            egui::Area::new(egui::Id::new("playlist_sheet"))
                .fixed_pos(playlist_sheet_pos)
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    ui.set_width(sheet_width);
                    ui.set_height(playlist_sheet_height);
                    egui::Frame::NONE
                        .fill(ui.visuals().panel_fill)
                        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .multiply_with_opacity(0.9)
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
                                        crate::playlist_panel::playlist_panel(ui, &paths, cur),
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
                });

            for cmd in playlist_commands {
                self.handle_command(cmd);
            }
        }

        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let has_media = self.player.video().is_some() || t.duration_ms > 0;
        let controls_visible = bottom_controls_visible(
            has_media,
            pointer_pos,
            screen_rect,
            self.screenshot_notice.is_some(),
        );

        let mut bottom_commands = Vec::new();
        let mut screenshot_notice_pos = None;
        let tracks = self.player.subtitle_tracks().to_vec();
        if controls_visible {
            let control_width = (video_rect.width() - CONTROL_BAR_SIDE_MARGIN * 2.0).max(0.0);
            egui::Area::new(egui::Id::new("floating_controls"))
                .anchor(
                    egui::Align2::CENTER_BOTTOM,
                    egui::vec2(0.0, -CONTROL_BAR_SIDE_MARGIN),
                )
                .order(egui::Order::Foreground)
                .show(&ctx, |ui| {
                    ui.set_max_width(control_width);
                    egui::Frame::NONE
                        .fill(ui.visuals().panel_fill)
                        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .multiply_with_opacity(0.9)
                        .show(ui, |ui| {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    bottom_commands.extend(controls::controls_bar(ui, &t));

                                    let actions = crate::enhance::enhance_bar(ui, t.rate_pct);
                                    bottom_commands.extend(actions.commands);
                                    if actions.screenshot {
                                        screenshot_notice_pos = Some(
                                            actions
                                                .screenshot_notice_pos
                                                .unwrap_or(egui::pos2(16.0, 16.0)),
                                        );
                                    }

                                    if !tracks.is_empty() {
                                        if let Some(cmd) =
                                            controls::subtitle_track_combo(ui, &tracks)
                                        {
                                            bottom_commands.push(cmd);
                                        }
                                    }
                                },
                            );
                        });
                });
        }
        for cmd in bottom_commands {
            self.handle_command(cmd);
        }
        if let Some(notice_pos) = screenshot_notice_pos {
            if let Some((rgba, w, h)) = self.video_view.last_frame() {
                let screenshot_dir = std::path::Path::new(&self.player.prefs().screenshot_dir);
                match crate::enhance::save_screenshot(rgba, w, h, screenshot_dir) {
                    Ok(p) => {
                        eprintln!("截图已保存: {}", p.display());
                        self.set_screenshot_notice(
                            format!("{}: {}", t!("screenshot_saved"), p.display()),
                            notice_pos,
                        );
                    }
                    Err(e) => {
                        eprintln!("截图失败: {e}");
                        self.set_screenshot_notice(
                            format!("{}: {e}", t!("screenshot_failed")),
                            notice_pos,
                        );
                    }
                }
            } else {
                self.set_screenshot_notice(t!("screenshot_no_frame").to_string(), notice_pos);
            }
        }

        let titlebar_actions = crate::titlebar::show_custom_titlebar(&ctx, self.show_playlist);
        if titlebar_actions.toggle_playlist {
            self.show_playlist = !self.show_playlist;
        }
        if titlebar_actions.toggle_settings {
            self.show_settings = !self.show_settings;
        }
        crate::settings::settings_window(&ctx, &mut self.show_settings, &mut self.player);
        #[cfg(target_os = "macos")]
        if let Some(status) = self.update_check.take_finished_status() {
            crate::macos::show_update_result(&status);
        }
        #[cfg(not(target_os = "macos"))]
        crate::updater::update_window(
            &ctx,
            &mut self.show_update_window,
            &mut self.update_check,
            self.player.prefs().check_beta_updates,
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
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
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
    fn screenshot_notice_is_near_button_and_includes_path() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        assert!(!source.contains("Align2::RIGHT_BOTTOM"));
        assert!(source.contains("notice_pos"));
        assert!(source.contains("p.display()"));
    }

    #[test]
    fn screenshots_are_saved_under_configured_directory() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("prefs().screenshot_dir"));
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
    fn bottom_controls_are_centered_and_exclude_titlebar_actions() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("Layout::left_to_right(egui::Align::Center)"));
        assert!(source.contains("set_max_width(control_width)"));
        assert!(!source.contains("set_width(control_width)"));
        assert!(!source.contains("playlist_toggle_button"));
        assert!(!source.contains(".button(\"⚙\")"));
    }

    #[test]
    fn bottom_controls_use_content_height_and_hide_immediately() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(!source.contains("CONTROL_BAR_HEIGHT"));
        assert!(!source.contains("CONTROL_BAR_HOLD"));
        assert!(!source.contains("controls_keep_visible_until"));
        assert!(!source.contains("set_min_height(CONTROL_BAR_HEIGHT)"));
        assert!(!source.contains("is_pointer_over_egui"));
    }

    #[test]
    fn bottom_controls_are_centered_on_window_not_constrained_by_playlist() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("egui::vec2(0.0, -CONTROL_BAR_SIDE_MARGIN)"));
        assert!(!source.contains("sheet_reserved_width"));
        assert!(!source.contains("control_rect"));
    }

    #[test]
    fn bottom_controls_visibility_rule_uses_whole_window() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0));

        assert!(super::bottom_controls_visible(false, None, screen, false));
        assert!(super::bottom_controls_visible(true, None, screen, true));
        assert!(super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 700.0)),
            screen,
            false
        ));
        assert!(super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 500.0)),
            screen,
            false
        ));
        assert!(!super::bottom_controls_visible(
            true,
            Some(egui::pos2(640.0, 721.0)),
            screen,
            false
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
    fn playlist_sheet_floats_as_rounded_card_between_titlebar_and_controls() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        let titlebar_source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let playlist_source = source
            .split("egui::Area::new(egui::Id::new(\"playlist_sheet\"))")
            .nth(1)
            .unwrap()
            .split("let pointer_pos")
            .next()
            .unwrap();

        assert_eq!(super::PLAYLIST_TITLEBAR_HEIGHT, 28.0);
        assert!(titlebar_source.contains("const TITLEBAR_HEIGHT: f32 = 28.0;"));
        assert!(source
            .contains("screen_rect.min.y + PLAYLIST_TITLEBAR_HEIGHT + CONTROL_BAR_SIDE_MARGIN"));
        assert!(source.contains("screen_rect.max.x - sheet_width - CONTROL_BAR_SIDE_MARGIN"));
        assert!(playlist_source.contains(".corner_radius(6.0)"));
    }

    #[test]
    fn app_draws_auto_hiding_custom_titlebar() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("crate::titlebar::paint_window_background"));
        assert!(source.contains("crate::titlebar::show_custom_titlebar(&ctx, self.show_playlist)"));
        assert!(source.contains("titlebar_actions.toggle_playlist"));
        assert!(source.contains("titlebar_actions.toggle_settings"));
    }

    #[test]
    fn transparent_window_uses_transparent_clear_color() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("fn clear_color"));
        assert!(source.contains("Color32::TRANSPARENT"));
    }

    #[test]
    fn settings_and_playlist_buttons_live_in_titlebar() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        let titlebar_source = include_str!("titlebar.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(!source.contains(".button(\"⚙\")"));
        assert!(!source.contains("playlist_toggle_button"));
        assert!(titlebar_source.contains("Button::new(\"☰\")"));
        assert!(titlebar_source.contains("Button::new(\"⚙\")"));
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
    fn app_opens_update_window_from_macos_menu_and_startup_preference() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("show_update_window"));
        assert!(source.contains("update_check"));
        assert!(source.contains("check_updates_on_startup"));
        assert!(source.contains("check_beta_updates"));
        assert!(source.contains("take_check_update_request"));
        assert!(source.contains("update_window"));
    }

    #[test]
    fn macos_update_feedback_uses_native_alerts() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("show_update_check_started"));
        assert!(source.contains("show_update_result"));
        assert!(source.contains("cfg(not(target_os = \"macos\"))"));
    }
}
