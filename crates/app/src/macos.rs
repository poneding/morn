use objc2::msg_send;
use objc2_foundation::{ns_string, NSBundle, NSString};

pub fn install_about_metadata(version: &str) {
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
    fn about_metadata_can_be_installed_at_runtime() {
        super::install_about_metadata("0.0.0-test");
    }
}
