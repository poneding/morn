use eframe::egui;
use engine::{PlaybackMode, Player};
use rust_i18n::t;

/// 绘制设置窗口。`open` 控制显隐, 直接读写 player 的偏好。
pub fn settings_window(ctx: &egui::Context, open: &mut bool, player: &mut Player) {
    egui::Window::new(t!("settings").to_string())
        .open(open)
        .resizable(false)
        .show(ctx, |ui| {
            // 外观
            ui.heading(t!("appearance").to_string());
            settings_row(ui, t!("language").to_string(), |ui| {
                let mut lang = player.prefs().language.clone();
                egui::ComboBox::from_id_salt("lang")
                    .selected_text(lang_label(&lang))
                    .show_ui(ui, |ui| {
                        for (code, label) in [
                            ("zh-CN", "简体中文"),
                            ("zh-TW", "繁體中文"),
                            ("en", "English"),
                        ] {
                            ui.selectable_value(&mut lang, code.to_string(), label);
                        }
                    });
                if lang != player.prefs().language {
                    player.set_language(&lang);
                }
            });
            settings_row(ui, t!("theme").to_string(), |ui| {
                let mut theme = player.prefs().theme.clone();
                egui::ComboBox::from_id_salt("theme")
                    .selected_text(theme_label(&theme))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut theme,
                            "dark".to_string(),
                            t!("theme_dark").to_string(),
                        );
                        ui.selectable_value(
                            &mut theme,
                            "light".to_string(),
                            t!("theme_light").to_string(),
                        );
                        ui.selectable_value(
                            &mut theme,
                            "system".to_string(),
                            t!("theme_system").to_string(),
                        );
                    });
                if theme != player.prefs().theme {
                    player.set_theme(&theme);
                }
            });
            ui.separator();
            // 播放
            ui.heading(t!("playback").to_string());
            settings_row(ui, t!("seek_step").to_string(), |ui| {
                let mut step = player.prefs().seek_step_secs;
                egui::ComboBox::from_id_salt("seek_step")
                    .selected_text(format!("{} {}", step, t!("seconds")))
                    .show_ui(ui, |ui| {
                        for s in [5u64, 10, 20, 30] {
                            ui.selectable_value(&mut step, s, format!("{} {}", s, t!("seconds")));
                        }
                    });
                if step != player.prefs().seek_step_secs {
                    player.set_seek_step(step);
                }
            });
            settings_row(ui, t!("playback_mode").to_string(), |ui| {
                let mut mode = player.prefs().playback_mode;
                egui::ComboBox::from_id_salt("playback_mode")
                    .selected_text(playback_mode_label(mode))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut mode,
                            PlaybackMode::StopAtEnd,
                            t!("playback_stop_at_end").to_string(),
                        );
                        ui.selectable_value(
                            &mut mode,
                            PlaybackMode::LoopPlaylist,
                            t!("playback_loop_playlist").to_string(),
                        );
                        ui.selectable_value(
                            &mut mode,
                            PlaybackMode::RepeatOne,
                            t!("playback_repeat_one").to_string(),
                        );
                    });
                if mode != player.prefs().playback_mode {
                    player.set_playback_mode(mode);
                }
            });
            ui.separator();
            // 字幕
            ui.heading(t!("subtitle").to_string());
            settings_row(ui, t!("subtitle_size").to_string(), |ui| {
                let mut size = player.prefs().subtitle_font_size;
                if ui.add(egui::Slider::new(&mut size, 12.0..=48.0)).changed() {
                    player.set_subtitle_font_size(size);
                }
            });
        });
}

fn settings_row(ui: &mut egui::Ui, label: String, add_value: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            add_value(ui);
        });
    });
}

fn lang_label(code: &str) -> &'static str {
    match code {
        "zh-TW" => "繁體中文",
        "en" => "English",
        _ => "简体中文",
    }
}
fn theme_label(code: &str) -> String {
    match code {
        "dark" => t!("theme_dark").to_string(),
        "light" => t!("theme_light").to_string(),
        _ => t!("theme_system").to_string(),
    }
}

fn playback_mode_label(mode: PlaybackMode) -> String {
    match mode {
        PlaybackMode::StopAtEnd => t!("playback_stop_at_end").to_string(),
        PlaybackMode::LoopPlaylist => t!("playback_loop_playlist").to_string(),
        PlaybackMode::RepeatOne => t!("playback_repeat_one").to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn settings_exposes_playback_mode() {
        let source = include_str!("settings.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("playback_mode"));
        assert!(source.contains("LoopPlaylist"));
        assert!(source.contains("RepeatOne"));
    }

    #[test]
    fn settings_rows_keep_labels_left_and_values_right() {
        let source = include_str!("settings.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("fn settings_row"));
        assert!(source.contains("right_to_left(egui::Align::Center)"));
        assert!(!source.contains("ui.horizontal(|ui| {\n                ui.label(t!(\"language\")"));
    }
}
