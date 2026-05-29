use crate::controls;
use crate::video_view::VideoView;
use eframe::egui;
use engine::Player;

pub struct PlayerApp {
    player: Player,
    video_view: VideoView,
    rate_pct: u16,
}

impl PlayerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            player: Player::new(),
            video_view: VideoView::new(),
            rate_pct: 100,
        }
    }
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(path) = f.path {
                match path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .as_deref()
                {
                    Some("srt") | Some("ass") | Some("ssa") => self.player.load_subtitle(&path),
                    _ => self.player.handle(player_core::Command::Open(path)),
                }
            }
        }

        self.player.tick();

        let t = self.player.timeline();

        egui::Panel::bottom("controls").show_inside(ui, |ui| {
            for cmd in controls::controls_bar(ui, &t) {
                self.player.handle(cmd);
            }
            let actions = crate::enhance::enhance_bar(ui, self.rate_pct);
            for cmd in actions.commands {
                if let player_core::Command::SetRate(p) = cmd {
                    self.rate_pct = p;
                }
                self.player.handle(cmd);
            }
            if actions.screenshot {
                if let Some((rgba, w, h)) = self.video_view.last_frame() {
                    match crate::enhance::save_screenshot(rgba, w, h) {
                        Ok(p) => eprintln!("截图已保存: {}", p.display()),
                        Err(e) => eprintln!("截图失败: {e}"),
                    }
                }
            }
        });

        egui::Panel::left("playlist")
            .default_size(200.0)
            .show_inside(ui, |ui| {
                let paths = self.player.playlist_paths();
                let cur = self.player.current_index();
                for cmd in crate::playlist_panel::playlist_panel(ui, &paths, cur) {
                    self.player.handle(cmd);
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.video_view.show(ui, frame, &self.player);
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
