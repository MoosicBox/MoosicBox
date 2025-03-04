use hyperchad_transformer::calc::FontMetrics;

pub struct FltkFontMetrics;

impl FontMetrics for FltkFontMetrics {
    fn measure_text(&self, _text: &str, _wrap_width: f32) -> bool {
        false
    }
}

