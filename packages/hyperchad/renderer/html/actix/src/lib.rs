#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::marker::PhantomData;

use actix_cors::Cors;
pub use actix_web::http::header::HeaderMap;
use actix_web::{
    http::{self},
    middleware,
    web::{self, Data},
    App, HttpRequest, HttpResponse,
};
use async_trait::async_trait;
use flume::Receiver;
use hyperchad_renderer::{Content, RenderRunner, RendererEvent, ToRenderRunner};
use moosicbox_env_utils::default_env_u16;
use tokio::runtime::Handle;

pub use actix_web;

#[cfg(feature = "sse")]
mod sse;

#[async_trait]
pub trait ActixResponseProcessor<T: Send + Sync + Clone> {
    /// # Errors
    ///
    /// * If the request fails to prepare
    fn prepare_request(&self, req: HttpRequest) -> Result<T, actix_web::Error>;

    async fn to_response(&self, data: T) -> Result<HttpResponse, actix_web::Error>;

    async fn to_body(&self, content: Content, data: T) -> Result<String, actix_web::Error>;
}

#[derive(Clone)]
pub struct ActixApp<T: Send + Sync + Clone, R: ActixResponseProcessor<T> + Send + Sync + Clone> {
    pub processor: R,
    pub renderer_event_rx: Receiver<RendererEvent>,
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone, R: ActixResponseProcessor<T> + Send + Sync + Clone> ActixApp<T, R> {
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
        R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
    > ToRenderRunner for ActixApp<T, R>
{
    fn to_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(ActixAppRunner { app: self, handle }))
    }
}

#[derive(Clone)]
pub struct ActixAppRunner<
    T: Send + Sync + Clone,
    R: ActixResponseProcessor<T> + Send + Sync + Clone,
> {
    pub app: ActixApp<T, R>,
    pub handle: Handle,
}

impl<
        T: Send + Sync + Clone + 'static,
        R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
    > RenderRunner for ActixAppRunner<T, R>
{
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
                    use std::path::PathBuf;
                    use std::str::FromStr as _;

                    use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};

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
                                    &format!("/{route}"),
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
                                    &format!("/{route}/{{path:.*}}"),
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

                let catchall = move |req: HttpRequest, app: web::Data<ActixApp<T, R>>| async move {
                    let data = app.processor.prepare_request(req)?;
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
