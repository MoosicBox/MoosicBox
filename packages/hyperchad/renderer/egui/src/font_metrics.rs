use eframe::egui::{self};
use hyperchad_transformer::calc::FontMetrics;

#[derive(Clone)]
pub struct EguiFontMetrics {
    ctx: egui::Context,
}

impl FontMetrics for EguiFontMetrics {
    fn measure_text(&self, _text: &str, _wrap_width: f32) -> bool {
        let _galley = self.ctx.fonts(|x| {
            let text = "test".to_string();
            let font_id = egui::FontId::default();
            let color = egui::Color32::WHITE;
            x.layout(text, font_id, color, f32::INFINITY)
        });
        false
    }
}
