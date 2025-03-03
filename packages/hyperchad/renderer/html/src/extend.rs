use async_trait::async_trait;
use flume::{Receiver, Sender};
use hyperchad_renderer::{
    PartialView, RendererEvent, View,
    canvas::{self},
};
use thiserror::Error;

#[derive(Clone)]
pub struct HtmlRendererEventPub {
    sender: Sender<RendererEvent>,
}

#[derive(Debug, Error)]
pub enum HtmlRendererEventPubError {
    #[error(transparent)]
    Sender(#[from] flume::SendError<RendererEvent>),
}

impl HtmlRendererEventPub {
    #[must_use]
    pub fn new() -> (Self, Receiver<RendererEvent>) {
        let (tx, rx) = flume::unbounded();

        (Self { sender: tx }, rx)
    }

    /// # Errors
    ///
    /// * If the sender failed to send the event
    pub fn publish(&self, event: RendererEvent) -> Result<(), HtmlRendererEventPubError> {
        Ok(self.sender.send(event)?)
    }
}

#[async_trait]
pub trait ExtendHtmlRenderer {
    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to emit the event.
    async fn emit_event(
        &self,
        _pub: HtmlRendererEventPub,
        _event_name: String,
        _event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to render the view.
    async fn render(
        &self,
        _pub: HtmlRendererEventPub,
        _view: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to render the partial elements.
    async fn render_partial(
        &self,
        _pub: HtmlRendererEventPub,
        _partial: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if `ExtendHtmlRenderer` implementation fails to render the canvas update.
    async fn render_canvas(
        &self,
        _pub: HtmlRendererEventPub,
        _update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }
}
