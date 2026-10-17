//! Layout calculation traits for the egui renderer.
//!
//! This module provides the `EguiCalc` trait that extends the base layout calculation
//! trait with egui-specific context handling, enabling layout calculations that can
//! access egui's font system and other context-dependent features.

use eframe::egui::{self};
use hyperchad_transformer::{Container, layout::Calc};

use crate::geometry::{GeometryError, GeometryViewport};

/// Seeds the root from the viewport and runs one `HyperChad` calculator pass.
///
/// The returned boolean has exactly `Calc::calc` semantics; it is not a promise
/// of nested-layout convergence. Callers own invalidation and subsequent passes.
/// No egui cursor, panel, widget allocation, or DPI conversion participates.
/// Responsive/runtime styles must already have been applied to `tree`.
///
/// # Errors
/// * Returns `GeometryError::InvalidViewport` for invalid viewport/zoom input.
///
/// # Panics
/// * Propagates calculator panics for unsupported shared layout or missing inputs.
pub fn calculate_layout(
    calculator: &impl Calc,
    tree: &mut Container,
    viewport: GeometryViewport,
) -> Result<bool, GeometryError> {
    let size = viewport.logical_size()?;
    tree.calculated_x = Some(0.0);
    tree.calculated_y = Some(0.0);
    tree.calculated_width = Some(size.x);
    tree.calculated_height = Some(size.y);
    Ok(calculator.calc(tree))
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
            Err(GeometryError::InvalidViewport)
        );
        assert_eq!(probe.0.get(), 0);
        assert_eq!(tree.calculated_width, None);
    }
}
