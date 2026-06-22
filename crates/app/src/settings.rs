//! Settings window sections and preference bindings.
//!
//! This module owns the immediate-mode controls for user preferences, but it does
//! not persist files directly.  Each row edits a local copy, compares it against
//! `Player::prefs()`, and delegates changes back through `Player` setters.  That
//! keeps validation, persistence, and playback side effects in the engine-facing
//! layer while the window remains a thin UI surface.
//!
//! Sections are ordered to match how users scan the dialog: appearance, playback,
//! screenshots, updates, then subtitles.  Shared row sizing lives in
//! `settings_row` so localized labels and controls keep a consistent two-column
//! layout as new settings are added.

use eframe::egui;
use engine::{PlaybackMode, Player};
use rust_i18n::t;

const SETTINGS_WINDOW_MARGIN_X: i8 = 20;
const SETTINGS_WINDOW_MARGIN_Y: i8 = 16;
const SETTINGS_CONTENT_WIDTH: f32 = 420.0;
const SETTINGS_ROW_SPACING: f32 = crate::visuals::FLOATING_PANEL_MARGIN;
const SETTINGS_ROW_HEIGHT: f32 = crate::visuals::ICON_BUTTON_SIZE;
const SETTINGS_CLOSE_BUTTON_SIZE: f32 = 22.0;
const SETTINGS_CLOSE_ICON_INSET: f32 = 8.2;
const SETTINGS_CLOSE_ICON_STROKE: f32 = 1.35;
const SETTINGS_CHECKBOX_SIZE: f32 = 20.0;
const SETTINGS_CHECKBOX_INNER_SIZE: f32 = 12.0;
const SETTINGS_TAB_PADDING_X: f32 = 14.0;
const SETTINGS_SECTION_GAP: f32 = 16.0;
const SETTINGS_HEADING_SIZE: f32 = 13.0;
const SETTINGS_HEADING_GAP: f32 = 8.0;
const SETTINGS_TITLE_SIZE: f32 = 16.0;
const SETTINGS_COMBO_WIDTH: f32 = 176.0;
const SETTINGS_COMBO_POPUP_HEIGHT: f32 = SETTINGS_ROW_HEIGHT * 4.25;
/// 设置窗口底部与浮动控制栏之间至少留出的安全间距, 保证内容不被控制栏遮挡。
const SETTINGS_CONTROL_BAR_SAFE_GAP: f32 = 96.0;
/// 内容区(滚动区)的固定高度: 切换 Tab 时窗口高度保持不变, 不再随内容多少跳动。
/// 屏幕过矮时会被 `settings_body_height` 夹小以保证不超出安全区。
const SETTINGS_BODY_HEIGHT: f32 = 340.0;
const SETTINGS_BODY_MIN_HEIGHT: f32 = 180.0;
/// 标题行 + Tab 行 + 各处留白占用的竖向空间(用于从可用高度里扣除)。
const SETTINGS_BODY_OVERHEAD: f32 = 118.0;

/// 设置面板顶部分组 Tab。分组让面板更清晰、高度更可控。
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SettingsTab {
    #[default]
    Appearance,
    Playback,
    Updates,
    Subtitle,
}

impl SettingsTab {
    const ALL: [SettingsTab; 4] = [
        SettingsTab::Appearance,
        SettingsTab::Playback,
        SettingsTab::Updates,
        SettingsTab::Subtitle,
    ];

    fn label(self) -> String {
        match self {
            SettingsTab::Appearance => t!("appearance").to_string(),
            SettingsTab::Playback => t!("playback").to_string(),
            SettingsTab::Updates => t!("updates").to_string(),
            SettingsTab::Subtitle => t!("subtitle").to_string(),
        }
    }
}

fn settings_tab_state(ctx: &egui::Context) -> SettingsTab {
    let id = egui::Id::new("settings_active_tab");
    ctx.data(|d| d.get_temp::<SettingsTab>(id).unwrap_or_default())
}

