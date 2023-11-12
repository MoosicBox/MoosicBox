#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
#[cfg(feature = "server")]
mod server;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, App};
use lambda_runtime::Error;
use lazy_static::lazy_static;
use std::env;
use tokio::runtime::{self, Runtime};
#[cfg(feature = "server")]
use tokio::{task::spawn, try_join};

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

#[cfg(feature = "server")]
static CHAT_SERVER_HANDLE: once_cell::sync::Lazy<
    std::sync::Mutex<Option<ws::server::ChatServerHandle>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

#[cfg(feature = "server")]
static CONN_ID: once_cell::sync::Lazy<std::sync::Mutex<Option<usize>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

#[actix_web::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    #[cfg(feature = "serverless")]
    {
        if let Ok(host) = env::var("WS_HOST") {
            use crate::api::TUNNEL_SENDERS;
            use moosicbox_tunnel::ws::{
                init_host,
                sender::{start, TunnelMessage},
            };

            init_host(host).expect("Failed to initialize websocket host");

            let (ready, rx) = start();
            ready.recv().unwrap();

            RT.spawn(async move {
                while let Ok(m) = rx.recv() {
                    match m {
                        TunnelMessage::Text(m) => {
                            log::debug!("Received text TunnelMessage: {m}");
                        }
                        TunnelMessage::Binary(bytes) => {
                            let data = bytes.slice(8..);
                            let id = &bytes[..8];
                            let id = usize::from_be_bytes(id.try_into().unwrap());
                            if let Some(sender) = TUNNEL_SENDERS.lock().unwrap().get(&id) {
                                sender.send(data).unwrap();
                            } else {
                                log::error!("unexpected binary message {id} (size {})", data.len());
                            }
                        }
                        TunnelMessage::Ping(_) => {}
                        TunnelMessage::Pong(_) => todo!(),
                        TunnelMessage::Close => todo!(),
                        TunnelMessage::Frame(_) => todo!(),
                    }
                }
            });
        }
    }

    #[cfg(feature = "server")]
    let service_port = {
        let args: Vec<String> = env::args().collect();

        if args.len() > 1 {
            args[1].parse::<u16>().unwrap()
        } else {
            8000
        }
    };

    #[cfg(feature = "server")]
    let (chat_server, server_tx) = ws::server::ChatServer::new();

    #[cfg(feature = "server")]
    let chat_server = spawn(chat_server.run());

    let app = move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .supports_credentials()
            .max_age(3600);

        #[allow(unused_mut)]
        let mut app = App::new()
            .wrap(cors)
            .wrap(middleware::Compress::default())
            .service(api::track_server_endpoint);

        #[cfg(feature = "server")]
        {
            CHAT_SERVER_HANDLE
                .lock()
                .unwrap()
                .replace(server_tx.clone());

            app = app.service(ws::api::websocket);
        }

        app
    };

    #[cfg(all(feature = "server", feature = "serverless"))]
    {
        if lambda_web::is_running_on_lambda() {
            lambda_web::run_actix_on_lambda(app).await?;
        } else {
            let http_server = actix_web::HttpServer::new(app)
                .bind(("0.0.0.0", service_port))?
                .run();
            try_join!(http_server, async move { chat_server.await.unwrap() })?;
        }
    }
    #[cfg(all(not(feature = "server"), feature = "serverless"))]
    {
        lambda_web::run_actix_on_lambda(app).await?;
    }
    #[cfg(all(not(feature = "serverless"), feature = "server"))]
    {
        let http_server = actix_web::HttpServer::new(app)
            .bind(("0.0.0.0", service_port))?
            .run();
        try_join!(http_server, async move { chat_server.await.unwrap() })?;
    }

    Ok(())
}
