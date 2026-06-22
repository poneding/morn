//! Secondary playback tools.
//!
//! The enhance bar hosts controls that are adjacent to playback but are not core
//! transport: playback rate selection and screenshot capture.  It returns a compact
//! action object so the app can route commands through the normal player path while
//! handling screenshots against the currently presented video frame.
//!
//! Screenshot saving validates the destination directory and surfaces filesystem
//! errors to the caller.  The UI layer owns user-facing notices because it knows
//! whether a frame was available and where the configured screenshot directory lives.

use eframe::egui;
use player_core::Command;
use rust_i18n::t;
use std::path::{Path, PathBuf};

pub const RATE_OPTIONS: [u16; 8] = [25, 50, 75, 100, 125, 150, 175, 200];
const RATE_BUTTON_WIDTH: f32 = 68.0;

/// 增强控件本帧产生的动作。
pub struct EnhanceActions {
    pub commands: Vec<Command>,
    pub screenshot: bool,
}

/// 绘制增强控件(倍速下拉 / 截图)。
/// `rate_pct` 为当前倍速(百分比), 用于下拉显示当前值。
pub fn enhance_bar(ui: &mut egui::Ui, rate_pct: u16) -> EnhanceActions {
    let mut commands = Vec::new();
    // The rate label is a plain button (not a dropdown widget) so the popup can
    // share the same floating-control anchoring as the volume panel.
    let rate_response = ui
        .add_sized(
            [RATE_BUTTON_WIDTH, crate::controls::CONTROL_BUTTON_SIZE],
            egui::Button::new(
                egui::RichText::new(format!(
                    "{:.2}x {}",
                    rate_pct as f32 / 100.0,
                    crate::symbols::RATE_DROPDOWN
                ))
                .family(egui::FontFamily::Monospace),
            ),
        )
        .on_hover_text(crate::shortcuts::shortcut_tooltip(
            t!("rate"),
            crate::shortcuts::rate_shortcut_label(),
        ));
    rate_popup(&rate_response, rate_pct, &mut commands);

    EnhanceActions {
        commands,
        screenshot: screenshot_button_clicked(ui),
    }
}

fn screenshot_button_clicked(ui: &mut egui::Ui) -> bool {
    ui.add_sized(
        [
            crate::controls::CONTROL_BUTTON_SIZE,
            crate::controls::CONTROL_BUTTON_SIZE,
        ],
        egui::Button::new(crate::symbols::SCREENSHOT),
    )
    .on_hover_text(crate::shortcuts::shortcut_tooltip(t!("screenshot"), "S"))
    .clicked()
}

fn rate_popup(rate_response: &egui::Response, rate_pct: u16, commands: &mut Vec<Command>) {
    // Rate changes are returned as normal player commands; screenshot remains a
    // separate UI action because it needs the currently uploaded frame.
    egui::Popup::menu(rate_response)
        .anchor(crate::visuals::popup_anchor_above_floating_control_bar(
            rate_response,
        ))
        .align(egui::RectAlign::TOP)
        .align_alternatives(&[])
        .gap(crate::visuals::FLOATING_PANEL_MARGIN)
        .style(crate::visuals::panel_popup_style())
        .show(|ui| {
            for pct in RATE_OPTIONS {
                if ui
                    .selectable_label(
                        rate_pct == pct,
                        egui::RichText::new(format!("{:.2}x", pct as f32 / 100.0))
                            .family(egui::FontFamily::Monospace),
                    )
                    .clicked()
                {
                    commands.push(Command::SetRate(pct));
                    let _closed = ui.close();
                }
            }
        });
}

/// 把 RGBA8 帧写为 PNG, 返回保存路径。
pub fn save_screenshot(rgba: &[u8], w: u32, h: u32, dir: &Path) -> std::io::Result<PathBuf> {
    ensure_screenshot_dir(dir)?;
    // The timestamp keeps filenames stable enough for repeated screenshots without
    // introducing another counter into app state.
    let path = dir.join(format!("morn-shot-{}.png", now_stamp()));
    image::save_buffer(&path, rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(std::io::Error::other)?;
    Ok(path)
}

fn ensure_screenshot_dir(dir: &Path) -> std::io::Result<()> {
    // Reject an empty path before create_dir_all, which would otherwise target the
    // process working directory on some platforms.
    if dir.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "screenshot directory is empty",
        ));
    }
    std::fs::create_dir_all(dir)
}

fn now_stamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    // If the system clock is before the Unix epoch, keep the saver deterministic
    // instead of surfacing an unrelated time error to the screenshot flow.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

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
        let source = include_str!("enhance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("Popup::menu(rate_response)"));
        assert!(source.contains("popup_anchor_above_floating_control_bar"));
        assert!(source.contains("RectAlign::TOP"));
        assert!(source.contains("FLOATING_PANEL_MARGIN"));
        assert!(!source.contains("ComboBox"));
        assert!(!source.contains("from_label"));
    }

    #[test]
    fn enhance_tooltips_include_keyboard_shortcuts() {
        let source = include_str!("enhance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        for expected in [
            "shortcut_tooltip",
            "t!(\"rate\")",
            "rate_shortcut_label",
            "t!(\"screenshot\")",
            "\"S\"",
        ] {
            assert!(source.contains(expected));
        }
    }

    #[test]
    fn screenshot_saver_uses_configured_directory_instead_of_temp() {
        let source = include_str!("enhance.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(!source.contains("temp_dir()"));
        assert!(!source.contains("morn-shots"));
        assert!(source.contains("ensure_screenshot_dir(dir)"));
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
        let _cleanup = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_screenshot_creates_nested_missing_directory() {
        let dir = std::env::temp_dir().join(format!(
            "morn_screenshot_nested_test_{}_{}",
            std::process::id(),
            super::now_stamp()
        ));
        let nested = dir.join("Pictures").join("Morn");
        let rgba = [0u8, 255, 0, 255];

        let path = super::save_screenshot(&rgba, 1, 1, &nested).unwrap();

        assert!(nested.exists());
        assert!(path.starts_with(&nested));
        assert!(path.exists());
        let _cleanup = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_screenshot_rejects_empty_directory() {
        let rgba = [0u8, 0, 255, 255];
        let err = super::save_screenshot(&rgba, 1, 1, Path::new("")).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}