fn set_settings_tab_state(ctx: &egui::Context, tab: SettingsTab) {
    let id = egui::Id::new("settings_active_tab");
    ctx.data_mut(|d| d.insert_temp(id, tab));
}

/// 绘制设置窗口。`open` 控制显隐, 直接读写 player 的偏好。
pub fn settings_window(
    ctx: &egui::Context,
    open: &mut bool,
    player: &mut Player,
    update_check: &mut crate::updater::UpdateChecker,
) {
    if !*open {
        return;
    }
    // 自绘紧凑标题(关掉 egui 自带的高标题栏); 内容区固定高度 + 滚动,
    // 切换 Tab 不改变窗口高度, 竖直空间不足时底部也不会被浮动控制栏遮挡。
    let inner_margin = egui::Margin::symmetric(SETTINGS_WINDOW_MARGIN_X, SETTINGS_WINDOW_MARGIN_Y);
    let panel_frame = crate::visuals::panel_frame_for_style(&ctx.global_style(), inner_margin);
    let max_height = settings_max_height(ctx);
    let body_height = settings_body_height(ctx);
    let mut active_tab = settings_tab_state(ctx);
    egui::Window::new("settings")
        .id(egui::Id::new("settings_window"))
        .title_bar(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .max_height(max_height)
        .frame(panel_frame)
        .show(ctx, |ui| {
            ui.set_width(SETTINGS_CONTENT_WIDTH);
            ui.spacing_mut().item_spacing.y = SETTINGS_ROW_SPACING;
            settings_header(ui, open);
            settings_tab_bar(ui, &mut active_tab);
            ui.add_space(SETTINGS_HEADING_GAP);
            // 固定高度的内容区: 用 allocate_ui 预留满高度, 内部滚动条按需出现。
            ui.allocate_ui_with_layout(
                egui::vec2(SETTINGS_CONTENT_WIDTH, body_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(SETTINGS_CONTENT_WIDTH);
                            settings_tab_body(ui, active_tab, player, update_check);
                        });
                },
            );
        });
    // 设置面板与播放列表同属 Foreground; egui 同 Order 内的层级会随打开/点击顺序
    // 动态变化(area.rs: 每次可见或被点击都 move_to_top), 无法保证设置始终在上。
    // 故每帧显式把设置提到 Foreground 最上层, 确保它盖在播放列表之上。
    ctx.memory_mut(|m| {
        m.areas_mut().move_to_top(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("settings_window"),
        ));
    });
    set_settings_tab_state(ctx, active_tab);
}

/// 设置窗口最大高度: 屏高减去顶/底安全留白与控制栏占位, 居中显示时
/// 底部不会探到浮动控制栏后面。
fn settings_max_height(ctx: &egui::Context) -> f32 {
    let screen_height = ctx.content_rect().height();
    (screen_height - SETTINGS_CONTROL_BAR_SAFE_GAP * 2.0).max(220.0)
}

/// 内容区固定高度: 正常屏幕用 `SETTINGS_BODY_HEIGHT` 常量(切 Tab 高度不变),
/// 屏幕过矮时按可用空间夹小, 保证窗口整体不超出安全区。
fn settings_body_height(ctx: &egui::Context) -> f32 {
    let available =
        (settings_max_height(ctx) - SETTINGS_BODY_OVERHEAD).max(SETTINGS_BODY_MIN_HEIGHT);
    available.min(SETTINGS_BODY_HEIGHT)
}

/// 自绘紧凑标题行: 标题 + 右侧关闭按钮。比 egui 自带标题栏矮很多。
fn settings_header(ui: &mut egui::Ui, open: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(t!("settings").to_string())
                .size(SETTINGS_TITLE_SIZE)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if settings_close_button(ui)
                .on_hover_text(t!("closed").to_string())
                .clicked()
            {
                *open = false;
            }
        });
    });
    ui.add_space(SETTINGS_HEADING_GAP);
}

