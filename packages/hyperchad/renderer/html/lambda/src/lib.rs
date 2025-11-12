//! AWS Lambda renderer implementation for `HyperChad` HTML applications.
//!
//! This crate provides a Lambda-based runtime for `HyperChad` HTML renderers,
//! enabling serverless deployment of `HyperChad` applications on AWS Lambda.
//! It handles HTTP request/response processing, gzip compression, and
//! integrates with the `HyperChad` renderer framework.
//!
//! # Features
//!
//! * `assets` - Enable static asset route support (enabled by default)
//! * `json` - Enable JSON response content type (enabled by default)
//! * `debug` - Enable debug logging (enabled by default)
//!
//! # Example
//!
//! ```rust,no_run
//! use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor, Content};
//! use hyperchad_renderer::ToRenderRunner;
//! use async_trait::async_trait;
//! use std::sync::Arc;
//! use bytes::Bytes;
//! # use lambda_http::Request;
//!
//! #[derive(Clone)]
//! struct MyProcessor;
//!
//! #[async_trait]
//! impl LambdaResponseProcessor<String> for MyProcessor {
//!     fn prepare_request(
//!         &self,
//!         req: Request,
//!         body: Option<Arc<Bytes>>,
//!     ) -> Result<String, lambda_runtime::Error> {
//!         Ok(req.uri().path().to_string())
//!     }
//!
//!     fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
//!         None
//!     }
//!
//!     async fn to_response(
//!         &self,
//!         data: String,
//!     ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
//!         Ok(Some((Content::Html(format!("<h1>Path: {}</h1>", data)), None)))
//!     }
//!
//!     async fn to_body(
//!         &self,
//!         _content: hyperchad_renderer::Content,
//!         _data: String,
//!     ) -> Result<Content, lambda_runtime::Error> {
//!         Ok(Content::Html("<h1>Hello</h1>".to_string()))
//!     }
//! }
//! ```

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

/// Re-exported [`lambda_http`] crate for request/response types.
///
/// Provides access to HTTP request/response types, the `Request` and `Response`
/// builders, and other HTTP-related utilities needed when implementing
/// [`LambdaResponseProcessor`].
///
/// [`lambda_http`]: https://docs.rs/lambda_http
pub use lambda_http;

/// Re-exported [`lambda_runtime`] crate for Lambda runtime types.
///
/// Provides access to the `Error` type used throughout this crate's API,
/// as well as other Lambda runtime utilities.
///
/// [`lambda_runtime`]: https://docs.rs/lambda_runtime
pub use lambda_runtime;

/// HTTP response content types for Lambda responses.
///
/// Represents the different types of content that can be returned from a Lambda
/// function, each with appropriate MIME type handling.
pub enum Content {
    /// HTML content with UTF-8 encoding.
    ///
    /// The content will be sent with `Content-Type: text/html; charset=utf-8`.
    Html(String),
    /// Raw binary content with a custom content type.
    ///
    /// Use this variant for serving any binary data (images, PDFs, etc.) or
    /// non-HTML text formats with a specific MIME type.
    Raw {
        /// The binary data to send.
        data: Bytes,
        /// The MIME type for the content.
        content_type: String,
    },
    /// JSON content (requires `json` feature).
    ///
    /// The content will be sent with `Content-Type: application/json`.
    /// Automatically serializes the value to JSON string format.
    #[cfg(feature = "json")]
    Json(serde_json::Value),
}

/// Processes Lambda HTTP requests and generates responses.
///
/// This trait defines the interface for handling Lambda HTTP events, allowing
/// custom request processing, response generation, and content transformation.
/// Implementors control how requests are parsed, what content is generated,
/// and how it's formatted for the HTTP response.
#[async_trait]
pub trait LambdaResponseProcessor<T: Send + Sync + Clone> {
    /// Prepares request data for processing.
    ///
    /// Extracts and transforms the incoming Lambda HTTP request and optional
    /// body into the application's request type `T`.
    ///
    /// # Errors
    ///
    /// Implementations may return errors for:
    /// * Invalid request format or missing required data
    /// * Request parsing or validation failures
    /// * Authentication or authorization failures
    fn prepare_request(
        &self,
        req: Request,
        body: Option<Arc<Bytes>>,
    ) -> Result<T, lambda_runtime::Error>;

