use super::order_float;

#[derive(Debug, Clone, Copy)]
pub struct FontMetricsRow {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct FontMetricsBounds {
    pub rows: Vec<FontMetricsRow>,
}

impl FontMetricsBounds {
    #[must_use]
    pub fn width(&self) -> f32 {
        self.rows
            .iter()
            .map(|x| x.width)
            .max_by(order_float)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn height(&self) -> f32 {
        self.rows.iter().map(|x| x.height).sum()
    }
}

pub trait FontMetrics {
    fn measure_text(&self, text: &str, size: f32, wrap_width: f32) -> FontMetricsBounds;
}