fn settings_tab_bar(ui: &mut egui::Ui, active: &mut SettingsTab) {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = SETTINGS_TAB_PADDING_X;
        ui.horizontal(|ui| {
            for tab in SettingsTab::ALL {
                if ui
                    .add(
                        egui::Button::selectable(*active == tab, tab.label())
                            .min_size(egui::vec2(0.0, SETTINGS_ROW_HEIGHT)),
                    )
                    .clicked()
                {
                    *active = tab;
                }
            }
        });
    });
}

fn settings_close_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::splat(SETTINGS_CLOSE_BUTTON_SIZE),
        egui::Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let radius = (SETTINGS_CLOSE_BUTTON_SIZE * 0.5).round() as u8;
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(radius), visuals.weak_bg_fill);
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(radius),
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let icon_rect = rect.shrink(SETTINGS_CLOSE_ICON_INSET);
        let stroke = egui::Stroke::new(SETTINGS_CLOSE_ICON_STROKE, visuals.fg_stroke.color);
        ui.painter()
            .line_segment([icon_rect.left_top(), icon_rect.right_bottom()], stroke);
        ui.painter()
            .line_segment([icon_rect.right_top(), icon_rect.left_bottom()], stroke);
    }
    response
}

fn settings_tab_body(
    ui: &mut egui::Ui,
    tab: SettingsTab,
    player: &mut Player,
    update_check: &mut crate::updater::UpdateChecker,
) {
    match tab {
        SettingsTab::Appearance => appearance_section(ui, player),
        SettingsTab::Playback => {
            playback_section(ui, player);
            settings_section_gap(ui);
            screenshot_section(ui, player);
        }
        SettingsTab::Updates => updates_section(ui, player, update_check),
        SettingsTab::Subtitle => subtitle_section(ui, player),
    }
}

/// 段落之间的留白(同一 Tab 内多段时用), 比默认 `separator` 更克制。
fn settings_section_gap(ui: &mut egui::Ui) {
    ui.add_space(SETTINGS_SECTION_GAP);
}

/// 段落标题: 小号、弱化、带字距的「eyebrow」式标题, 比默认大号 heading 更现代。
fn section_heading(ui: &mut egui::Ui, text: String) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(text)
            .size(SETTINGS_HEADING_SIZE)
            .color(ui.visuals().weak_text_color())
            .strong(),
    );
    ui.add_space(SETTINGS_HEADING_GAP);
}

fn appearance_section(ui: &mut egui::Ui, player: &mut Player) {
    language_row(ui, player);
}

fn language_row(ui: &mut egui::Ui, player: &mut Player) {
    settings_row(ui, t!("language").to_string(), |ui| {
        let mut lang = player.prefs().language.clone();
        // Combo boxes edit a local copy first; setters run only when the selected
        // value actually changes, avoiding needless preference writes.
        settings_combo(ui, "lang", lang_label(&lang), |ui| {
            ui.selectable_value(&mut lang, "zh-CN".to_string(), "简体中文");
            ui.selectable_value(&mut lang, "zh-TW".to_string(), "繁體中文");
            ui.selectable_value(&mut lang, "en".to_string(), "English");
        });
        if lang != player.prefs().language {
            player.set_language(&lang);
        }
    });
}

fn playback_section(ui: &mut egui::Ui, player: &mut Player) {
    seek_step_row(ui, player);
    playback_mode_row(ui, player);
}

fn seek_step_row(ui: &mut egui::Ui, player: &mut Player) {
    settings_row(ui, t!("seek_step").to_string(), |ui| {
        let mut step = player.prefs().seek_step_secs;
        // Keep the menu values in seconds because the engine converts user seek
        // steps to millisecond targets at the command boundary.
        settings_combo(
            ui,
            "seek_step",
            format!("{} {}", step, t!("seconds")),
            |ui| {
                ui.selectable_value(&mut step, 5, format!("{} {}", 5, t!("seconds")));
                ui.selectable_value(&mut step, 10, format!("{} {}", 10, t!("seconds")));
                ui.selectable_value(&mut step, 20, format!("{} {}", 20, t!("seconds")));
                ui.selectable_value(&mut step, 30, format!("{} {}", 30, t!("seconds")));
            },
        );
        if step != player.prefs().seek_step_secs {
            player.set_seek_step(step);
        }
    });
}

