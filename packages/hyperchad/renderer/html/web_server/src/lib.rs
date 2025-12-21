//! Web server renderer for `HyperChad`.
//!
//! This crate provides a web server backend for rendering `HyperChad` HTML content.
//! It integrates the `HyperChad` renderer with `switchy_web_server` to enable
//! server-side rendering of hyperchad views over HTTP.
//!
//! # Features
//!
//! * `assets` - Enable static asset serving (enabled by default)
//! * `actix` - Use the Actix-web backend
//! * `simulator` - Enable simulator mode for testing
//! * `debug` - Enable debug logging (enabled by default)
//!
//! # Example
//!
//! ```rust,no_run
//! use hyperchad_renderer_html_web_server::{
//!     WebServerApp, WebServerResponseProcessor, HttpRequest, HttpResponse, WebServerError
//! };
//! use hyperchad_renderer::{RendererEvent, Content};
//! use async_trait::async_trait;
//! use bytes::Bytes;
//! use std::sync::Arc;
//!
//! // Define your request data type
//! #[derive(Clone)]
//! struct MyRequestData {
//!     path: String,
//! }
//!
//! // Implement the response processor
//! #[derive(Clone)]
//! struct MyProcessor;
//!
//! #[async_trait]
//! impl WebServerResponseProcessor<MyRequestData> for MyProcessor {
//!     fn prepare_request(
//!         &self,
//!         req: HttpRequest,
//!         _body: Option<Arc<Bytes>>,
//!     ) -> Result<MyRequestData, WebServerError> {
//!         Ok(MyRequestData {
//!             path: req.path().to_string(),
//!         })
//!     }
//!
//!     async fn to_response(&self, data: MyRequestData) -> Result<HttpResponse, WebServerError> {
//!         Ok(HttpResponse::ok().with_body(format!("Path: {}", data.path)))
//!     }
//!
//!     async fn to_body(&self, content: Content, _data: MyRequestData) -> Result<(Bytes, String), WebServerError> {
//!         // Convert content to bytes and content type
//!         Ok((Bytes::from("example"), "text/html".to_string()))
//!     }
//! }
//!
//! # fn main() {
//! // Create the web server app
//! let (tx, rx) = flume::unbounded::<RendererEvent>();
//! let app = WebServerApp::new(MyProcessor, rx);
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use flume::Receiver;
use hyperchad_renderer::{Content, Handle, RenderRunner, RendererEvent, ToRenderRunner};
use moosicbox_env_utils::default_env_u16;
use switchy_env::var_or;

/// Re-export of the `switchy_web_server` crate.
///
/// Provides direct access to the underlying web server implementation
/// for advanced configuration and customization.
pub use switchy_web_server;

use switchy_web_server::Scope;

/// Web server error type.
///
/// Represents errors that can occur during web server operations.
pub use switchy_web_server::Error as WebServerError;

/// HTTP request type.
///
/// Represents an incoming HTTP request to the web server.
pub use switchy_web_server::HttpRequest;

/// HTTP response type.
///
/// Represents an HTTP response to be sent to the client.
pub use switchy_web_server::HttpResponse;

/// Trait for processing web server requests and responses.
///
/// Implementors define how HTTP requests are transformed into application data,
/// and how that data is rendered into HTTP responses.
#[async_trait]
pub trait WebServerResponseProcessor<T: Send + Sync + Clone> {
    /// Prepares an HTTP request by extracting and transforming it into application data.
    ///
    /// This method is called by the web server for each incoming request to transform
    /// the raw HTTP request and body into application-specific data.
    ///
    /// # Errors
    ///
    /// * Returns `WebServerError` if the request data cannot be parsed or validated
    /// * Returns `WebServerError` if required headers or parameters are missing
    /// * Returns `WebServerError` if the request body is malformed or cannot be deserialized
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<Arc<Bytes>>,
    ) -> Result<T, WebServerError>;

    /// Converts application data into an HTTP response.
    ///
    /// This method is responsible for transforming the prepared application data
    /// into a complete HTTP response with status code, headers, and body.
    ///
    /// # Errors
    ///
    /// * Returns `WebServerError` if the response cannot be serialized
    /// * Returns `WebServerError` if required response headers cannot be set
    /// * Returns `WebServerError` if the application data is in an invalid state
    async fn to_response(&self, data: T) -> Result<HttpResponse, WebServerError>;

    /// Converts rendered content and application data into response body bytes and content type.
    ///
    /// This method transforms the rendered hyperchad content into raw bytes suitable
    /// for sending over HTTP, along with the appropriate content type header value.
    ///
    /// # Errors
    ///
    /// * Returns `WebServerError` if the content cannot be serialized to bytes
    /// * Returns `WebServerError` if the content type cannot be determined
    /// * Returns `WebServerError` if the content encoding fails
    async fn to_body(&self, content: Content, data: T) -> Result<(Bytes, String), WebServerError>;
}

