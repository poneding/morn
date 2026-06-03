use eframe::egui;
use engine::Player;
use player_core::Command;
use render::VideoTexture;
use rust_i18n::t;
use sync::{decide_frame, FrameDecision};

/// 视频帧相对主时钟的容差(毫秒): PTS 落在 [master-TOL, master+TOL] 内即显示。
const FRAME_TOL_MS: u64 = 15;
/// UI 时钟单帧跳变超过该值时按 seek 处理, 清理等待中的旧帧。
const SEEK_JUMP_MS: u64 = 500;
/// seek 后遇到远在目标之后的未来帧, 视作 seek 前残留帧并丢弃。
const STALE_FUTURE_AFTER_SEEK_MS: u64 = 1000;

/// 对从队列取出的一帧的处理动作(decide_frame 的调用方语义)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameAction {
    /// 到点了: 显示这一帧。
    Show,
    /// 已过期: 丢弃, 继续取下一帧追赶主时钟。
    Discard,
    /// 还没到点: 暂存为 pending, 停止取帧(保持当前画面)。
    Hold,
}

/// 主时钟冻结(暂停)时, 未来帧必须 Hold 而非 Show, 否则视频会在音频暂停后继续推进。
fn frame_action(master_ms: u64, pts_ms: u64, tol: u64) -> FrameAction {
    match decide_frame(master_ms, pts_ms, tol) {
        FrameDecision::Display => FrameAction::Show,
        FrameDecision::Drop => FrameAction::Discard,
        FrameDecision::Wait { .. } => FrameAction::Hold,
    }
}

fn stale_future_after_seek(master_ms: u64, pts_ms: u64) -> bool {
    pts_ms.saturating_sub(master_ms) > STALE_FUTURE_AFTER_SEEK_MS
}

fn empty_state_contents(ui: &mut egui::Ui, commands: &mut Vec<Command>) {
    ui.vertical_centered(|ui| {
        ui.label(t!("drop_hint").to_string());
        ui.add_space(8.0);
        commands.extend(crate::playlist_panel::open_menu_button(ui));
    });
}

fn empty_state_top_padding(ui: &egui::Ui) -> f32 {
    let spacing = ui.spacing();
    let content_height = spacing.interact_size.y * 2.0 + spacing.item_spacing.y + 8.0;
    ((ui.available_height() - content_height) * 0.5).max(0.0)
}

fn empty_state(ui: &mut egui::Ui) -> Vec<Command> {
    let mut commands = Vec::new();
    ui.allocate_ui_with_layout(
        ui.available_size(),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space(empty_state_top_padding(ui));
            empty_state_contents(ui, &mut commands);
        },
    );
    commands
}

fn fit_rect(container: egui::Rect, content: egui::Vec2) -> egui::Rect {
    if content.x <= 0.0 || content.y <= 0.0 || container.is_negative() {
        return egui::Rect::from_center_size(container.center(), egui::Vec2::ZERO);
    }
    let scale = (container.width() / content.x)
        .min(container.height() / content.y)
        .max(0.0);
    egui::Rect::from_center_size(container.center(), content * scale)
}

/// 持有 wgpu 纹理与其在 egui 中的注册 id。
pub struct VideoView {
    texture: Option<VideoTexture>,
    tex_id: Option<egui::TextureId>,
    size: (u32, u32),
    last_frame: Option<(Vec<u8>, u32, u32)>,
    /// 已从队列取出但"尚未到显示时间"的帧, 留到主时钟追上时再显示。
    pending: Option<media::VideoFrame>,
    /// 上一次的主时钟读数, 用于检测向后 seek(时钟回退)。
    last_master_ms: u64,
    /// seek 后短暂丢弃远未来帧, 防止解码线程阻塞发送出的旧帧被 Hold 住。
    recovering_after_seek: bool,
}

impl VideoView {
    pub fn new() -> Self {
        Self {
            texture: None,
            tex_id: None,
            size: (0, 0),
            last_frame: None,
            pending: None,
            last_master_ms: 0,
            recovering_after_seek: false,
        }
    }

    /// 每帧调用: 按主时钟挑选并显示视频帧。
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        player: &Player,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        let master_ms = player.timeline().position_ms;

        // seek 会让时钟单帧大幅跳变; 之前暂存的"未来帧"已失效。
        let jumped = master_ms.saturating_add(SEEK_JUMP_MS) < self.last_master_ms
            || master_ms > self.last_master_ms.saturating_add(SEEK_JUMP_MS);
        if jumped {
            self.pending = None;
            self.recovering_after_seek = true;
        }
        self.last_master_ms = master_ms;

