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
use std::path::{Path, PathBuf};

fn main() -> eframe::Result {
    #[cfg(target_os = "macos")]
    macos::install_about_metadata(env!("CARGO_PKG_VERSION"));

    let initial_inner_size = startup_window_size();
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(initial_inner_size)
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

fn startup_window_size() -> [f32; 2] {
    restored_selected_video_path(app::prefs_path())
        .and_then(|path| selected_video_dimensions(&path))
        .and_then(|(width, height)| app::window_size_for_video_dimensions(width, height))
        .map(Into::into)
        .unwrap_or(app::DEFAULT_INITIAL_INNER_SIZE)
}

fn restored_selected_video_path(prefs_path: PathBuf) -> Option<PathBuf> {
    let player = engine::Player::with_prefs(prefs_path);
    let index = player.current_index()?;
    player.playlist_paths().get(index).cloned()
}

fn selected_video_dimensions(path: &Path) -> Option<(u32, u32)> {
    if !path.is_file() {
        return None;
    }
    let decoder = media::VideoDecoder::open(path).ok()?;
    Some((decoder.width(), decoder.height()))
}

fn app_icon() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/morn-logo-256.png"))
        .expect("embedded app icon should be a valid PNG")
}

#[cfg(test)]
mod tests {
    fn close_enough(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

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
        assert!(source.contains("let initial_inner_size = startup_window_size()"));
        assert!(source.contains(".with_inner_size(initial_inner_size)"));
        assert!(source.contains(".with_title(\"Morn\")"));
        assert!(source.contains(".with_decorations(true)"));
        assert!(source.contains(".with_transparent(false)"));
        assert!(!source.contains("with_fullsize_content_view(true)"));
    }

    #[test]
    fn startup_window_size_uses_selected_video_aspect_ratio() {
        assert_eq!(super::app::window_size_for_video_dimensions(0, 120), None);
        assert_eq!(super::app::window_size_for_video_dimensions(160, 0), None);

        let wide = super::app::window_size_for_video_dimensions(1920, 1080).unwrap();
        assert!(close_enough(wide.x, 1066.6666));
        assert!(close_enough(wide.y, 600.0));

        let squareish = super::app::window_size_for_video_dimensions(160, 120).unwrap();
        assert!(close_enough(squareish.x, super::APP_MIN_WIDTH));
        assert!(close_enough(squareish.y, 690.0));
    }

    #[test]
    fn startup_window_size_reads_restored_selection_before_native_window_creation() {
        let source = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("startup_window_size()"));
        assert!(source.contains("restored_selected_video_path(app::prefs_path())"));
        assert!(source.contains("engine::Player::with_prefs"));
        assert!(source.contains("media::VideoDecoder::open(path)"));
    }
}
