use eframe::egui;
use engine::Player;
use render::VideoTexture;
use sync::{decide_frame, FrameDecision};

/// 持有 wgpu 纹理与其在 egui 中的注册 id。
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

    /// 每帧调用: 按主时钟挑选并显示视频帧。
    pub fn show(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame, player: &Player) {
        let master_ms = player.timeline().position_ms;

        if let Some(dt) = player.video() {
            let mut chosen = None;
            while let Some(vf) = dt.try_recv_frame() {
                match decide_frame(master_ms, vf.pts_ms, 15) {
                    FrameDecision::Display => {
                        chosen = Some(vf);
                        break;
                    }
                    FrameDecision::Drop => {
                        continue;
                    }
                    FrameDecision::Wait { .. } => {
                        chosen = Some(vf);
                        break;
                    }
                }
            }
            if let Some(vf) = chosen {
                self.upload(frame, &vf);
            }
        }

        if let (Some(id), (w, h)) = (self.tex_id, self.size) {
            if w > 0 && h > 0 {
                let avail = ui.available_size();
                let scale = (avail.x / w as f32).min(avail.y / h as f32).max(0.0);
                let draw = egui::vec2(w as f32 * scale, h as f32 * scale);
                ui.centered_and_justified(|ui| {
                    ui.image((id, draw));
                });
            }
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("拖入视频文件开始播放");
            });
        }

        let rect = ui.min_rect();
        if let Some(text) = player.current_subtitle() {
            crate::subtitle_overlay::draw_subtitle(ui, rect, &text);
        }
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
