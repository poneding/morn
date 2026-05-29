use crate::controls;
use eframe::egui;
use engine::Player;

pub struct PlayerApp {
    player: Player,
}

impl PlayerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            player: Player::new(),
        }
    }
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
            ui.centered_and_justified(|ui| {
                ui.label("拖入视频文件开始播放");
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
