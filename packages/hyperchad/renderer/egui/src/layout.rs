use eframe::egui::{self};
use hyperchad_transformer::layout::Calc;

pub trait EguiCalc: Calc {
    #[must_use]
    fn with_context(self, context: egui::Context) -> Self;
}