        if let Some(dt) = player.video() {
            // 先消费 pending, 再从解码队列取帧。Hold 时停止取帧并保留当前画面,
            // 这样暂停(主时钟冻结)期间不会继续显示后续帧。
            loop {
                let vf = match self.pending.take() {
                    Some(f) => f,
                    None => match dt.try_recv_frame() {
                        Some(f) => f,
                        None => break,
                    },
                };
                match frame_action(master_ms, vf.pts_ms, FRAME_TOL_MS) {
                    FrameAction::Show => {
                        self.recovering_after_seek = false;
                        self.upload(frame, vf);
                        break;
                    }
                    FrameAction::Discard => continue,
                    FrameAction::Hold => {
                        if self.recovering_after_seek
                            && stale_future_after_seek(master_ms, vf.pts_ms)
                        {
                            continue;
                        }
                        self.recovering_after_seek = false;
                        self.pending = Some(vf);
                        break;
                    }
                }
            }
        }

        let mut subtitle_rect = ui.available_rect_before_wrap();
        if let (Some(id), (w, h)) = (self.tex_id, self.size) {
            if w > 0 && h > 0 {
                let panel_rect = ui.available_rect_before_wrap();
                ui.allocate_rect(panel_rect, egui::Sense::hover());
                let image_rect = fit_rect(panel_rect, egui::vec2(w as f32, h as f32));
                subtitle_rect = image_rect;
                ui.painter().image(
                    id,
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        } else {
            commands.extend(empty_state(ui));
            subtitle_rect = ui.min_rect();
        }

        if let Some(text) = player.current_subtitle() {
            crate::subtitle_overlay::draw_subtitle(
                ui,
                subtitle_rect,
                &text,
                player.prefs().subtitle_font_size,
            );
        }

        commands
    }

    fn upload(&mut self, frame: &mut eframe::Frame, vf: media::VideoFrame) {
        let render_state = frame
            .wgpu_render_state()
            .expect("需要 wgpu 后端 (NativeOptions.renderer = Wgpu)");
        let device = &render_state.device;
        let queue = &render_state.queue;

        let need_new = match &self.texture {
            Some(t) => t.size() != (vf.width, vf.height),
            None => true,
        };
        if need_new {
            let tex = VideoTexture::new(device, vf.width, vf.height);
            let view = tex.create_view();
            let id = render_state.renderer.write().register_native_texture(
                device,
                &view,
                wgpu::FilterMode::Linear,
            );
            self.texture = Some(tex);
            self.tex_id = Some(id);
            self.size = (vf.width, vf.height);
        }

        // 1080p 一帧 RGBA ≈ 8MB, clone 会浪费内存带宽; 直接 move 进 last_frame。
        let media::VideoFrame {
            rgba,
            width,
            height,
            pts_ms: _,
        } = vf;
        if let Some(tex) = self.texture.as_mut() {
            tex.upload(queue, &rgba);
        }
        self.last_frame = Some((rgba, width, height));
    }

    /// 返回最近一次显示的帧 (RGBA8, 宽, 高), 供截图使用。
    pub fn last_frame(&self) -> Option<(&[u8], u32, u32)> {
        self.last_frame
            .as_ref()
            .map(|(d, w, h)| (d.as_slice(), *w, *h))
    }
}

impl Default for VideoView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const TOL: u64 = 15;

    #[test]
    fn future_frame_holds_past_frame_discards() {
        assert_eq!(frame_action(1000, 1000, TOL), FrameAction::Show);
        assert_eq!(frame_action(1000, 1010, TOL), FrameAction::Show); // 容差内
        assert_eq!(frame_action(2000, 1000, TOL), FrameAction::Discard); // 已过期
        assert_eq!(frame_action(1000, 2000, TOL), FrameAction::Hold); // 未来帧: 必须 Hold
    }

    /// 模拟一次 show() 的取帧循环: 返回应显示帧的 PTS(或 None=保持当前画面)。
    fn pump(master_ms: u64, pending: &mut Option<u64>, buf: &mut VecDeque<u64>) -> Option<u64> {
        let mut recovering = false;
        pump_with_recovery(master_ms, pending, buf, &mut recovering)
    }

