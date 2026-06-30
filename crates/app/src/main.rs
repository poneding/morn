#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! Desktop entry point for Morn.
//!
//! Startup does only process-level setup: platform metadata, initial window sizing,
//! renderer selection, icon loading, and `PlayerApp` construction.  Playback state
//! restoration stays in the engine/app layers; this file only peeks at restored media
//! dimensions so the first window can open at a useful size before egui starts.
//!
//! The sizing probe is deliberately optional.  If preferences are absent, the stored
//! playlist entry is invalid, or decoding metadata fails, startup falls back to the
//! fixed default inner size instead of delaying or failing the app launch.
//!
//! Native window options are assembled here rather than inside `PlayerApp` because
//! they are process-level choices: renderer backend, decorations, icon, title, and
//! minimum size must exist before the egui app state is created.

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
mod symbols;
mod theme;
mod titlebar;
mod updater;
mod video_view;
mod visuals;

rust_i18n::i18n!("locales", fallback = "en");

use app::{PlayerApp, APP_MIN_HEIGHT, APP_MIN_WIDTH};
use std::path::{Path, PathBuf};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "ts", "mts", "m2ts",
    "3gp", "ogv", "rm", "rmvb",
];

fn collect_cli_video_paths() -> Vec<PathBuf> {
    std::env::args_os()
        .skip(1)
        .filter_map(|arg| {
            let path = std::path::PathBuf::from(arg);
            if !path.is_file() {
                return None;
            }
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            VIDEO_EXTENSIONS
                .contains(&ext.as_str())
                .then(|| path.canonicalize().unwrap_or(path))
        })
        .collect()
}

fn main() -> eframe::Result {
    #[cfg(target_os = "macos")]
    macos::install_about_metadata(env!("CARGO_PKG_VERSION"));

    let initial_paths = collect_cli_video_paths();
    let initial_inner_size = startup_window_size();
    let native_options = eframe::NativeOptions {
        viewport: app_viewport(initial_inner_size).with_icon(app_icon()),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Morn",
        native_options,
        Box::new(move |cc| Ok(Box::new(PlayerApp::new(cc, initial_paths)))),
    )
}

fn app_viewport(initial_inner_size: [f32; 2]) -> eframe::egui::ViewportBuilder {
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(initial_inner_size)
        .with_min_inner_size([APP_MIN_WIDTH, APP_MIN_HEIGHT]);

    #[cfg(target_os = "macos")]
    {
        // Match Translator/Tauri's native overlay titlebar style: keep the real
        // macOS traffic lights and rounded window frame, but let egui paint app
        // actions in the titlebar content area.
        viewport
            .with_title("")
            .with_decorations(true)
            .with_transparent(false)
            .with_fullsize_content_view(true)
            .with_title_shown(false)
            .with_titlebar_shown(false)
            .with_titlebar_buttons_shown(true)
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Other platforms keep the existing custom borderless window path.
        viewport
            .with_title("Morn")
            .with_decorations(false)
            .with_transparent(true)
    }
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
    let decoder = media::VideoDecoder::open_path(path).ok()?;
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
    fn viewport_uses_native_macos_titlebar_overlay_and_custom_fallback_elsewhere() {
        let source = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("ViewportBuilder::default()"));
        assert!(source.contains("let initial_inner_size = startup_window_size()"));
        assert!(source.contains(".with_inner_size(initial_inner_size)"));
        assert!(source.contains(".with_title(\"\")"));
        assert!(source.contains(".with_title(\"Morn\")"));
        assert!(source.contains(".with_fullsize_content_view(true)"));
        assert!(source.contains(".with_title_shown(false)"));
        assert!(source.contains(".with_titlebar_shown(false)"));
        assert!(source.contains(".with_titlebar_buttons_shown(true)"));
        // Non-macOS keeps the custom borderless transparent window path.
        assert!(source.contains(".with_decorations(false)"));
        assert!(source.contains(".with_transparent(true)"));
    }

    #[test]
    fn release_windows_build_uses_gui_subsystem() {
        let source = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("windows_subsystem = \"windows\""));
        assert!(source.contains("not(debug_assertions)"));
    }

    #[test]
    fn startup_window_size_uses_selected_video_aspect_ratio() {
        assert_eq!(super::app::window_size_for_video_dimensions(0, 120), None);
        assert_eq!(super::app::window_size_for_video_dimensions(160, 0), None);

        let wide = super::app::window_size_for_video_dimensions(1920, 1080).unwrap();
        // 16:9 video: inner_width = (600 - 28) * 16/9, inner_height = 600
        assert!(close_enough(wide.x, 1016.8889));
        assert!(close_enough(wide.y, 600.0));

        let squareish = super::app::window_size_for_video_dimensions(160, 120).unwrap();
        // 4:3 video clamped to APP_MIN_WIDTH: video_height = 920 * 3/4 = 690,
        // inner_height = 690 + 28 (titlebar offset)
        assert!(close_enough(squareish.x, super::APP_MIN_WIDTH));
        assert!(close_enough(squareish.y, 718.0));
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
        assert!(source.contains("media::VideoDecoder::open_path(path)"));
    }
}
