use hyperchad_transformer::calc::{FontMetrics, FontMetricsBounds};

pub struct FltkFontMetrics;

impl FontMetrics for FltkFontMetrics {
    fn measure_text(&self, _text: &str, _size: f32, _wrap_width: f32) -> FontMetricsBounds {
        FontMetricsBounds { rows: vec![] }
    }
}
