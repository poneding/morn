//! App frame orchestration for egui.
//!
//! The top-level `eframe::App` implementation deliberately stays as a scheduler:
//! it syncs preferences, advances the engine, asks each overlay to render, then
//! schedules repaint work.  The state object below keeps per-frame values such as
//! pointer position, current playlist index, hover state, and screenshot intent in
//! one place so the playlist sheet and bottom controls do not trade long argument
//! lists.
//!
//! Two ordering rules matter for playback smoothness and startup restore:
//! video content is drawn before floating controls, and repaint requests are made
//! after the seek gate and texture visibility have been inspected.  That lets a
//! restored paused video keep repainting until the first decoded frame reaches the
//! texture, while still keeping resize repaint narrowly scoped to actual live
//! window-size changes instead of every pointer drag.
//!
//! The playlist sheet renders against a cloned snapshot of playlist/history paths.
//! Commands are collected during UI painting and applied afterward, which keeps
//! egui layout borrowing separate from `Player` mutation and makes row deletion or
//! tab switching deterministic.
//!
//! Maintenance notes:
//! - playlist sheet sizing is derived from the current content rect and the
//!   floating control bar height; do not reserve permanent side-panel space for it.
//! - bottom controls are anchored to the window, not to video or playlist width,
//!   so they remain centered while the playlist sheet is open.
//! - app action buttons (⚙/☰) are appended inline at the end of the controls
//!   row; media controls and enhancement controls stay on the left.  The bar
//!   sizes to its content and is centered by its `Area`, so it no longer spans
//!   the full window width.
//! - subtitle track UI is only shown when embedded tracks exist; sidecar subtitle
//!   loading is handled by the engine and should not allocate empty controls here.
//! - hover state is sampled from the current egui `Area` response every frame.
//!   Avoid storing overlay rects across frames because egui can relayout after
//!   DPI, font, locale, or window-size changes.
//! - playlist auto-hide is armed only after the pointer has entered the window
//!   following an explicit open.  This prevents the sheet from closing immediately
//!   when it is opened by keyboard while the pointer is outside the app.
//! - repaint scheduling is deliberately split into update-check polling, window-
//!   resize repaint, overlay fade/notice repaint, pointer idle recheck, and
//!   player repaint.  Combining them tends to reintroduce either resize jank or
//!   blank paused-startup frames.

use super::*;

pub(super) struct UiFrameState {
    pub(super) screen_rect: egui::Rect,
    pub(super) pointer_pos: Option<egui::Pos2>,
    pub(super) now: std::time::Instant,
    pub(super) current_index: Option<usize>,
    pub(super) playlist_len: usize,
    pub(super) playlist_visible: bool,
    pub(super) has_prev: bool,
    pub(super) has_next: bool,
    pub(super) playlist_opened_this_frame: bool,
    pub(super) playlist_hovered: bool,
    pub(super) bottom_controls_hovered: bool,
    pub(super) screenshot_requested: bool,
}

impl UiFrameState {
    fn snapshot(
        ctx: &egui::Context,
        app: &PlayerApp,
        screenshot_requested: bool,
        playlist_opened_this_frame: bool,
    ) -> Self {
        let playlist = PlaylistFrameInfo::snapshot(app);

        Self {
            screen_rect: ctx.content_rect(),
            pointer_pos: ctx.input(|i| i.pointer.hover_pos()),
            now: std::time::Instant::now(),
            current_index: playlist.current_index,
            playlist_len: playlist.len,
            playlist_visible: app.show_playlist && !app.playlist_auto_hidden,
            has_prev: playlist.has_prev,
            has_next: playlist.has_next,
            playlist_opened_this_frame,
            playlist_hovered: false,
            bottom_controls_hovered: false,
            screenshot_requested,
        }
    }
}

struct PlaylistFrameInfo {
    current_index: Option<usize>,
    len: usize,
    has_prev: bool,
    has_next: bool,
}

impl PlaylistFrameInfo {
    fn snapshot(app: &PlayerApp) -> Self {
        let current_index = app.player.current_index();
        let len = app.player.playlist_paths().len();
        Self {
            current_index,
            len,
            has_prev: playlist_has_prev(len, current_index),
            has_next: playlist_has_next(len, current_index),
        }
    }
}

struct BottomControlsData<'a> {
    timeline: engine::Timeline,
    tracks: &'a [media::SubtitleTrack],
    max_width: f32,
    slider_width: f32,
}

struct BottomControlGroups<'a, 'commands> {
    timeline: engine::Timeline,
    tracks: &'a [media::SubtitleTrack],
    slider_width: f32,
    commands: &'commands mut Vec<player_core::Command>,
}

