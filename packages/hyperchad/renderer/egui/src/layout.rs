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
