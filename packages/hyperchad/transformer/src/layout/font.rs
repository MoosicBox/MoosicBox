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
