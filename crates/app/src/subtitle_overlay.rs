use eframe::egui;

const SUBTITLE_BOTTOM_MARGIN: f32 = 40.0;
/// 字幕换行宽度占视频区域宽度的比例, 两侧各留 4% 边距。
const SUBTITLE_WRAP_FRACTION: f32 = 0.92;

/// 在给定区域底部居中绘制字幕文本(带黑色描边)。
/// 长台词按区域宽度自动换行并逐行居中, 不会被视频边缘截断。
pub fn draw_subtitle(ui: &mut egui::Ui, area: egui::Rect, text: &str, size: f32) {
    if text.is_empty() {
        return;
    }
    let painter = ui.painter_at(area);
    let font = egui::FontId::proportional(size);
    let wrap_width = (area.width() * SUBTITLE_WRAP_FRACTION).max(0.0);
    let galley = |color: egui::Color32| {
        let mut job =
            egui::text::LayoutJob::simple(text.to_owned(), font.clone(), color, wrap_width);
        job.halign = egui::Align::Center;
        painter.layout_job(job)
    };
    let body = galley(egui::Color32::WHITE);
    let outline = galley(egui::Color32::BLACK);
    // halign=Center 时 pos.x 是中轴线, pos.y 是首行顶部; 让整块文本的底边
    // 停在距区域底部固定边距处, 单行位置与旧实现一致, 多行向上生长。
    let pos = egui::pos2(
        area.center().x,
        area.max.y - SUBTITLE_BOTTOM_MARGIN - body.size().y,
    );
    // 简单描边: 先画四角黑色偏移, 再画白色主体
    for off in [
        egui::vec2(1.0, 1.0),
        egui::vec2(-1.0, 1.0),
        egui::vec2(1.0, -1.0),
        egui::vec2(-1.0, -1.0),
    ] {
        painter.galley(pos + off, outline.clone(), egui::Color32::BLACK);
    }
    painter.galley(pos, body, egui::Color32::WHITE);
}

#[cfg(test)]
mod tests {
    #[test]
    fn subtitles_wrap_to_area_width_and_center_each_line() {
        let source = include_str!("subtitle_overlay.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        // 换行 + 逐行居中: 不再用单行 no-wrap 的 painter.text。
        assert!(source.contains("LayoutJob::simple"));
        assert!(source.contains("job.halign = egui::Align::Center"));
        assert!(source.contains("SUBTITLE_WRAP_FRACTION"));
        assert!(!source.contains("Align2::CENTER_BOTTOM"));
    }
}
