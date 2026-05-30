use eframe::egui;
use player_core::Command;
use rust_i18n::t;
use std::path::PathBuf;

/// 增强控件本帧产生的动作。
pub struct EnhanceActions {
    pub commands: Vec<Command>,
    pub screenshot: bool,
}

/// 绘制增强控件(倍速下拉 / 逐帧 / 设A / 设B / 清除循环 / 截图)。
/// `rate_pct` 为当前倍速(百分比), 用于下拉显示当前值。
pub fn enhance_bar(ui: &mut egui::Ui, rate_pct: u16) -> EnhanceActions {
    let mut commands = Vec::new();
    let mut screenshot = false;
    ui.horizontal(|ui| {
        let mut rate = rate_pct;
        egui::ComboBox::from_label(t!("rate").to_string())
            .selected_text(format!("{:.2}x", rate as f32 / 100.0))
            .show_ui(ui, |ui| {
                for pct in [50u16, 100, 150, 200] {
                    ui.selectable_value(&mut rate, pct, format!("{:.2}x", pct as f32 / 100.0));
                }
            });
        if rate != rate_pct {
            commands.push(Command::SetRate(rate));
        }
        if ui
            .button("⏭|")
            .on_hover_text(t!("step_frame").to_string())
            .clicked()
        {
            commands.push(Command::StepFrame);
        }
        if ui
            .button("Ⓐ")
            .on_hover_text(t!("loop_a").to_string())
            .clicked()
        {
            commands.push(Command::SetLoopA);
        }
        if ui
            .button("Ⓑ")
            .on_hover_text(t!("loop_b").to_string())
            .clicked()
        {
            commands.push(Command::SetLoopB);
        }
        if ui
            .button("✖")
            .on_hover_text(t!("clear_loop").to_string())
            .clicked()
        {
            commands.push(Command::ClearLoop);
        }
        if ui
            .button("📷")
            .on_hover_text(t!("screenshot").to_string())
            .clicked()
        {
            screenshot = true;
        }
    });
    EnhanceActions {
        commands,
        screenshot,
    }
}

/// 把 RGBA8 帧写为 PNG, 返回保存路径。
pub fn save_screenshot(rgba: &[u8], w: u32, h: u32) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("morn-shots");
    std::fs::create_dir_all(&dir)?;
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
