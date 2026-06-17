//! Video surface rendering.
//!
//! The engine decides which decoded frame is current; `VideoView` only uploads that
//! frame to a native texture and draws it into the available egui rectangle.  Keeping
//! frame selection out of the UI avoids resize/layout work from changing playback
//! timing.
//!
//! The empty state uses normal egui layout, while video playback uses a fixed fitted
//! rect.  That distinction prevents image widgets from asking egui for a second
//! sizing pass during window resize and keeps startup restore repainting until the
//! first paused frame has reached the texture.
//!
//! Native texture ids are reused across same-size frames and replaced only when
//! dimensions change.  That keeps upload work predictable during playback.

use eframe::egui;
use engine::Player;
use player_core::Command;
use render::VideoTexture;
use rust_i18n::t;

fn empty_state_contents(ui: &mut egui::Ui, commands: &mut Vec<Command>) {
    ui.vertical_centered(|ui| {
        ui.label(t!("drop_hint").to_string());
        ui.add_space(8.0);

        // 使用图标+文本的按钮
        let button_text = format!("+ {}", t!("open_file"));
        if ui.button(button_text).clicked() {
            commands.push(Command::OpenDialog);
        }
    });
}

fn empty_state_top_padding(ui: &egui::Ui) -> f32 {
    // Center the compact empty state within the remaining panel without making the
    // button participate in a second full-panel layout pass.
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
    // Degenerate frames still allocate a zero-sized rect at center so callers can
    // keep subtitle/layout math simple.
    if content.x <= 0.0 || content.y <= 0.0 || container.is_negative() {
        return egui::Rect::from_center_size(container.center(), egui::Vec2::ZERO);
    }
    let scale = (container.width() / content.x)
        .min(container.height() / content.y)
        .max(0.0);
    egui::Rect::from_center_size(container.center(), content * scale)
}

/// 持有 wgpu 纹理与其在 egui 中的注册 id。选帧/同步已在引擎(`Player::present_frame`)完成,
/// 这里只负责: 有新帧时上传纹理, 然后按宽高比绘制 + 叠加字幕。
pub struct VideoView {
    texture: Option<VideoTexture>,
    tex_id: Option<egui::TextureId>,
    size: (u32, u32),
}

impl VideoView {
    pub fn new() -> Self {
        Self {
            texture: None,
            tex_id: None,
            size: (0, 0),
        }
    }

    pub fn has_texture(&self) -> bool {
        self.tex_id.is_some()
    }