fn playback_mode_row(ui: &mut egui::Ui, player: &mut Player) {
    settings_row(ui, t!("playback_mode").to_string(), |ui| {
        let mut mode = player.prefs().playback_mode;
        // Playback mode is stored as an enum in preferences, so the UI can bind
        // directly without string parsing.
        settings_combo(ui, "playback_mode", playback_mode_label(mode), |ui| {
            ui.selectable_value(
                &mut mode,
                PlaybackMode::StopAtEnd,
                t!("playback_stop_at_end").to_string(),
            );
            ui.selectable_value(
                &mut mode,
                PlaybackMode::LoopPlaylist,
                t!("playback_loop_playlist").to_string(),
            );
            ui.selectable_value(
                &mut mode,
                PlaybackMode::RepeatOne,
                t!("playback_repeat_one").to_string(),
            );
        });
        if mode != player.prefs().playback_mode {
            player.set_playback_mode(mode);
        }
    });
}

fn screenshot_section(ui: &mut egui::Ui, player: &mut Player) {
    section_heading(ui, t!("screenshot").to_string());
    screenshot_dir_row(ui, player);
}

fn screenshot_dir_row(ui: &mut egui::Ui, player: &mut Player) {
    settings_row(ui, t!("screenshot_dir").to_string(), |ui| {
        let configured_dir_is_empty = player.prefs().screenshot_dir.is_empty();
        let current_dir_path = player.screenshot_dir();
        // Display the resolved directory while preserving the button's starting
        // directory behavior for empty legacy configs.
        choose_screenshot_dir_button(ui, player, configured_dir_is_empty, &current_dir_path);
        ui.label(current_dir_path.display().to_string());
    });
}

fn choose_screenshot_dir_button(
    ui: &mut egui::Ui,
    player: &mut Player,
    configured_dir_is_empty: bool,
    current_dir_path: &std::path::Path,
) {
    // FileDialog is only created after a click so opening the settings window
    // never touches the filesystem or blocks the UI thread.
    if !ui
        .add_sized(
            [SETTINGS_ROW_HEIGHT, SETTINGS_ROW_HEIGHT],
            egui::Button::new(crate::symbols::FOLDER),
        )
        .on_hover_text(t!("choose_folder").to_string())
        .clicked()
    {
        return;
    }

    let dialog = if configured_dir_is_empty {
        rfd::FileDialog::new()
    } else {
        rfd::FileDialog::new().set_directory(current_dir_path)
    };
    if let Some(dir) = dialog.pick_folder() {
        let dir = dir.to_string_lossy().into_owned();
        player.set_screenshot_dir(&dir);
    }
}

fn updates_section(
    ui: &mut egui::Ui,
    player: &mut Player,
    update_check: &mut crate::updater::UpdateChecker,
) {
    // Update preferences live beside the manual action so beta/stable checks use
    // the same flags whether they are started here or during app startup.
    update_action_row(ui, player, update_check);
    startup_update_check_row(ui, player);
    if player.prefs().check_updates_on_startup {
        beta_update_check_row(ui, player);
    }
    update_status(ui, update_check.status());
}

fn update_action_row(
    ui: &mut egui::Ui,
    player: &Player,
    update_check: &mut crate::updater::UpdateChecker,
) {
    // 当前版本(左)与检查更新按钮(右)共用一行, 避免版本号独占一行浪费纵向空间。
    ui.horizontal(|ui| {
        ui.set_min_height(SETTINGS_ROW_HEIGHT);
        ui.label(format!(
            "{} {}",
            t!("current_version"),
            update_check.current_version()
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            update_header_actions(ui, player, update_check);
        });
    });
}

