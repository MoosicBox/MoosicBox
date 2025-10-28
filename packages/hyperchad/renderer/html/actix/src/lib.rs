//! Actix web server renderer for `HyperChad` HTML applications.
//!
//! This crate provides an Actix Web integration for the `HyperChad` renderer framework,
//! enabling server-side rendering of `HyperChad` applications with support for:
//!
//! * Server-sent events (SSE) for real-time updates (with `sse` feature)
//! * Action handling for interactive user events (with `actions` feature)
//! * Static asset serving (with `assets` feature)
//! * Custom response processing through the [`ActixResponseProcessor`] trait
//!
//! # Example
//!
//! ```rust,no_run
//! # use hyperchad_renderer_html_actix::{ActixApp, ActixResponseProcessor};
//! # use hyperchad_renderer::{RendererEvent, Content};
//! # use actix_web::{HttpRequest, HttpResponse};
//! # use bytes::Bytes;
//! # use std::sync::Arc;
//! # use async_trait::async_trait;
//! #
//! # #[derive(Clone)]
//! # struct MyProcessor;
//! #
//! # #[async_trait]
//! # impl ActixResponseProcessor<()> for MyProcessor {
//! #     fn prepare_request(&self, _req: HttpRequest, _body: Option<Arc<Bytes>>) -> Result<(), actix_web::Error> {
//! #         Ok(())
//! #     }
//! #     async fn to_response(&self, _data: ()) -> Result<HttpResponse, actix_web::Error> {
//! #         Ok(HttpResponse::Ok().finish())
//! #     }
//! #     async fn to_body(&self, _content: Content, _data: ()) -> Result<(Bytes, String), actix_web::Error> {
//! #         Ok((Bytes::new(), "text/html".to_string()))
//! #     }
//! # }
//! #
//! # fn main() {
//! let (tx, rx) = flume::unbounded::<RendererEvent>();
//! let processor = MyProcessor;
//! let app = ActixApp::new(processor, rx);
//! // Use app.to_runner() to create a RenderRunner
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{marker::PhantomData, sync::Arc};

use actix_cors::Cors;
pub use actix_web::http::header::HeaderMap;
use actix_web::{
    App, HttpRequest, HttpResponse,
    http::{self},
    middleware,
    web::{self, Data},
};
use async_trait::async_trait;
use bytes::Bytes;
use flume::Receiver;
use hyperchad_renderer::{Content, Handle, RenderRunner, RendererEvent, ToRenderRunner};
use moosicbox_env_utils::default_env_u16;

pub use actix_web;

#[cfg(feature = "actions")]
mod actions;

#[cfg(feature = "sse")]
mod sse;

/// Processes Actix HTTP requests and converts content to responses.
#[async_trait]
pub trait ActixResponseProcessor<T: Send + Sync + Clone> {
    /// Prepares request data from the HTTP request and body.
    ///
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<Arc<Bytes>>,
    ) -> Result<T, actix_web::Error>;

    /// Converts prepared data into an HTTP response.
    ///
    /// # Errors
    ///
    /// * If the response fails to construct
    async fn to_response(&self, data: T) -> Result<HttpResponse, actix_web::Error>;

    /// Converts content and prepared data into response body bytes and content type.
    ///
    /// # Errors
    ///
    /// * If content conversion fails
    async fn to_body(&self, content: Content, data: T)
    -> Result<(Bytes, String), actix_web::Error>;
}