    /// Returns additional HTTP headers for the response based on content.
    ///
    /// Allows adding custom headers like `Cache-Control`, `ETag`, or
    /// `Content-Security-Policy` based on the rendered content.
    fn headers(&self, content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>>;

    /// Generates the response content and headers from processed data.
    ///
    /// Produces the final response content and optional headers from the
    /// prepared request data. Returns `None` to indicate no response should
    /// be sent (for handling by other middleware or routes).
    ///
    /// # Errors
    ///
    /// Implementations may return errors for:
    /// * Data fetching or database query failures
    /// * Business logic validation errors
    /// * Template rendering failures
    async fn to_response(
        &self,
        data: T,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error>;

    /// Converts rendered content to the appropriate response body type.
    ///
    /// Transforms `hyperchad_renderer::Content` into the Lambda response
    /// `Content` format, allowing customization of how rendered content
    /// is serialized for HTTP responses.
    ///
    /// # Errors
    ///
    /// Implementations may return errors for:
    /// * Content serialization or encoding failures
    /// * Resource loading failures when building the response
    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        data: T,
    ) -> Result<Content, lambda_runtime::Error>;
}

/// Lambda application with configurable response processing.
///
/// The main entry point for creating a Lambda-based `HyperChad` application.
/// Combines a custom `LambdaResponseProcessor` with optional static asset
/// routing to handle HTTP requests in AWS Lambda environment.
#[derive(Clone)]
pub struct LambdaApp<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> {
    /// The response processor for handling requests.
    pub processor: R,
    /// Static asset routes (requires `assets` feature).
    ///
    /// Defines routes that serve embedded static files like CSS, JavaScript,
    /// or images. Only available when the `assets` feature is enabled.
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: LambdaResponseProcessor<T> + Send + Sync + Clone> LambdaApp<T, R> {
    /// Creates a new Lambda application with the given response processor.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor};
    /// # use async_trait::async_trait;
    /// # #[derive(Clone)]
    /// # struct MyProcessor;
    /// # #[async_trait]
    /// # impl LambdaResponseProcessor<String> for MyProcessor {
    /// #     fn prepare_request(&self, req: lambda_http::Request, body: Option<std::sync::Arc<bytes::Bytes>>) -> Result<String, lambda_runtime::Error> { Ok(String::new()) }
    /// #     fn headers(&self, content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> { None }
    /// #     async fn to_response(&self, data: String) -> Result<Option<(hyperchad_renderer_html_lambda::Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> { Ok(None) }
    /// #     async fn to_body(&self, content: hyperchad_renderer::Content, data: String) -> Result<hyperchad_renderer_html_lambda::Content, lambda_runtime::Error> { Ok(hyperchad_renderer_html_lambda::Content::Html(String::new())) }
    /// # }
    /// let processor = MyProcessor;
    /// let app = LambdaApp::new(processor);
    /// ```
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
///
/// Wraps a `LambdaApp` with a runtime handle to execute the Lambda event loop.
/// This type is created automatically when converting a `LambdaApp` to a
/// `RenderRunner` via the `ToRenderRunner` trait.
pub struct LambdaAppRunner<
    T: Send + Sync + Clone,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone,
> {
    /// The Lambda application configuration.
    pub app: LambdaApp<T, R>,
    /// Runtime handle for async execution.
    ///
    /// Provides the async runtime context for executing Lambda handlers.
    pub handle: Handle,
}

impl<
    T: Send + Sync + Clone + 'static,
    R: LambdaResponseProcessor<T> + Send + Sync + Clone + 'static,
> RenderRunner for LambdaAppRunner<T, R>
{
    /// Runs the Lambda runtime event loop to handle incoming HTTP requests.
    ///
    /// # Errors
    ///
    /// * If the Lambda runtime fails to start or process events
    /// * If request preparation fails via `prepare_request`
    /// * If response generation fails via `to_response`
    /// * If gzip compression fails during encoding
    /// * If JSON serialization fails (when using `json` feature)
    /// * If response building fails
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
