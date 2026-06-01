use eframe::egui;

/// 按操作系统安装原生字体: 原生 UI 字体作为主字体, 系统中文字体作为回退。
/// egui 自带字体保留在最后兜底, 系统字体缺失时不至于无字可显。
pub fn install_fonts(ctx: &egui::Context, locale: &str) {
    let mut fonts = egui::FontDefinitions::default();

    // 原生 UI 字体(拉丁/系统外观): 插到 Proportional 列表最前作为主字体。
    if let Some((key, font_data)) = load_first(UI_FONTS) {
        fonts.font_data.insert(key.clone(), font_data.into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, key);
    }

    // 中文字体: 按 locale 选择 SC/TC 优先字体, 避免 CJK 统一字走到 JP 字形。
    if let Some((key, font_data)) = load_first(&cjk_font_candidates(locale)) {
        fonts.font_data.insert(key.clone(), font_data.into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(key.clone());
        }
    } else {
        eprintln!("警告: 未找到系统中文字体, 中文可能无法显示");
    }

    ctx.set_fonts(fonts);
}

#[derive(Clone, Copy)]
struct FontCandidate {
    key: &'static str,
    path: &'static str,
    index: u32,
}

/// 依次尝试候选路径, 返回第一个能读取的字体。
fn load_first(candidates: &[FontCandidate]) -> Option<(String, egui::FontData)> {
    candidates.iter().copied().find_map(load_font)
}

fn load_font(candidate: FontCandidate) -> Option<(String, egui::FontData)> {
    let bytes = std::fs::read(resolve_font_path(candidate.path)?).ok()?;
    let mut font_data = egui::FontData::from_owned(bytes);
    font_data.index = candidate.index;
    Some((font_key(candidate), font_data))
}

fn font_key(candidate: FontCandidate) -> String {
    format!("{}-{}", candidate.key, candidate.index)
}

fn resolve_font_path(path: &str) -> Option<std::path::PathBuf> {
    let Some(file_name) = path.strip_prefix(MACOS_MOBILE_ASSET_FONT_PREFIX) else {
        return Some(std::path::PathBuf::from(path));
    };
    find_macos_mobile_asset_font_at(std::path::Path::new("/System/Library/AssetsV2"), file_name)
}

fn find_macos_mobile_asset_font_at(
    root: &std::path::Path,
    file_name: &str,
) -> Option<std::path::PathBuf> {
    let mut matches = Vec::new();
    for bundle in std::fs::read_dir(root).ok()?.flatten() {
        let Ok(file_type) = bundle.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let bundle_name = bundle.file_name();
        let Some(bundle_name) = bundle_name.to_str() else {
            continue;
        };
        if !bundle_name.starts_with("com_apple_MobileAsset_Font") {
            continue;
        }
        let Ok(assets) = std::fs::read_dir(bundle.path()) else {
            continue;
        };
        for asset in assets.flatten() {
            let candidate = asset.path().join("AssetData").join(file_name);
            if candidate.is_file() {
                matches.push(candidate);
            }
        }
    }
    matches.sort();
    matches.into_iter().next()
}

fn cjk_font_candidates(locale: &str) -> Vec<FontCandidate> {
    if locale == "zh-TW" {
        let mut candidates = TC_CJK_FONTS.to_vec();
        candidates.extend_from_slice(SC_CJK_FONTS);
        candidates
    } else {
        let mut candidates = SC_CJK_FONTS.to_vec();
        candidates.extend_from_slice(TC_CJK_FONTS);
        candidates
    }
}

// 各平台候选字体路径(按优先级)。.ttc 集合显式指定 SC/TC 子字体 index。
const MACOS_MOBILE_ASSET_FONT_PREFIX: &str = "macos-mobile-asset:";

#[cfg(target_os = "macos")]
const UI_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "sf-pro",
        path: "/System/Library/Fonts/SFNS.ttf",
        index: 0,
    },
    FontCandidate {
        key: "helvetica-neue",
        path: "/System/Library/Fonts/HelveticaNeue.ttc",
        index: 0,
    },
    FontCandidate {
        key: "helvetica",
        path: "/System/Library/Fonts/Helvetica.ttc",
        index: 0,
    },
]; // SFNS 是 macOS 暴露给非原生渲染栈使用的系统 UI 字体文件, Helvetica 作为兜底。
#[cfg(target_os = "macos")]
const SC_CJK_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "pingfang-sc",
        path: "macos-mobile-asset:PingFang.ttc",
        index: 3,
    },
    FontCandidate {
        key: "heiti-sc-light",
        path: "/System/Library/Fonts/STHeiti Light.ttc",
        index: 1,
    },
    FontCandidate {
        key: "heiti-sc-medium",
        path: "/System/Library/Fonts/STHeiti Medium.ttc",
        index: 1,
    },
    FontCandidate {
        key: "hiragino-sans-gb",
        path: "/System/Library/Fonts/Hiragino Sans GB.ttc",
        index: 0,
    },
    FontCandidate {
        key: "arial-unicode",
        path: "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        index: 0,
    },
];
#[cfg(target_os = "macos")]
const TC_CJK_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "pingfang-tc",
        path: "macos-mobile-asset:PingFang.ttc",
        index: 2,
    },
    FontCandidate {
        key: "heiti-tc-light",
        path: "/System/Library/Fonts/STHeiti Light.ttc",
        index: 0,
    },
    FontCandidate {
        key: "heiti-tc-medium",
        path: "/System/Library/Fonts/STHeiti Medium.ttc",
        index: 0,
    },
    FontCandidate {
        key: "songti-tc-regular",
        path: "/System/Library/Fonts/Supplemental/Songti.ttc",
        index: 7,
    },
    FontCandidate {
        key: "arial-unicode",
        path: "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        index: 0,
    },
];

