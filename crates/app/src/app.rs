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
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label("拖入视频文件开始播放");
            });
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    }
}
