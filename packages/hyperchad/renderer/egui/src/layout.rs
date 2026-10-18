//! Layout calculation traits for the egui renderer.
//!
//! This module provides the `EguiCalc` trait that extends the base layout calculation
//! trait with egui-specific context handling, enabling layout calculations that can
//! access egui's font system and other context-dependent features.

use eframe::egui::{self};
use hyperchad_transformer::{Container, layout::Calc};

use crate::geometry::{GeometryError, GeometryViewport};

/// Failure to seed a viewport or complete checked shared layout.
#[derive(Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// Invalid viewport geometry.
    Viewport(GeometryError),
    /// Shared checked calculator failure, retaining its path and kind.
    Calculation(hyperchad_transformer::layout::LayoutError),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Viewport(error) => error.fmt(f),
            Self::Calculation(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for LayoutError {}

/// Seeds the root from the viewport and runs one `HyperChad` calculator pass.
///
/// The returned boolean has exactly `Calc::try_calc` semantics; it is not a promise
/// of nested-layout convergence. Callers own invalidation and subsequent passes.
/// No egui cursor, panel, widget allocation, or DPI conversion participates.
/// Responsive/runtime styles must already have been applied to `tree`.
///
/// # Errors
/// * Returns `LayoutError::Viewport` for invalid viewport/zoom input.
/// * Returns `LayoutError::Calculation` for unsupported checked calculators or
///   shared structural, budget, and geometry errors.
pub fn calculate_layout(
    calculator: &impl Calc,
    tree: &mut Container,
    viewport: GeometryViewport,
) -> Result<bool, LayoutError> {
    let size = viewport.logical_size().map_err(LayoutError::Viewport)?;
    tree.calculated_x = Some(0.0);
    tree.calculated_y = Some(0.0);
    tree.calculated_width = Some(size.x);
    tree.calculated_height = Some(size.y);
    calculator.try_calc(tree).map_err(LayoutError::Calculation)
}

/// Layout calculation trait for egui renderer.
///
/// Extends the `Calc` trait with egui-specific context handling.
pub trait EguiCalc: Calc {
    /// Associates this calculator with an egui context.
    #[must_use]
    fn with_context(self, context: egui::Context) -> Self;
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use eframe::egui::{Rect, pos2, vec2};

    struct Probe(Cell<usize>);

    impl Calc for Probe {
        fn try_calc(
            &self,
            tree: &mut Container,
        ) -> Result<bool, hyperchad_transformer::layout::LayoutError> {
            Ok(self.calc(tree))
        }
        fn calc(&self, tree: &mut Container) -> bool {
            self.0.set(self.0.get() + 1);
            assert_eq!(tree.calculated_width, Some(100.0));
            assert_eq!(tree.calculated_height, Some(50.0));
            assert_eq!(tree.calculated_x, Some(0.0));
            true
        }
    }

    #[test]
    fn root_uses_document_units_and_exactly_one_shared_pass() {
        let probe = Probe(Cell::new(0));
        let mut tree = Container::default();
        let viewport = GeometryViewport {
            rect: Rect::from_min_size(pos2(20.0, 30.0), vec2(200.0, 100.0)),
            zoom: 2.0,
        };
        assert!(calculate_layout(&probe, &mut tree, viewport).unwrap());
        assert_eq!(probe.0.get(), 1);
        assert_eq!(tree.calculated_y, Some(0.0));
    }

    #[test]
    fn invalid_viewport_never_invokes_calculator() {
        let probe = Probe(Cell::new(0));
        let mut tree = Container::default();
        let viewport = GeometryViewport {
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0)),
            zoom: 0.0,
        };
        assert_eq!(
            calculate_layout(&probe, &mut tree, viewport),
            Err(LayoutError::Viewport(GeometryError::InvalidViewport))
        );
        assert_eq!(probe.0.get(), 0);
        assert_eq!(tree.calculated_width, None);
    }
}
