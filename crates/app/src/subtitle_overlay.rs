use eframe::egui;

/// 在给定区域底部居中绘制字幕文本(带黑色描边)。
pub fn draw_subtitle(ui: &mut egui::Ui, area: egui::Rect, text: &str) {
    if text.is_empty() {
        return;
    }
    let painter = ui.painter_at(area);
    let font = egui::FontId::proportional(24.0);
    let pos = egui::pos2(area.center().x, area.max.y - 40.0);
    // 简单描边: 先画四角黑色偏移, 再画白色主体
    for off in [
        egui::vec2(1.0, 1.0),
        egui::vec2(-1.0, 1.0),
        egui::vec2(1.0, -1.0),
        egui::vec2(-1.0, -1.0),
    ] {
        painter.text(
            pos + off,
            egui::Align2::CENTER_BOTTOM,
            text,
            font.clone(),
            egui::Color32::BLACK,
        );
    }
    painter.text(
        pos,
        egui::Align2::CENTER_BOTTOM,
        text,
        font,
        egui::Color32::WHITE,
    );
}
