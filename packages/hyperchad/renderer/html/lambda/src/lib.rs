#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{io::Write, marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use flate2::{Compression, write::GzEncoder};
use hyperchad_renderer::{Handle, RenderRunner, ToRenderRunner};
use lambda_http::{
    Request, Response,
    http::header::{CONTENT_ENCODING, CONTENT_TYPE},
    service_fn,
};

pub use lambda_http;
pub use lambda_runtime;

/// HTTP response content types for Lambda responses.
pub enum Content {
    /// HTML content with UTF-8 encoding.
    Html(String),
    /// Raw binary content with a custom content type.
    Raw {
        /// The binary data to send.
        data: Bytes,
        /// The MIME type for the content.
        content_type: String,
    },
    /// JSON content (requires `json` feature).
    #[cfg(feature = "json")]
    Json(serde_json::Value),
}

/// Processes Lambda HTTP requests and generates responses.
#[async_trait]
pub trait LambdaResponseProcessor<T: Send + Sync + Clone> {
    /// Prepares request data for processing.
    ///
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(
        &self,
        req: Request,
        body: Option<Arc<Bytes>>,
    ) -> Result<T, lambda_runtime::Error>;

    /// Returns additional HTTP headers for the response based on content.
    fn headers(&self, content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>>;

    /// Generates the response content and headers from processed data.
    ///
    /// # Errors
    ///
    /// * If response generation fails
    async fn to_response(
        &self,
        data: T,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error>;

    /// Converts rendered content to the appropriate response body type.
    ///
    /// # Errors
    ///
    /// * If content conversion fails
    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        data: T,
    ) -> Result<Content, lambda_runtime::Error>;
}

/// Lambda application with configurable response processing.
#[derive(Clone)]
pub struct LambdaApp<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> {
    /// The response processor for handling requests.
    pub processor: R,
    /// Static asset routes (requires `assets` feature).
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> LambdaApp<T, R> {
    /// Creates a new Lambda application with the given response processor.
    #[must_use]
    pub const fn new(to_html: R) -> Self {
        Self {
            processor: to_html,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            _phantom: PhantomData,
        }
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

/// Runtime handler for executing the Lambda application.
pub struct LambdaAppRunner<
    T: Send + Sync + Clone,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone,
> {
    /// The Lambda application configuration.
    pub app: LambdaApp<T, R>,
    /// Runtime handle for async execution.
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
        let func = service_fn(move |event: Request| {
            let app = app.clone();
            async move {
                let body: &[u8] = event.body().as_ref();
                let body = Bytes::copy_from_slice(body);
                let body = if body.is_empty() {
                    None
                } else {
                    Some(Arc::new(body))
                };
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
                            log::debug!("run: sending HTML response type");
                            gz.write_all(x.as_bytes())?;
                            response = response.header(CONTENT_TYPE, "text/html; charset=utf-8");
                        }
                        Content::Raw { data, content_type } => {
                            log::debug!("run: sending raw response type '{content_type}'");
                            gz.write_all(&data)?;
                            response = response.header(CONTENT_TYPE, content_type);
                        }
                        #[cfg(feature = "json")]
                        Content::Json(x) => {
                            log::debug!("run: sending JSON response type");
                            gz.write_all(serde_json::to_string(&x)?.as_bytes())?;
                            response = response.header(CONTENT_TYPE, "application/json");
                        }
                    }
                }

                let gzip = gz.finish()?;

                let response = response
                    .body(lambda_http::Body::Binary(gzip))
                    .map_err(Box::new)?;

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