fn update_header_actions(
    ui: &mut egui::Ui,
    player: &Player,
    update_check: &mut crate::updater::UpdateChecker,
) {
    // The checker guards its own worker state; the button is disabled here to make
    // that background state visible in the UI.
    if ui
        .add_enabled(
            !update_check.is_checking(),
            egui::Button::new(crate::symbols::REFRESH)
                .min_size(egui::vec2(SETTINGS_ROW_HEIGHT, SETTINGS_ROW_HEIGHT)),
        )
        .on_hover_text(t!("check_now").to_string())
        .clicked()
    {
        update_check.begin(player.prefs().check_beta_updates);
    }
    if update_check.is_checking() {
        ui.add(egui::Spinner::new());
    }
}

fn startup_update_check_row(ui: &mut egui::Ui, player: &mut Player) {
    settings_row(ui, t!("check_updates_on_startup").to_string(), |ui| {
        let mut enabled = player.prefs().check_updates_on_startup;
        if settings_checkbox(ui, &mut enabled).changed() {
            player.set_check_updates_on_startup(enabled);
        }
    });
}

fn beta_update_check_row(ui: &mut egui::Ui, player: &mut Player) {
    settings_row(ui, t!("check_beta_updates").to_string(), |ui| {
        let mut enabled = player.prefs().check_beta_updates;
        // This row is only shown when startup checks are enabled; the player setter
        // still enforces that relationship for programmatic calls.
        if settings_checkbox(ui, &mut enabled).changed() {
            player.set_check_beta_updates(enabled);
        }
    });
}

fn subtitle_section(ui: &mut egui::Ui, player: &mut Player) {
    settings_row(ui, t!("subtitle_size").to_string(), |ui| {
        let mut size = player.prefs().subtitle_font_size;
        if ui.add(egui::Slider::new(&mut size, 12.0..=48.0)).changed() {
            player.set_subtitle_font_size(size);
        }
    });
}

fn update_status(ui: &mut egui::Ui, status: &crate::updater::UpdateStatus) {
    ui.horizontal_wrapped(|ui| update_status_contents(ui, status));
}

fn update_status_contents(ui: &mut egui::Ui, status: &crate::updater::UpdateStatus) {
    // Status text is rendered in a wrapped row because release URLs and localized
    // failure messages can be wider than the fixed settings panel.
    match status {
        crate::updater::UpdateStatus::Idle => {
            ui.label(t!("update_ready_to_check").to_string());
        }
        crate::updater::UpdateStatus::Checking => {
            ui.add(egui::Spinner::new());
            ui.label(t!("checking_updates").to_string());
        }
        crate::updater::UpdateStatus::UpToDate => {
            ui.label(t!("update_up_to_date").to_string());
        }
        crate::updater::UpdateStatus::Available(update) => {
            let channel = if update.prerelease {
                t!("update_channel_beta").to_string()
            } else {
                t!("update_channel_stable").to_string()
            };
            ui.label(format!(
                "{}: {} ({})",
                t!("update_available"),
                update.version,
                channel
            ));
            ui.hyperlink_to(t!("open_release_page").to_string(), &update.html_url);
        }
        crate::updater::UpdateStatus::Error(err) => {
            ui.label(format!("{}: {err}", t!("update_check_failed")));
        }
    }
}

fn settings_row(ui: &mut egui::Ui, label: String, add_value: impl FnOnce(&mut egui::Ui)) {
    // Values are laid out from the right edge so controls with different widths do
    // not make the left labels jitter between rows.
    ui.horizontal(|ui| {
        ui.set_min_height(SETTINGS_ROW_HEIGHT);
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            add_value(ui);
        });
    });
}

fn settings_combo<R>(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    selected_text: impl Into<egui::WidgetText>,
    add_options: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = SETTINGS_ROW_HEIGHT;
        egui::ComboBox::from_id_salt(id_salt)
            .width(SETTINGS_COMBO_WIDTH)
            .height(SETTINGS_COMBO_POPUP_HEIGHT)
            .selected_text(selected_text)
            .popup_style(crate::visuals::panel_popup_style())
            .show_ui(ui, |ui| {
                ui.spacing_mut().interact_size.y = SETTINGS_ROW_HEIGHT;
                ui.spacing_mut().item_spacing.y = 0.0;
                add_options(ui)
            })
    })
    .inner
}