impl eframe::App for PlayerApp {
    fn logic(&mut self, ctx: &egui::Context, _eframe_frame: &mut eframe::Frame) {
        self.advance_video_presentation_without_paint(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, eframe_frame: &mut eframe::Frame) {
        // macOS/Windows: 首帧应用各自的 resize 旧帧拉伸掩盖(macOS 改 layer 放置
        // 策略, Windows 禁用 DWM 过渡动画; 详见 macos.rs / windows.rs)。仅执行一次,
        // 失败则下一帧重试。
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if !self.resize_glitch_mask_applied {
            #[cfg(target_os = "macos")]
            {
                self.resize_glitch_mask_applied =
                    crate::macos::apply_resize_glitch_masking(eframe_frame);
            }
            #[cfg(target_os = "windows")]
            {
                self.resize_glitch_mask_applied =
                    crate::windows::apply_resize_glitch_masking(eframe_frame);
            }
        }

        let ctx = ui.ctx().clone();

        self.sync_runtime_preferences(&ctx);
        self.handle_dropped_files(&ctx);

        // 窗口正在缩放(含全屏切换动画): 暂停 tick 降低推进压力, 但仍然
        // present/paint 当前帧, 避免非 macOS 平台在缩放期间露出空白视频面。
        let now = std::time::Instant::now();
        let resizing = should_request_window_resize_repaint(self.last_window_resize, now);

        let (timeline, shortcut_outcome) = self.prepare_ui_frame(&ctx, resizing);
        let mut state = UiFrameState::snapshot(
            &ctx,
            self,
            shortcut_outcome.screenshot_requested,
            shortcut_outcome.playlist_opened_this_frame,
        );

        self.show_titlebar(&ctx, &mut state);

        self.show_video_panel(ui, eframe_frame, resizing);
        self.update_pointer_activity(&mut state);
        self.show_playlist_overlay(&ctx, &mut state);
        self.show_bottom_controls_overlay(&ctx, timeline, &mut state);
        self.update_playlist_auto_hide(&state);
        self.save_screenshot_if_requested(state.screenshot_requested);
        self.finish_ui_frame(&ctx, &state);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.player.save_state();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        #[cfg(target_os = "macos")]
        {
            // macOS uses the native rounded window frame and titlebar overlay.
            [0.0, 0.0, 0.0, 1.0]
        }

        #[cfg(not(target_os = "macos"))]
        {
            // 自绘窗口需要透明背景，让 egui 绘制的圆角背景可见。
            [0.0, 0.0, 0.0, 0.0]
        }
    }
}

impl PlayerApp {
    fn advance_video_presentation_without_paint(&mut self, ctx: &egui::Context) {
        let should_advance = ctx.input(|i| {
            let viewport = i.viewport();
            video_presentation_should_advance_without_paint(viewport.minimized, viewport.occluded)
        });
        if !should_advance {
            return;
        }

        self.player.tick();
        let _ = self.player.present_frame();
        let interacting = ctx.input(|i| i.pointer.any_down());
        if player_should_request_repaint(
            interacting,
            self.player.timeline().state,
            self.player.seek_pending(),
            self.player.video().is_some(),
            self.video_view.has_current_frame_texture(&self.player),
        ) {
            ctx.request_repaint_after(OCCLUDED_VIDEO_PRESENTATION_REPAINT_INTERVAL);
        }
    }

    fn prepare_ui_frame(
        &mut self,
        ctx: &egui::Context,
        resizing: bool,
    ) -> (engine::Timeline, KeyboardShortcutOutcome) {
        // 窗口缩放期间暂停播放时钟推进, 减少动画期间的解码与上传开销。
        if !resizing {
            self.player.tick();
        }
        self.update_check.poll();
        let timeline = self.player.timeline();
        let shortcut_outcome = self.handle_keyboard_shortcuts(ctx, timeline);
        (timeline, shortcut_outcome)
    }