/// Actix web application for hyperchad rendering with configurable response processing.
#[derive(Clone)]
pub struct ActixApp<T: Send + Sync + Clone, R: ActixResponseProcessor<T> + Send + Sync + Clone> {
    /// The response processor that handles HTTP request/response conversion.
    pub processor: R,
    /// Receiver channel for renderer events from the hyperchad application.
    pub renderer_event_rx: Receiver<RendererEvent>,
    /// Optional sender channel for user-triggered actions (requires `actions` feature).
    #[cfg(feature = "actions")]
    pub action_tx: Option<
        flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    >,
    /// Static asset routes for serving files and directories (requires `assets` feature).
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: ActixResponseProcessor<T> + Send + Sync + Clone> ActixApp<T, R> {
    /// Creates a new Actix application with the given processor and event receiver.
    #[must_use]
    pub const fn new(processor: R, renderer_event_rx: Receiver<RendererEvent>) -> Self {
        Self {
            processor,
            renderer_event_rx,
            #[cfg(feature = "actions")]
            action_tx: None,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            _phantom: PhantomData,
        }
    }

    /// Sets the action transmitter channel and returns the modified app.
    #[cfg(feature = "actions")]
    #[must_use]
    pub fn with_action_tx(
        mut self,
        tx: flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    ) -> Self {
        self.action_tx = Some(tx);
        self
    }

    /// Sets the action transmitter channel in place.
    #[cfg(feature = "actions")]
    pub fn set_action_tx(
        &mut self,
        tx: flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    ) {
        self.action_tx = Some(tx);
    }
}

impl<T: Send + Sync + Clone + 'static, R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static>
    ToRenderRunner for ActixApp<T, R>
{
    fn to_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(ActixAppRunner { app: self, handle }))
    }
}

/// Runner for executing the Actix application with a render handle.
#[derive(Clone)]
pub struct ActixAppRunner<
    T: Send + Sync + Clone,
    R: ActixResponseProcessor<T> + Send + Sync + Clone,
> {
    /// The Actix application configuration and state.
    pub app: ActixApp<T, R>,
    /// The async runtime handle for executing the server.
    pub handle: Handle,
}

