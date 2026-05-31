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
pub const PLAYLIST_TOGGLE_ICON: &str = "☰";

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
}

impl PlayerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::font::install_fonts(&cc.egui_ctx);
        Self {
            player: Player::with_prefs(prefs_path()),
            video_view: VideoView::new(),
            show_settings: false,
            show_playlist: true,
            sidebar_tab: SidebarTab::Playlist,
            screenshot_notice: None,
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

fn playlist_toggle_button(ui: &mut egui::Ui, show_playlist: bool) -> bool {
    ui.add(egui::Button::new(PLAYLIST_TOGGLE_ICON).selected(show_playlist))
        .on_hover_text(t!("playlist").to_string())
        .clicked()
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 每帧应用语言与主题偏好(幂等), 保证设置窗口切换后立即生效。
        rust_i18n::set_locale(&self.player.prefs().language);
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
        let t = self.player.timeline();

        // 键盘快捷键: 空格=播放/暂停, ↑↓=音量(吸附5), ←→=按步长 seek。
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
            if left {
                self.player
                    .handle(player_core::Command::SeekTo(pos.saturating_sub(step_ms)));
            }
            if right {
                let target = if dur > 0 {
                    (pos + step_ms).min(dur)
                } else {
                    pos + step_ms
                };
                self.player.handle(player_core::Command::SeekTo(target));
            }
        }

        let mut bottom_commands = Vec::new();
        let mut screenshot_notice_pos = None;
        let mut toggle_settings = false;
        let mut toggle_playlist = false;
        let tracks = self.player.subtitle_tracks().to_vec();
        egui::Panel::bottom("controls").show_inside(ui, |ui| {
            egui::containers::Sides::new().shrink_left().show(
                ui,
                |ui| {
                    ui.horizontal_wrapped(|ui| {
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
                            if let Some(cmd) = controls::subtitle_track_combo(ui, &tracks) {
                                bottom_commands.push(cmd);
                            }
                        }
                    });
                },
                |ui| {
                    if ui
                        .button("⚙")
                        .on_hover_text(t!("settings").to_string())
                        .clicked()
                    {
                        toggle_settings = true;
                    }
                    if playlist_toggle_button(ui, self.show_playlist) {
                        toggle_playlist = true;
                    }
                },
            );
        });
        if toggle_playlist {
            self.show_playlist = !self.show_playlist;
        }
        if toggle_settings {
            self.show_settings = !self.show_settings;
        }
        for cmd in bottom_commands {
            self.handle_command(cmd);
        }
        if let Some(notice_pos) = screenshot_notice_pos {
            if let Some((rgba, w, h)) = self.video_view.last_frame() {
                match crate::enhance::save_screenshot(rgba, w, h) {
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

        if self.show_playlist {
            egui::Panel::right("playlist")
                .default_size(240.0)
                .min_size(crate::playlist_panel::PLAYLIST_MIN_WIDTH)
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        for cmd in crate::playlist_panel::open_menu_button(ui) {
                            self.handle_command(cmd);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        });
                    });
                    ui.separator();
                    match self.sidebar_tab {
                        SidebarTab::Playlist => {
                            let paths = self.player.playlist_paths();
                            let cur = self.player.current_index();
                            for cmd in crate::playlist_panel::playlist_panel(ui, &paths, cur) {
                                self.handle_command(cmd);
                            }
                        }
                        SidebarTab::History => {
                            let hist: Vec<std::path::PathBuf> =
                                self.player.history().iter().map(Into::into).collect();
                            if let Some(cmd) = crate::playlist_panel::history_panel(ui, &hist) {
                                self.handle_command(cmd);
                            }
                        }
                    }
                });
        }

        let mut video_commands = Vec::new();
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.set_min_width(VIDEO_MIN_WIDTH);
            video_commands = self.video_view.show(ui, frame, &self.player);
        });
        for cmd in video_commands {
            self.handle_command(cmd);
        }

        crate::settings::settings_window(&ctx, &mut self.show_settings, &mut self.player);
        self.show_screenshot_notice(&ctx);

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.player.save_state();
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
    fn screenshot_notice_is_near_button_and_includes_path() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        assert!(!source.contains("Align2::RIGHT_BOTTOM"));
        assert!(source.contains("notice_pos"));
        assert!(source.contains("p.display()"));
    }

    #[test]
    fn playlist_panel_is_right_side_and_toggleable_from_bottom_bar() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();

        assert!(source.contains("show_playlist"));
        assert!(source.contains("Panel::right(\"playlist\")"));
        assert!(!source.contains("Panel::left(\"playlist\")"));
        assert!(source.contains("playlist_toggle_button"));
        assert!(source.contains("Sides::new().shrink_left()"));
        assert!(source.contains("self.show_playlist = !self.show_playlist"));
    }

    #[test]
    fn settings_button_is_right_of_playlist_toggle() {
        let source = include_str!("app.rs").split("#[cfg(test)]").next().unwrap();
        let settings_idx = source.find(".button(\"⚙\")").unwrap();
        let playlist_idx = source
            .find("playlist_toggle_button(ui, self.show_playlist)")
            .unwrap();

        assert!(
            settings_idx < playlist_idx,
            "right-to-left bottom controls draw the first button at the far right"
        );
    }
}
