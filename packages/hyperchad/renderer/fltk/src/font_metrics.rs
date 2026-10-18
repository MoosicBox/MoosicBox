//! Font metrics implementation for FLTK.
//!
//! Provides font measurement capabilities for layout calculations in the FLTK renderer.

use hyperchad_transformer::layout::font::{
    FontMetrics, FontMetricsBounds, TextMeasureError, TextMeasureRequest,
};

/// Font metrics implementation for FLTK rendering.
///
/// Provides text measurement capabilities for the FLTK renderer to calculate
/// layout dimensions based on text content and font properties.
pub struct FltkFontMetrics;

impl FontMetrics for FltkFontMetrics {
    /// Reports unavailable native measurement rather than inventing geometry.
    ///
    /// # Errors
    /// * Native styled measurement is not yet implemented for FLTK.
    fn measure_text(
        &self,
        _request: &TextMeasureRequest<'_>,
    ) -> Result<FontMetricsBounds, TextMeasureError> {
        Err(TextMeasureError::Backend {
            message: "FLTK MetricsUnavailable: checked native text measurement is not implemented"
                .to_owned(),
        })
    }
}
