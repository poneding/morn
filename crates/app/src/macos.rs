//! macOS-specific app metadata wiring.
//!
//! This module patches runtime bundle metadata used by the native About panel and
//! installs the bundled PNG as the application icon.  The Objective-C calls stay
//! isolated here so the cross-platform startup path only has to pass the current
//! version string.

use objc2::rc::Retained;
use objc2::{msg_send, ClassType};
use objc2_app_kit::{
    NSApplication, NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage, NSImageNameApplicationIcon,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSBundle, NSSize, NSString};

const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icons/morn-logo-256.png");

pub fn install_about_metadata(version: &str) {
    install_version_metadata(version);
    install_about_icon(APP_ICON_PNG);
}

fn install_version_metadata(version: &str) {
    let bundle = NSBundle::mainBundle();
    let Some(info) = bundle.infoDictionary() else {
        return;
    };
    let version = NSString::from_str(version);

    unsafe {
        let _: () = msg_send![
            &*info,
            setObject: &*version,
            forKey: ns_string!("CFBundleShortVersionString"),
        ];
        let _: () = msg_send![
            &*info,
            setObject: &*version,
            forKey: ns_string!("CFBundleVersion"),
        ];
    }
}

fn install_about_icon(icon_png: &[u8]) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(image) = ns_image_from_png(icon_png) else {
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    unsafe {
        let _ = image.setName(Some(NSImageNameApplicationIcon));
        app.setApplicationIconImage(Some(&image));
    }
}

fn ns_image_from_png(icon_png: &[u8]) -> Option<Retained<NSImage>> {
    let rgba = image::load_from_memory_with_format(icon_png, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let pixels = rgba.into_raw();

    let image_rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            (width * 4) as isize,
            32,
        )?
    };
    let bitmap = unsafe { image_rep.bitmapData() };
    if bitmap.is_null() {
        return None;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bitmap, pixels.len());
    }

    let image = unsafe {
        NSImage::initWithSize(NSImage::alloc(), NSSize::new(width as f64, height as f64))
    };
    unsafe {
        image.addRepresentation(&image_rep);
    }
    Some(image)
}

/// 把窗口 contentView 的 layer 放置策略改为 `topLeft`, 掩盖 macOS + Metal 在
/// resize/最大化期间把旧帧拉伸的抖动。
///
/// 见 `Cargo.toml` 里关闭 `macos-window-resize-jitter-fix` 的注释: egui 的 jitter-fix
/// 用 `presentsWithTransaction` + `waitUntilScheduled` 同步 present 消除旧帧拉伸, 但会
/// 带来顶部/左侧拉伸卡顿(egui#8043)。这里走另一条路 —— 不碰 present 机制, 只改
/// CoreAnimation 复用旧帧时的放置策略: 默认 `scaleAxesIndependently` 会把整张旧帧拉伸
/// 铺满新尺寸(表现为 UI 跟着放大缩小), 改成 `topLeft` 后旧帧锚定左上角不动, 新露出的
/// 区域显示背景, glitch 退化成移动边缘附近几像素的小破损条, 几乎不可见。
///
/// 在 AppKit 主线程执行; 返回 `true` 表示已成功应用, 调用方可据此只执行一次。
pub fn apply_resize_glitch_masking(frame: &eframe::Frame) -> bool {
    use objc2_app_kit::{NSView, NSViewLayerContentsPlacement};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // setLayerContentsPlacement: 是 AppKit UI 调用, 必须在主线程。
    if MainThreadMarker::new().is_none() {
        return false;
    }

    let Ok(handle) = frame.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return false;
    };

    // SAFETY: ns_view 指向 winit 创建并持有的 NSView, 指针在整个窗口生命周期内有效;
    // 且已确认在主线程。
    let ns_view: &NSView = unsafe { &*(appkit.ns_view.as_ptr() as *const NSView) };
    unsafe {
        ns_view.setLayerContentsPlacement(NSViewLayerContentsPlacement::TopLeft);
    }
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn apply_resize_glitch_masking_uses_top_left_layer_placement() {
        let source = include_str!("macos.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(source.contains("pub fn apply_resize_glitch_masking"));
        assert!(source.contains("setLayerContentsPlacement"));
        assert!(source.contains("NSViewLayerContentsPlacement::TopLeft"));
    }

    #[test]
    fn about_metadata_keys_include_short_and_build_versions() {
        let source = include_str!("macos.rs");

        assert!(source.contains("CFBundleShortVersionString"));
        assert!(source.contains("CFBundleVersion"));
        assert!(source.contains("NSBundle::mainBundle"));
    }

    #[test]
    fn about_metadata_installs_morn_logo_as_application_icon() {
        let source = include_str!("macos.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("morn-logo-256.png"));
        assert!(source.contains("NSBitmapImageRep"));
        assert!(source.contains("NSImageNameApplicationIcon"));
        assert!(source.contains("setApplicationIconImage"));
        assert!(
            !source.contains("NSImage::initWithData"),
            "avoid constructing NSImage directly from PNG data"
        );
    }

    #[test]
    fn about_metadata_can_be_installed_at_runtime() {
        super::install_about_metadata("0.0.0-test");
    }

    #[test]
    fn check_update_menu_item_is_not_installed_from_about_menu() {
        let source = include_str!("macos.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(!source.contains("install_check_update_menu_item"));
        assert!(!source.contains("checkForUpdates:"));
        assert!(!source.contains("CHECK_UPDATES_MENU_TAG"));
        assert!(!source.contains("NSAlert"));
    }
}
