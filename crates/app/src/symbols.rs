//! Shared icon symbols for compact toolbar controls.
//!
//! 物体类图标(喇叭/相机/文件夹等)用 emoji 码点, 由 egui 内嵌的 NotoEmoji-Regular
//! 渲染为黑白单色字形——跨平台一致(不依赖系统彩色 emoji), 比抽象几何符号(◉▣▤)更
//! 直观。几何/控制类符号(▶⏸☰⚙)仍用通用 Unicode。集中在此避免图标散落各处。

pub const PREV: &str = "⏮";
pub const PLAY: &str = "▶";
pub const PAUSE: &str = "⏸";
pub const NEXT: &str = "⏭";
pub const VOLUME: &str = "🔊";
pub const MUTE: &str = "🔇";
pub const FULLSCREEN: &str = "⛶";
pub const RATE_DROPDOWN: &str = "▼";
pub const SCREENSHOT: &str = "📷";
pub const FOLDER: &str = "📁";
pub const REFRESH: &str = "↻";
pub const DOWNLOAD: &str = "⇩";
pub const PLAYLIST: &str = "☰";
pub const SETTINGS: &str = "⚙";
