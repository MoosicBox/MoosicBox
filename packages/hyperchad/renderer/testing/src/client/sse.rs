use hyperchad_renderer::View;

/// In-memory SSE frame model for test-driven stream simulation.
#[derive(Debug, Clone)]
pub enum SseFrame {
    /// Full view payload.
    View(View),
    /// Partial view payload.
    PartialView(View),
    /// Generic event payload.
    Event { name: String, value: Option<String> },
    /// Canvas update payload.
    #[cfg(feature = "canvas")]
    CanvasUpdate(hyperchad_renderer::canvas::CanvasUpdate),
}
