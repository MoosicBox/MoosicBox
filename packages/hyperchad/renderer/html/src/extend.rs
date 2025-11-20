//! Extensions for custom HTML renderer behavior.
//!
//! This module provides traits and utilities for extending the HTML renderer
//! with custom event handling, server-sent events, WebSocket updates, or other
//! real-time features. It enables publishing and subscribing to renderer events.

use async_trait::async_trait;
use flume::{Receiver, Sender};
use hyperchad_renderer::{
    RendererEvent, View,
    canvas::{self},
};
use thiserror::Error;

/// Publisher for HTML renderer events.
///
/// Allows publishing renderer events to subscribers through a channel.
#[derive(Clone)]
pub struct HtmlRendererEventPub {
    sender: Sender<RendererEvent>,
}

/// Errors that can occur when publishing HTML renderer events.
#[derive(Debug, Error)]
pub enum HtmlRendererEventPubError {
    /// Error occurred when sending an event through the channel.
    #[error(transparent)]
    Sender(#[from] Box<flume::SendError<RendererEvent>>),
}

impl HtmlRendererEventPub {
    /// Creates a new event publisher and returns the publisher along with a receiver.
    #[must_use]
    pub fn new() -> (Self, Receiver<RendererEvent>) {
        let (tx, rx) = flume::unbounded();

        (Self { sender: tx }, rx)
    }

    /// # Errors
    ///
    /// * If the sender failed to send the event
    pub fn publish(&self, event: RendererEvent) -> Result<(), HtmlRendererEventPubError> {
        Ok(self.sender.send(event).map_err(Box::new)?)
    }
}

/// Trait for extending HTML renderer with custom behavior.
///
/// Implementations can hook into rendering events to add custom functionality
/// like server-sent events, WebSocket updates, or other real-time features.
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
    /// Will error if `ExtendHtmlRenderer` implementation fails to render the canvas update.
    async fn render_canvas(
        &self,
        _pub: HtmlRendererEventPub,
        _update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_renderer_event_pub_new() {
        let (pub_handle, rx) = HtmlRendererEventPub::new();
        assert!(!rx.is_disconnected());
        drop(pub_handle);
        assert!(rx.is_disconnected());
    }

    #[test]
    fn test_html_renderer_event_pub_publish() {
        let (pub_handle, rx) = HtmlRendererEventPub::new();
        let event = RendererEvent::Event {
            name: "test".to_string(),
            value: None,
        };
        pub_handle.publish(event).unwrap();
        let received = rx.recv().unwrap();
        assert!(matches!(received, RendererEvent::Event { .. }));
    }

    #[test]
    fn test_html_renderer_event_pub_publish_disconnected() {
        let (pub_handle, rx) = HtmlRendererEventPub::new();
        drop(rx);
        let event = RendererEvent::Event {
            name: "test".to_string(),
            value: None,
        };
        let result = pub_handle.publish(event);
        assert!(result.is_err());
    }

    #[test]
    fn test_html_renderer_event_pub_clone() {
        let (pub_handle, rx) = HtmlRendererEventPub::new();
        let pub_handle_clone = pub_handle.clone();

        let event1 = RendererEvent::Event {
            name: "event1".to_string(),
            value: Some("value1".to_string()),
        };
        let event2 = RendererEvent::Event {
            name: "event2".to_string(),
            value: Some("value2".to_string()),
        };

        pub_handle.publish(event1).unwrap();
        pub_handle_clone.publish(event2).unwrap();

        assert!(rx.recv().is_ok());
        assert!(rx.recv().is_ok());
    }
}
