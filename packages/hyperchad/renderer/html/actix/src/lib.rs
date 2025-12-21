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

/// Re-export of Actix Web's `HeaderMap` for convenient access to HTTP headers.
///
/// This allows users of this crate to work with HTTP headers without needing
/// to import `actix_web::http::header::HeaderMap` directly.
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

/// Re-export of the Actix Web framework.
///
/// This re-export provides access to the underlying Actix Web types and utilities,
/// allowing implementors of [`ActixResponseProcessor`] to use Actix Web's request
/// and response types without needing a separate dependency.
pub use actix_web;

#[cfg(feature = "actions")]
mod actions;

#[cfg(feature = "sse")]
mod sse;

/// Generates the route pattern for a directory asset route.
/// Handles the special case where route="/" or "" to avoid producing "//" and
/// uses `.+` (one or more) instead of `.*` to prevent matching the root path itself.
#[cfg(feature = "assets")]
fn directory_route_pattern(route: &str) -> String {
    if route == "/" || route.is_empty() {
        "/{path:.+}".to_string()
    } else {
        format!("{route}/{{path:.*}}")
    }
}

/// Creates a guard that only matches if the requested file exists in the directory.
///
/// This is used for the `Fallthrough` behavior where we want non-existent files
/// to fall through to the router's catchall handler instead of returning an error.
#[cfg(feature = "assets")]
fn file_exists_guard(
    base_dir: std::path::PathBuf,
    route_prefix: String,
) -> impl actix_web::guard::Guard {
    actix_web::guard::fn_guard(move |ctx| {
        let uri_path = ctx.head().uri.path();

        // Strip the route prefix to get the relative file path
        let relative = if route_prefix.is_empty() {
            uri_path.trim_start_matches('/')
        } else {
            uri_path
                .strip_prefix(&route_prefix)
                .unwrap_or(uri_path)
                .trim_start_matches('/')
        };

        // Don't match empty paths (the directory route itself)
        if relative.is_empty() {
            return false;
        }

        // Check if the file exists
        let file_path = base_dir.join(relative);
        file_path.is_file()
    })
}

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
    /// Default behavior when a requested asset file is not found (requires `assets` feature).
    #[cfg(feature = "assets")]
    pub asset_not_found_behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
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
            #[cfg(feature = "assets")]
            asset_not_found_behavior: hyperchad_renderer::assets::AssetNotFoundBehavior::NotFound,
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

    /// Sets the default behavior when a requested asset file is not found.
    #[cfg(feature = "assets")]
    #[must_use]
    pub const fn with_asset_not_found_behavior(
        mut self,
        behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) -> Self {
        self.asset_not_found_behavior = behavior;
        self
    }

    /// Sets the default behavior when a requested asset file is not found (in place).
    #[cfg(feature = "assets")]
    pub const fn set_asset_not_found_behavior(
        &mut self,
        behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) {
        self.asset_not_found_behavior = behavior;
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
    /// Starts the Actix web server and begins processing renderer events.
    ///
    /// This method blocks the current thread and runs the Actix HTTP server, handling
    /// incoming requests and streaming renderer events through SSE connections. The server
    /// listens on the configured address and port (default: `0.0.0.0:8343`).
    ///
    /// # Errors
    ///
    /// * Returns an error if the event loop fails to run
    ///
    /// # Panics
    ///
    /// * Panics if the server fails to bind to the configured address and port
    /// * Panics if file path parsing fails for static asset routes
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

                    use hyperchad_renderer::assets::{
                        AssetNotFoundBehavior, AssetPathTarget, StaticAssetRoute,
                    };

                    for StaticAssetRoute {
                        route,
                        target,
                        not_found_behavior,
                    } in &html_app.static_asset_routes
                    {
                        // Determine the effective behavior: per-route override or global default
                        let behavior =
                            not_found_behavior.unwrap_or(html_app.asset_not_found_behavior);

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
                                let route_prefix = if route == "/" || route.is_empty() {
                                    String::new()
                                } else {
                                    route.clone()
                                };

                                match behavior {
                                    AssetNotFoundBehavior::Fallthrough => {
                                        // Use a guard that only matches if the file exists
                                        let guard_dir = target.clone();
                                        let guard_prefix = route_prefix.clone();
                                        app = app.route(
                                            &directory_route_pattern(route),
                                            web::get()
                                                .guard(file_exists_guard(guard_dir, guard_prefix))
                                                .to(
                                                    move |req: HttpRequest,
                                                          path: web::Path<String>| {
                                                        let target = target.clone();
                                                        async move {
                                                            let file_path = target.join(path.as_str());
                                                            let file =
                                                                actix_files::NamedFile::open_async(
                                                                    file_path,
                                                                )
                                                                .await
                                                                .map_err(
                                                                    actix_web::error::ErrorInternalServerError,
                                                                )?;
                                                            Ok::<_, actix_web::Error>(
                                                                file.into_response(&req),
                                                            )
                                                        }
                                                    },
                                                ),
                                        );
                                    }
                                    AssetNotFoundBehavior::NotFound => {
                                        // Check in handler, return 404 if not found
                                        app = app.route(
                                            &directory_route_pattern(route),
                                            web::get().to(
                                                move |req: HttpRequest,
                                                      path: web::Path<String>| {
                                                    let target = target.clone();
                                                    async move {
                                                        let file_path = target.join(path.as_str());
                                                        if !file_path.is_file() {
                                                            return Ok(HttpResponse::NotFound()
                                                                .finish());
                                                        }
                                                        let file =
                                                            actix_files::NamedFile::open_async(
                                                                file_path,
                                                            )
                                                            .await
                                                            .map_err(
                                                                actix_web::error::ErrorInternalServerError,
                                                            )?;
                                                        Ok::<_, actix_web::Error>(
                                                            file.into_response(&req),
                                                        )
                                                    }
                                                },
                                            ),
                                        );
                                    }
                                    AssetNotFoundBehavior::InternalServerError => {
                                        // Original behavior - let NamedFile::open_async fail
                                        app = app.route(
                                            &directory_route_pattern(route),
                                            web::get().to(
                                                move |req: HttpRequest,
                                                      path: web::Path<String>| {
                                                    let target = target.clone();
                                                    async move {
                                                        let file_path = target.join(path.as_str());
                                                        let file =
                                                            actix_files::NamedFile::open_async(
                                                                file_path,
                                                            )
                                                            .await
                                                            .map_err(
                                                                actix_web::error::ErrorInternalServerError,
                                                            )?;
                                                        Ok::<_, actix_web::Error>(
                                                            file.into_response(&req),
                                                        )
                                                    }
                                                },
                                            ),
                                        );
                                    }
                                }
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

#[cfg(any(feature = "actions", feature = "assets"))]
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestProcessor;

    #[async_trait]
    impl ActixResponseProcessor<()> for TestProcessor {
        fn prepare_request(
            &self,
            _req: HttpRequest,
            _body: Option<Arc<Bytes>>,
        ) -> Result<(), actix_web::Error> {
            Ok(())
        }

        async fn to_response(&self, _data: ()) -> Result<HttpResponse, actix_web::Error> {
            Ok(HttpResponse::Ok().finish())
        }

        async fn to_body(
            &self,
            _content: Content,
            _data: (),
        ) -> Result<(Bytes, String), actix_web::Error> {
            Ok((Bytes::new(), "text/html".to_string()))
        }
    }

    #[test_log::test]
    fn test_actix_app_new() {
        let (_tx, rx) = flume::unbounded::<RendererEvent>();
        let processor = TestProcessor;
        let app = ActixApp::new(processor, rx);

        #[cfg(feature = "actions")]
        assert!(app.action_tx.is_none());

        #[cfg(feature = "assets")]
        assert!(app.static_asset_routes.is_empty());
    }

    #[cfg(feature = "actions")]
    #[test_log::test]
    fn test_actix_app_with_action_tx() {
        let (_tx, rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, _action_rx) = flume::unbounded();
        let processor = TestProcessor;

        let app = ActixApp::new(processor, rx).with_action_tx(action_tx.clone());

        assert!(app.action_tx.is_some());
        if let Some(tx) = app.action_tx {
            assert!(tx.same_channel(&action_tx));
        }
    }

    #[cfg(feature = "actions")]
    #[test_log::test]
    fn test_actix_app_set_action_tx() {
        let (_tx, rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, _action_rx) = flume::unbounded();
        let processor = TestProcessor;

        let mut app = ActixApp::new(processor, rx);
        assert!(app.action_tx.is_none());

        app.set_action_tx(action_tx.clone());

        assert!(app.action_tx.is_some());
        if let Some(tx) = app.action_tx {
            assert!(tx.same_channel(&action_tx));
        }
    }

    #[cfg(feature = "actions")]
    #[test_log::test]
    fn test_actix_app_with_action_tx_chaining() {
        let (_tx, rx) = flume::unbounded::<RendererEvent>();
        let (action_tx1, _action_rx1) = flume::unbounded();
        let (action_tx2, _action_rx2) = flume::unbounded();
        let processor = TestProcessor;

        let app = ActixApp::new(processor, rx)
            .with_action_tx(action_tx1)
            .with_action_tx(action_tx2.clone());

        assert!(app.action_tx.is_some());
        if let Some(tx) = app.action_tx {
            // Should have the last set action_tx (action_tx2)
            assert!(tx.same_channel(&action_tx2));
        }
    }

    #[cfg(feature = "assets")]
    #[test_log::test]
    #[allow(clippy::literal_string_with_formatting_args)]
    fn test_directory_route_pattern() {
        use super::directory_route_pattern;

        // Root routes use .+ to avoid matching "/" itself
        assert_eq!(directory_route_pattern("/"), "/{path:.+}");
        assert_eq!(directory_route_pattern(""), "/{path:.+}");
        // Non-root routes use .* since the prefix already prevents matching the route itself
        assert_eq!(directory_route_pattern("/assets"), "/assets/{path:.*}");
        assert_eq!(
            directory_route_pattern("/static/files"),
            "/static/files/{path:.*}"
        );
    }
}
