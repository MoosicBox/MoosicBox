//! `MoosicBox` tunnel server application.
//!
//! This binary provides a WebSocket-based HTTP tunneling server that allows clients
//! to establish persistent connections and proxy HTTP requests through them. The server
//! handles authentication, connection management, and bidirectional request/response streaming.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod api;
mod auth;
mod db;
mod ws;

use actix_cors::Cors;
use actix_web::{App, http, middleware};
use api::health_endpoint;
use moosicbox_logging::free_log_client::DynLayer;
use moosicbox_tunnel_server::CANCELLATION_TOKEN;
use std::{env, sync::LazyLock};
use switchy_env::{var_or, var_parse_opt, var_parse_or};
use tokio::try_join;

static WS_SERVER_HANDLE: LazyLock<
    switchy_async::sync::RwLock<Option<ws::server::service::Handle>>,
> = LazyLock::new(|| switchy_async::sync::RwLock::new(None));

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), std::io::Error> {
    let service_port = {
        let args: Vec<String> = env::args().collect();

        if args.len() > 1 {
            args[1].parse::<u16>().expect("Invalid port argument")
        } else {
            var_parse_or("PORT", 8000usize)
                .try_into()
                .expect("Invalid PORT environment variable")
        }
    };

    actix_web::rt::System::with_tokio_rt(|| {
        let threads = var_parse_or("MAX_THREADS", 64usize);
        log::debug!("Running with {threads} max blocking threads");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(threads)
            .build()
            .unwrap()
    })
    .block_on(async move {
        let mut layers = vec![];

        if matches!(
            switchy_env::var("TOKIO_CONSOLE").as_deref(),
            Ok("1" | "true")
        ) {
            layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
        }

        #[cfg(feature = "telemetry")]
        layers.push(
            switchy_telemetry::init_tracer(env!("CARGO_PKG_NAME"))
                .map_err(std::io::Error::other)?,
        );

        moosicbox_logging::init(Some("moosicbox_tunnel_server.log"), Some(layers))
            .expect("Failed to initialize FreeLog");

        #[cfg(feature = "telemetry")]
        let metrics_handler = std::sync::Arc::new(switchy_telemetry::get_http_metrics_handler());

        db::init().await.expect("Failed to init postgres DB");

        let ws_server = ws::server::WsServer::new();
        let ws_service = ws::server::service::Service::new(ws_server);
        let ws_service_handle = ws_service.handle();
        let ws_service = ws_service.with_name("WsServer").start();

        WS_SERVER_HANDLE
            .write()
            .await
            .replace(ws_service_handle.clone());

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

            let app = App::new()
                .wrap(cors)
                .wrap(moosicbox_middleware::api_logger::ApiLogger::default())
                .wrap(middleware::Compress::default());

            #[cfg(feature = "telemetry")]
            let app = app
                .app_data(actix_web::web::Data::new(metrics_handler.clone()))
                .service(switchy_telemetry::metrics)
                .wrap(metrics_handler.request_middleware())
                .wrap(switchy_telemetry::RequestTracing::new());

            let app = app
                .service(health_endpoint)
                .service(ws::api::websocket)
                .service(api::auth_register_client_endpoint)
                .service(api::auth_signature_token_endpoint)
                .service(api::auth_get_magic_token_endpoint)
                .service(api::auth_magic_token_endpoint)
                .service(api::auth_validate_signature_token_endpoint)
                .service(api::track_endpoint)
                .service(api::artist_cover_endpoint)
                .service(api::album_cover_endpoint)
                .service(api::tunnel_endpoint);

            #[allow(clippy::let_and_return)]
            app
        };

        let mut http_server = actix_web::HttpServer::new(app);

        if let Some(workers) = var_parse_opt::<usize>("ACTIX_WORKERS").unwrap_or(None) {
            log::debug!("Running with {workers} Actix workers");
            http_server = http_server.workers(workers);
        }

        let http_server = http_server
            .bind((var_or("BIND_ADDR", "0.0.0.0"), service_port))?
            .run();

        if let Err(err) = try_join!(
            async move {
                let resp = http_server.await;

                log::debug!("Cancelling token...");
                CANCELLATION_TOKEN.cancel();

                log::debug!("Shutting down ws server...");
                WS_SERVER_HANDLE.write().await.take();

                log::debug!("Shutting down db client...");
                db::DB.lock().await.take();

                #[cfg(feature = "postgres-raw")]
                {
                    let mut db = db::DB.lock().await;
                    if let Some(db_connection_handle) = db.as_mut() {
                        log::debug!("Shutting down db connection...");
                        if let Err(e) = db_connection_handle.close().await {
                            log::error!("Failed to close database connection: {e:?}");
                        }
                        log::debug!("Database connection closed");
                    } else {
                        log::debug!("No database connection");
                    }
                }

                log::trace!("Connections closed");

                resp
            },
            async move {
                let resp = ws_service
                    .await
                    .expect("Failed to shut down ws server")
                    .map_err(std::io::Error::other);
                log::debug!("WsServer connection closed");
                resp
            },
        ) {
            log::error!("Error on shutdown: {err:?}");
            return Err(err);
        }

        log::debug!("Server shut down");

        Ok(())
    })
}