    fn show_titlebar(&mut self, ctx: &egui::Context, state: &mut UiFrameState) {
        let title = self.current_playlist_name().unwrap_or_default();
        let actions = crate::titlebar::show_custom_titlebar(
            ctx,
            &title,
            self.show_playlist,
            self.show_settings,
        );
        if actions.toggle_playlist {
            self.toggle_playlist_from_titlebar(state);
        }
        if actions.toggle_settings {
            self.show_settings = !self.show_settings;
        }
        if actions.minimize {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if actions.maximize {
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        if actions.close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn update_pointer_activity(&mut self, state: &mut UiFrameState) {
        let pointer_moved =
            pointer_moved_since_last_frame(self.last_pointer_pos, state.pointer_pos);
        if pointer_moved || state.playlist_opened_this_frame {
            self.refresh_pointer_move_timestamp(state);
        }
        self.restore_playlist_after_pointer_motion(pointer_moved, state);
        self.last_pointer_pos = state.pointer_pos;
    }

    fn refresh_pointer_move_timestamp(&mut self, state: &UiFrameState) {
        self.last_pointer_move = state.now;
    }

    fn restore_playlist_after_pointer_motion(
        &mut self,
        pointer_moved: bool,
        state: &mut UiFrameState,
    ) {
        if playlist_should_restore_from_auto_hide(
            self.show_playlist,
            self.playlist_auto_hidden,
            pointer_moved,
            state.pointer_pos,
            state.screen_rect,
        ) {
            self.playlist_auto_hidden = false;
            state.playlist_visible = true;
        }
    }

    fn show_bottom_controls_overlay(
        &mut self,
        ctx: &egui::Context,
        t: engine::Timeline,
        state: &mut UiFrameState,
    ) {
        if !self.bottom_controls_visible(t, state) {
            return;
        }

        let mut bottom_commands = Vec::new();
        let tracks = self.player.subtitle_tracks().to_vec();
        let data = BottomControlsData {
            timeline: t,
            tracks: &tracks,
            max_width: control_bar_max_width(state.screen_rect.width()),
            slider_width: control_bar_slider_width(state.screen_rect.width()),
        };
        let floating_controls_area = egui::Area::new(egui::Id::new("floating_controls"))
            .anchor(
                egui::Align2::CENTER_BOTTOM,
                egui::vec2(0.0, -crate::visuals::FLOATING_PANEL_MARGIN),
            )
            .order(egui::Order::Foreground)
            .fade_in(false)
            .show(ctx, |ui| {
                self.show_bottom_controls_area(ui, state, data, &mut bottom_commands)
            });

        state.bottom_controls_hovered =
            floating_controls_area.inner || floating_controls_area.response.hovered();
        refresh_pointer_activity_for_current_overlay_hover(
            state.bottom_controls_hovered,
            &mut self.last_pointer_move,
            state.now,
        );
        self.apply_bottom_commands(bottom_commands);
    }

    fn bottom_controls_visible(&self, t: engine::Timeline, state: &UiFrameState) -> bool {
        let has_media = self.player.video().is_some() || t.duration_ms > 0;
        bottom_controls_visible(BottomControlsVisibilityInput {
            has_media,
            pointer_pos: state.pointer_pos,
            screen_rect: state.screen_rect,
            screenshot_notice_visible: self.screenshot_notice.is_some(),
            active_overlay_hovered: state.playlist_hovered,
            last_pointer_move: self.last_pointer_move,
            now: state.now,
        })
    }

    fn show_bottom_controls_area(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut UiFrameState,
        data: BottomControlsData<'_>,
        commands: &mut Vec<player_core::Command>,
    ) -> bool {
        // 不再强制撑满窗口宽度: 给一个上限, 让控制栏按内容自适应。Area 锚定
        // CENTER_BOTTOM, 内容多宽就多宽并自动居中。
        ui.set_max_width(data.max_width);
        let inner_margin = egui::Margin::symmetric(
            CONTROL_BAR_INNER_PADDING_X,
            crate::visuals::FLOATING_CONTROL_BAR_INNER_MARGIN_Y,
        );
        let frame = crate::visuals::panel_frame(ui, inner_margin).show(ui, |ui| {
            ui.set_max_width(
                (data.max_width - f32::from(CONTROL_BAR_INNER_PADDING_X) * 2.0).max(0.0),
            );
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                self.show_bottom_control_groups(
                    ui,
                    state,
                    BottomControlGroups {
                        timeline: data.timeline,
                        tracks: data.tracks,
                        slider_width: data.slider_width,
                        commands,
                    },
                );
            });
        });
        crate::visuals::paint_panel_bevel(
            ui,
            frame.response.rect,
            crate::visuals::PANEL_CORNER_RADIUS,
        );
        frame.response.hovered() || frame.response.contains_pointer()
    }

    fn show_bottom_control_groups(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut UiFrameState,
        groups: BottomControlGroups<'_, '_>,
    ) {
        groups.commands.extend(controls::controls_bar(
            ui,
            controls::ControlsBarInput {
                timeline: &groups.timeline,
                has_prev: state.has_prev,
                has_next: state.has_next,
                slider_width: groups.slider_width,
            },
        ));

        let actions = crate::enhance::enhance_bar(ui, groups.timeline.rate_pct);
        groups.commands.extend(actions.commands);
        state.screenshot_requested |= actions.screenshot;

        if !groups.tracks.is_empty() {
            if let Some(cmd) = controls::subtitle_track_combo(ui, groups.tracks) {
                groups.commands.push(cmd);
            }
        }
        // ☰/⚙ 已移到自绘标题栏右侧, 不再出现在底部控制栏。
    }

    fn toggle_playlist_from_titlebar(&mut self, state: &mut UiFrameState) {
        toggle_playlist_visibility(
            &mut self.show_playlist,
            &mut self.playlist_auto_hidden,
            &mut self.playlist_candidate,
            state.current_index,
            state.playlist_len,
        );
        state.playlist_visible = self.show_playlist && !self.playlist_auto_hidden;
        self.history_candidate = None;
        if self.show_playlist {
            state.playlist_opened_this_frame = true;
            self.sidebar_tab = SidebarTab::Playlist;
        }
    }
}

impl PlayerApp {
    fn apply_bottom_commands(&mut self, commands: Vec<player_core::Command>) {
        for cmd in commands {
            self.handle_command(cmd);
        }
    }

