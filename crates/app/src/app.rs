use crate::controls;
use crate::video_view::VideoView;
use eframe::egui;
use engine::Player;

pub struct PlayerApp {
    player: Player,
    video_view: VideoView,
}

impl PlayerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            player: Player::new(),
            video_view: VideoView::new(),
        }
    }
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if let Some(cmd) = controls::dropped_file_command(&ctx) {
            self.player.handle(cmd);
        }

        let t = self.player.timeline();

        egui::Panel::bottom("controls").show_inside(ui, |ui| {
            for cmd in controls::controls_bar(ui, &t) {
                self.player.handle(cmd);
            }
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.video_view.show(ui, frame, &self.player);
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