    fn pump_with_recovery(
        master_ms: u64,
        pending: &mut Option<u64>,
        buf: &mut VecDeque<u64>,
        recovering_after_seek: &mut bool,
    ) -> Option<u64> {
        loop {
            let pts = match pending.take() {
                Some(p) => p,
                None => buf.pop_front()?,
            };
            match frame_action(master_ms, pts, TOL) {
                FrameAction::Show => {
                    *recovering_after_seek = false;
                    return Some(pts);
                }
                FrameAction::Discard => continue,
                FrameAction::Hold => {
                    if *recovering_after_seek && stale_future_after_seek(master_ms, pts) {
                        continue;
                    }
                    *recovering_after_seek = false;
                    *pending = Some(pts);
                    return None;
                }
            }
        }
    }

    #[test]
    fn paused_clock_freezes_video() {
        // 主时钟冻结在 1000(暂停), 缓冲区里是后续帧。
        let mut buf: VecDeque<u64> = [1000, 1033, 1066, 1100].into_iter().collect();
        let mut pending = None;

        // 第一次 pump: 显示当前帧 1000。
        assert_eq!(pump(1000, &mut pending, &mut buf), Some(1000));
        assert_eq!(buf.len(), 3);

        // 后续 pump(时钟仍冻结): 下一帧 1033 是未来帧 → Hold, 不显示也不再消费队列。
        assert_eq!(pump(1000, &mut pending, &mut buf), None);
        assert_eq!(pending, Some(1033));
        assert_eq!(buf.len(), 2);

        // 再 pump 多次, 缓冲区必须纹丝不动 —— 视频已冻结(修复前这里会连续耗尽队列)。
        for _ in 0..5 {
            assert_eq!(pump(1000, &mut pending, &mut buf), None);
            assert_eq!(buf.len(), 2);
        }
    }

    #[test]
    fn advancing_clock_shows_held_frame_when_due() {
        let mut buf: VecDeque<u64> = [1033, 1066].into_iter().collect();
        let mut pending = None;
        // 时钟 1000: 1033 还没到 → Hold。
        assert_eq!(pump(1000, &mut pending, &mut buf), None);
        assert_eq!(pending, Some(1033));
        // 时钟推进到 1033: 暂存帧到点 → 显示。
        assert_eq!(pump(1033, &mut pending, &mut buf), Some(1033));
        assert_eq!(pending, None);
    }

    #[test]
    fn seek_recovery_discards_stale_far_future_frame() {
        let mut buf: VecDeque<u64> = [60_000, 1033, 1066].into_iter().collect();
        let mut pending = None;
        let mut recovering = true;

        assert_eq!(
            pump_with_recovery(1000, &mut pending, &mut buf, &mut recovering),
            None
        );
        assert_eq!(pending, Some(1033));
        assert!(!recovering);
        assert_eq!(buf, VecDeque::from([1066]));
    }

    #[test]
    fn empty_state_exposes_single_open_entry() {
        let source = include_str!("video_view.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("crate::playlist_panel::open_menu_button(ui)"));
        assert!(!source.contains("Command::OpenDialog"));
        assert!(!source.contains("Command::OpenFolder"));
    }

    #[test]
    fn empty_state_centers_hint_and_actions() {
        let source = include_str!("video_view.rs")
            .split("/// 持有 wgpu")
            .next()
            .unwrap();

        assert!(source.contains("fn empty_state"));
        assert!(source.contains("empty_state_top_padding"));
        assert!(source.contains("ui.add_space(empty_state_top_padding(ui))"));
        assert!(source.contains("vertical_centered"));
        assert!(
            !source.contains("ui.horizontal("),
            "empty state actions should be individually centered, not left-aligned in a full-width row"
        );
    }

    #[test]
    fn empty_state_avoids_extra_sizing_pass_during_resize() {
        let source = include_str!("video_view.rs")
            .split("/// 持有 wgpu")
            .next()
            .unwrap();

        assert!(
            !source.contains("sizing_pass()"),
            "empty state should not do hidden measurement every frame"
        );
        assert!(
            !source.contains("empty_state_content_size"),
            "empty state should use a single layout pass"
        );
    }

    #[test]
    fn video_texture_draw_uses_fixed_rect_not_image_widget_layout() {
        let source = include_str!("video_view.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("fn fit_rect"));
        assert!(source.contains("ui.allocate_rect"));
        assert!(source.contains("ui.painter().image"));
        assert!(!source.contains("centered_and_justified"));
        assert!(!source.contains("ui.image((id, draw))"));
    }

    #[test]
    fn fit_rect_preserves_aspect_inside_container() {
        let container = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let fitted = fit_rect(container, egui::vec2(1920.0, 1080.0));

        assert!((fitted.width() - 800.0).abs() < f32::EPSILON);
        assert!((fitted.height() - 450.0).abs() < f32::EPSILON);
        assert_eq!(fitted.center(), container.center());
    }
}
