use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub volume: u8,
    pub window_size: (u32, u32),
    pub language: String,
    pub seek_step_secs: u64,
    pub theme: String,
    pub subtitle_font_size: f32,
    pub playback_mode: PlaybackMode,
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
            screenshot_dir: default_screenshot_dir_string(),
            last_playlist: Vec::new(),
            last_index: 0,
            history: Vec::new(),
            resume_points: HashMap::new(),
        }
    }
}

pub fn default_screenshot_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("Pictures")
        .join("Morn")
}

fn default_screenshot_dir_string() -> String {
    default_screenshot_dir().to_string_lossy().into_owned()
}

impl Preferences {
    pub fn resume_point(&self, file: &str) -> Option<u64> {
        self.resume_points.get(file).copied()
    }

    pub fn set_resume_point(&mut self, file: &str, ms: u64) {
        self.resume_points.insert(file.to_string(), ms);
    }

    /// 从 JSON 文件加载。文件不存在时返回默认值(非错误)。
    pub fn load(path: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            // 解析失败按默认处理: 偏好属低风险数据, 不阻断启动。
            Ok(s) => Ok(serde_json::from_str(&s).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// 序列化为 JSON 写入文件。自动创建父目录。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn preferences_include_configurable_screenshot_directory() {
        let source = include_str!("prefs.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

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
    fn save_then_load_roundtrips_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");

        let mut p = Preferences::default();
        p.volume = 55;
        p.window_size = (1920, 1080);
        p.screenshot_dir = dir.path().join("shots").to_string_lossy().into_owned();
        p.set_resume_point("/v.mp4", 12_345);
        p.save(&path).unwrap();

        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.volume, 55);
        assert_eq!(loaded.window_size, (1920, 1080));
        assert_eq!(loaded.screenshot_dir, p.screenshot_dir);
        assert_eq!(loaded.resume_point("/v.mp4"), Some(12_345));
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.volume, 100);
    }

    #[test]
    fn load_old_preferences_without_screenshot_dir_uses_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.json");
        std::fs::write(&path, r#"{"volume":55,"window_size":[1024,768]}"#).unwrap();

        let loaded = Preferences::load(&path).unwrap();

        assert_eq!(loaded.volume, 55);
        assert_eq!(
            loaded.screenshot_dir,
            default_screenshot_dir().to_string_lossy()
        );
    }

    #[test]
    fn load_corrupt_json_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.json");
        std::fs::write(&path, "{ not valid json").unwrap();
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.volume, 100);
        assert_eq!(loaded.window_size, (1280, 720));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn new_settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let mut p = Preferences::default();
        p.language = "zh-TW".into();
        p.seek_step_secs = 20;
        p.theme = "dark".into();
        p.subtitle_font_size = 30.0;
        p.playback_mode = PlaybackMode::RepeatOne;
        p.save(&path).unwrap();
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.language, "zh-TW");
        assert_eq!(loaded.seek_step_secs, 20);
        assert_eq!(loaded.theme, "dark");
        assert_eq!(loaded.subtitle_font_size, 30.0);
        assert_eq!(loaded.playback_mode, PlaybackMode::RepeatOne);
    }

    #[test]
    fn settings_defaults() {
        let p = Preferences::default();
        assert_eq!(p.language, "zh-CN");
        assert_eq!(p.seek_step_secs, 10);
        assert_eq!(p.theme, "system");
        assert_eq!(p.subtitle_font_size, 24.0);
        assert_eq!(p.playback_mode, PlaybackMode::StopAtEnd);
        assert_eq!(p.screenshot_dir, default_screenshot_dir().to_string_lossy());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn playlist_history_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let mut p = Preferences::default();
        p.last_playlist = vec!["/a.mp4".into(), "/b.mp4".into()];
        p.last_index = 1;
        p.history = vec!["/b.mp4".into(), "/a.mp4".into()];
        p.save(&path).unwrap();
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(
            loaded.last_playlist,
            vec!["/a.mp4".to_string(), "/b.mp4".to_string()]
        );
        assert_eq!(loaded.last_index, 1);
        assert_eq!(
            loaded.history,
            vec!["/b.mp4".to_string(), "/a.mp4".to_string()]
        );
    }

    #[test]
    fn playlist_history_defaults_empty() {
        let p = Preferences::default();
        assert!(p.last_playlist.is_empty());
        assert_eq!(p.last_index, 0);
        assert!(p.history.is_empty());
    }
}
