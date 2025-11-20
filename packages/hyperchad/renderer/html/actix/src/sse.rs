//! Server-Sent Events (SSE) support for real-time `HyperChad` updates.
//!
//! This module implements SSE streaming endpoints that push renderer events to connected clients
//! in real-time. Events include view updates, canvas updates, and custom application events.
//!
//! SSE connections are established via GET requests to the `/$sse` endpoint and remain open
//! for continuous streaming of updates. The module supports optional content encoding (gzip,
//! deflate, zstd) for efficient transmission.
//!
//! This module is only available when the `sse` feature is enabled.

use std::{io::Write as _, sync::Arc};

use actix_web::{
    HttpRequest, HttpResponse, Responder,
    error::ErrorInternalServerError,
    http::header::{CacheControl, CacheDirective, ContentEncoding},
    web,
};
use bytes::{BufMut as _, Bytes, BytesMut};
use flate2::{
    Compression,
    write::{DeflateEncoder, GzEncoder, ZlibEncoder},
};
use futures_util::{StreamExt as _, TryStreamExt};
use hyperchad_renderer::{Content, RendererEvent};

use crate::{ActixApp, ActixResponseProcessor};

/// Server-sent event data message with optional event type and ID fields.
///
/// This structure represents a single SSE message that will be sent to connected clients.
/// It can include an event type name, an ID for tracking, and the actual data payload.
#[must_use]
#[derive(Debug, Clone)]
pub struct EventData {
    /// Optional event type name (maps to SSE `event:` field).
    event: Option<String>,
    /// Optional event ID for tracking (maps to SSE `id:` field).
    id: Option<String>,
    /// The data payload (maps to SSE `data:` field).
    data: String,
}

impl EventData {
    /// Creates a new event data message with the specified data payload.
    ///
    /// The event type and ID are initially unset and can be added using
    /// the [`event`](Self::event) and [`id`](Self::id) methods.
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event: None,
            id: None,
            data: data.into(),
        }
    }

    /// Sets the event ID field and returns the modified message.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets the event type field and returns the modified message.
    pub fn event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }
}

impl From<EventData> for Event {
    fn from(data: EventData) -> Self {
        Self::Data(data)
    }
}

/// Server-sent events message containing one or more fields.
#[must_use]
#[derive(Debug, Clone)]
pub enum Event {
    /// A data event containing event data with optional event type and ID.
    Data(EventData),
}

impl Event {
    /// Splits data into lines and prepend each line with `prefix`.
    fn line_split_with_prefix(buf: &mut BytesMut, prefix: &'static str, data: &str) {
        // initial buffer size guess is len(data) + 10 lines of prefix + EOLs + EOF
        buf.reserve(data.len() + (10 * (prefix.len() + 1)) + 1);

        // append prefix + space + line to buffer
        for line in data.split('\n') {
            buf.put_slice(prefix.as_bytes());
            buf.put_slice(line.as_bytes());
            buf.put_u8(b'\n');
        }
    }

    /// Serializes message into event-stream format.
    pub(crate) fn into_bytes(self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            Self::Data(EventData { id, event, data }) => {
                if let Some(text) = id {
                    buf.put_slice(b"id: ");
                    buf.put_slice(text.as_bytes());
                    buf.put_u8(b'\n');
                }
                if let Some(text) = event {
                    buf.put_slice(b"event: ");
                    buf.put_slice(text.as_bytes());
                    buf.put_u8(b'\n');
                }

                Self::line_split_with_prefix(&mut buf, "data: ", &data);
            }
        }

        // final newline to mark end of message
        buf.put_u8(b'\n');

        buf.freeze()
    }
}

