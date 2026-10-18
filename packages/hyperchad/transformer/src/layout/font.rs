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

/// Font-family selection, retaining whether a family was authored or inherited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFontFamily<'a> {
    /// Preserve the backend's historical default selection.
    CompatibilityDefault,
    /// Ordered authored fallback names, including inherited explicit names.
    Authored(&'a [String]),
}

/// Font-weight selection, retaining explicit inherited provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFontWeight {
    /// Preserve the backend's historical default weight.
    CompatibilityDefault,
    /// Absolute weight in the inclusive range 100 through 900.
    Absolute(u16),
}

/// Resolves authored weight while preserving explicit inherited provenance.
///
/// # Errors
///
/// * Relative weights from compatibility require a backend-declared baseline.
/// * An inherited absolute weight outside 100..=900 is invalid.
/// * A compatibility baseline outside that range is invalid when used for a relative weight.
pub fn resolve_text_weight(
    authored: hyperchad_transformer_models::FontWeight,
    inherited: TextFontWeight,
    compatibility_weight: Option<u16>,
) -> Result<TextFontWeight, TextMeasureError> {
    use hyperchad_transformer_models::FontWeight;
    if matches!(inherited, TextFontWeight::Absolute(value) if !(100..=900).contains(&value)) {
        return Err(TextMeasureError::InvalidRequest {
            message: "inherited font weight must be in 100..=900".into(),
        });
    }
    let weight = match authored {
        FontWeight::Thin | FontWeight::Weight100 => 100,
        FontWeight::ExtraLight | FontWeight::Weight200 => 200,
        FontWeight::Light | FontWeight::Weight300 => 300,
        FontWeight::Normal | FontWeight::Weight400 => 400,
        FontWeight::Medium | FontWeight::Weight500 => 500,
        FontWeight::SemiBold | FontWeight::Weight600 => 600,
        FontWeight::Bold | FontWeight::Weight700 => 700,
        FontWeight::ExtraBold | FontWeight::Weight800 => 800,
        FontWeight::Black | FontWeight::Weight900 => 900,
        FontWeight::Bolder => {
            let parent = relative_parent(inherited, compatibility_weight)?;
            if parent < 350 {
                400
            } else if parent < 550 {
                700
            } else {
                900
            }
        }
        FontWeight::Lighter => {
            let parent = relative_parent(inherited, compatibility_weight)?;
            if parent < 550 {
                100
            } else if parent < 750 {
                400
            } else {
                700
            }
        }
    };
    Ok(TextFontWeight::Absolute(weight))
}

fn relative_parent(
    inherited: TextFontWeight,
    baseline: Option<u16>,
) -> Result<u16, TextMeasureError> {
    let value = match inherited {
        TextFontWeight::Absolute(value) => value,
        TextFontWeight::CompatibilityDefault => {
            baseline.ok_or_else(|| TextMeasureError::UnsupportedStyle {
                message: "relative weight requires the backend compatibility weight".into(),
            })?
        }
    };
    if !(100..=900).contains(&value) {
        return Err(TextMeasureError::InvalidRequest {
            message: "font weight must be in 100..=900".into(),
        });
    }
    Ok(value)
}

/// Complete backend-independent request for logical text measurement.
#[derive(Clone, Copy, Debug)]
pub struct TextMeasureRequest<'a> {
    /// Text to measure, without backend-specific preprocessing.
    pub text: &'a str,
    /// Ordered family selection.
    pub family: TextFontFamily<'a>,
    /// Resolved weight with explicit provenance.
    pub weight: TextFontWeight,
    /// Font size in logical layout units.
    pub size: f32,
    /// Available logical width; positive infinity requests intrinsic width.
    pub wrap_width: f32,
    /// Effective inherited whitespace policy; `None` retains compatibility.
    pub whitespace: Option<hyperchad_transformer_models::WhiteSpace>,
}

/// Explicit text measurement failures; never substitute fallback geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextMeasureError {
    /// Invalid dimensions, weight, or another request property.
    InvalidRequest {
        /// Human-readable diagnostic.
        message: String,
    },
    /// The backend cannot faithfully resolve the requested style.
    UnsupportedStyle {
        /// Human-readable diagnostic.
        message: String,
    },
    /// The measurement backend failed.
    Backend {
        /// Human-readable diagnostic.
        message: String,
    },
}

impl std::fmt::Display for TextMeasureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { message }
            | Self::UnsupportedStyle { message }
            | Self::Backend { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for TextMeasureError {}

/// Trait for measuring logical text dimensions with a resolved style.
pub trait FontMetrics {
    /// Actual weight of the historical compatibility face, when known.
    ///
    /// `None` explicitly rejects relative-from-default resolution; it does not
    /// assume a backend-independent regular weight.
    fn compatibility_weight(&self) -> Option<u16> {
        None
    }

    /// Measures logical line bounds, not glyph mesh/ink bounds.
    ///
    /// # Errors
    ///
    /// * Returns an invalid-request error for unsupported dimensions or weights.
    /// * Returns an unsupported-style error rather than silently substituting an
    ///   unavailable authored family or weight.
    /// * Returns a backend error when measurement fails.
    fn measure_text(
        &self,
        request: &TextMeasureRequest<'_>,
    ) -> Result<FontMetricsBounds, TextMeasureError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_transformer_models::FontWeight;

    #[test_log::test]
    fn relative_weight_requires_actual_compatibility_baseline() {
        assert!(matches!(
            resolve_text_weight(
                FontWeight::Bolder,
                TextFontWeight::CompatibilityDefault,
                None
            ),
            Err(TextMeasureError::UnsupportedStyle { .. })
        ));
        assert!(matches!(
            resolve_text_weight(
                FontWeight::Bolder,
                TextFontWeight::CompatibilityDefault,
                Some(300)
            ),
            Ok(TextFontWeight::Absolute(400))
        ));
        assert!(matches!(
            resolve_text_weight(
                FontWeight::Bolder,
                TextFontWeight::CompatibilityDefault,
                Some(400)
            ),
            Ok(TextFontWeight::Absolute(700))
        ));
        assert!(matches!(
            resolve_text_weight(FontWeight::Lighter, TextFontWeight::Absolute(700), None),
            Ok(TextFontWeight::Absolute(400))
        ));
        assert!(matches!(
            resolve_text_weight(
                FontWeight::Bolder,
                TextFontWeight::CompatibilityDefault,
                Some(0)
            ),
            Err(TextMeasureError::InvalidRequest { .. })
        ));
    }

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