    /// 每帧调用: 取引擎按主时钟选出的当前帧并绘制。`present_frame` 返回 None 表示
    /// 画面无变化(沿用上一帧纹理)——暂停/未到点/队列暂空都走这条, 不重复上传。
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        player: &mut Player,
    ) -> Vec<Command> {
        let mut commands = Vec::new();

        let uploaded_new_frame = self.upload_presented_frame(frame, player);
        self.upload_cached_frame_if_needed(frame, player, uploaded_new_frame);
        let subtitle_rect = self.show_video_content(ui, player.video().is_some(), &mut commands);

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

    fn upload_presented_frame(&mut self, frame: &mut eframe::Frame, player: &mut Player) -> bool {
        match player.present_frame() {
            Some(vf) => {
                self.upload_frame(frame, vf);
                true
            }
            None => false,
        }
    }

    fn upload_cached_frame_if_needed(
        &mut self,
        frame: &mut eframe::Frame,
        player: &Player,
        uploaded_new_frame: bool,
    ) {
        // Startup restore can pause before a new present result arrives.  In that
        // case upload the engine's cached frame so the first screen is not blank.
        if uploaded_new_frame || self.tex_id.is_some() {
            return;
        }
        if let Some((rgba, w, h)) = player.current_frame_rgba() {
            self.upload_rgba(frame, rgba, w, h);
        }
    }

    fn show_video_content(
        &self,
        ui: &mut egui::Ui,
        has_video: bool,
        commands: &mut Vec<Command>,
    ) -> egui::Rect {
        if let Some(rect) = self.paint_current_texture(ui) {
            return rect;
        }
        if !has_video {
            commands.extend(empty_state(ui));
            return ui.min_rect();
        }
        let panel_rect = ui.available_rect_before_wrap();
        ui.allocate_rect(panel_rect, egui::Sense::hover());
        panel_rect
    }

    fn paint_current_texture(&self, ui: &mut egui::Ui) -> Option<egui::Rect> {
        let (Some(id), (w, h)) = (self.tex_id, self.size) else {
            return None;
        };
        if w == 0 || h == 0 {
            return None;
        }

        let panel_rect = ui.available_rect_before_wrap();
        ui.allocate_rect(panel_rect, egui::Sense::hover());
        let image_rect = fit_rect(panel_rect, egui::vec2(w as f32, h as f32));
        // 自绘窗口: 视频需要圆角裁剪，与窗口背景的圆角半径一致。
        let corner_radius = crate::titlebar::window_corner_radius();
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        let shape =
            egui::epaint::RectShape::filled(image_rect, corner_radius, egui::Color32::WHITE)
                .with_texture(id, uv);
        ui.painter().add(egui::Shape::Rect(shape));
        Some(image_rect)
    }

    fn upload_frame(&mut self, frame: &mut eframe::Frame, vf: &media::VideoFrame) {
        self.upload_rgba(frame, &vf.rgba, vf.width, vf.height);
    }

    fn upload_rgba(&mut self, frame: &mut eframe::Frame, rgba: &[u8], width: u32, height: u32) {
        let render_state = frame
            .wgpu_render_state()
            .expect("需要 wgpu 后端 (NativeOptions.renderer = Wgpu)");
        let device = &render_state.device;
        let queue = &render_state.queue;

        let need_new = match &self.texture {
            Some(t) => t.size() != (width, height),
            None => true,
        };
        // Recreate the native texture only when dimensions change; same-size frames
        // reuse the registered egui texture id.
        if need_new {
            if let Some(id) = self.tex_id.take() {
                let mut renderer = render_state.renderer.write();
                renderer.free_texture(&id);
            }
            let tex = VideoTexture::new(device, width, height);
            let view = tex.create_view();
            let id = render_state.renderer.write().register_native_texture(
                device,
                &view,
                wgpu::FilterMode::Linear,
            );
            self.texture = Some(tex);
            self.tex_id = Some(id);
            self.size = (width, height);
        }

        if let Some(tex) = self.texture.as_mut() {
            tex.upload(queue, rgba);
        }
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

    #[test]
    fn empty_state_exposes_single_open_entry() {
        let source = include_str!("video_view.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("Command::OpenDialog"));
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
    fn show_pulls_current_frame_from_engine_not_ui_side_selection() {
        // 选帧逻辑已移入引擎: UI 只调 present_frame 上传, 不再自己 decide_frame/丢帧。
        let source = include_str!("video_view.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("player.present_frame()"));
        assert!(source.contains("upload_cached_frame_if_needed"));
        assert!(source.contains("self.tex_id.is_some()"));
        assert!(source.contains("player.current_frame_rgba()"));
        assert!(!source.contains("decide_frame"));
        assert!(!source.contains("fn frame_action"));
        assert!(!source.contains("request_repaint"));
    }

    #[test]
    fn video_texture_draw_uses_fixed_rect_not_image_widget_layout() {
        let source = include_str!("video_view.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("fn fit_rect"));
        assert!(source.contains("ui.allocate_rect"));
        // 自绘窗口: 视频用圆角裁剪。
        assert!(source.contains("RectShape::filled(image_rect"));
        assert!(source.contains(".with_texture(id, uv)"));
        assert!(source.contains("window_corner_radius"));
        assert!(!source.contains("centered_and_justified"));
        assert!(!source.contains("ui.image((id, draw))"));
    }

    #[test]
    fn texture_replacement_frees_previous_native_texture_id() {
        let source = include_str!("video_view.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let replacement_source = source.split("if need_new").nth(1).unwrap();

        assert!(replacement_source.contains("self.tex_id.take()"));
        assert!(replacement_source.contains("free_texture(&id)"));
        assert!(
            replacement_source.find("free_texture(&id)").unwrap()
                < replacement_source.find("register_native_texture").unwrap()
        );
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
