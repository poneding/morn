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

use app::{PlayerApp, APP_MIN_HEIGHT, APP_MIN_WIDTH};

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([APP_MIN_WIDTH, APP_MIN_HEIGHT])
            .with_title("Morn")
            .with_icon(app_icon()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Morn",
        native_options,
        Box::new(|cc| Ok(Box::new(PlayerApp::new(cc)))),
    )
}

fn app_icon() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/morn-logo-256.png"))
        .expect("embedded app icon should be a valid PNG")
}
