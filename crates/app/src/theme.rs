//! 集中化视觉主题：配色 / 强调色 / 圆角 / 控件质感（**不涉及布局**）。
//!
//! 全应用的零散组件几乎都从 `ctx` 的 [`egui::Visuals`] 派生颜色（控制栏、
//! 浮层、侧栏、设置窗口都是），所以这里覆写一套自定义 `Visuals`（暗 + 亮）
//! 即可重塑整体观感，无需逐个组件改色。
//!
//! 想整体换强调色方向（青 / 紫 / 绿…），只改 [`PALETTE_DARK`] /
//! [`PALETTE_LIGHT`] 里的 `accent` / `on_accent` 与几个基色即可，结构不动。
//!
//! egui 0.34 着色映射（已对源码核实，是本模块的设计依据）：
//! - 进度条 / 时间轴**已播填充** = `selection.bg_fill`
//! - 时间轴**轨道** + 滑块(静止) = `widgets.inactive.bg_fill`
//! - 滑块(hover/拖动) = `widgets.{hovered,active}.bg_fill`
//! - **按钮**底 = `widgets.{state}.weak_bg_fill`（与上面 `bg_fill` 解耦）
//! - 按钮/图标/正文**文字** = `widgets.{state}.fg_stroke.color`
//! - 选中项(☰/⚙ 选中、当前列表行、选中下拉项)底 = `selection.bg_fill`，
//!   其**文字** = `selection.stroke.color`（`interact_selectable` 用它当 fg）

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke};

const WIDGET_RADIUS: u8 = crate::visuals::CONTROL_CORNER_RADIUS;
const WINDOW_RADIUS: u8 = crate::visuals::PANEL_CORNER_RADIUS;
/// 时间轴/音量轨道的视觉粗细（不改控件占位高度，故不影响布局）。
const SLIDER_RAIL_HEIGHT: f32 = 4.0;

