//! Font metrics implementation for FLTK.
//!
//! Provides font measurement capabilities for layout calculations in the FLTK renderer.

use hyperchad_transformer::layout::font::{FontMetrics, FontMetricsBounds};

/// Font metrics implementation for FLTK rendering.
///
/// Provides text measurement capabilities for the FLTK renderer to calculate
/// layout dimensions based on text content and font properties.
pub struct FltkFontMetrics;

impl FontMetrics for FltkFontMetrics {
    /// Measures text dimensions for layout calculation.
    ///
    /// Currently returns empty bounds as text measurement is not yet implemented
    /// for the FLTK renderer. Layout calculations use default font metrics instead.
    fn measure_text(&self, _text: &str, _size: f32, _wrap_width: f32) -> FontMetricsBounds {
        FontMetricsBounds { rows: vec![] }
    }
}
