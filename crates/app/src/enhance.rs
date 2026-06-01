use eframe::egui;
use player_core::Command;
use rust_i18n::t;
use std::path::{Path, PathBuf};

pub const RATE_OPTIONS: [u16; 8] = [25, 50, 75, 100, 125, 150, 175, 200];
pub const RATE_COMBO_WIDTH: f32 = 0.0;

/// 增强控件本帧产生的动作。
pub struct EnhanceActions {
    pub commands: Vec<Command>,
    pub screenshot: bool,
    pub screenshot_notice_pos: Option<egui::Pos2>,
}

/// 绘制增强控件(倍速下拉 / 截图)。
/// `rate_pct` 为当前倍速(百分比), 用于下拉显示当前值。
pub fn enhance_bar(ui: &mut egui::Ui, rate_pct: u16) -> EnhanceActions {
    let mut commands = Vec::new();
    let mut screenshot = false;
    let mut screenshot_notice_pos = None;
    let mut rate = rate_pct;
    egui::ComboBox::from_id_salt("rate")
        .width(RATE_COMBO_WIDTH)
        .selected_text(format!("{:.2}x", rate as f32 / 100.0))
        .show_ui(ui, |ui| {
            for pct in RATE_OPTIONS {
                ui.selectable_value(&mut rate, pct, format!("{:.2}x", pct as f32 / 100.0));
            }
        })
        .response
        .on_hover_text(t!("rate").to_string());
    if rate != rate_pct {
        commands.push(Command::SetRate(rate));
    }
    let screenshot_response = ui.button("📷").on_hover_text(t!("screenshot").to_string());
    if screenshot_response.clicked() {
        screenshot = true;
        screenshot_notice_pos = Some(screenshot_response.rect.left_bottom() + egui::vec2(0.0, 6.0));
    }
    EnhanceActions {
        commands,
        screenshot,
        screenshot_notice_pos,
    }
}

/// 把 RGBA8 帧写为 PNG, 返回保存路径。
pub fn save_screenshot(rgba: &[u8], w: u32, h: u32, dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("morn-shot-{}.png", now_stamp()));
    image::save_buffer(&path, rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(std::io::Error::other)?;
    Ok(path)
}

fn now_stamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rate_options_include_quarter_and_three_quarter_steps() {
        assert_eq!(
            super::RATE_OPTIONS,
            [25u16, 50, 75, 100, 125, 150, 175, 200]
        );
    }

    #[test]
    fn enhance_bar_does_not_expose_ab_loop_controls() {
        let source = include_str!("enhance.rs");

        for removed in [
            concat!("Set", "LoopA"),
            concat!("Set", "LoopB"),
            concat!("Clear", "Loop"),
            concat!("loop", "_a"),
            concat!("loop", "_b"),
            concat!("clear", "_loop"),
        ] {
            assert!(
                !source.contains(removed),
                "enhance bar still references removed AB loop control: {removed}"
            );
        }
    }

    #[test]
    fn enhance_bar_excludes_removed_frame_step() {
        let source = include_str!("enhance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        for removed in [concat!("Step", "Frame"), concat!("step", "_frame")] {
            assert!(
                !source.contains(removed),
                "enhance bar still references removed step-frame control: {removed}"
            );
        }
    }

    #[test]
    fn rate_dropdown_uses_adaptive_width() {
        assert_eq!(super::RATE_COMBO_WIDTH, 0.0);
        let source = include_str!("enhance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("from_id_salt"));
        assert!(source.contains("width(RATE_COMBO_WIDTH)"));
        assert!(!source.contains("from_label"));
    }

    #[test]
    fn screenshot_saver_uses_configured_directory_instead_of_temp() {
        let source = include_str!("enhance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(!source.contains("temp_dir()"));
        assert!(!source.contains("morn-shots"));
        assert!(source.contains("create_dir_all(dir)"));
    }

    #[test]
    fn save_screenshot_writes_file_inside_requested_directory() {
        let dir = std::env::temp_dir().join(format!(
            "morn_screenshot_test_{}_{}",
            std::process::id(),
            super::now_stamp()
        ));
        let rgba = [255u8, 0, 0, 255];

        let path = super::save_screenshot(&rgba, 1, 1, &dir).unwrap();

        assert!(path.starts_with(&dir));
        assert!(path.exists());
        std::fs::remove_dir_all(dir).ok();
    }
}