fn settings_checkbox(ui: &mut egui::Ui, checked: &mut bool) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().icon_width = SETTINGS_CHECKBOX_SIZE;
        ui.spacing_mut().icon_width_inner = SETTINGS_CHECKBOX_INNER_SIZE;
        ui.spacing_mut().interact_size.y = SETTINGS_ROW_HEIGHT;
        ui.add(egui::Checkbox::without_text(checked))
    })
    .inner
}

fn lang_label(code: &str) -> &'static str {
    match code {
        "zh-TW" => "繁體中文",
        "en" => "English",
        _ => "简体中文",
    }
}

fn playback_mode_label(mode: PlaybackMode) -> String {
    match mode {
        PlaybackMode::StopAtEnd => t!("playback_stop_at_end").to_string(),
        PlaybackMode::LoopPlaylist => t!("playback_loop_playlist").to_string(),
        PlaybackMode::RepeatOne => t!("playback_repeat_one").to_string(),
    }
}

#[cfg(test)]
mod tests {
    fn source_before_tests() -> &'static str {
        include_str!("settings.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
    }

    #[test]
    fn settings_exposes_playback_mode() {
        let source = source_before_tests();

        assert!(source.contains("playback_mode"));
        assert!(source.contains("LoopPlaylist"));
        assert!(source.contains("RepeatOne"));
    }

    #[test]
    fn settings_exposes_update_preferences_with_beta_gated_by_startup_check() {
        let source = source_before_tests();

        assert!(source.contains("updates"));
        assert!(source.contains("check_updates_on_startup"));
        assert!(source.contains("check_beta_updates"));
        assert!(source.contains("set_check_updates_on_startup"));
        assert!(source.contains("set_check_beta_updates"));
        assert!(source.contains("if player.prefs().check_updates_on_startup"));
    }

    #[test]
    fn settings_update_section_owns_manual_check_and_status() {
        let source = source_before_tests();

        assert!(source.contains("UpdateChecker"));
        assert!(source.contains("Button::new(crate::symbols::REFRESH)"));
        assert!(source.contains("on_hover_text(t!(\"check_now\").to_string())"));
        assert!(source.contains("update_check.begin(player.prefs().check_beta_updates)"));
        assert!(source.contains("update_status(ui, update_check.status())"));
        assert!(source.contains("current_version"));
        assert!(source.contains("open_release_page"));
    }

    #[test]
    fn settings_exposes_configurable_screenshot_directory() {
        let source = source_before_tests();

        assert!(source.contains("screenshot_dir"));
        assert!(source.contains("player.screenshot_dir()"));
        assert!(!source.contains("home_dir_candidates"));
        assert!(source.contains("FileDialog"));
        assert!(source.contains("pick_folder"));
        assert!(source.contains("set_screenshot_dir"));
    }

    #[test]
    fn screenshot_directory_button_uses_icon_only_with_hover_text() {
        let source = source_before_tests();

        assert!(source.contains("egui::Button::new(crate::symbols::FOLDER)"));
        assert!(source.contains("[SETTINGS_ROW_HEIGHT, SETTINGS_ROW_HEIGHT]"));
        assert!(source.contains("on_hover_text(t!(\"choose_folder\").to_string())"));
        assert!(!source.contains("ui.button(t!(\"choose_folder\").to_string())"));
    }