/// 一套配色的语义令牌。颜色之外的结构（圆角 / 阴影 / 描边粗细）由
/// [`Palette::apply`] 统一处理，两套配色共享。
struct Palette {
    is_dark: bool,
    // 背景层级：纸面 → 深井
    base: Color32,    // 窗口 / 面板底
    surface: Color32, // 浮层 / 菜单 / 设置窗口底
    sunken: Color32,  // 文本框 / 代码块等深井
    faint: Color32,   // 斑马纹 / 极弱高亮
    // 文本层级
    text: Color32,        // 正文
    text_muted: Color32,  // 次级 / 空闲图标
    text_strong: Color32, // hover 高亮
    // 按钮表面（weak_bg_fill，与滑条解耦）
    surface_idle: Color32,
    surface_hover: Color32,
    surface_active: Color32,
    // 填充元素（bg_fill：滑条轨静止 / 复选框静止底）
    rail: Color32,
    // 描边
    border: Color32,
    // 强调
    accent: Color32,    // 进度填充 + 选中底
    on_accent: Color32, // 选中文字（amber 上的深墨）
    // 状态
    warn: Color32,
    error: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// 石墨金属（暗）—— 应用唯一主题（仅深色）。
const PALETTE_DARK: Palette = Palette {
    is_dark: true,
    base: rgb(0x18, 0x18, 0x17),
    surface: rgb(0x25, 0x24, 0x21),
    sunken: rgb(0x10, 0x10, 0x0F),
    faint: rgb(0x2A, 0x29, 0x26),
    text: rgb(0xE8, 0xE3, 0xDA),
    text_muted: rgb(0xB7, 0xB0, 0xA4),
    text_strong: rgb(0xFF, 0xFA, 0xF0),
    surface_idle: rgb(0x32, 0x30, 0x2C),
    surface_hover: rgb(0x3E, 0x3B, 0x35),
    surface_active: rgb(0x24, 0x22, 0x20),
    rail: rgb(0x40, 0x3D, 0x37),
    border: rgb(0x12, 0x12, 0x10),
    accent: rgb(0xD8, 0xA1, 0x40),
    on_accent: rgb(0x16, 0x12, 0x0D),
    warn: rgb(0xE7, 0xB7, 0x54),
    error: rgb(0xEF, 0x73, 0x65),
};

/// 安装自定义主题。应用**仅深色**: 两个主题槽都写入深色 `Style`, 这样即便
/// 系统或代码切到 Light 也始终是同一套深色, 不会闪白。`install` 只需启动时调一次。
pub fn install(ctx: &egui::Context) {
    ctx.style_mut_of(egui::Theme::Dark, |style| PALETTE_DARK.apply(style));
    ctx.style_mut_of(egui::Theme::Light, |style| PALETTE_DARK.apply(style));
}

impl Palette {
    fn apply(&self, style: &mut egui::Style) {
        let widget_radius = CornerRadius::same(WIDGET_RADIUS);
        let window_radius = CornerRadius::same(WINDOW_RADIUS);
        let v = &mut style.visuals;

        v.dark_mode = self.is_dark;
        v.panel_fill = self.base;
        v.window_fill = self.surface;
        v.extreme_bg_color = self.sunken;
        v.faint_bg_color = self.faint;
        v.code_bg_color = self.sunken;
        v.warn_fg_color = self.warn;
        v.error_fg_color = self.error;
        v.hyperlink_color = self.accent;

        // 强调：进度填充 / 选中底 = accent；选中文字 = on_accent（深墨压 amber，保对比度）
        v.selection.bg_fill = self.accent;
        v.selection.stroke = Stroke::new(1.0, self.on_accent);

        // 窗口 / 浮层：统一圆角、细描边、克制阴影。
        v.window_corner_radius = window_radius;
        v.menu_corner_radius = widget_radius;
        v.window_stroke = Stroke::new(1.0, self.border);
        v.window_shadow = self.shadow([0, 7], 18, 135, 38);
        v.popup_shadow = self.shadow([0, 5], 14, 115, 30);

        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;

        let w = &mut v.widgets;

        // 非交互：正文 / 标签 / 分隔线 / 面板描边
        w.noninteractive.bg_fill = self.base;
        w.noninteractive.weak_bg_fill = self.base;
        w.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        w.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        w.noninteractive.corner_radius = widget_radius;

        // 空闲按钮(weak_bg_fill) / 滑条轨 + 滑块静止(bg_fill)
        w.inactive.weak_bg_fill = self.surface_idle;
        w.inactive.bg_fill = self.rail;
        w.inactive.bg_stroke = Stroke::new(1.0, self.border);
        w.inactive.fg_stroke = Stroke::new(1.0, self.text_muted);
        w.inactive.corner_radius = widget_radius;
        w.inactive.expansion = 0.0;

        // hover：按钮底(weak_bg_fill)提亮成石墨；滑块/复选框(bg_fill)点亮成
        // amber 强调色——二者取色解耦，所以划过按钮不会被点成强调色，而滑块游标
        // / 复选框在 hover/选中时是醒目的橙色而非纯白/纯黑（看得清状态）。
        w.hovered.weak_bg_fill = self.surface_hover;
        w.hovered.bg_fill = self.accent;
        w.hovered.bg_stroke = Stroke::new(1.0, self.border);
        w.hovered.fg_stroke = Stroke::new(1.5, self.text_strong);
        w.hovered.corner_radius = widget_radius;
        w.hovered.expansion = 0.0;

        // active：按下 / 拖动，滑块/复选框仍是 amber，描边也去掉
        w.active.weak_bg_fill = self.surface_active;
        w.active.bg_fill = self.accent;
        w.active.bg_stroke = Stroke::new(1.0, self.border);
        w.active.fg_stroke = Stroke::new(1.5, self.text_strong);
        w.active.corner_radius = widget_radius;
        w.active.expansion = 0.0;

        // open：下拉 / 菜单按钮展开态
        w.open.weak_bg_fill = self.surface_active;
        w.open.bg_fill = self.surface_active;
        w.open.bg_stroke = Stroke::new(1.0, self.border);
        w.open.fg_stroke = Stroke::new(1.0, self.text);
        w.open.corner_radius = widget_radius;
        w.open.expansion = 0.0;
        w.open.expansion = 0.0;

        // 仅视觉精修、不动布局：时间轴/音量轨道收薄
        style.spacing.slider_rail_height = SLIDER_RAIL_HEIGHT;
    }

