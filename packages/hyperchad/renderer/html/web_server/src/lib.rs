//! Web server renderer for `HyperChad`.
//!
//! This crate provides a web server backend for rendering `HyperChad` HTML content.
//! It integrates the `HyperChad` renderer with `moosicbox_web_server` to enable
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

pub use moosicbox_web_server;

use moosicbox_web_server::Scope;
// Re-export types for compatibility
pub use moosicbox_web_server::{Error as WebServerError, HttpRequest, HttpResponse};

/// Trait for processing web server requests and responses.
///
/// Implementors define how HTTP requests are transformed into application data,
/// and how that data is rendered into HTTP responses.
#[async_trait]
pub trait WebServerResponseProcessor<T: Send + Sync + Clone> {
    /// Prepares an HTTP request by extracting and transforming it into application data.
    ///
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<Arc<Bytes>>,
    ) -> Result<T, WebServerError>;

    /// Converts application data into an HTTP response.
    ///
    /// # Errors
    ///
    /// * If the response fails to be created
    async fn to_response(&self, data: T) -> Result<HttpResponse, WebServerError>;

    /// Converts rendered content and application data into response body bytes and content type.
    ///
    /// # Errors
    ///
    /// * If the body fails to be created
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
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: WebServerResponseProcessor<T> + Send + Sync + Clone>
    WebServerApp<T, R>
{
    /// Creates a new web server application.
    #[must_use]
    pub const fn new(processor: R, renderer_event_rx: Receiver<RendererEvent>) -> Self {
        Self {
            processor,
            renderer_event_rx,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            _phantom: PhantomData,
        }
    }
}

impl<
    T: Send + Sync + Clone + 'static,
    R: WebServerResponseProcessor<T> + Send + Sync + Clone + 'static,
> ToRenderRunner for WebServerApp<T, R>
{
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
    /// # Errors
    ///
    /// Will error if web server fails to run the event loop.
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        log::debug!("run: starting web server backend");

        let _html_app = self.app.clone();
        self.handle.block_on(async move {
            let addr = var_or("BIND_ADDR", "0.0.0.0");
            let port = default_env_u16!("PORT", 8343);

            let cors = moosicbox_web_server::cors::Cors::default()
                .allow_any_origin()
                .allow_any_method()
                .allow_any_header()
                .expose_any_header();

            let server = moosicbox_web_server::WebServerBuilder::new()
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
