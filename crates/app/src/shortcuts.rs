//! Keyboard shortcut formatting and step helpers.
//!
//! These helpers stay free of `PlayerApp` state so both UI tooltips and shortcut
//! dispatch can share the same platform modifier rules.  Volume and rate snapping
//! also live here because keyboard handling and on-screen notices need identical
//! step labels.

use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutPlatform {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Macos,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Windows,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Linux,
    #[allow(dead_code)]
    Other,
}

#[cfg(target_os = "macos")]
const CURRENT_SHORTCUT_PLATFORM: ShortcutPlatform = ShortcutPlatform::Macos;
#[cfg(target_os = "windows")]
const CURRENT_SHORTCUT_PLATFORM: ShortcutPlatform = ShortcutPlatform::Windows;
#[cfg(target_os = "linux")]
const CURRENT_SHORTCUT_PLATFORM: ShortcutPlatform = ShortcutPlatform::Linux;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const CURRENT_SHORTCUT_PLATFORM: ShortcutPlatform = ShortcutPlatform::Other;

impl ShortcutPlatform {
    pub fn current() -> Self {
        // Compile-time cfg keeps tooltip labels aligned with egui's modifier model
        // for the platform the app was built for.
        CURRENT_SHORTCUT_PLATFORM
    }

    fn is_desktop_navigation(self) -> bool {
        self == Self::Windows || self == Self::Linux
    }
}

pub fn shortcut_tooltip(label: impl AsRef<str>, shortcut: impl AsRef<str>) -> String {
    let label = label.as_ref();
    let shortcut = shortcut.as_ref().trim();
    // Empty shortcuts are allowed for controls that have a hover label but no
    // keyboard equivalent.
    if shortcut.is_empty() {
        label.to_string()
    } else {
        format!("{label} ({shortcut})")
    }
}

fn platform_modifier_label(
    platform: ShortcutPlatform,
    macos: &'static str,
    desktop: &'static str,
    fallback: &'static str,
) -> &'static str {
    if platform == ShortcutPlatform::Macos {
        macos
    } else if platform.is_desktop_navigation() {
        desktop
    } else {
        fallback
    }
}

fn command_modifier_label(platform: ShortcutPlatform) -> &'static str {
    platform_modifier_label(platform, "Cmd", "Ctrl", "Cmd/Ctrl")
}

fn navigation_modifier_label(platform: ShortcutPlatform) -> &'static str {
    platform_modifier_label(platform, "Cmd", "Alt", "Cmd/Alt")
}

pub fn open_shortcut_label() -> String {
    format!("{}+O", command_modifier_label(ShortcutPlatform::current()))
}

pub fn settings_shortcut_label() -> String {
    format!("{}+,", command_modifier_label(ShortcutPlatform::current()))
}

pub fn playlist_shortcut_label() -> String {
    format!("{}+P", command_modifier_label(ShortcutPlatform::current()))
}

pub fn prev_shortcut_label() -> String {
    format!(
        "{}+←",
        navigation_modifier_label(ShortcutPlatform::current())
    )
}

pub fn next_shortcut_label() -> String {
    format!(
        "{}+→",
        navigation_modifier_label(ShortcutPlatform::current())
    )
}

pub fn rate_shortcut_label() -> String {
    format!(
        "{}+↑/↓",
        navigation_modifier_label(ShortcutPlatform::current())
    )
}

/// macOS 使用 Cmd, Windows/Linux 使用 Alt 控制播放列表导航与倍速快捷键。
pub fn navigation_modifier_pressed(platform: ShortcutPlatform, modifiers: egui::Modifiers) -> bool {
    if platform.is_desktop_navigation() {
        modifiers.alt
    } else {
        modifiers.command
    }
}

/// 音量上调到下一个 5 的倍数(已是倍数则 +5), clamp 100。例: 43→45, 45→50。
pub fn snap_volume_up(vol: u8) -> u8 {
    (((vol / 5) + 1) * 5).min(100)
}

/// 音量下调到上一个 5 的倍数(已是倍数则 -5), clamp 0。例: 43→40, 45→40, 40→35。
pub fn snap_volume_down(vol: u8) -> u8 {
    (vol.saturating_sub(1) / 5) * 5
}

