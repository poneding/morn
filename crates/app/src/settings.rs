use eframe::egui;
use engine::Player;
use rust_i18n::t;

/// 绘制设置窗口。`open` 控制显隐, 直接读写 player 的偏好。
pub fn settings_window(ctx: &egui::Context, open: &mut bool, player: &mut Player) {
    egui::Window::new(t!("settings").to_string())
        .open(open)
        .resizable(false)
        .show(ctx, |ui| {
            // 外观
            ui.heading(t!("appearance").to_string());
            ui.horizontal(|ui| {
                ui.label(t!("language").to_string());
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
            ui.horizontal(|ui| {
                ui.label(t!("theme").to_string());
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
            ui.horizontal(|ui| {
                ui.label(t!("seek_step").to_string());
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
            ui.separator();
            // 字幕
            ui.heading(t!("subtitle").to_string());
            ui.horizontal(|ui| {
                ui.label(t!("subtitle_size").to_string());
                let mut size = player.prefs().subtitle_font_size;
                if ui.add(egui::Slider::new(&mut size, 12.0..=48.0)).changed() {
                    player.set_subtitle_font_size(size);
                }
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
