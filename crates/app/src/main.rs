mod app;
mod controls;
mod video_view;

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