    #[test]
    fn settings_header_close_button_is_smaller_and_round() {
        let source = source_before_tests();

        assert!(source.contains("const SETTINGS_CLOSE_BUTTON_SIZE: f32 = 22.0"));
        assert!(source.contains("fn settings_close_button"));
        assert!(source.contains("allocate_exact_size"));
        assert!(source.contains("egui::Vec2::splat(SETTINGS_CLOSE_BUTTON_SIZE)"));
        assert!(source.contains("SETTINGS_CLOSE_ICON_INSET"));
        assert!(source.contains("line_segment([icon_rect.left_top(), icon_rect.right_bottom()]"));
        assert!(source.contains("line_segment([icon_rect.right_top(), icon_rect.left_bottom()]"));
        assert!(!source.contains("egui::Button::new(\"×\")"));
    }

    #[test]
    fn settings_tabs_use_wider_horizontal_padding() {
        let source = source_before_tests();

        assert!(source.contains("const SETTINGS_TAB_PADDING_X: f32 = 14.0"));
        assert!(source.contains("ui.spacing_mut().button_padding.x = SETTINGS_TAB_PADDING_X"));
        assert!(source.contains("ui.horizontal(|ui|"));
    }

    #[test]
    fn settings_tabs_do_not_repeat_active_tab_heading() {
        let source = source_before_tests();

        assert!(!source.contains("section_heading(ui, t!(\"appearance\")"));
        assert!(!source.contains("section_heading(ui, t!(\"playback\")"));
        assert!(!source.contains("section_heading(ui, t!(\"updates\")"));
        assert!(!source.contains("section_heading(ui, t!(\"subtitle\")"));
        assert!(source.contains("section_heading(ui, t!(\"screenshot\")"));
    }

    #[test]
    fn settings_checkboxes_are_larger_than_default() {
        let source = source_before_tests();

        assert!(source.contains("const SETTINGS_CHECKBOX_SIZE: f32 = 20.0"));
        assert!(source.contains("const SETTINGS_CHECKBOX_INNER_SIZE: f32 = 12.0"));
        assert!(source.contains("fn settings_checkbox"));
        assert!(source.contains("ui.spacing_mut().icon_width = SETTINGS_CHECKBOX_SIZE"));
        assert!(source.contains("ui.spacing_mut().icon_width_inner = SETTINGS_CHECKBOX_INNER_SIZE"));
        assert!(source.contains("egui::Checkbox::without_text(checked)"));
        assert!(source.contains("settings_checkbox(ui, &mut enabled)"));
        assert!(!source.contains("ui.checkbox(&mut enabled, \"\")"));
    }

    #[test]
    fn settings_rows_keep_labels_left_and_values_right() {
        let source = source_before_tests();

        assert!(source.contains("fn settings_row"));
        assert!(source.contains("right_to_left(egui::Align::Center)"));
        assert!(!source.contains("ui.horizontal(|ui| {\n                ui.label(t!(\"language\")"));
    }

    #[test]
    fn settings_combo_boxes_share_size_and_menu_item_height() {
        let source = source_before_tests();

        assert!(source.contains("const SETTINGS_COMBO_WIDTH"));
        assert!(source.contains("const SETTINGS_COMBO_POPUP_HEIGHT"));
        assert!(source.contains("fn settings_combo"));
        assert!(source.contains(".width(SETTINGS_COMBO_WIDTH)"));
        assert!(source.contains(".height(SETTINGS_COMBO_POPUP_HEIGHT)"));
        assert!(source.contains("ui.spacing_mut().interact_size.y = SETTINGS_ROW_HEIGHT"));
        assert!(source.contains("ui.spacing_mut().item_spacing.y = 0.0"));
        assert!(source.contains("settings_combo(ui, \"lang\""));
        assert!(source.contains("\"seek_step\""));
        assert!(source.contains("settings_combo(ui, \"playback_mode\""));
    }

    #[test]
    fn settings_window_uses_stable_id_and_cannot_collapse() {
        let source = source_before_tests();

        assert!(source.contains(".id(egui::Id::new(\"settings_window\"))"));
        assert!(source.contains(".collapsible(false)"));
    }

    #[test]
    fn settings_window_opens_centered() {
        let source = source_before_tests();

        assert!(source.contains(".anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)"));
    }
}
