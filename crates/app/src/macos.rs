use objc2::ffi::NSInteger;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{declare_class, msg_send, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSBitmapImageRep, NSColor,
    NSDeviceRGBColorSpace, NSImage, NSImageNameApplicationIcon, NSWorkspace,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSBundle, NSObject, NSObjectProtocol, NSSize, NSString, NSURL,
};
use rust_i18n::t;
use std::sync::atomic::{AtomicBool, Ordering};

const CHECK_UPDATES_MENU_TAG: NSInteger = 43_001;
const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icons/morn-logo-256.png");
static CHECK_UPDATE_REQUESTED: AtomicBool = AtomicBool::new(false);

declare_class!(
    struct UpdateMenuTarget;

    unsafe impl ClassType for UpdateMenuTarget {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "MornUpdateMenuTarget";
    }

    impl DeclaredClass for UpdateMenuTarget {
        type Ivars = ();
    }

    unsafe impl NSObjectProtocol for UpdateMenuTarget {}

    unsafe impl UpdateMenuTarget {
        #[method(checkForUpdates:)]
        fn check_for_updates(&self, _sender: Option<&AnyObject>) {
            CHECK_UPDATE_REQUESTED.store(true, Ordering::SeqCst);
        }
    }
);

impl UpdateMenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc();
        let this = this.set_ivars(());
        unsafe { msg_send_id![super(this), init] }
    }
}

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

pub fn install_check_update_menu_item(title: &str) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = (unsafe { app.mainMenu() }) else {
        return;
    };
    let Some(app_item) = (unsafe { main_menu.itemAtIndex(0) }) else {
        return;
    };
    let Some(app_menu) = (unsafe { app_item.submenu() }) else {
        return;
    };
    let title = NSString::from_str(title);

    if let Some(item) = unsafe { app_menu.itemWithTag(CHECK_UPDATES_MENU_TAG) } {
        unsafe { item.setTitle(&title) };
        return;
    }

    let item = unsafe {
        app_menu.insertItemWithTitle_action_keyEquivalent_atIndex(
            &title,
            Some(sel!(checkForUpdates:)),
            ns_string!(""),
            1,
        )
    };
    unsafe { item.setTag(CHECK_UPDATES_MENU_TAG) };
    let target = UpdateMenuTarget::new(mtm);
    let target: Retained<NSObject> = Retained::into_super(target);
    let target: Retained<AnyObject> = Retained::into_super(target);
    let target = unsafe { &*Retained::into_raw(target) };
    unsafe { item.setTarget(Some(target)) };
}

pub fn take_check_update_request() -> bool {
    CHECK_UPDATE_REQUESTED.swap(false, Ordering::SeqCst)
}

pub fn configure_frameless_window_appearance() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(window) = (unsafe { app.mainWindow() }) else {
        return false;
    };
    let Some(content_view) = window.contentView() else {
        return false;
    };

    let clear = unsafe { NSColor::clearColor() };
    window.setOpaque(false);
    window.setBackgroundColor(Some(&clear));
    window.setHasShadow(true);
    content_view.setWantsLayer(true);
    if let Some(layer) = unsafe { content_view.layer() } {
        layer.setCornerRadius(10.0);
        layer.setMasksToBounds(true);
    }
    true
}

pub fn show_update_check_started() {
    show_alert(
        t!("checking_updates").as_ref(),
        "",
        NSAlertStyle::Informational,
        None,
    );
}

pub fn show_update_result(status: &crate::updater::UpdateStatus) {
    match status {
        crate::updater::UpdateStatus::Idle | crate::updater::UpdateStatus::Checking => {}
        crate::updater::UpdateStatus::UpToDate => {
            show_alert(
                t!("check_updates").as_ref(),
                t!("update_up_to_date").as_ref(),
                NSAlertStyle::Informational,
                None,
            );
        }
        crate::updater::UpdateStatus::Available(update) => {
            let channel = if update.prerelease {
                t!("update_channel_beta").to_string()
            } else {
                t!("update_channel_stable").to_string()
            };
            show_alert(
                &format!("{}: {}", t!("update_available"), update.version),
                &format!("{} ({channel})", update.name),
                NSAlertStyle::Informational,
                Some(&update.html_url),
            );
        }
        crate::updater::UpdateStatus::Error(err) => {
            show_alert(
                t!("update_check_failed").as_ref(),
                err,
                NSAlertStyle::Warning,
                None,
            );
        }
    }
}

fn show_alert(message: &str, informative: &str, style: NSAlertStyle, release_url: Option<&str>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let alert = unsafe { NSAlert::new(mtm) };
    let message = NSString::from_str(message);
    let informative = NSString::from_str(informative);
    unsafe {
        alert.setMessageText(&message);
        alert.setInformativeText(&informative);
        alert.setAlertStyle(style);
    }

    if let Some(url) = release_url {
        let open_title = NSString::from_str(t!("open_release_page").as_ref());
        let ok_title = NSString::from_str("OK");
        unsafe {
            let _ = alert.addButtonWithTitle(&open_title);
            let _ = alert.addButtonWithTitle(&ok_title);
        }
        let response = unsafe { alert.runModal() };
        if response == NSAlertFirstButtonReturn {
            open_url(url);
        }
    } else {
        unsafe {
            let _ = alert.runModal();
        }
    }
}

fn open_url(url: &str) {
    let url = NSString::from_str(url);
    let Some(url) = (unsafe { NSURL::URLWithString(&url) }) else {
        return;
    };
    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    unsafe {
        let _ = workspace.openURL(&url);
    }
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
    fn check_update_menu_item_is_inserted_below_about() {
        let source = include_str!("macos.rs");

        assert!(source.contains("install_check_update_menu_item"));
        assert!(source.contains("checkForUpdates:"));
        assert!(source.contains("insertItemWithTitle_action_keyEquivalent_atIndex"));
        assert!(source.contains("CHECK_UPDATES_MENU_TAG"));
        assert!(source.contains(", 1)"));
    }

    #[test]
    fn update_feedback_uses_native_nsalert() {
        let source = include_str!("macos.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("NSAlert"));
        assert!(source.contains("show_update_check_started"));
        assert!(source.contains("show_update_result"));
        assert!(source.contains("runModal"));
        assert!(source.contains("openURL"));
    }

    #[test]
    fn frameless_window_appearance_uses_content_view_corner_mask() {
        let source = include_str!("macos.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("configure_frameless_window_appearance"));
        assert!(source.contains("setOpaque(false)"));
        assert!(source.contains("clearColor"));
        assert!(source.contains("setCornerRadius"));
        assert!(source.contains("setMasksToBounds(true)"));
    }
}
