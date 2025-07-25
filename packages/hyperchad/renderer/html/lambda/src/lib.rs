#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{io::Write, marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use flate2::{Compression, write::GzEncoder};
#[cfg(feature = "sse")]
use flume::Receiver;
#[cfg(feature = "sse")]
use hyperchad_renderer::RendererEvent;
use hyperchad_renderer::{Handle, RenderRunner, ToRenderRunner};
use lambda_http::{
    Request, Response,
    http::header::{CONTENT_ENCODING, CONTENT_TYPE},
    service_fn,
};
use lambda_runtime::streaming::channel;

pub use lambda_http;
pub use lambda_runtime;

pub enum Content {
    Html(String),
    Raw {
        data: Bytes,
        content_type: String,
    },
    #[cfg(feature = "json")]
    Json(serde_json::Value),
}

#[async_trait]
pub trait LambdaResponseProcessor<T: Send + Sync + Clone> {
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(
        &self,
        req: Request,
        body: Option<Arc<Bytes>>,
    ) -> Result<T, lambda_runtime::Error>;

    fn headers(&self, content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>>;

    async fn to_response(
        &self,
        data: T,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error>;

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        data: T,
    ) -> Result<Content, lambda_runtime::Error>;
}

#[derive(Clone)]
pub struct LambdaApp<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> {
    pub processor: R,
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    #[cfg(feature = "sse")]
    pub renderer_event_rx: Option<Receiver<RendererEvent>>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> LambdaApp<T, R> {
    pub const fn new(to_html: R) -> Self {
        Self {
            processor: to_html,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            #[cfg(feature = "sse")]
            renderer_event_rx: None,
            _phantom: PhantomData,
        }
    }

    #[cfg(feature = "sse")]
    #[must_use]
    pub fn with_renderer_event_rx(mut self, rx: Receiver<RendererEvent>) -> Self {
        self.renderer_event_rx = Some(rx);
        self
    }
}

impl<
    T: Send + Sync + Clone + 'static,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone + 'static,
> ToRenderRunner for LambdaApp<T, R>
{
    fn to_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(LambdaAppRunner { app: self, handle }))
    }
}

pub struct LambdaAppRunner<
    T: Send + Sync + Clone,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone,
> {
    pub app: LambdaApp<T, R>,
    pub handle: Handle,
}

impl<
    T: Send + Sync + Clone + 'static,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone + 'static,
