#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{
    error::ErrorInternalServerError,
    http, middleware, route,
    web::{self, Data},
    App, HttpRequest, HttpResponse,
};
use async_trait::async_trait;
use flume::Receiver;
use gigachad_renderer::{RenderRunner, Renderer};
use gigachad_router::Router;
use gigachad_transformer::ContainerElement;
use tokio::runtime::{Handle, Runtime};

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

fn container_element_to_html(container: &ContainerElement) -> String {
    let html = container
        .elements
        .iter()
        .map(ToString::to_string)
        .collect::<String>();

    format!("<html><body>{html}</body></html>")
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
) -> Result<HttpResponse, actix_web::Error> {
    let query_string = req.query_string();
    let query_string = if query_string.is_empty() {
        String::new()
    } else {
        format!("?{query_string}")
    };
    let container = app
        .router
        .clone()
        .navigate(&format!("{}{}", req.path(), query_string))
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to navigate: {e:?}")))?;

    Ok(HttpResponse::Ok().body(container_element_to_html(&container)))
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
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.width = Some(width);
        self.height = Some(height);
        self.x = x;
        self.y = y;

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
        elements: ContainerElement,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(("render: start"), ("render: start {elements:?}"));

        log::debug!("render: finished");

        Ok(())
    }
}

#[derive(Clone)]
struct HtmxApp {
    router: Router,
}

impl HtmxApp {
    const fn new(router: Router) -> Self {
        Self { router }
    }
}
