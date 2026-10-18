//! Egui text shaping shared by HyperChad measurement and exact galley painting.
//!
//! Egui already caches galleys by layout job and font generation. This adapter
//! deliberately uses that cache rather than maintaining a second text cache.

use std::sync::Arc;

use crate::text_layout::{FontRegistry, request_job};
use eframe::egui;
use hyperchad_transformer::layout::font::{
    FontMetrics, FontMetricsBounds, FontMetricsRow, TextFontFamily, TextFontWeight,
    TextMeasureError, TextMeasureRequest,
};

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

    /// Shapes compatibility text without changing its whitespace. New styled
    /// consumers should use [`Self::layout_request`].
    ///
    /// # Panics
    /// * Invalid dimensions or fonts not initialized for an egui pass.
    #[must_use]
    pub fn layout_text(&self, text: &str, size: f32, wrap_width: f32) -> Arc<egui::Galley> {
        self.layout_request(&TextMeasureRequest {
            text,
            family: TextFontFamily::CompatibilityDefault,
            weight: TextFontWeight::CompatibilityDefault,
            size,
            wrap_width,
            whitespace: Some(hyperchad_transformer::models::WhiteSpace::PreserveWrap),
        })
        .expect("compatibility text shaping failed")
    }

    /// Shapes the exact styled logical galley used by native painting.
    ///
    /// # Errors
    /// * Invalid dimensions or unavailable authored face/weight.
    ///
    /// # Panics
    /// * Fonts have not been initialized by an active egui pass.
    pub fn layout_request(
        &self,
        request: &TextMeasureRequest<'_>,
    ) -> Result<Arc<egui::Galley>, TextMeasureError> {
        let registry = FontRegistry::for_context(&self.ctx);
        let job = request_job(&registry, request)?;
        Ok(self.ctx.fonts_mut(|fonts| fonts.layout_job(job)))
    }
}

impl FontMetrics for EguiFontMetrics {
    fn compatibility_weight(&self) -> Option<u16> {
        FontRegistry::for_context(&self.ctx).compatibility_weight()
    }

    fn measure_text(
        &self,
        request: &TextMeasureRequest<'_>,
    ) -> Result<FontMetricsBounds, TextMeasureError> {
        self.layout_request(request)
            .map(|galley| from_galley(&galley))
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

    fn measure(
        metrics: &EguiFontMetrics,
        text: &str,
        size: f32,
        wrap_width: f32,
    ) -> FontMetricsBounds {
        metrics
            .measure_text(&TextMeasureRequest {
                text,
                family: TextFontFamily::CompatibilityDefault,
                weight: TextFontWeight::CompatibilityDefault,
                size,
                wrap_width,
                whitespace: Some(hyperchad_transformer::models::WhiteSpace::PreserveWrap),
            })
            .unwrap()
    }

    fn with_fonts(test: impl FnOnce(EguiFontMetrics)) {
        let context = egui::Context::default();
        context.begin_pass(egui::RawInput::default());
        test(EguiFontMetrics::new(context.clone()));
        let _ = context.end_pass();
    }

    #[test]
    fn bundled_light_measurement_matches_shared_paint_job() {
        with_fonts(|metrics| {
            let families = ["Ubuntu Light".to_owned()];
            let request = TextMeasureRequest {
                text: " a\n  b\u{a0}c ",
                family: TextFontFamily::Authored(&families),
                weight: TextFontWeight::Absolute(300),
                size: 17.375,
                wrap_width: 40.0,
                whitespace: Some(hyperchad_transformer::models::WhiteSpace::PreserveWrap),
            };
            let measured = metrics.measure_text(&request).unwrap();
            let registry = FontRegistry::for_context(&metrics.ctx);
            let job = request_job(&registry, &request).unwrap();
            let painted = metrics.ctx.fonts_mut(|fonts| fonts.layout_job(job));
            assert!((measured.width() - painted.rect.width()).abs() < 0.001);
            assert!((measured.height() - painted.rect.height()).abs() < 0.001);
            let unavailable = TextMeasureRequest {
                weight: TextFontWeight::Absolute(700),
                ..request
            };
            assert!(matches!(
                metrics.measure_text(&unavailable),
                Err(TextMeasureError::UnsupportedStyle { .. })
            ));
        });
    }

    #[test]
    fn whitespace_has_advance_and_line_height_without_ink() {
        with_fonts(|metrics| {
            let measured = measure(&metrics, "   ", 16.0, f32::INFINITY);
            let galley = metrics.layout_text("   ", 16.0, f32::INFINITY);
            assert!(measured.width() > 0.0);
            assert!(measured.height() > 0.0);
            assert_eq!(measured.width(), galley.rect.width());
            assert_eq!(measured.height(), galley.rect.height());
            assert_eq!(galley.rows[0].visuals.mesh.vertices, []);
        });
    }

    #[test]
    fn blank_lines_and_final_newline_match_painted_logical_height() {
        with_fonts(|metrics| {
            for text in ["a\n\nb", "a\n", "\n", ""] {
                let galley = metrics.layout_text(text, 16.0, f32::INFINITY);
                let measured = measure(&metrics, text, 16.0, f32::INFINITY);
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
                let measured = measure(&metrics, text, 16.0, 45.0);
                assert!((measured.width() - first.rect.width()).abs() < 0.001);
                assert!((measured.height() - first.rect.height()).abs() < 0.001);
            }
        });
    }

    #[test]
    fn font_height_does_not_depend_on_glyph_ink() {
        with_fonts(|metrics| {
            let height = measure(&metrics, "A", 16.0, f32::INFINITY).height();
            for text in [".", "g", " "] {
                assert_eq!(
                    measure(&metrics, text, 16.0, f32::INFINITY).height(),
                    height
                );
            }
        });
    }
}
