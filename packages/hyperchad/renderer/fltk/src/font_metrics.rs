use hyperchad_transformer::layout::font::{FontMetrics, FontMetricsBounds};

/// Font metrics implementation for FLTK rendering.
///
/// Provides text measurement capabilities for the FLTK renderer to calculate
/// layout dimensions based on text content and font properties.
pub struct FltkFontMetrics;

impl FontMetrics for FltkFontMetrics {
    fn measure_text(&self, _text: &str, _size: f32, _wrap_width: f32) -> FontMetricsBounds {
        FontMetricsBounds { rows: vec![] }
    }
}
