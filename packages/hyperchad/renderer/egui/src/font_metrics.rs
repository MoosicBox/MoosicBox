use eframe::egui::{self};
use hyperchad_transformer::layout::font::{FontMetrics, FontMetricsBounds, FontMetricsRow};

/// Font metrics implementation for egui renderer.
///
/// Provides text measurement capabilities using egui's font system.
#[derive(Clone)]
pub struct EguiFontMetrics {
    ctx: egui::Context,
}

impl EguiFontMetrics {
    /// Creates a new `EguiFontMetrics` instance with the given egui context.
    #[must_use]
    pub const fn new(ctx: egui::Context) -> Self {
        Self { ctx }
    }
}

impl FontMetrics for EguiFontMetrics {
    /// Measures text dimensions using egui's font system.
    ///
    /// Returns the bounds containing width and height information for each row
    /// of text after layout with the specified font size and wrap width.
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

/// Converts an egui `PlacedRow` to a `FontMetricsRow`.
///
/// Extracts width and height from the visual mesh bounds.
fn from_row(value: &egui::epaint::text::PlacedRow) -> FontMetricsRow {
    FontMetricsRow {
        width: value.visuals.mesh_bounds.width(),
        height: value.visuals.mesh_bounds.height(),
    }
}

/// Converts an egui `Galley` to `FontMetricsBounds`.
///
/// Maps all rows in the galley to font metrics rows.
fn from_galley(value: &egui::Galley) -> FontMetricsBounds {
    log::trace!("from_galley");
    FontMetricsBounds {
        rows: value.rows.iter().map(from_row).collect(),
    }
}