impl<T: Send + Sync + Clone + 'static, R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static>
    RenderRunner for ActixAppRunner<T, R>
{
    /// # Errors
    ///
    /// Will error if html fails to run the event loop.
    #[allow(clippy::too_many_lines)]
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        log::debug!("run: starting");

        let html_app = self.app.clone();

        self.handle.block_on(async move {
            let app = move || {
                let cors = Cors::default()
                    .allow_any_origin()
                    .allowed_methods(vec!["GET", "POST", "OPTIONS", "DELETE", "PUT", "PATCH"])
                    .allowed_headers(vec![
                        http::header::AUTHORIZATION,
                        http::header::ACCEPT,
                        http::header::CONTENT_TYPE,
                        http::header::HeaderName::from_static("moosicbox-profile"),
                        http::header::HeaderName::from_static("hx-boosted"),
                        http::header::HeaderName::from_static("hx-current-url"),
                        http::header::HeaderName::from_static("hx-history-restore-request"),
                        http::header::HeaderName::from_static("hx-prompt"),
                        http::header::HeaderName::from_static("hx-request"),
                        http::header::HeaderName::from_static("hx-target"),
                        http::header::HeaderName::from_static("hx-trigger-name"),
                        http::header::HeaderName::from_static("hx-trigger"),
                    ])
                    .expose_headers(vec![
                        http::header::HeaderName::from_static("hx-location"),
                        http::header::HeaderName::from_static("hx-push-url"),
                        http::header::HeaderName::from_static("hx-redirect"),
                        http::header::HeaderName::from_static("hx-refresh"),
                        http::header::HeaderName::from_static("hx-replace-url"),
                        http::header::HeaderName::from_static("hx-reswap"),
                        http::header::HeaderName::from_static("hx-retarget"),
                        http::header::HeaderName::from_static("hx-reselect"),
                        http::header::HeaderName::from_static("hx-trigger"),
                        http::header::HeaderName::from_static("hx-trigger-after-settle"),
                        http::header::HeaderName::from_static("hx-trigger-after-swap"),
                    ])
                    .supports_credentials()
                    .max_age(3600);

                #[allow(unused_mut)]
                let mut app = App::new()
                    .app_data(Data::new(html_app.clone()))
                    .wrap(cors)
                    .wrap(middleware::Compress::default())
                    .wrap(moosicbox_middleware::api_logger::ApiLogger::default());

                #[cfg(feature = "assets")]
                {
                    use std::path::PathBuf;
                    use std::str::FromStr as _;

                    use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};

                    for StaticAssetRoute { route, target } in &html_app.static_asset_routes {
                        match target {
                            AssetPathTarget::File(target) => {
                                let target = target.clone();
                                app = app.route(
                                    route,
                                    web::get().to(move |req: HttpRequest| {
                                        let target = target.clone();
                                        async move {
                                            let file = actix_files::NamedFile::open_async(target)
                                                .await
                                                .map_err(
                                                    actix_web::error::ErrorInternalServerError,
                                                )?;

                                            Ok::<_, actix_web::Error>(file.into_response(&req))
                                        }
                                    }),
                                );
                            }
                            AssetPathTarget::FileContents(target) => {
                                let target = target.clone();
                                let extension = PathBuf::from_str(route)
                                    .unwrap()
                                    .extension()
                                    .and_then(|x| x.to_str().map(str::to_lowercase));

                                let content_type = match extension.as_deref() {
                                    Some("js" | "mjs" | "cjs") => "text/javascript;charset=UTF-8",
                                    _ => "application/octet-stream",
                                };

                                app = app.route(
                                    route,
                                    web::get().to(move || {
                                        let target = target.clone();
                                        async move {
                                            Ok::<_, actix_web::Error>(
                                                HttpResponse::Ok()
                                                    .content_type(content_type)
                                                    .body(target),
                                            )
                                        }
                                    }),
                                );
                            }
                            AssetPathTarget::Directory(target) => {
                                let target = target.clone();
                                app = app.route(
                                    &format!("{route}/{{path:.*}}"),
                                    web::get().to(
                                        move |req: HttpRequest, path: web::Path<String>| {
                                            let target = target.clone();
                                            async move {
                                                let target = target.join(path.clone());

                                                let file = actix_files::NamedFile::open_async(
                                                    target,
                                                )
                                                .await
                                                .map_err(
                                                    actix_web::error::ErrorInternalServerError,
                                                )?;

                                                Ok::<_, actix_web::Error>(file.into_response(&req))
                                            }
                                        },
                                    ),
                                );
                            }
                        }
                    }
                }

                #[cfg(feature = "sse")]
                let app = app
                    .service(web::resource("/$sse").route(web::get().to(sse::handle_sse::<T, R>)));

                #[cfg(feature = "actions")]
                let app = app.service(
                    web::resource("/$action").route(web::post().to(actions::handle_action::<T, R>)),
                );

                let catchall = move |req: HttpRequest,
                                     app: web::Data<ActixApp<T, R>>,
                                     body: Option<web::Bytes>| async move {
                    log::trace!("catchall: req={req:?} body={body:?}");
                    let data = app.processor.prepare_request(req, body.map(Arc::new))?;
                    app.processor.to_response(data).await
                };

                app.service(
                    web::resource("/{path:.*}")
                        .route(web::get().to(catchall))
                        .route(web::post().to(catchall))
                        .route(web::delete().to(catchall))
                        .route(web::put().to(catchall))
                        .route(web::patch().to(catchall))
                        .route(web::head().to(catchall)),
                )
            };

            let mut http_server = actix_web::HttpServer::new(app);

            let addr = "0.0.0.0";
            let service_port = default_env_u16!("PORT", 8343);

            log::info!("Server started on {addr}:{service_port}");

            http_server = http_server
                .bind((addr, service_port))
                .expect("Failed to bind the address");

            if let Err(e) = http_server.run().await {
                log::error!("Error from http server: {e:?}");
            } else {
                log::debug!("server finished");
            }
        });

        log::debug!("run: finished");

        Ok(())
    }
}
