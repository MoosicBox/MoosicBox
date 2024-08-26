#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
mod auth;
mod db;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, App};
use api::health_endpoint;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_tunnel_server::CANCELLATION_TOKEN;
use std::env;
use tokio::try_join;

static WS_SERVER_HANDLE: once_cell::sync::Lazy<
    tokio::sync::RwLock<Option<ws::server::service::Handle>>,
> = once_cell::sync::Lazy::new(|| tokio::sync::RwLock::new(None));

fn main() -> Result<(), std::io::Error> {
    if std::env::var("TOKIO_CONSOLE") == Ok("1".to_string()) {
        console_subscriber::init();
    } else {
        moosicbox_logging::init(Some("moosicbox_tunnel_server.log"))
            .expect("Failed to initialize FreeLog");
    }

    let service_port = {
        let args: Vec<String> = env::args().collect();

        if args.len() > 1 {
            args[1].parse::<u16>().expect("Invalid port argument")
        } else {
            default_env_usize("PORT", 8000)
                .unwrap_or(8000)
                .try_into()
                .expect("Invalid PORT environment variable")
        }
    };

    actix_web::rt::System::with_tokio_rt(|| {
        let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
        log::debug!("Running with {threads} max blocking threads");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(threads)
            .build()
            .unwrap()
    })
    .block_on(async move {
        #[cfg(feature = "postgres-raw")]
        db::init_postgres_raw()
            .await
            .expect("Failed to init postgres DB");
        #[cfg(feature = "postgres-sqlx")]
        db::init_postgres_sqlx()
            .await
            .expect("Failed to init postgres DB");

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
                .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
                .allowed_header(http::header::CONTENT_TYPE)
                .supports_credentials()
                .max_age(3600);

            App::new()
                .wrap(cors)
                .wrap(moosicbox_middleware::api_logger::ApiLogger::default())
                .wrap(middleware::Compress::default())
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
                .service(api::tunnel_endpoint)
        };

        let mut http_server = actix_web::HttpServer::new(app);

        if let Ok(Some(workers)) = option_env_usize("ACTIX_WORKERS") {
            log::debug!("Running with {workers} Actix workers");
            http_server = http_server.workers(workers);
        }

        let http_server = http_server
            .bind((default_env("BIND_ADDR", "0.0.0.0"), service_port))?
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
                if let Some(db_connection_handle) = db::DB_CONNECTION.lock().await.as_mut() {
                    log::debug!("Shutting down db connection...");
                    db_connection_handle.abort();
                    log::debug!("Database connection closed");
                } else {
                    log::debug!("No database connection");
                }

                log::trace!("Connections closed");

                resp
            },
            async move {
                let resp = ws_service
                    .await
                    .expect("Failed to shut down ws server")
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
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
