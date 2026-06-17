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

#[cfg(test)]
mod tests {
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
