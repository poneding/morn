//! Floating playlist/history sidebar UI.
//!
//! This module owns the sheet snapshot, header layout, tab switching, and command
//! application for the playlist/history overlay.  Keeping it outside `app_ui`
//! leaves the frame scheduler focused on draw order and repaint timing.
//!
//! The sidebar paints from cloned playlist/history snapshots, then applies collected
//! commands after layout.  That avoids mutating `Player` while rows are being drawn
//! and keeps hover/candidate state stable for the whole frame.
//!
//! Playlist and history keyboard candidates are normalized every visible frame.  Tab
//! switches additionally clear the inactive candidate so Enter/Delete never applies
//! to a stale row from the other list.

use super::app_ui::UiFrameState;
use super::*;

const SIDEBAR_TAB_PADDING_X: f32 = 14.0;
const SIDEBAR_TAB_HEIGHT: f32 = crate::visuals::ICON_BUTTON_SIZE;

#[derive(Clone, Copy)]
struct PlaylistSheetData<'a> {
    // The overlay paints from frame-local snapshots so row iteration never borrows
    // `Player` across command collection.
    playlist_paths: &'a [std::path::PathBuf],
    history: &'a [std::path::PathBuf],
    sheet_width: f32,
    sheet_height: f32,
}

#[derive(Clone, Copy)]
struct SidebarHeaderRects {
    // Header widgets are manually centered as a group while the open/clear
    // actions stay pinned to the sheet edges.
    open: egui::Rect,
    clear: egui::Rect,
    playlist: egui::Rect,
    history: egui::Rect,
}

#[derive(Clone, Copy)]
struct SidebarTabs<'a> {
    // Tab labels are built once per frame because translation strings affect the
    // measured tab widths and therefore the centered header layout.
    history_len: usize,
    rects: SidebarHeaderRects,
    playlist_label: &'a str,
    history_label: &'a str,
}

struct SidebarHeader {
    rects: SidebarHeaderRects,
    playlist_label: String,
    history_label: String,
}

struct SidebarHeaderLabels {
    playlist: String,
    history: String,
}

impl PlayerApp {
    pub(super) fn show_playlist_overlay(&mut self, ctx: &egui::Context, state: &mut UiFrameState) {
        if !state.playlist_visible {
            return;
        }

        let history: Vec<std::path::PathBuf> =
            self.player.history().iter().map(Into::into).collect();
        let playlist_paths = self.player.playlist_paths().to_vec();
        self.sync_sidebar_candidate(state.current_index, state.playlist_len, history.len());

        // Size is derived before painting so all child helpers can work with
        // stable rectangles even if row content is clipped or translated.
        let style = ctx.global_style();
        let data = PlaylistSheetData {
            playlist_paths: &playlist_paths,
            history: &history,
            sheet_width: playlist_sheet_width(state.screen_rect.width()),
            sheet_height: playlist_sheet_height(state.screen_rect, style.as_ref()),
        };
        let mut playlist_commands = Vec::new();
        let playlist_area = self.show_playlist_area(ctx, state, data, &mut playlist_commands);
        state.playlist_hovered = playlist_area.inner || playlist_area.response.hovered();
        self.apply_sidebar_commands(playlist_commands);
    }

    fn sync_sidebar_candidate(
        &mut self,
        current_index: Option<usize>,
        playlist_len: usize,
        history_len: usize,
    ) {
        // Candidate indices are clamped to the currently visible list.  This is
        // what makes keyboard actions track the active tab instead of stale rows.
        match self.sidebar_tab {
            SidebarTab::Playlist => {
                self.playlist_candidate = playlist_candidate_for_open(
                    self.playlist_candidate.or(current_index),
                    playlist_len,
                )
            }
            SidebarTab::History => {
                self.history_candidate =
                    playlist_candidate_for_open(self.history_candidate, history_len)
            }
        }
    }

    fn show_playlist_area(
        &mut self,
        ctx: &egui::Context,
        state: &mut UiFrameState,
        data: PlaylistSheetData<'_>,
        commands: &mut Vec<player_core::Command>,
    ) -> egui::InnerResponse<bool> {
        egui::Area::new(egui::Id::new("playlist_sheet_fixed_v2"))
            .anchor(
                egui::Align2::RIGHT_TOP,
                egui::vec2(
                    -crate::visuals::FLOATING_PANEL_MARGIN,
                    playlist_sheet_top_offset(),
                ),
            )
            .default_size(egui::vec2(data.sheet_width, data.sheet_height))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .show(ctx, |ui| {
                self.show_playlist_sheet(ui, state, data, commands)
            })
    }

