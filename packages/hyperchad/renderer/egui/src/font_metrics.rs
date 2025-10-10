use eframe::egui::{self};
use hyperchad_transformer::layout::font::{FontMetrics, FontMetricsBounds, FontMetricsRow};

#[derive(Clone)]
pub struct EguiFontMetrics {
    ctx: egui::Context,
}

impl EguiFontMetrics {
    #[must_use]
    pub const fn new(ctx: egui::Context) -> Self {
        Self { ctx }
    }
}

impl FontMetrics for EguiFontMetrics {
    fn measure_text(&self, text: &str, size: f32, wrap_width: f32) -> FontMetricsBounds {
        log::trace!("measure_text: measuring text={text} size={size} wrap_width={wrap_width}");
        from_galley(&self.ctx.fonts_mut(|x| {
            log::trace!("measure_text: got fonts");
            let font_id = egui::FontId {
                size,
                ..Default::default()
            };
            let color = egui::Color32::WHITE;
            x.layout(text.to_string(), font_id, color, wrap_width)
        }))
    }
}

fn from_row(value: &egui::epaint::text::PlacedRow) -> FontMetricsRow {
    FontMetricsRow {
        width: value.visuals.mesh_bounds.width(),
        height: value.visuals.mesh_bounds.height(),
    }
}

fn from_galley(value: &egui::Galley) -> FontMetricsBounds {
    log::trace!("from_galley");
    FontMetricsBounds {
        rows: value.rows.iter().map(from_row).collect(),
    }
}