> RenderRunner for LambdaAppRunner<T, R>
{
    /// # Errors
    ///
    /// Will error if html fails to run the event loop.
    #[allow(clippy::too_many_lines)]
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        log::debug!("run: starting");

        let app = self.app.clone();

        // Always use regular HTTP runtime, but intercept SSE requests early
        let func = service_fn(move |event: Request| {
            let app = app.clone();
            async move {
                // CRITICAL: Check if this is an SSE request BEFORE calling any processor methods
                // This prevents the hyperchad router from ever seeing the /$sse path
                #[cfg(feature = "sse")]
                if event.uri().path() == "/$sse" {
                    if let Some(rx) = app.renderer_event_rx {
                        log::debug!("SSE request intercepted BEFORE hyperchad router processing");

                        // Create lambda streaming body channel for streaming SSE data
                        let (mut tx, body) = channel();

                        // Spawn task to convert RendererEvent stream to SSE format
                        switchy::unsync::task::spawn(async move {
                            use futures_util::StreamExt;
                            let mut stream = rx.into_stream();

                            // Send initial connection message
                            if let Err(e) = tx
                                .send_data("data: SSE connection established\n\n".into())
                                .await
                            {
                                log::error!("Failed to send SSE initial message: {e}");
                                return;
                            }

                            while let Some(event) = stream.next().await {
                                log::debug!("SSE: received renderer event");

                                let sse_data = match event {
                                    RendererEvent::View(view) => {
                                        let body = format!("{view:?}");
                                        format!("event: view\ndata: {body}\n\n")
                                    }
                                    RendererEvent::Partial(partial_view) => {
                                        let id = partial_view.target.to_string();
                                        let body = format!("{partial_view:?}");
                                        format!("id: {id}\nevent: partial_view\ndata: {body}\n\n")
                                    }
                                    RendererEvent::CanvasUpdate(canvas_update) => {
                                        let id = canvas_update.target.clone();
                                        let body = format!("{canvas_update:?}");
                                        format!("id: {id}\nevent: canvas_update\ndata: {body}\n\n")
                                    }
                                    RendererEvent::Event { name, value } => {
                                        let data = format!("{name}:{}", value.unwrap_or_default());
                                        format!("event: event\ndata: {data}\n\n")
                                    }
                                };

                                if let Err(e) = tx.send_data(sse_data.into()).await {
                                    log::error!("Failed to send SSE data: {e}");
                                    break;
                                }
                            }

                            log::debug!("SSE stream ended");
                        });

                        return Ok(Response::builder()
                            .status(200)
                            .header("Content-Type", "text/event-stream")
                            .header("Cache-Control", "no-cache")
                            .header("Connection", "keep-alive")
                            .header("Access-Control-Allow-Origin", "*")
                            .body(body)
                            .map_err(Box::new)?);
                    }

                    moosicbox_assert::die_or_panic!(
                        "SSE request received but no renderer_event_rx configured"
                    );
                }

                // Only process non-SSE requests through hyperchad router
                // This ensures /$sse never reaches the hyperchad router
                log::debug!(
                    "Processing regular request through hyperchad router: {}",
                    event.uri().path()
                );

                let body: &[u8] = event.body().as_ref();
                let body = Bytes::copy_from_slice(body);
                let body = if body.is_empty() {
                    None
                } else {
                    Some(Arc::new(body))
                };

                // These processor methods will only be called for non-SSE requests
                let data = app.processor.prepare_request(event, body)?;
                let content = app.processor.to_response(data).await?;

                let mut response = Response::builder()
                    .status(200)
                    .header(CONTENT_ENCODING, "gzip");

                let mut gz = GzEncoder::new(vec![], Compression::default());

                if let Some((content, headers)) = content {
                    if let Some(headers) = headers {
                        for (key, value) in headers {
                            response = response.header(key, value);
                        }
                    }
                    match content {
                        Content::Html(x) => {
                            gz.write_all(x.as_bytes())?;
                            response = response.header(CONTENT_TYPE, "text/html");
                        }
                        Content::Raw { data, content_type } => {
                            gz.write_all(&data)?;
                            response = response.header(CONTENT_TYPE, content_type);
                        }
                        #[cfg(feature = "json")]
                        Content::Json(x) => {
                            gz.write_all(serde_json::to_string(&x)?.as_bytes())?;
                            response = response.header(CONTENT_TYPE, "application/json");
                        }
                    }
                }

                let gzip = gz.finish()?;

                // For regular HTTP responses, use chunked streaming approach
                let (mut tx, body) = channel();

                log::debug!(
                    "Sending regular HTTP response data in chunks, total length: {}",
                    gzip.len()
                );

                // Send data in chunks to create proper streaming behavior
                switchy::unsync::task::spawn(async move {
                    let chunk_size = 8192; // 8KB chunks
                    let mut offset = 0;

                    while offset < gzip.len() {
                        let end = std::cmp::min(offset + chunk_size, gzip.len());
                        let chunk = &gzip[offset..end];

                        if let Err(e) = tx.send_data(chunk.to_vec().into()).await {
                            log::error!("Failed to send chunk at offset {offset}: {e}");
                            break;
                        }

                        log::debug!("Sent chunk {offset}-{end} ({} bytes)", chunk.len());
                        offset = end;
                    }

                    log::debug!("Finished sending all chunks, stream will close naturally");
                    // tx drops here naturally, signaling end of stream
                });

                let response = response.body(body).map_err(Box::new)?;

                Ok::<_, lambda_runtime::Error>(response)
            }
        });

        self.handle
            .block_on(async move { lambda_http::run_with_streaming_response(func).await })
            .map_err(|e| e as Box<dyn std::error::Error + Send>)?;

        log::debug!("run: finished");

        Ok(())
    }
}
