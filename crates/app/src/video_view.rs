use eframe::egui;
use engine::Player;
use player_core::Command;
use render::VideoTexture;
use rust_i18n::t;

fn empty_state_contents(ui: &mut egui::Ui, commands: &mut Vec<Command>) {
    ui.vertical_centered(|ui| {
        ui.label(t!("drop_hint").to_string());
        ui.add_space(8.0);
        commands.extend(crate::playlist_panel::open_file_button(ui));
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

    /// 每帧调用: 取引擎按主时钟选出的当前帧并绘制。`present_frame` 返回 None 表示
    /// 画面无变化(沿用上一帧纹理)——暂停/未到点/队列暂空都走这条, 不重复上传。
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        player: &mut Player,
    ) -> Vec<Command> {
        let mut commands = Vec::new();

        if let Some(vf) = player.present_frame() {
            self.upload(frame, vf);
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

    fn upload(&mut self, frame: &mut eframe::Frame, vf: &media::VideoFrame) {
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
            if let Some(id) = self.tex_id.take() {
                render_state.renderer.write().free_texture(&id);
            }
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

        if let Some(tex) = self.texture.as_mut() {
            tex.upload(queue, &vf.rgba);
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

        assert!(source.contains("crate::playlist_panel::open_file_button(ui)"));
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
    fn show_pulls_current_frame_from_engine_not_ui_side_selection() {
        // 选帧逻辑已移入引擎: UI 只调 present_frame 上传, 不再自己 decide_frame/丢帧。
        let source = include_str!("video_view.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("player.present_frame()"));
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
        assert!(source.contains("ui.painter().image"));
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