    fn update_playlist_auto_hide(&mut self, state: &UiFrameState) {
        if state.playlist_opened_this_frame {
            self.last_pointer_move = state.now;
        }
        update_playlist_auto_hide_armed(
            &mut self.playlist_auto_hide_armed,
            self.show_playlist,
            state.playlist_opened_this_frame,
            state.pointer_pos,
            state.screen_rect,
        );
        if playlist_should_auto_hide(PlaylistAutoHideInput {
            playlist_visible: state.playlist_visible,
            auto_hide_armed: self.playlist_auto_hide_armed,
            pointer_pos: state.pointer_pos,
            screen_rect: state.screen_rect,
            playlist_hovered: state.playlist_hovered,
            bottom_controls_hovered: state.bottom_controls_hovered,
            last_pointer_move: self.last_pointer_move,
            now: state.now,
            opened_this_frame: state.playlist_opened_this_frame,
        }) {
            self.playlist_auto_hidden = true;
        }
    }

    fn save_screenshot_if_requested(&mut self, screenshot_requested: bool) {
        if screenshot_requested {
            self.save_current_screenshot();
        }
    }

    fn finish_ui_frame(&mut self, ctx: &egui::Context, state: &UiFrameState) {
        self.track_window_resize(state);
        self.resize_window_for_current_video(ctx);
        crate::settings::settings_window(
            ctx,
            &mut self.show_settings,
            &mut self.player,
            &mut self.update_check,
        );
        self.show_screenshot_notice(ctx);
        self.show_shortcut_notice(ctx);
        self.request_update_check_repaint(ctx);
        self.request_window_resize_repaint(ctx, state.now);
        self.request_overlay_repaints(ctx, state);
        self.request_player_repaint(ctx);
    }

    fn track_window_resize(&mut self, state: &UiFrameState) {
        if window_content_rect_changed(self.last_screen_rect, state.screen_rect) {
            self.last_window_resize = Some(state.now);
        }
        self.last_screen_rect = Some(state.screen_rect);
    }

    fn request_update_check_repaint(&self, ctx: &egui::Context) {
        if self.update_check.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn request_window_resize_repaint(&self, ctx: &egui::Context, now: std::time::Instant) {
        if should_request_window_resize_repaint(self.last_window_resize, now) {
            ctx.request_repaint();
        }
    }

    fn request_overlay_repaints(&self, ctx: &egui::Context, state: &UiFrameState) {
        let interacting = ctx.input(|i| i.pointer.any_down());
        if should_request_continuous_repaint(
            self.screenshot_notice.is_some() || self.shortcut_notice.is_some(),
            interacting,
        ) {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        if let Some(delay) = pointer_activity_repaint_delay(
            state.pointer_pos,
            state.screen_rect,
            self.last_pointer_move,
            state.now,
        ) {
            ctx.request_repaint_after(delay);
        }
    }

    fn request_player_repaint(&self, ctx: &egui::Context) {
        let interacting = ctx.input(|i| i.pointer.any_down());
        if player_should_request_repaint(
            interacting,
            self.player.timeline().state,
            self.player.seek_pending(),
            self.player.video().is_some(),
            self.video_view.has_current_frame_texture(&self.player),
        ) {
            ctx.request_repaint();
        }
    }
}
