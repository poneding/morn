//! Persistent user preferences.
//!
//! Preferences are low-risk state, so loading is forgiving: missing files and corrupt
//! JSON both fall back to defaults rather than blocking startup.  Saving is stricter
//! and uses a temporary file followed by rename so a crash cannot leave a partially
//! written preferences file at the target path.
//!
//! Paths are normalized at load/save boundaries.  In particular the screenshot
//! directory accepts the current user's `~` prefix for legacy configs, but does not
//! try to expand another user's home directory.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    #[default]
    StopAtEnd,
    LoopPlaylist,
    RepeatOne,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    // Every field has a default so old preference files can be loaded after new
    // settings are added.
    pub volume: u8,
    pub window_size: (u32, u32),
    pub language: String,
    pub seek_step_secs: u64,
    pub theme: String,
    pub subtitle_font_size: f32,
    pub playback_mode: PlaybackMode,
    pub check_updates_on_startup: bool,
    pub check_beta_updates: bool,
    #[serde(default = "default_screenshot_dir_string")]
    pub screenshot_dir: String,
    pub last_playlist: Vec<String>,
    pub last_index: usize,
    pub history: Vec<String>,
    /// 文件路径(字符串) → 续播位置(毫秒)。
    resume_points: HashMap<String, u64>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            volume: 100,
            window_size: (1280, 720),
            language: "zh-CN".to_string(),
            seek_step_secs: 10,
            theme: "system".to_string(),
            subtitle_font_size: 24.0,
            playback_mode: PlaybackMode::StopAtEnd,
            check_updates_on_startup: false,
            check_beta_updates: false,
            screenshot_dir: default_screenshot_dir_string(),
            last_playlist: Vec::new(),
            last_index: 0,
            history: Vec::new(),
            resume_points: HashMap::new(),
        }
    }
}

pub fn default_screenshot_dir() -> PathBuf {
    default_home_dir().join("Pictures").join("Morn")
}

/// 解析截图目录配置。只支持当前用户 Home 的 `~` 前缀, 不处理 `~other`。
pub fn resolve_screenshot_dir(path: &str) -> PathBuf {
    if path.is_empty() {
        return PathBuf::new();
    }
    if path == "~" {
        return default_home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        let mut resolved = default_home_dir();
        push_path_segments(&mut resolved, rest);
        return resolved;
    }
    PathBuf::from(path)
}

