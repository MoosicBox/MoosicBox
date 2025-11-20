//! Font metrics traits and types for text measurement.
//!
//! This module provides the [`FontMetrics`](crate::layout::font::FontMetrics) trait for measuring text dimensions
//! and related types for representing text bounds and row measurements.

use super::order_float;

/// Metrics for a single row of text.
#[derive(Debug, Clone, Copy)]
pub struct FontMetricsRow {
    /// Width of the text row.
    pub width: f32,
    /// Height of the text row.
    pub height: f32,
}

/// Bounding box measurements for multi-row text.
#[derive(Debug, Clone)]
pub struct FontMetricsBounds {
    /// Metrics for each row of text.
    pub rows: Vec<FontMetricsRow>,
}

impl FontMetricsBounds {
    /// Returns the maximum width across all rows.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.rows
            .iter()
            .map(|x| x.width)
            .max_by(order_float)
            .unwrap_or_default()
    }

    /// Returns the total height of all rows.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.rows.iter().map(|x| x.height).sum()
    }
}

/// Trait for measuring text dimensions with specific fonts.
pub trait FontMetrics {
    /// Measures the bounding box of text with the given size and wrap width.
    ///
    /// Returns measurements for each row when text wraps.
    fn measure_text(&self, text: &str, size: f32, wrap_width: f32) -> FontMetricsBounds;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn font_metrics_bounds_width_returns_max_width_across_rows() {
        let bounds = FontMetricsBounds {
            rows: vec![
                FontMetricsRow {
                    width: 100.0,
                    height: 20.0,
                },
                FontMetricsRow {
                    width: 150.0,
                    height: 20.0,
                },
                FontMetricsRow {
                    width: 75.0,
                    height: 20.0,
                },
            ],
        };

        assert!((bounds.width() - 150.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn font_metrics_bounds_width_returns_zero_for_empty_rows() {
        let bounds = FontMetricsBounds { rows: vec![] };

        assert!((bounds.width() - 0.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn font_metrics_bounds_height_returns_sum_of_all_row_heights() {
        let bounds = FontMetricsBounds {
            rows: vec![
                FontMetricsRow {
                    width: 100.0,
                    height: 20.0,
                },
                FontMetricsRow {
                    width: 150.0,
                    height: 25.0,
                },
                FontMetricsRow {
                    width: 75.0,
                    height: 15.0,
                },
            ],
        };

        assert!((bounds.height() - 60.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn font_metrics_bounds_height_returns_zero_for_empty_rows() {
        let bounds = FontMetricsBounds { rows: vec![] };

        assert!((bounds.height() - 0.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn font_metrics_bounds_width_handles_single_row() {
        let bounds = FontMetricsBounds {
            rows: vec![FontMetricsRow {
                width: 42.5,
                height: 10.0,
            }],
        };

        assert!((bounds.width() - 42.5).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn font_metrics_bounds_height_handles_single_row() {
        let bounds = FontMetricsBounds {
            rows: vec![FontMetricsRow {
                width: 42.5,
                height: 10.0,
            }],
        };

        assert!((bounds.height() - 10.0).abs() < f32::EPSILON);
    }
}
