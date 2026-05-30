mod app;
mod controls;
mod enhance;
mod font;
mod playlist_panel;
mod settings;
mod shortcuts;
mod subtitle_overlay;
mod video_view;

rust_i18n::i18n!("locales", fallback = "en");

use app::PlayerApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_title("Morn"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Morn",
        native_options,
        Box::new(|cc| Ok(Box::new(PlayerApp::new(cc)))),
    )
}
