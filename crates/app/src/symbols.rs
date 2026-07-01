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

// 窗口控制符号: 非 macOS 无边框窗口下自绘的最小化/最大化/还原/关闭按钮。
// 用通用 Unicode 几何符号, 跨平台一致渲染。
// 仅非 macOS 引用, macOS 走原生交通灯, 故 allow 死代码告警。
#[allow(dead_code)]
pub const WINDOW_MINIMIZE: &str = "–";
#[allow(dead_code)]
pub const WINDOW_MAXIMIZE: &str = "▢";
#[allow(dead_code)]
pub const WINDOW_RESTORE: &str = "❐";
#[allow(dead_code)]
pub const WINDOW_CLOSE: &str = "✕";
