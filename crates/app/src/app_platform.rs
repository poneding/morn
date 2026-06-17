//! Platform-facing app helpers.
//!
//! File reveal and preferences path selection are separated from UI rendering so the
//! behavior can be tested with synthetic environments.  The public app code asks for
//! a path or command outcome; this module owns OS-specific directory conventions,
//! legacy preference migration, and file-manager invocation.
//!
//! Migration is best-effort.  Failure to copy old temp preferences is logged, but the
//! app continues with the platform config path so a filesystem issue cannot block
//! launch.

pub(crate) fn prefs_path() -> std::path::PathBuf {
    let preferred = prefs_path_from_env(&PrefsPathEnv::current());
    if let Err(err) = migrate_legacy_prefs(&legacy_temp_prefs_path(), &preferred) {
        eprintln!("迁移旧偏好失败: {err}");
    }
    preferred
}

#[derive(Debug, Clone)]
pub(super) struct PrefsPathEnv {
    pub(super) home: Option<std::path::PathBuf>,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(super) appdata: Option<std::path::PathBuf>,
    #[cfg_attr(any(target_os = "windows", target_os = "macos"), allow(dead_code))]
    pub(super) xdg_config_home: Option<std::path::PathBuf>,
}

impl PrefsPathEnv {
    fn current() -> Self {
        Self {
            home: std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(Into::into)
                .or_else(|| {
                    std::env::var_os("USERPROFILE")
                        .filter(|value| !value.is_empty())
                        .map(Into::into)
                }),
            appdata: std::env::var_os("APPDATA")
                .filter(|value| !value.is_empty())
                .map(Into::into),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(Into::into),
        }
    }
}

pub(super) fn prefs_path_from_env(env: &PrefsPathEnv) -> std::path::PathBuf {
    config_dir_from_env(env).join("prefs.json")
}

pub(super) fn config_dir_from_env(env: &PrefsPathEnv) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        env.appdata
            .clone()
            .or_else(|| {
                env.home
                    .as_ref()
                    .map(|home| home.join("AppData").join("Roaming"))
            })
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("Morn")
    }

    #[cfg(target_os = "macos")]
    {
        env.home
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("Library")
            .join("Application Support")
            .join("Morn")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        env.xdg_config_home
            .clone()
            .or_else(|| env.home.as_ref().map(|home| home.join(".config")))
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("morn")
    }
}

fn legacy_temp_prefs_path() -> std::path::PathBuf {
    std::env::temp_dir().join("morn-prefs.json")
}

pub(super) fn migrate_legacy_prefs(
    legacy: &std::path::Path,
    preferred: &std::path::Path,
) -> std::io::Result<()> {
    if preferred.exists() || !legacy.exists() {
        return Ok(());
    }
    if let Some(parent) = preferred.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(legacy, preferred)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileRevealCommand {
    pub(super) program: &'static str,
    pub(super) args: Vec<String>,
}

pub(super) fn file_reveal_command(path: &std::path::Path) -> FileRevealCommand {
    #[cfg(target_os = "windows")]
    {
        FileRevealCommand {
            program: "explorer",
            args: vec![format!("/select,{}", path.display())],
        }
    }

    #[cfg(target_os = "macos")]
    {
        FileRevealCommand {
            program: "open",
            args: vec!["-R".to_string(), path.to_string_lossy().to_string()],
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let target = path.parent().unwrap_or(path);
        FileRevealCommand {
            program: "xdg-open",
            args: vec![target.to_string_lossy().to_string()],
        }
    }
}

pub(super) fn reveal_file(path: &std::path::Path) -> bool {
    let command = file_reveal_command(path);
    std::process::Command::new(command.program)
        .args(command.args)
        .spawn()
        .is_ok()
}
