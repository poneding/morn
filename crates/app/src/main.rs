mod app;
mod controls;
mod enhance;
mod font;
#[cfg(target_os = "macos")]
mod macos;
mod playlist_panel;
mod settings;
mod shortcuts;
mod subtitle_overlay;
mod updater;
mod video_view;
mod visuals;

rust_i18n::i18n!("locales", fallback = "en");

use app::{PlayerApp, APP_MIN_HEIGHT, APP_MIN_WIDTH};

fn main() -> eframe::Result {
    #[cfg(target_os = "macos")]
    macos::install_about_metadata(env!("CARGO_PKG_VERSION"));

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([APP_MIN_WIDTH, APP_MIN_HEIGHT])
            .with_title("Morn")
            .with_decorations(true)
            .with_transparent(false)
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

#[cfg(test)]
mod tests {
    #[test]
    fn startup_installs_macos_about_metadata_from_package_version() {
        let source = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("macos::install_about_metadata(env!(\"CARGO_PKG_VERSION\"))"));
        assert!(
            source.find("install_about_metadata").unwrap() < source.find("run_native").unwrap()
        );
    }

    #[test]
    fn viewport_uses_native_window_decorations() {
        let source = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("ViewportBuilder::default()"));
        assert!(source.contains(".with_title(\"Morn\")"));
        assert!(source.contains(".with_decorations(true)"));
        assert!(source.contains(".with_transparent(false)"));
        assert!(!source.contains("with_fullsize_content_view(true)"));
    }
}
