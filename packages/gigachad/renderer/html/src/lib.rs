#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    ops::Deref,
    path::PathBuf,
    str::FromStr as _,
    sync::{Arc, LazyLock, RwLockReadGuard, RwLockWriteGuard},
};

use actix_cors::Cors;
pub use actix_web::http::header::HeaderMap;
use actix_web::{
    error::ErrorInternalServerError,
    http::{self, header::USER_AGENT},
    middleware, route,
    web::{self, Data},
    App, HttpRequest, HttpResponse,
};
use async_trait::async_trait;
use flume::{Receiver, Sender};
use gigachad_actions::logic::Value;
use gigachad_renderer::{canvas::CanvasUpdate, Color, PartialView, RenderRunner, Renderer, View};
use gigachad_router::{ClientInfo, ClientOs, Container, RequestInfo, Router};
use html::{container_element_to_html_response, HtmlTagRenderer};
use moosicbox_app_native_image::image;
use moosicbox_env_utils::default_env_u16;
use tokio::runtime::{Handle, Runtime};
use uaparser::{Parser as _, UserAgentParser};

pub mod html;

pub struct DefaultHtmlTagRenderer;

impl HtmlTagRenderer for DefaultHtmlTagRenderer {}

#[derive(Clone)]
pub struct HtmlRenderer {
    width: Option<f32>,
    height: Option<f32>,
    x: Option<i32>,
    y: Option<i32>,
    pub app: HtmlApp,
    receiver: Receiver<String>,
    runtime: Arc<Runtime>,
}

impl HtmlRenderer {
    #[must_use]
    pub fn new(
        router: Router,
        runtime: Arc<Runtime>,
        request_action: Sender<(String, Option<Value>)>,
    ) -> Self {
        Self::new_with_tag_renderer(router, runtime, request_action, DefaultHtmlTagRenderer)
    }

    #[must_use]
    pub fn new_with_tag_renderer(
        router: Router,
        runtime: Arc<Runtime>,
        request_action: Sender<(String, Option<Value>)>,
        tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
    ) -> Self {
        let (_tx, rx) = flume::unbounded();
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            app: HtmlApp::new(router, request_action, tag_renderer),
            receiver: rx,
            runtime,
        }
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
    }

    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.app = self.app.with_static_asset_routes(paths);
        self
    }
}

#[route(
    "/{path:.*}",
    method = "GET",
    method = "POST",
    method = "DELETE",
    method = "PUT",
    method = "PATCH",
    method = "HEAD"
)]
#[allow(clippy::future_not_send)]
pub async fn catchall_endpoint(
    app: web::Data<HtmlApp>,
    req: HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    static UA_PARSER: LazyLock<UserAgentParser> = LazyLock::new(|| {
        UserAgentParser::from_bytes(include_bytes!("../ua-regexes.yaml"))
            .expect("Parser creation failed")
    });

    let query_string = req.query_string();
    let query_string = if query_string.is_empty() {
        String::new()
    } else {
        format!("?{query_string}")
    };

    let path = format!("{}{}", req.path(), query_string);

    if path == "/favicon.ico" {
        let favicon = image!("../../../../../app-website/public/favicon.ico");
        if let Some(favicon) = moosicbox_app_native_image::get_image(favicon) {
            return Ok(HttpResponse::Ok()
                .content_type("image/ico")
                .body(favicon.deref().clone()));
        }
    } else if path.starts_with("/app-website/") {
        let path = PathBuf::from_str(&format!("../../../../..{path}")).unwrap();
        if let Some(path_str) = path.to_str() {
            if let Some(image) = moosicbox_app_native_image::get_image(path_str) {
                if let Some(extension) = path.extension().and_then(|x| x.to_str()) {
                    let extension = extension.to_lowercase();
                    let mut response = HttpResponse::Ok();

                    match extension.as_str() {
                        "png" | "jpeg" | "jpg" | "ico" => {
                            response.content_type(format!("image/{extension}"));
                        }
                        "svg" => {
                            response.content_type(format!("image/{extension}+xml"));
                        }
                        _ => {
                            moosicbox_assert::die_or_warn!(
                                "unknown content-type for image {path_str}"
                            );
                        }
                    }

                    return Ok(response.body(image.deref().clone()));
                }
            }
        }
    }

    let os_name = if let Some(Ok(user_agent)) = req.headers().get(USER_AGENT).map(|x| x.to_str()) {
        let os = UA_PARSER.parse_os(user_agent);

        os.family.to_string()
    } else {
        "unknown".to_string()
    };

    let info = RequestInfo {
        client: Arc::new(ClientInfo {
            os: ClientOs { name: os_name },
        }),
    };

    let container = app
        .router
        .navigate(&path, info)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to navigate: {e:?}")))?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(
            container_element_to_html_response(
                req.headers(),
                &container.immediate,
                app.background,
                &**app.tag_renderer,
            )
            .map_err(ErrorInternalServerError)?,
        ))
}