    fn show_playlist_sheet(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut UiFrameState,
        data: PlaylistSheetData<'_>,
        commands: &mut Vec<player_core::Command>,
    ) -> bool {
        ui.set_width(data.sheet_width);
        ui.set_height(data.sheet_height);
        let inner_margin =
            egui::Margin::symmetric(PLAYLIST_SHEET_INNER_MARGIN_X, PLAYLIST_SHEET_INNER_MARGIN_Y);
        let playlist_frame = crate::visuals::panel_frame(ui, inner_margin);
        // Child widgets receive the inner size after panel-frame margins so
        // scroll areas do not accidentally paint under the rounded border.
        let content_size = playlist_sheet_content_size(data, playlist_frame.total_margin().sum());

        let frame = playlist_frame.show(ui, |ui| {
            ui.set_width(content_size.x);
            ui.set_height(content_size.y);
            self.show_playlist_sheet_contents(ui, state, data, commands);
        });
        crate::visuals::paint_panel_bevel(
            ui,
            frame.response.rect,
            crate::visuals::PANEL_CORNER_RADIUS,
        );
        frame.response.hovered() || frame.response.contains_pointer()
    }

    fn show_playlist_sheet_contents(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut UiFrameState,
        data: PlaylistSheetData<'_>,
        commands: &mut Vec<player_core::Command>,
    ) {
        let header = sidebar_header(ui);

        commands.extend(crate::playlist_panel::open_file_button_at(
            ui,
            header.rects.open,
        ));
        self.show_sidebar_tabs(
            ui,
            state,
            SidebarTabs {
                history_len: data.history.len(),
                rects: header.rects,
                playlist_label: &header.playlist_label,
                history_label: &header.history_label,
            },
        );
        self.show_sidebar_clear_action(ui, data, header.rects.clear, commands);
        ui.separator();
        self.show_sidebar_content(ui, state, data, commands);
    }

    fn show_sidebar_tabs(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut UiFrameState,
        tabs: SidebarTabs<'_>,
    ) {
        let tab_before = self.sidebar_tab;
        for (tab, rect, label) in [
            (
                SidebarTab::Playlist,
                tabs.rects.playlist,
                tabs.playlist_label,
            ),
            (SidebarTab::History, tabs.rects.history, tabs.history_label),
        ] {
            if sidebar_tab_clicked(ui, rect, self.sidebar_tab == tab, label) {
                self.sidebar_tab = tab;
            }
        }
        if self.sidebar_tab != tab_before {
            self.update_sidebar_candidate_after_tab_change(state, tabs.history_len);
        }
    }

    fn update_sidebar_candidate_after_tab_change(
        &mut self,
        state: &UiFrameState,
        history_len: usize,
    ) {
        self.sync_sidebar_candidate(state.current_index, state.playlist_len, history_len);
        self.clear_inactive_sidebar_candidate();
    }

    fn clear_inactive_sidebar_candidate(&mut self) {
        match self.sidebar_tab {
            SidebarTab::Playlist => self.history_candidate = None,
            SidebarTab::History => self.playlist_candidate = None,
        }
    }

    fn show_sidebar_clear_action(
        &mut self,
        ui: &mut egui::Ui,
        data: PlaylistSheetData<'_>,
        clear_rect: egui::Rect,
        commands: &mut Vec<player_core::Command>,
    ) {
        let (clear_cmd, clear_enabled) = match self.sidebar_tab {
            SidebarTab::Playlist => (
                player_core::Command::ClearPlaylist,
                !data.playlist_paths.is_empty(),
            ),
            SidebarTab::History => (player_core::Command::ClearHistory, !data.history.is_empty()),
        };
        commands.extend(crate::playlist_panel::clear_all_button_at(
            ui,
            clear_rect,
            clear_cmd,
            clear_enabled,
        ));
    }

    fn show_sidebar_content(
        &mut self,
        ui: &mut egui::Ui,
        state: &UiFrameState,
        data: PlaylistSheetData<'_>,
        commands: &mut Vec<player_core::Command>,
    ) {
        let visible_commands = match self.sidebar_tab {
            SidebarTab::Playlist => crate::playlist_panel::playlist_panel(
                ui,
                data.playlist_paths,
                state.current_index,
                self.playlist_candidate,
            ),
            SidebarTab::History => {
                crate::playlist_panel::history_panel(ui, data.history, self.history_candidate)
            }
        };
        commands.extend(visible_commands);
    }

    fn apply_sidebar_commands(&mut self, commands: Vec<player_core::Command>) {
        for cmd in commands {
            // Capture lengths before mutation so candidate repair can distinguish
            // between removals, clears, and commands that only start playback.
            let playlist_len_before = self.player.playlist_paths().len();
            let history_len_before = self.player.history().len();
            self.handle_command(cmd.clone());
            self.update_candidates_after_sidebar_command(
                &cmd,
                playlist_len_before,
                history_len_before,
            );
        }
    }
}

