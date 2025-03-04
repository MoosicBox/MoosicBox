use hyperchad_transformer::calc::{FontMetrics, FontMetricsValue};

pub struct FltkFontMetrics;

impl FontMetrics for FltkFontMetrics {
    fn measure_text(&self, _text: &str, _size: f32, _wrap_width: f32) -> FontMetricsValue {
        FontMetricsValue { rows: vec![] }
    }
}
