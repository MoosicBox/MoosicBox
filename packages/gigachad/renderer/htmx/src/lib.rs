#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{ops::Deref, path::PathBuf, str::FromStr as _, sync::Arc};

use actix_cors::Cors;
use actix_htmx::Htmx;
use actix_web::{
    error::ErrorInternalServerError,
    http, middleware, route,
    web::{self, Data},
    App, HttpRequest, HttpResponse,
};
use async_trait::async_trait;
use flume::Receiver;
use gigachad_renderer::{Color, RenderRunner, Renderer, View};
use gigachad_router::Router;
use html::container_element_to_html_response;
use moosicbox_app_native_image::image;
use tokio::runtime::{Handle, Runtime};

mod html;

#[derive(Clone)]
pub struct HtmxRenderer {
    width: Option<u16>,
    height: Option<u16>,
    x: Option<i32>,
    y: Option<i32>,
    app: HtmxApp,
    receiver: Receiver<String>,
    runtime: Arc<Runtime>,
}

impl HtmxRenderer {
    #[must_use]
    pub fn new(router: Router, runtime: Arc<Runtime>) -> Self {
        let (_tx, rx) = flume::unbounded();
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            app: HtmxApp::new(router),
            receiver: rx,
            runtime,
        }
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
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
    app: web::Data<HtmxApp>,
    req: HttpRequest,
    htmx: Htmx,
) -> Result<HttpResponse, actix_web::Error> {
    let query_string = req.query_string();
    let query_string = if query_string.is_empty() {
        String::new()
    } else {
        format!("?{query_string}")
    };

    let path = format!("{}{}", req.path(), query_string);
    drop(req);

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

    let container = app
        .router
        .navigate(&path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to navigate: {e:?}")))?;

    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(
            container_element_to_html_response(&container.immediate, app.background, &htmx)
                .map_err(ErrorInternalServerError)?,
        ))
}

pub struct HtmxRenderRunner {
    app: HtmxApp,
    handle: Handle,
}

impl RenderRunner for HtmxRenderRunner {
    /// # Errors
    ///
    /// Will error if htmx fails to run the event loop.
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        log::debug!("run: starting");

        let htmx_app = self.app.clone();

        moosicbox_task::block_on_runtime("htmx server", &self.handle, async move {
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

                App::new()
                    .app_data(Data::new(htmx_app.clone()))
                    .wrap(cors)
                    .wrap(middleware::Compress::default())
                    .wrap(moosicbox_middleware::api_logger::ApiLogger::default())
                    .service(catchall_endpoint)
            };

            let mut http_server = actix_web::HttpServer::new(app);

            let addr = "0.0.0.0";
            let service_port = 8343;

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
impl Renderer for HtmxRenderer {
    /// # Errors
    ///
    /// Will error if htmx app fails to start
    async fn init(
        &mut self,
        width: u16,
        height: u16,
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
    /// Will error if htmx fails to run the event loop.
    async fn to_runner(
        &mut self,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(HtmxRenderRunner {
            app: self.app.clone(),
            handle: self.runtime.handle().clone(),
        }))
    }

    /// # Errors
    ///
    /// Will error if htmx fails to render the elements.
    fn render(
        &mut self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(
            ("render: start"),
            ("render: start {:?}", elements.immediate)
        );

        log::debug!("render: finished");

        Ok(())
    }
}

#[derive(Clone)]
struct HtmxApp {
    router: Router,
    background: Option<Color>,
}

impl HtmxApp {
    const fn new(router: Router) -> Self {
        Self {
            router,
            background: None,
        }
    }
}