pub struct HtmlRenderRunner {
    app: HtmlApp,
    handle: Handle,
}

impl RenderRunner for HtmlRenderRunner {
    /// # Errors
    ///
    /// Will error if html fails to run the event loop.
    #[allow(clippy::too_many_lines)]
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        log::debug!("run: starting");

        let html_app = self.app.clone();

        moosicbox_task::block_on_runtime("html server", &self.handle, async move {
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
                    use gigachad_renderer::assets::{AssetPathTarget, StaticAssetRoute};

                    for StaticAssetRoute { route, target } in &html_app.static_asset_routes {
                        match target {
                            AssetPathTarget::File(target) => {
                                let target = target.clone();
                                app = app.route(
                                    &format!("/{route}"),
                                    web::get().to(move |req: HttpRequest| {
                                        let target = target.clone();
                                        async move {
                                            let file = actix_files::NamedFile::open_async(target)
                                                .await
                                                .map_err(ErrorInternalServerError)?;

                                            Ok::<_, actix_web::Error>(file.into_response(&req))
                                        }
                                    }),
                                );
                            }
                            AssetPathTarget::Directory(target) => {
                                let target = target.clone();
                                app = app.route(
                                    &format!("/{route}/{{path:.*}}"),
                                    web::get().to(
                                        move |req: HttpRequest, path: web::Path<String>| {
                                            let target = target.clone();
                                            async move {
                                                let target = target.join(path.clone());

                                                let file =
                                                    actix_files::NamedFile::open_async(target)
                                                        .await
                                                        .map_err(ErrorInternalServerError)?;

                                                Ok::<_, actix_web::Error>(file.into_response(&req))
                                            }
                                        },
                                    ),
                                );
                            }
                        }
                    }
                }

                app.service(catchall_endpoint)
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

#[async_trait]
impl Renderer for HtmlRenderer {
    /// # Errors
    ///
    /// Will error if html app fails to start
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.width = Some(width);
        self.height = Some(height);
        self.x = x;
        self.y = y;
        self.app.background = background;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to run the event loop.
    async fn to_runner(&self) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(HtmlRenderRunner {
            app: self.app.clone(),
            handle: self.runtime.handle().clone(),
        }))
    }

    /// # Errors
    ///
    /// Will error if html app fails to emit the event.
    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("emit_event: event_name={event_name} event_value={event_value:?}");

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to render the elements.
    async fn render(
        &self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(
            ("render: start"),
            ("render: start {:?}", elements.immediate)
        );

        log::debug!("render: finished");

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to render the partial view.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render_partial(
        &self,
        view: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(
            ("render_partial: start"),
            ("render_partial: start {:?}", view)
        );

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to render the canvas update.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render_canvas(
        &self,
        _update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("render_canvas");

        Ok(())
    }

    fn container(&self) -> RwLockReadGuard<Container> {
        unimplemented!();
    }

    fn container_mut(&self) -> RwLockWriteGuard<Container> {
        unimplemented!();
    }
}

#[derive(Clone)]
pub struct HtmlApp {
    router: Router,
    background: Option<Color>,
    #[allow(unused)]
    request_action: Sender<(String, Option<Value>)>,
    pub tag_renderer: Arc<Box<dyn HtmlTagRenderer + Send + Sync>>,
    #[cfg(feature = "assets")]
    static_asset_routes: Vec<gigachad_renderer::assets::StaticAssetRoute>,
}

impl HtmlApp {
    fn new(
        router: Router,
        request_action: Sender<(String, Option<Value>)>,
        tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
    ) -> Self {
        Self {
            router,
            background: None,
            request_action,
            tag_renderer: Arc::new(Box::new(tag_renderer)),
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
        }
    }

    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.static_asset_routes = paths.into();
        self
    }
}
