//! Layout calculation traits for the egui renderer.
//!
//! This module provides the `EguiCalc` trait that extends the base layout calculation
//! trait with egui-specific context handling, enabling layout calculations that can
//! access egui's font system and other context-dependent features.

use eframe::egui::{self};
use hyperchad_transformer::layout::Calc;

/// Layout calculation trait for egui renderer.
///
/// Extends the `Calc` trait with egui-specific context handling.
pub trait EguiCalc: Calc {
    /// Associates this calculator with an egui context.
    #[must_use]
    fn with_context(self, context: egui::Context) -> Self;
}