    fn shadow(&self, offset: [i8; 2], blur: u8, dark_alpha: u8, light_alpha: u8) -> Shadow {
        Shadow {
            offset,
            blur,
            spread: 0,
            color: Color32::from_black_alpha(if self.is_dark {
                dark_alpha
            } else {
                light_alpha
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(palette: &Palette) -> egui::Style {
        let mut style = egui::Style::default();
        palette.apply(&mut style);
        style
    }

    fn luminance(c: Color32) -> f32 {
        0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b())
    }

    #[test]
    fn selection_uses_accent_fill_and_on_accent_text() {
        let style = styled(&PALETTE_DARK);
        assert_eq!(style.visuals.selection.bg_fill, PALETTE_DARK.accent);
        assert_eq!(style.visuals.selection.stroke.color, PALETTE_DARK.on_accent);
    }

    #[test]
    fn timeline_progress_is_accent_and_rail_is_distinct() {
        // 进度填充取 selection.bg_fill；轨道取 widgets.inactive.bg_fill。
        let style = styled(&PALETTE_DARK);
        assert_eq!(style.visuals.selection.bg_fill, PALETTE_DARK.accent);
        assert_eq!(style.visuals.widgets.inactive.bg_fill, PALETTE_DARK.rail);
        assert_ne!(
            style.visuals.selection.bg_fill,
            style.visuals.widgets.inactive.bg_fill
        );
    }

    #[test]
    fn button_surface_and_slider_fill_are_decoupled_on_hover() {
        // 按钮底走 weak_bg_fill、滑块走 bg_fill：hover 时不应是同色，
        // 否则鼠标划过按钮会把它点亮成滑块那种高亮。
        let style = styled(&PALETTE_DARK);
        assert_ne!(
            style.visuals.widgets.hovered.weak_bg_fill,
            style.visuals.widgets.hovered.bg_fill
        );
    }

    #[test]
    fn widgets_and_window_are_rounded() {
        let style = styled(&PALETTE_DARK);
        assert_eq!(
            style.visuals.widgets.inactive.corner_radius,
            CornerRadius::same(WIDGET_RADIUS)
        );
        assert_eq!(
            style.visuals.window_corner_radius,
            CornerRadius::same(WINDOW_RADIUS)
        );
    }

    #[test]
    fn dark_flag_maps_to_visuals_dark_mode() {
        assert!(styled(&PALETTE_DARK).visuals.dark_mode);
    }

    #[test]
    fn selected_text_has_strong_contrast_against_accent() {
        // on_accent 与 accent 亮度差足够大，保证「amber 块上的文字」可读。
        let diff = (luminance(PALETTE_DARK.accent) - luminance(PALETTE_DARK.on_accent)).abs();
        assert!(diff > 90.0, "选中态对比度不足: {diff}");
    }

    #[test]
    fn install_writes_dark_style_into_both_theme_slots() {
        // 仅深色: 两个主题槽都应是深色, 切到 Light 也不会闪白。
        let ctx = egui::Context::default();
        install(&ctx);
        assert_eq!(
            ctx.style_of(egui::Theme::Dark).visuals.selection.bg_fill,
            PALETTE_DARK.accent
        );
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.selection.bg_fill,
            PALETTE_DARK.accent
        );
        assert!(ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
        assert!(ctx.style_of(egui::Theme::Light).visuals.dark_mode);
    }
}
