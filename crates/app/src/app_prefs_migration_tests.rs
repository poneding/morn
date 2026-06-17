//! Preferences path and migration regression tests.
//!
//! These assertions keep platform config path selection and legacy temp-file
//! migration near the app platform helpers without mixing them into UI behavior
//! tests.

use crate::app::app_platform::{migrate_legacy_prefs, prefs_path_from_env, PrefsPathEnv};

#[test]
fn prefs_path_uses_platform_config_directory() {
    let env = PrefsPathEnv {
        home: Some(std::path::PathBuf::from("/home/alice")),
        appdata: Some(std::path::PathBuf::from("C:/Users/alice/AppData/Roaming")),
        xdg_config_home: Some(std::path::PathBuf::from("/xdg/alice")),
    };

    let path = prefs_path_from_env(&env);

    #[cfg(target_os = "windows")]
    assert_eq!(
        path,
        std::path::PathBuf::from("C:/Users/alice/AppData/Roaming/Morn/prefs.json")
    );
    #[cfg(target_os = "macos")]
    assert_eq!(
        path,
        std::path::PathBuf::from("/home/alice/Library/Application Support/Morn/prefs.json")
    );
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    assert_eq!(path, std::path::PathBuf::from("/xdg/alice/morn/prefs.json"));
}

#[test]
fn legacy_temp_preferences_are_migrated_without_overwriting_existing_config() -> std::io::Result<()>
{
    let root = std::env::temp_dir().join(format!(
        "morn_prefs_migrate_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    let legacy = root.join("legacy/morn-prefs.json");
    let preferred = root.join("config/Morn/prefs.json");
    if let Some(parent) = legacy.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let write_result = std::fs::write(&legacy, r#"{"volume":42}"#);
    write_result?;

    migrate_legacy_prefs(&legacy, &preferred)?;
    assert_eq!(std::fs::read_to_string(&preferred)?, r#"{"volume":42}"#);

    let write_result = std::fs::write(&legacy, r#"{"volume":5}"#);
    write_result?;
    migrate_legacy_prefs(&legacy, &preferred)?;
    assert_eq!(std::fs::read_to_string(&preferred)?, r#"{"volume":42}"#);

    let _cleanup = std::fs::remove_dir_all(root);
    Ok(())
}