/// Handles GET requests to the `/$sse` endpoint for server-sent events streaming.
///
/// This function establishes a long-lived SSE connection that streams renderer events
/// (view updates, canvas updates, custom events) to the connected client in real-time.
/// The connection remains open until closed by the client or server.
///
/// Events are formatted according to the SSE specification with optional compression
/// (gzip, deflate, zstd) based on client capabilities.
///
/// # Errors
///
/// * Returns an error if request preparation fails via `prepare_request`
/// * Returns an error if content conversion fails via `to_body`
/// * Returns an error if UTF-8 conversion of body content fails
///
/// # Panics
///
/// * Panics if JSON serialization of canvas update fails
/// * Panics if compression encoding fails (gzip, deflate, or zstd)
#[allow(clippy::future_not_send, clippy::too_many_lines)]
pub async fn handle_sse<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    req: HttpRequest,
    app: web::Data<ActixApp<T, R>>,
    body: Option<web::Bytes>,
) -> impl Responder {
    log::debug!("handle_sse: initializing SSE connection");
    let data = app
        .processor
        .prepare_request(req, body.map(Arc::new))
        .map_err(ErrorInternalServerError)?;

    let encoding = ContentEncoding::Identity;

    let stream = app
        .renderer_event_rx
        .clone()
        .into_stream()
        .then(move |event| {
            let app = app.clone();
            let data = data.clone();
            async move {
                log::debug!("handle_sse: received renderer_event_rx event");
                Ok::<_, actix_web::Error>(match event {
                    RendererEvent::View(view) => {
                        moosicbox_logging::debug_or_trace!(
                            ("handle_sse: SSE sending view"),
                            ("handle_sse: SSE sending view={view:?}")
                        );
                        let (body, _content_type) =
                            app.processor.to_body(Content::View(view), data).await?;

                        let body = str::from_utf8(&body).map_err(ErrorInternalServerError)?;

                        crate::sse::EventData::new(body).event("view")
                    }
                    // Note: RendererEvent::Partial was removed
                    // Partial views are now just View with fragments
                    // SSE events should send View events instead
                    RendererEvent::CanvasUpdate(canvas_update) => {
                        moosicbox_logging::debug_or_trace!(
                            ("handle_sse: SSE sending canvas_update"),
                            ("handle_sse: SSE sending canvas_update={canvas_update:?}")
                        );
                        let id = canvas_update.target.clone();
                        crate::sse::EventData::new(serde_json::to_string(&canvas_update).unwrap())
                            .id(id)
                            .event("canvas_update")
                    }
                    RendererEvent::Event { name, value } => {
                        moosicbox_logging::debug_or_trace!(
                            ("handle_sse: SSE sending event name={name}"),
                            ("handle_sse: SSE sending event name={name} value={value:?}")
                        );
                        crate::sse::EventData::new(format!("{name}:{}", value.unwrap_or_default()))
                            .event("event")
                    }
                })
            }
        })
        .map(move |x| {
            x.map(crate::sse::Event::Data)
                .map(crate::sse::Event::into_bytes)
                .inspect(|x| {
                    assert!(x.len() > 2);
                    assert!(x.ends_with(b"\n\n"));
                })
                .map(|x| match encoding {
                    ContentEncoding::Gzip => {
                        let mut encoder = GzEncoder::new(vec![], Compression::default());
                        encoder.write_all(&x).unwrap();
                        Bytes::from(encoder.finish().unwrap())
                    }
                    ContentEncoding::Zstd => {
                        let mut ecnoder = ZlibEncoder::new(vec![], Compression::default());
                        ecnoder.write_all(&x).unwrap();
                        Bytes::from(ecnoder.flush_finish().unwrap())
                    }
                    ContentEncoding::Deflate => {
                        let mut ecnoder = DeflateEncoder::new(vec![], Compression::default());
                        ecnoder.write_all(&x).unwrap();
                        Bytes::from(ecnoder.flush_finish().unwrap())
                    }
                    ContentEncoding::Identity | ContentEncoding::Brotli | _ => x,
                })
        })
        .inspect_ok(|_| log::debug!("handle_sse: sending data"))
        .inspect_err(|e| log::error!("handle_sse: error: {e:?}"));

    Ok::<_, actix_web::Error>(
        HttpResponse::Ok()
            .content_type("text/event-stream")
            .insert_header(if encoding == ContentEncoding::Zstd {
                ContentEncoding::Deflate
            } else {
                encoding
            })
            .insert_header(CacheControl(vec![CacheDirective::NoCache]))
            .streaming(stream),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_data_new() {
        let data = EventData::new("test data");
        assert_eq!(data.data, "test data");
        assert_eq!(data.event, None);
        assert_eq!(data.id, None);
    }

    #[test]
    fn test_event_data_with_id() {
        let data = EventData::new("test data").id("event-123");
        assert_eq!(data.data, "test data");
        assert_eq!(data.id, Some("event-123".to_string()));
        assert_eq!(data.event, None);
    }

    #[test]
    fn test_event_data_with_event() {
        let data = EventData::new("test data").event("custom-event");
        assert_eq!(data.data, "test data");
        assert_eq!(data.event, Some("custom-event".to_string()));
        assert_eq!(data.id, None);
    }

    #[test]
    fn test_event_data_with_id_and_event() {
        let data = EventData::new("test data").id("event-456").event("update");
        assert_eq!(data.data, "test data");
        assert_eq!(data.id, Some("event-456".to_string()));
        assert_eq!(data.event, Some("update".to_string()));
    }

    #[test]
    fn test_event_into_bytes_simple_data() {
        let event = Event::Data(EventData::new("hello"));
        let bytes = event.into_bytes();
        assert_eq!(bytes.as_ref(), b"data: hello\n\n");
    }

    #[test]
    fn test_event_into_bytes_with_id() {
        let event = Event::Data(EventData::new("test message").id("123"));
        let bytes = event.into_bytes();
        assert_eq!(bytes.as_ref(), b"id: 123\ndata: test message\n\n");
    }

    #[test]
    fn test_event_into_bytes_with_event_type() {
        let event = Event::Data(EventData::new("payload").event("custom"));
        let bytes = event.into_bytes();
        assert_eq!(bytes.as_ref(), b"event: custom\ndata: payload\n\n");
    }

    #[test]
    fn test_event_into_bytes_with_all_fields() {
        let event = Event::Data(
            EventData::new("complete message")
                .id("999")
                .event("notification"),
        );
        let bytes = event.into_bytes();
        assert_eq!(
            bytes.as_ref(),
            b"id: 999\nevent: notification\ndata: complete message\n\n"
        );
    }

    #[test]
    fn test_event_into_bytes_multiline_data() {
        let event = Event::Data(EventData::new("line1\nline2\nline3"));
        let bytes = event.into_bytes();
        assert_eq!(
            bytes.as_ref(),
            b"data: line1\ndata: line2\ndata: line3\n\n"
        );
    }

    #[test]
    fn test_event_into_bytes_multiline_with_id_and_event() {
        let event = Event::Data(
            EventData::new("first line\nsecond line")
                .id("multi-123")
                .event("multiline"),
        );
        let bytes = event.into_bytes();
        assert_eq!(
            bytes.as_ref(),
            b"id: multi-123\nevent: multiline\ndata: first line\ndata: second line\n\n"
        );
    }

    #[test]
    fn test_event_into_bytes_empty_data() {
        let event = Event::Data(EventData::new(""));
        let bytes = event.into_bytes();
        assert_eq!(bytes.as_ref(), b"data: \n\n");
    }

    #[test]
    fn test_line_split_with_prefix_single_line() {
        let mut buf = BytesMut::new();
        Event::line_split_with_prefix(&mut buf, "data: ", "single line");
        assert_eq!(buf.as_ref(), b"data: single line\n");
    }

    #[test]
    fn test_line_split_with_prefix_multiple_lines() {
        let mut buf = BytesMut::new();
        Event::line_split_with_prefix(&mut buf, "data: ", "line1\nline2\nline3");
        assert_eq!(buf.as_ref(), b"data: line1\ndata: line2\ndata: line3\n");
    }

    #[test]
    fn test_line_split_with_prefix_empty_string() {
        let mut buf = BytesMut::new();
        Event::line_split_with_prefix(&mut buf, "data: ", "");
        assert_eq!(buf.as_ref(), b"data: \n");
    }

    #[test]
    fn test_line_split_with_prefix_trailing_newline() {
        let mut buf = BytesMut::new();
        Event::line_split_with_prefix(&mut buf, "data: ", "line1\nline2\n");
        assert_eq!(buf.as_ref(), b"data: line1\ndata: line2\ndata: \n");
    }

    #[test]
    fn test_event_data_from_conversion() {
        let data = EventData::new("test");
        let event: Event = data.into();

        let Event::Data(event_data) = event;
        assert_eq!(event_data.data, "test");
    }
}
