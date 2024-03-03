#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
mod auth;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, App};
use api::health_endpoint;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use std::env;
use tokio::try_join;

static CHAT_SERVER_HANDLE: once_cell::sync::Lazy<
    std::sync::RwLock<Option<ws::server::ChatServerHandle>>,
> = once_cell::sync::Lazy::new(|| std::sync::RwLock::new(None));

fn main() -> Result<(), std::io::Error> {
    env_logger::init();

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
        ws::db::init().await.expect("Failed to initialize database");

        let (chat_server, server_tx) = ws::server::ChatServer::new();
        let chat_server = tokio::task::spawn(chat_server.run());

        let app = move || {
            let cors = Cors::default()
                .allow_any_origin()
                .allowed_methods(vec!["GET", "POST", "OPTIONS", "DELETE", "PUT", "PATCH"])
                .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
                .allowed_header(http::header::CONTENT_TYPE)
                .supports_credentials()
                .max_age(3600);

            CHAT_SERVER_HANDLE
                .write()
                .unwrap()
                .replace(server_tx.clone());

            App::new()
                .wrap(cors)
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
                CHAT_SERVER_HANDLE
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .take();
                resp
            },
            async move {
                match chat_server.await {
                    Ok(value) => value,
                    Err(err) => {
                        panic!("Failed to shut down chat server: {err:?}");
                    }
                }
            }
        ) {
            log::error!("Error on shutdown: {err:?}");
            return Err(err);
        }

        log::info!("Server shut down");

        Ok(())
    })
}