fn sidebar_tab_clicked(ui: &mut egui::Ui, rect: egui::Rect, selected: bool, label: &str) -> bool {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding.x = SIDEBAR_TAB_PADDING_X;
            ui.spacing_mut().interact_size.y = SIDEBAR_TAB_HEIGHT;
            ui.put(
                rect,
                egui::Button::selectable(selected, label)
                    .min_size(egui::vec2(0.0, SIDEBAR_TAB_HEIGHT)),
            )
        })
        .inner;
    response.clicked()
}

fn allocate_sidebar_header(ui: &mut egui::Ui) -> egui::Rect {
    let header_height = SIDEBAR_TAB_HEIGHT + crate::visuals::FLOATING_PANEL_MARGIN;
    let header_width = ui.available_width().max(0.0);
    let (header_rect, _) = ui.allocate_exact_size(
        egui::vec2(header_width, header_height),
        egui::Sense::hover(),
    );
    header_rect
}

fn playlist_sheet_content_size(
    data: PlaylistSheetData<'_>,
    frame_margin: egui::Vec2,
) -> egui::Vec2 {
    egui::vec2(
        (data.sheet_width - frame_margin.x).max(0.0),
        (data.sheet_height - frame_margin.y).max(0.0),
    )
}

fn sidebar_header(ui: &mut egui::Ui) -> SidebarHeader {
    let header_rect = allocate_sidebar_header(ui);
    let labels = sidebar_header_labels();
    let rects = sidebar_header_rects(ui, header_rect, &labels.playlist, &labels.history);
    SidebarHeader {
        rects,
        playlist_label: labels.playlist,
        history_label: labels.history,
    }
}

fn sidebar_header_labels() -> SidebarHeaderLabels {
    SidebarHeaderLabels {
        playlist: t!("playlist").to_string(),
        history: t!("history").to_string(),
    }
}

fn sidebar_header_rects(
    ui: &egui::Ui,
    header_rect: egui::Rect,
    playlist_label: &str,
    history_label: &str,
) -> SidebarHeaderRects {
    // Action buttons are measured independently from tab labels; long localized
    // tab text should not push the open/clear icons away from the edges.
    let tab_rects = sidebar_tab_rects(ui, header_rect, playlist_label, history_label);
    let clear_size = crate::playlist_panel::clear_all_button_size(ui);
    SidebarHeaderRects {
        open: header_button_rect(
            header_rect,
            header_rect.left(),
            crate::playlist_panel::open_file_button_size(ui),
        ),
        clear: header_button_rect(header_rect, header_rect.right() - clear_size.x, clear_size),
        playlist: tab_rects.0,
        history: tab_rects.1,
    }
}

fn header_button_rect(header_rect: egui::Rect, left: f32, size: egui::Vec2) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(left, header_rect.center().y - size.y * 0.5),
        size,
    )
}

fn sidebar_tab_rects(
    ui: &egui::Ui,
    header_rect: egui::Rect,
    playlist_label: &str,
    history_label: &str,
) -> (egui::Rect, egui::Rect) {
    // The two tabs are centered as a pair.  Measuring both before positioning keeps
    // the active tab from shifting when labels have different widths.
    let tab_gap = ui.spacing().item_spacing.x + crate::visuals::FLOATING_PANEL_MARGIN;
    let tab_padding = SIDEBAR_TAB_PADDING_X * 2.0;
    let playlist_width = sidebar_tab_width(ui, playlist_label, tab_padding);
    let history_width = sidebar_tab_width(ui, history_label, tab_padding);
    let tab_height = SIDEBAR_TAB_HEIGHT;
    let tab_y = header_rect.center().y - tab_height * 0.5;
    let tabs_width = playlist_width + tab_gap + history_width;
    let tabs_left = header_rect.center().x - tabs_width * 0.5;
    let playlist_rect = egui::Rect::from_min_size(
        egui::pos2(tabs_left, tab_y),
        egui::vec2(playlist_width, tab_height),
    );
    let history_rect = egui::Rect::from_min_size(
        egui::pos2(tabs_left + playlist_width + tab_gap, tab_y),
        egui::vec2(history_width, tab_height),
    );
    (playlist_rect, history_rect)
}

fn sidebar_tab_width(ui: &egui::Ui, label: &str, tab_padding: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(
            label.to_string(),
            egui::TextStyle::Button.resolve(ui.style()),
            ui.visuals().text_color(),
        )
        .size()
        .x
        + tab_padding
}