#[cfg(target_os = "windows")]
const UI_FONTS: &[FontCandidate] = &[FontCandidate {
    key: "segoe-ui",
    path: r"C:\Windows\Fonts\segoeui.ttf",
    index: 0,
}]; // Segoe UI
#[cfg(target_os = "windows")]
const SC_CJK_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "microsoft-yahei",
        path: r"C:\Windows\Fonts\msyh.ttc",
        index: 0,
    },
    FontCandidate {
        key: "simhei",
        path: r"C:\Windows\Fonts\simhei.ttf",
        index: 0,
    },
];
#[cfg(target_os = "windows")]
const TC_CJK_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "microsoft-jhenghei",
        path: r"C:\Windows\Fonts\msjh.ttc",
        index: 0,
    },
    FontCandidate {
        key: "mingliu",
        path: r"C:\Windows\Fonts\mingliu.ttc",
        index: 0,
    },
];

#[cfg(target_os = "linux")]
const UI_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "noto-sans",
        path: "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        index: 0,
    },
    FontCandidate {
        key: "dejavu-sans-ttf",
        path: "/usr/share/fonts/TTF/DejaVuSans.ttf",
        index: 0,
    },
    FontCandidate {
        key: "dejavu-sans",
        path: "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        index: 0,
    },
];
#[cfg(target_os = "linux")]
const SC_CJK_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "noto-sans-cjk-sc",
        path: "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        index: 2,
    },
    FontCandidate {
        key: "noto-sans-cjk-sc",
        path: "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        index: 2,
    },
    FontCandidate {
        key: "wqy-microhei",
        path: "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        index: 0,
    },
];
#[cfg(target_os = "linux")]
const TC_CJK_FONTS: &[FontCandidate] = &[
    FontCandidate {
        key: "noto-sans-cjk-tc",
        path: "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        index: 3,
    },
    FontCandidate {
        key: "noto-sans-cjk-tc",
        path: "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        index: 3,
    },
];

// 其它平台无候选, 退回 egui 默认字体。
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const UI_FONTS: &[FontCandidate] = &[];
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const SC_CJK_FONTS: &[FontCandidate] = &[];
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const TC_CJK_FONTS: &[FontCandidate] = &[];

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_ui_font_prefers_sf_system_font() {
        assert_eq!(super::UI_FONTS[0].key, "sf-pro");
        assert_eq!(super::UI_FONTS[0].path, "/System/Library/Fonts/SFNS.ttf");
        assert_eq!(super::UI_FONTS[0].index, 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_cjk_font_order_prefers_sc_or_tc_before_unicode_fallback() {
        let sc = super::cjk_font_candidates("zh-CN");
        let tc = super::cjk_font_candidates("zh-TW");

        assert!(sc[0].key.contains("sc"));
        assert!(tc[0].key.contains("tc"));
        assert!(!sc[0].path.contains("Arial Unicode"));
        assert!(!tc[0].path.contains("Arial Unicode"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_cjk_font_order_prefers_pingfang() {
        let sc = super::cjk_font_candidates("zh-CN");
        let tc = super::cjk_font_candidates("zh-TW");

        assert_eq!(sc[0].key, "pingfang-sc");
        assert_eq!(sc[0].index, 3);
        assert_eq!(tc[0].key, "pingfang-tc");
        assert_eq!(tc[0].index, 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_noto_cjk_uses_sc_and_tc_collection_indices() {
        let sc = super::cjk_font_candidates("zh-CN");
        let tc = super::cjk_font_candidates("zh-TW");

        assert_eq!(sc[0].index, 2);
        assert_eq!(tc[0].index, 3);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_font_candidates_use_platform_defaults() {
        assert_eq!(super::UI_FONTS[0].key, "segoe-ui");
        assert_eq!(super::SC_CJK_FONTS[0].key, "microsoft-yahei");
        assert_eq!(super::TC_CJK_FONTS[0].key, "microsoft-jhenghei");
    }

    #[test]
    fn mobile_asset_font_search_finds_asset_data_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "morn-mobile-asset-font-test-{}-{unique}",
            std::process::id()
        ));
        let asset = root
            .join("com_apple_MobileAsset_Font7")
            .join("demo.asset")
            .join("AssetData")
            .join("PingFang.ttc");
        std::fs::create_dir_all(asset.parent().unwrap()).unwrap();
        std::fs::write(&asset, b"font").unwrap();

        assert_eq!(
            super::find_macos_mobile_asset_font_at(&root, "PingFang.ttc").as_deref(),
            Some(asset.as_path())
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn font_keys_include_collection_index() {
        let candidate = super::FontCandidate {
            key: "demo",
            path: "/tmp/demo.ttc",
            index: 3,
        };
        assert_eq!(super::font_key(candidate), "demo-3");
    }
}