/// Web server application configuration for hyperchad rendering.
///
/// Combines a response processor with renderer events to enable
/// server-side rendering of hyperchad content.
#[derive(Clone)]
pub struct WebServerApp<
    T: Send + Sync + Clone,
    R: WebServerResponseProcessor<T> + Send + Sync + Clone,
> {
    /// The response processor for handling requests and responses.
    pub processor: R,
    /// Channel receiver for renderer events.
    pub renderer_event_rx: Receiver<RendererEvent>,
    /// Static asset routes for serving assets (when "assets" feature is enabled).
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    /// Default behavior when a requested asset file is not found (when "assets" feature is enabled).
    #[cfg(feature = "assets")]
    pub asset_not_found_behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: WebServerResponseProcessor<T> + Send + Sync + Clone>
    WebServerApp<T, R>
{
    /// Creates a new web server application.
    ///
    /// # Parameters
    ///
    /// * `processor` - The response processor that handles HTTP request/response transformations
    /// * `renderer_event_rx` - Channel receiver for receiving renderer lifecycle events
    #[must_use]
    pub const fn new(processor: R, renderer_event_rx: Receiver<RendererEvent>) -> Self {
        Self {
            processor,
            renderer_event_rx,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            #[cfg(feature = "assets")]
            asset_not_found_behavior: hyperchad_renderer::assets::AssetNotFoundBehavior::NotFound,
            _phantom: PhantomData,
        }
    }
}

impl<
    T: Send + Sync + Clone + 'static,
    R: WebServerResponseProcessor<T> + Send + Sync + Clone + 'static,
> ToRenderRunner for WebServerApp<T, R>
{
    /// Converts the web server application into a render runner.
    ///
    /// This method wraps the web server application in a runner that can be
    /// executed by the hyperchad renderer runtime.
    ///
    /// # Errors
    ///
    /// This method is infallible and always returns `Ok`.
    fn to_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(WebServerAppRunner { app: self, handle }))
    }
}

/// Runtime wrapper for a web server application.
///
/// Manages the lifecycle of the web server and handles rendering events.
#[derive(Clone)]
pub struct WebServerAppRunner<
    T: Send + Sync + Clone,
    R: WebServerResponseProcessor<T> + Send + Sync + Clone,
> {
    /// The web server application configuration.
    pub app: WebServerApp<T, R>,
    /// Handle for async runtime operations.
    pub handle: Handle,
}

impl<
    T: Send + Sync + Clone + 'static,
    R: WebServerResponseProcessor<T> + Send + Sync + Clone + 'static,
> RenderRunner for WebServerAppRunner<T, R>
{
    /// Starts the web server and runs the event loop.
    ///
    /// Binds to the address and port specified by the `BIND_ADDR` and `PORT`
    /// environment variables (defaults to `0.0.0.0:8343`), configures CORS
    /// to allow any origin/method/header, and starts the web server to handle
    /// incoming HTTP requests.
    ///
    /// # Errors
    ///
    /// This method is infallible and always returns `Ok`. Server startup errors
    /// are logged but do not cause this method to return an error.
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        log::debug!("run: starting web server backend");

        let _html_app = self.app.clone();
        self.handle.block_on(async move {
            let addr = var_or("BIND_ADDR", "0.0.0.0");
            let port = default_env_u16!("PORT", 8343);

            let cors = switchy_web_server::cors::Cors::default()
                .allow_any_origin()
                .allow_any_method()
                .allow_any_header()
                .expose_any_header();

            let server = switchy_web_server::WebServerBuilder::new()
                .with_addr(addr)
                .with_port(port)
                .with_cors(cors)
                .with_scope(Scope::new("").get("/example", |req| {
                    let path = req.path().to_string();
                    let query = req.query_string().to_string();
                    Box::pin(async move {
                        Ok(HttpResponse::ok()
                            .with_body(format!("hello, world! path={path} query={query}")))
                    })
                }))
                .build();

            log::debug!("Starting web server");
            server.start().await;
            log::debug!("Web server finished");
        });

        Ok(())
    }
}