fn default_home_dir() -> PathBuf {
    home_dir_from_env(std::env::var_os("HOME"), std::env::var_os("USERPROFILE"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn home_dir_from_env(home: Option<OsString>, user_profile: Option<OsString>) -> Option<PathBuf> {
    // Prefer the platform-native home variable but keep the other as a fallback for
    // shells that bridge Unix and Windows environments.
    #[cfg(windows)]
    let selected = non_empty_env_path(user_profile).or_else(|| non_empty_env_path(home));
    #[cfg(not(windows))]
    let selected = non_empty_env_path(home).or_else(|| non_empty_env_path(user_profile));
    selected.map(PathBuf::from)
}

fn non_empty_env_path(path: Option<OsString>) -> Option<OsString> {
    path.filter(|path| !path.is_empty())
}

fn default_screenshot_dir_string() -> String {
    default_screenshot_dir().to_string_lossy().into_owned()
}

fn push_path_segments(path: &mut PathBuf, rest: &str) {
    // Split both separator styles so legacy config files migrate cleanly across
    // platforms.
    for segment in rest
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
    {
        path.push(segment);
    }
}

impl Preferences {
    fn normalize_paths(&mut self) {
        // Persist resolved screenshot paths so later app versions do not need to
        // keep interpreting legacy `~` spellings on every UI read.
        self.screenshot_dir = resolve_screenshot_dir(&self.screenshot_dir)
            .to_string_lossy()
            .into_owned();
    }

    pub fn resume_point(&self, file: &str) -> Option<u64> {
        return self.resume_points.get(file).copied();
    }

    pub fn set_resume_point(&mut self, file: &str, ms: u64) {
        // Resume points are keyed by display path strings because playlist/history
        // persistence already stores paths in that format.
        self.resume_points.insert(file.to_string(), ms);
    }

    /// 从 JSON 文件加载。文件不存在时返回默认值(非错误)。
    pub fn load(path: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            // 解析失败按默认处理: 偏好属低风险数据, 不阻断启动。
            Ok(s) => {
                let mut prefs: Self = serde_json::from_str(&s).unwrap_or_default();
                prefs.normalize_paths();
                Ok(prefs)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut prefs = Self::default();
                prefs.normalize_paths();
                Ok(prefs)
            }
            Err(e) => Err(e),
        }
    }

    /// 序列化为 JSON 写入文件。自动创建父目录。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Normalize a clone before serialization so callers keep their in-memory
        // value untouched until they explicitly set a new preference.
        let mut prefs = self.clone();
        prefs.normalize_paths();
        let json = serde_json::to_string_pretty(&prefs).map_err(std::io::Error::other)?;
        let tmp = temp_save_path(path);
        let write_result = std::fs::write(&tmp, json);
        write_result?;
        replace_file(&tmp, path)
    }
}

fn temp_save_path(path: &Path) -> PathBuf {
    // Put the temp file beside the destination so the final rename stays within
    // the same filesystem when possible.
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("prefs.json");
    path.with_file_name(format!(".{name}.tmp-{}", std::process::id()))
}

fn replace_file(tmp: &Path, path: &Path) -> std::io::Result<()> {
    // Windows rename will not replace an existing file, while Unix rename does.
    // Keep the platform difference here so save() reads the same everywhere.
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    return std::fs::rename(tmp, path);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_prefs_path(name: &str) -> std::io::Result<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join(name);
        Ok((dir, path))
    }

    fn save_and_load(prefs: &Preferences) -> std::io::Result<Preferences> {
        let (_dir, path) = temp_prefs_path("prefs.json")?;
        prefs.save(&path)?;
        Preferences::load(&path)
    }

    fn load_json_preferences(json: &str) -> std::io::Result<Preferences> {
        let (_dir, path) = temp_prefs_path("prefs.json")?;
        let write_result = std::fs::write(&path, json);
        write_result?;
        Preferences::load(&path)
    }

    fn save_read_and_load(prefs: &Preferences) -> std::io::Result<(String, Preferences)> {
        let (_dir, path) = temp_prefs_path("prefs.json")?;
        prefs.save(&path)?;
        let saved = std::fs::read_to_string(&path)?;
        let loaded = Preferences::load(&path)?;
        Ok((saved, loaded))
    }

    #[test]
    fn defaults_are_sane() {
        let p = Preferences::default();
        assert_eq!(p.volume, 100);
        assert_eq!(p.window_size, (1280, 720));
        assert_eq!(p.playback_mode, PlaybackMode::StopAtEnd);
        assert_eq!(p.screenshot_dir, default_screenshot_dir().to_string_lossy());
        assert!(
            Path::new(&p.screenshot_dir).ends_with(Path::new("Pictures").join("Morn")),
            "default screenshot dir should live under ~/Pictures/Morn"
        );
        assert!(p.resume_point("/any.mp4").is_none());
    }

    #[test]
    fn default_home_dir_uses_platform_home_preference() {
        #[cfg(windows)]
        assert_eq!(
            home_dir_from_env(
                Some(OsString::from("C:/msys/home/example")),
                Some(OsString::from("C:/Users/example"))
            ),
            Some(PathBuf::from("C:/Users/example"))
        );

        #[cfg(not(windows))]
        assert_eq!(
            home_dir_from_env(
                Some(OsString::from("/home/example")),
                Some(OsString::from("C:/Users/example"))
            ),
            Some(PathBuf::from("/home/example"))
        );

        assert_eq!(
            home_dir_from_env(None, Some(OsString::from("C:/Users/example"))),
            Some(PathBuf::from("C:/Users/example"))
        );
        assert_eq!(
            home_dir_from_env(
                Some(OsString::from("")),
                Some(OsString::from("C:/Users/example"))
            ),
            Some(PathBuf::from("C:/Users/example"))
        );
        assert_eq!(home_dir_from_env(None, None), None);
    }

    #[test]
    fn resolve_screenshot_dir_expands_tilde_prefix() {
        assert_eq!(
            resolve_screenshot_dir("~\\Pictures\\Morn"),
            default_home_dir().join("Pictures").join("Morn")
        );
        assert_eq!(
            resolve_screenshot_dir("~/Pictures/Morn"),
            default_home_dir().join("Pictures").join("Morn")
        );
        assert_eq!(resolve_screenshot_dir(""), PathBuf::new());
        assert_eq!(
            resolve_screenshot_dir("~other/Pictures"),
            PathBuf::from("~other/Pictures")
        );
    }

    #[test]
    fn preferences_include_configurable_screenshot_directory() {
        let source = include_str!("prefs.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        assert!(source.contains("pub screenshot_dir: String"));
        assert!(source.contains("default_screenshot_dir()"));
        assert!(source.contains("Pictures"));
        assert!(source.contains("Morn"));
    }

    #[test]
    fn resume_point_roundtrip() {
        let mut p = Preferences::default();
        p.set_resume_point("/v.mp4", 42_000);
        assert_eq!(p.resume_point("/v.mp4"), Some(42_000));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn save_then_load_roundtrips_all_fields() -> std::io::Result<()> {
        let (_dir, path) = temp_prefs_path("prefs.json")?;
        let mut p = Preferences::default();
        p.volume = 55;
        p.window_size = (1920, 1080);
        p.screenshot_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("shots")
            .to_string_lossy()
            .into_owned();
        p.set_resume_point("/v.mp4", 12_345);
        p.save(&path)?;

        let loaded = Preferences::load(&path)?;
        assert_eq!(loaded.volume, 55);
        assert_eq!(loaded.window_size, (1920, 1080));
        assert_eq!(loaded.screenshot_dir, p.screenshot_dir);
        assert_eq!(loaded.resume_point("/v.mp4"), Some(12_345));
        Ok(())
    }

    #[test]
    fn load_missing_file_returns_default() -> std::io::Result<()> {
        let (_dir, path) = temp_prefs_path("nonexistent.json")?;
        let loaded = Preferences::load(&path)?;
        assert_eq!(loaded.volume, 100);
        Ok(())
    }

    #[test]
    fn load_old_preferences_without_screenshot_dir_uses_default() -> std::io::Result<()> {
        let loaded = load_json_preferences(r#"{"volume":55,"window_size":[1024,768]}"#)?;

        assert_eq!(loaded.volume, 55);
        assert_eq!(
            loaded.screenshot_dir,
            default_screenshot_dir().to_string_lossy()
        );
        Ok(())
    }

    #[test]
    fn load_legacy_tilde_screenshot_dir_expands_to_home() -> std::io::Result<()> {
        let loaded = load_json_preferences(
            r#"{"volume":55,"window_size":[1024,768],"screenshot_dir":"~\\Pictures\\Morn"}"#,
        )?;

        assert_eq!(
            loaded.screenshot_dir,
            default_screenshot_dir().to_string_lossy()
        );
        Ok(())
    }

    #[test]
    fn save_legacy_tilde_screenshot_dir_writes_resolved_path() -> std::io::Result<()> {
        let prefs = Preferences {
            screenshot_dir: "~\\Pictures\\Morn".to_string(),
            ..Default::default()
        };

        let (saved, loaded) = save_read_and_load(&prefs)?;

        assert!(!saved.contains(r#""screenshot_dir": "~\\"#));
        assert_eq!(
            loaded.screenshot_dir,
            default_screenshot_dir().to_string_lossy()
        );
        Ok(())
    }

    #[test]
    fn load_corrupt_json_returns_default() -> std::io::Result<()> {
        let loaded = load_json_preferences("{ not valid json")?;
        assert_eq!(loaded.volume, 100);
        assert_eq!(loaded.window_size, (1280, 720));
        Ok(())
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn new_settings_round_trip() -> std::io::Result<()> {
        let mut p = Preferences::default();
        p.language = "zh-TW".into();
        p.seek_step_secs = 20;
        p.theme = "dark".into();
        p.subtitle_font_size = 30.0;
        p.playback_mode = PlaybackMode::RepeatOne;
        p.check_updates_on_startup = true;
        p.check_beta_updates = true;
        let loaded = save_and_load(&p)?;
        assert_eq!(loaded.language, "zh-TW");
        assert_eq!(loaded.seek_step_secs, 20);
        assert_eq!(loaded.theme, "dark");
        assert_eq!(loaded.subtitle_font_size, 30.0);
        assert_eq!(loaded.playback_mode, PlaybackMode::RepeatOne);
        assert!(loaded.check_updates_on_startup);
        assert!(loaded.check_beta_updates);
        Ok(())
    }

    #[test]
    fn settings_defaults() {
        let p = Preferences::default();
        assert_eq!(p.language, "zh-CN");
        assert_eq!(p.seek_step_secs, 10);
        assert_eq!(p.theme, "system");
        assert_eq!(p.subtitle_font_size, 24.0);
        assert_eq!(p.playback_mode, PlaybackMode::StopAtEnd);
        assert!(!p.check_updates_on_startup);
        assert!(!p.check_beta_updates);
        assert_eq!(p.screenshot_dir, default_screenshot_dir().to_string_lossy());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn playlist_history_round_trip() -> std::io::Result<()> {
        let mut p = Preferences::default();
        p.last_playlist = vec!["/a.mp4".into(), "/b.mp4".into()];
        p.last_index = 1;
        p.history = vec!["/b.mp4".into(), "/a.mp4".into()];
        let loaded = save_and_load(&p)?;
        assert_eq!(
            loaded.last_playlist,
            vec!["/a.mp4".to_string(), "/b.mp4".to_string()]
        );
        assert_eq!(loaded.last_index, 1);
        assert_eq!(
            loaded.history,
            vec!["/b.mp4".to_string(), "/a.mp4".to_string()]
        );
        Ok(())
    }

    #[test]
    fn playlist_history_defaults_empty() {
        let p = Preferences::default();
        assert!(p.last_playlist.is_empty());
        assert_eq!(p.last_index, 0);
        assert!(p.history.is_empty());
    }
}
