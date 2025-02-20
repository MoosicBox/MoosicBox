use async_trait::async_trait;
use hyperchad_renderer::{canvas, PartialView, View};

#[async_trait]
pub trait ExtendHtmlRenderer {
    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to emit the event.
    async fn emit_event(
        &self,
        _event_name: String,
        _event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to render the view.
    async fn render(&self, _view: View) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to render the partial elements.
    async fn render_partial(
        &self,
        _partial: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to render the canvas update.
    async fn render_canvas(
        &self,
        _update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }
}
