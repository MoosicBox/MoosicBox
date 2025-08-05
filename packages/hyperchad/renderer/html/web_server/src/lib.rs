#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use flume::Receiver;
use hyperchad_renderer::{Content, Handle, RenderRunner, RendererEvent, ToRenderRunner};
use moosicbox_env_utils::{default_env, default_env_u16};

pub use moosicbox_web_server;

use moosicbox_web_server::Scope;
// Re-export types for compatibility
pub use moosicbox_web_server::{Error as WebServerError, HttpRequest, HttpResponse};

#[async_trait]
pub trait WebServerResponseProcessor<T: Send + Sync + Clone> {
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<Arc<Bytes>>,
    ) -> Result<T, WebServerError>;

    async fn to_response(&self, data: T) -> Result<HttpResponse, WebServerError>;

    async fn to_body(&self, content: Content, data: T) -> Result<(Bytes, String), WebServerError>;
}

#[derive(Clone)]
pub struct WebServerApp<
    T: Send + Sync + Clone,
    R: WebServerResponseProcessor<T> + Send + Sync + Clone,
> {
    pub processor: R,
    pub renderer_event_rx: Receiver<RendererEvent>,
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: WebServerResponseProcessor<T> + Send + Sync + Clone>
    WebServerApp<T, R>
{
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

#[derive(Clone)]
pub struct WebServerAppRunner<
    T: Send + Sync + Clone,
    R: WebServerResponseProcessor<T> + Send + Sync + Clone,
> {
    pub app: WebServerApp<T, R>,
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
            let addr = default_env("BIND_ADDR", "0.0.0.0");
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
                .with_scope(Scope::new("").with_route(GET_EXAMPLE))
                .build();

            log::debug!("Starting web server");
            server.start().await;
            log::debug!("Web server finished");
        });

        Ok(())
    }
}

moosicbox_web_server::route!(GET, example, "/example", |req| {
    Box::pin(async move {
        Ok(HttpResponse::ok().with_body(format!(
            "hello, world! path={} query={}",
            req.path(),
            req.query_string()
        )))
    })
});