/// 倍速上调到菜单中的下一档。
pub fn snap_rate_up(rate_pct: u16) -> u16 {
    // Rate steps are shared with the rate popup so keyboard shortcuts never land
    // on a value the menu cannot display as selected.
    crate::enhance::RATE_OPTIONS
        .iter()
        .copied()
        .find(|pct| *pct > rate_pct)
        .unwrap_or(*crate::enhance::RATE_OPTIONS.last().unwrap_or(&rate_pct))
}

/// 倍速下调到菜单中的上一档。
pub fn snap_rate_down(rate_pct: u16) -> u16 {
    // Reverse iteration mirrors snap_rate_up while preserving the lower bound from
    // the configured menu options.
    crate::enhance::RATE_OPTIONS
        .iter()
        .copied()
        .rev()
        .find(|pct| *pct < rate_pct)
        .unwrap_or(*crate::enhance::RATE_OPTIONS.first().unwrap_or(&rate_pct))
}

pub fn format_rate_label(rate_pct: u16) -> String {
    // Notices use compact decimal labels: 1x, 1.5x, 1.25x.
    let whole = rate_pct / 100;
    let frac = rate_pct % 100;
    if frac == 0 {
        format!("{whole}x")
    } else if frac.is_multiple_of(10) {
        format!("{whole}.{}x", frac / 10)
    } else {
        format!("{whole}.{frac:02}x")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_rate_label, navigation_modifier_pressed, shortcut_tooltip, snap_rate_down,
        snap_rate_up, snap_volume_down, snap_volume_up, ShortcutPlatform,
    };

    #[test]
    fn shortcut_tooltip_appends_shortcut_when_present() {
        assert_eq!(shortcut_tooltip("Play", "Space"), "Play (Space)");
        assert_eq!(shortcut_tooltip("Screenshot", ""), "Screenshot");
    }

    #[test]
    fn up_snaps_to_next_multiple_of_5() {
        assert_eq!(snap_volume_up(43), 45);
        assert_eq!(snap_volume_up(45), 50);
        assert_eq!(snap_volume_up(40), 45);
        assert_eq!(snap_volume_up(98), 100);
        assert_eq!(snap_volume_up(100), 100);
    }

    #[test]
    fn down_snaps_to_prev_multiple_of_5() {
        assert_eq!(snap_volume_down(43), 40);
        assert_eq!(snap_volume_down(45), 40);
        assert_eq!(snap_volume_down(40), 35);
        assert_eq!(snap_volume_down(3), 0);
        assert_eq!(snap_volume_down(0), 0);
    }

    #[test]
    fn rate_up_uses_existing_menu_steps() {
        assert_eq!(snap_rate_up(100), 125);
        assert_eq!(snap_rate_up(125), 150);
        assert_eq!(snap_rate_up(126), 150);
        assert_eq!(snap_rate_up(199), 200);
        assert_eq!(snap_rate_up(200), 200);
    }

    #[test]
    fn rate_down_uses_existing_menu_steps() {
        assert_eq!(snap_rate_down(150), 125);
        assert_eq!(snap_rate_down(126), 125);
        assert_eq!(snap_rate_down(100), 75);
        assert_eq!(snap_rate_down(26), 25);
        assert_eq!(snap_rate_down(25), 25);
    }

    #[test]
    fn rate_notice_trims_trailing_zeroes() {
        assert_eq!(format_rate_label(100), "1x");
        assert_eq!(format_rate_label(150), "1.5x");
        assert_eq!(format_rate_label(125), "1.25x");
    }

    #[test]
    fn playlist_navigation_modifier_is_platform_specific() {
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let alt = egui::Modifiers {
            alt: true,
            ..Default::default()
        };

        assert!(navigation_modifier_pressed(
            ShortcutPlatform::Macos,
            command
        ));
        assert!(!navigation_modifier_pressed(ShortcutPlatform::Macos, alt));
        assert!(navigation_modifier_pressed(ShortcutPlatform::Windows, alt));
        assert!(navigation_modifier_pressed(ShortcutPlatform::Linux, alt));
        assert!(!navigation_modifier_pressed(
            ShortcutPlatform::Windows,
            command
        ));
        assert!(!navigation_modifier_pressed(
            ShortcutPlatform::Linux,
            command
        ));
    }
}
