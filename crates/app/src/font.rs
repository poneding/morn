use eframe::egui;

/// 按操作系统安装原生字体: 原生 UI 字体作为主字体, 系统中文字体作为回退。
/// egui 自带字体保留在最后兜底, 系统字体缺失时不至于无字可显。
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 原生 UI 字体(拉丁/系统外观): 插到 Proportional 列表最前作为主字体。
    if let Some((key, bytes)) = load_first(UI_FONTS) {
        fonts
            .font_data
            .insert(key.clone(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, key);
    }

    // 中文字体: 追加到 Proportional 与 Monospace 末尾作为回退。
    if let Some((key, bytes)) = load_first(CJK_FONTS) {
        fonts
            .font_data
            .insert(key.clone(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(key.clone());
        }
    } else {
        eprintln!("警告: 未找到系统中文字体, 中文可能无法显示");
    }

    ctx.set_fonts(fonts);
}

/// 依次尝试候选路径, 返回第一个能读取的 (字体键, 字节)。字体键取文件名(去扩展名)。
fn load_first(candidates: &[&str]) -> Option<(String, Vec<u8>)> {
    candidates.iter().find_map(|path| {
        let bytes = std::fs::read(path).ok()?;
        let key = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("sysfont")
            .to_string();
        Some((key, bytes))
    })
}

// 各平台候选字体路径(按优先级)。.ttc 集合按 index 0 加载。

#[cfg(target_os = "macos")]
const UI_FONTS: &[&str] = &["/System/Library/Fonts/SFNS.ttf"]; // SF Pro
#[cfg(target_os = "macos")]
const CJK_FONTS: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
];

#[cfg(target_os = "windows")]
const UI_FONTS: &[&str] = &[r"C:\Windows\Fonts\segoeui.ttf"]; // Segoe UI
#[cfg(target_os = "windows")]
const CJK_FONTS: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",   // 微软雅黑
    r"C:\Windows\Fonts\simhei.ttf", // 黑体
];

#[cfg(target_os = "linux")]
const UI_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];
#[cfg(target_os = "linux")]
const CJK_FONTS: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
];

// 其它平台无候选, 退回 egui 默认字体。
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const UI_FONTS: &[&str] = &[];
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const CJK_FONTS: &[&str] = &[];
