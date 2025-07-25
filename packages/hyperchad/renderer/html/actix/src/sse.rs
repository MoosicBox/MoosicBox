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

#[must_use]
#[derive(Debug, Clone)]
pub struct EventData {
    event: Option<String>,
    id: Option<String>,
    data: String,
}

impl EventData {
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event: None,
            id: None,
            data: data.into(),
        }
    }

    /// Sets `id` name field, returning a new data message.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets `event` name field, returning a new data message.
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
                    RendererEvent::Partial(partial_view) => {
                        moosicbox_logging::debug_or_trace!(
                            (
                                "handle_sse: SSE sending partial_view target={}",
                                partial_view.target
                            ),
                            ("handle_sse: SSE sending partial_view={partial_view:?}")
                        );
                        let id = partial_view.target.to_string();
                        let (body, _content_type) = app
                            .processor
                            .to_body(Content::PartialView(partial_view), data)
                            .await?;

                        let body = str::from_utf8(&body).map_err(ErrorInternalServerError)?;

                        crate::sse::EventData::new(body)
                            .id(id)
                            .event("partial_view")
                    }
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
