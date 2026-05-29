use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    pub volume: u8,
    pub window_size: (u32, u32),
    /// 文件路径(字符串) → 续播位置(毫秒)。
    resume_points: HashMap<String, u64>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            volume: 100,
            window_size: (1280, 720),
            resume_points: HashMap::new(),
        }
    }
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
            Ok(s) => Ok(serde_json::from_str(&s)
                .unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// 序列化为 JSON 写入文件。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
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
        assert!(p.resume_point("/any.mp4").is_none());
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
        p.set_resume_point("/v.mp4", 12_345);
        p.save(&path).unwrap();

        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.volume, 55);
        assert_eq!(loaded.window_size, (1920, 1080));
        assert_eq!(loaded.resume_point("/v.mp4"), Some(12_345));
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.volume, 100);
    }
}
