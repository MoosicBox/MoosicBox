#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
mod server;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, App};
use api::health_endpoint;
use lazy_static::lazy_static;
use std::env;
use tokio::runtime::{self, Runtime};
use tokio::try_join;

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

static CHAT_SERVER_HANDLE: once_cell::sync::Lazy<
    std::sync::Mutex<Option<ws::server::ChatServerHandle>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

static CONN_ID: once_cell::sync::Lazy<std::sync::Mutex<Option<usize>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    env_logger::init();

    let service_port = {
        let args: Vec<String> = env::args().collect();

        if args.len() > 1 {
            args[1].parse::<u16>().unwrap()
        } else {
            8000
        }
    };

    ws::db::init().await;

    let (chat_server, server_tx) = ws::server::ChatServer::new();
    let chat_server = tokio::task::spawn(chat_server.run());

    let app = move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .supports_credentials()
            .max_age(3600);

        let mut app = App::new()
            .wrap(cors)
            .wrap(middleware::Compress::default())
            .service(health_endpoint);

        CHAT_SERVER_HANDLE
            .lock()
            .unwrap()
            .replace(server_tx.clone());

        app = app.service(ws::api::websocket);

        app = app.service(api::track_endpoint);

        app
    };

    let http_server = actix_web::HttpServer::new(app)
        .bind(("0.0.0.0", service_port))?
        .run();

    try_join!(
        async move {
            let resp = http_server.await;
            CHAT_SERVER_HANDLE.lock().unwrap().take();
            resp
        },
        async move { chat_server.await.unwrap() }
    )?;

    Ok(())
}
