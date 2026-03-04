//! Font metrics implementation for the egui renderer.
//!
//! This module provides egui-specific implementations of font metrics,
//! enabling text measurement and layout calculations using egui's font system.
//! The `EguiFontMetrics` struct implements the `FontMetrics` trait from
//! `hyperchad_transformer`.

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

#[cfg(test)]
mod tests {
    use eframe::egui;
    use hyperchad_transformer::layout::font::FontMetrics as _;

    use super::EguiFontMetrics;

    fn initialized_context() -> egui::Context {
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |_ctx| {});
        context
    }

    fn metric_bounds(text: &str, size: f32, wrap_width: f32) -> Vec<(f32, f32)> {
        let metrics = EguiFontMetrics::new(initialized_context());
        metrics
            .measure_text(text, size, wrap_width)
            .rows
            .into_iter()
            .map(|row| (row.width, row.height))
            .collect()
    }

    #[test_log::test]
    fn measure_text_wrap_width_changes_row_count() {
        let text = "The quick brown fox jumps over the lazy dog";

        let wide_rows = metric_bounds(text, 16.0, 1_000.0);
        let narrow_rows = metric_bounds(text, 16.0, 40.0);

        assert_eq!(wide_rows.len(), 1);
        assert!(
            narrow_rows.len() > wide_rows.len(),
            "expected wrapped text to produce more rows (wide: {}, narrow: {})",
            wide_rows.len(),
            narrow_rows.len()
        );
    }

    #[test_log::test]
    fn measure_text_respects_explicit_newlines() {
        let rows = metric_bounds("first line\nsecond line", 16.0, 1_000.0);

        assert!(
            rows.len() >= 2,
            "expected at least two rows for explicit newline, got {}",
            rows.len()
        );
        assert!(
            rows.into_iter()
                .all(|(width, height)| width > 0.0 && height > 0.0)
        );
    }
}
