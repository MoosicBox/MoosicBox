//! Egui text shaping shared by HyperChad measurement and exact galley painting.
//!
//! Egui already caches galleys by layout job and font generation. This adapter
//! deliberately uses that cache rather than maintaining a second text cache.

use std::sync::Arc;

use eframe::egui;
use hyperchad_transformer::layout::font::{FontMetrics, FontMetricsBounds, FontMetricsRow};

/// Font metrics in logical document units, not physical pixels or glyph ink.
#[derive(Clone)]
pub struct EguiFontMetrics {
    ctx: egui::Context,
}

impl EguiFontMetrics {
    /// Creates an adapter. Fonts must be initialized by an active egui pass
    /// before measurement or shaping.
    #[must_use]
    pub const fn new(ctx: egui::Context) -> Self {
        Self { ctx }
    }

    /// Shapes the exact galley used for measurement, ready for `Painter::galley`.
    ///
    /// Uses the current default font family because the shared `FontMetrics`
    /// contract only carries size. Size and wrap width are logical points;
    /// native pixels-per-point must not be multiplied into either argument.
    /// Paint this galley at the snapshot's content origin (and apply document
    /// zoom consistently). Do not reshape with a widget's default font/wrapping.
    /// Egui's font cache owns reuse and invalidation on font/DPI changes.
    ///
    /// # Panics
    /// * Panics if called before egui has initialized fonts for a pass.
    #[must_use]
    pub fn layout_text(&self, text: &str, size: f32, wrap_width: f32) -> Arc<egui::Galley> {
        self.ctx.fonts_mut(|fonts| {
            let mut job = egui::text::LayoutJob::simple(
                text.to_owned(),
                egui::FontId {
                    size,
                    ..Default::default()
                },
                egui::Color32::WHITE,
                wrap_width,
            );
            // Egui's GUI-output rounding independently rounds row widths and
            // floors/clamps the aggregate galley width to the wrap constraint.
            // That can make max(row.width) differ from galley.rect.width().
            // HyperChad needs the actual logical row union, not this widget
            // allocation approximation. Glyph pixel snapping remains enabled.
            job.round_output_to_gui = false;
            fonts.layout_job(job)
        })
    }
}

impl FontMetrics for EguiFontMetrics {
    fn measure_text(&self, text: &str, size: f32, wrap_width: f32) -> FontMetricsBounds {
        from_galley(&self.layout_text(text, size, wrap_width))
    }
}

fn from_galley(galley: &egui::Galley) -> FontMetricsBounds {
    // Positions include egui's pixel-rounded line spacing. Encode successive
    // logical row intervals so summing heights reproduces the painted galley,
    // including blank rows and a final newline. Ink is galley.mesh_bounds and
    // must never determine layout width, height, or caret advancement.
    FontMetricsBounds {
        rows: galley
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| FontMetricsRow {
                width: row.size.x,
                height: galley
                    .rows
                    .get(i + 1)
                    .map_or_else(|| galley.rect.bottom(), |next| next.pos.y)
                    - row.pos.y,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_fonts(test: impl FnOnce(EguiFontMetrics)) {
        let context = egui::Context::default();
        context.begin_pass(egui::RawInput::default());
        test(EguiFontMetrics::new(context.clone()));
        let _ = context.end_pass();
    }

    #[test]
    fn whitespace_has_advance_and_line_height_without_ink() {
        with_fonts(|metrics| {
            let measured = metrics.measure_text("   ", 16.0, f32::INFINITY);
            let galley = metrics.layout_text("   ", 16.0, f32::INFINITY);
            assert!(measured.width() > 0.0);
            assert!(measured.height() > 0.0);
            assert_eq!(measured.width(), galley.rect.width());
            assert_eq!(measured.height(), galley.rect.height());
            assert!(galley.rows[0].visuals.mesh.vertices.is_empty());
        });
    }

    #[test]
    fn blank_lines_and_final_newline_match_painted_logical_height() {
        with_fonts(|metrics| {
            for text in ["a\n\nb", "a\n", "\n", ""] {
                let galley = metrics.layout_text(text, 16.0, f32::INFINITY);
                let measured = metrics.measure_text(text, 16.0, f32::INFINITY);
                assert_eq!(measured.rows.len(), galley.rows.len());
                assert!((measured.height() - galley.rect.height()).abs() < 0.001);
                assert!(measured.rows.iter().all(|r| r.height > 0.0));
            }
        });
    }

    #[test]
    fn wraps_and_unicode_use_shaped_logical_bounds_and_cached_galley() {
        with_fonts(|metrics| {
            for text in ["hello world hello", "Agjp", "e\u{301} 中文"] {
                let first = metrics.layout_text(text, 16.0, 45.0);
                let second = metrics.layout_text(text, 16.0, 45.0);
                assert!(Arc::ptr_eq(&first, &second));
                let measured = metrics.measure_text(text, 16.0, 45.0);
                assert!((measured.width() - first.rect.width()).abs() < 0.001);
                assert!((measured.height() - first.rect.height()).abs() < 0.001);
            }
        });
    }

    #[test]
    fn font_height_does_not_depend_on_glyph_ink() {
        with_fonts(|metrics| {
            let height = metrics.measure_text("A", 16.0, f32::INFINITY).height();
            for text in [".", "g", " "] {
                assert_eq!(
                    metrics.measure_text(text, 16.0, f32::INFINITY).height(),
                    height
                );
            }
        });
    }
}
