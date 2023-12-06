#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
#[cfg(feature = "static-token-auth")]
mod auth;
mod scan;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App};
use log::{debug, error, info};
use moosicbox_auth::get_client_id_and_access_token;
use moosicbox_core::app::{AppState, Db};
use moosicbox_tunnel::{
    sender::{tunnel_sender::TunnelSender, TunnelMessage},
    tunnel::TunnelRequest,
};
use once_cell::sync::Lazy;
use std::{
    env,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{task::spawn, try_join};
use url::Url;
use ws::server::ChatServer;

static CHAT_SERVER_HANDLE: Lazy<std::sync::RwLock<Option<ws::server::ChatServerHandle>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

fn main() -> std::io::Result<()> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    let service_port = if args.len() > 1 {
        args[1].parse::<u16>().unwrap()
    } else {
        8000
    };

    actix_web::rt::System::with_tokio_rt(|| {
        let threads = if let Ok(Ok(max)) = env::var("MAX_THREADS").map(|w| w.parse::<usize>()) {
            max
        } else {
            64
        };
        log::debug!("Running with {threads} max blocking threads");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(threads)
            .build()
            .unwrap()
    })
    .block_on(async move {
        static DB: OnceLock<Db> = OnceLock::new();
        let db = DB.get_or_init(|| {
            let library = ::rusqlite::Connection::open("library.db").unwrap();
            library
                .busy_timeout(Duration::from_millis(10))
                .expect("Failed to set busy timeout");
            Db {
                library: Arc::new(Mutex::new(library)),
            }
        });

        let (chat_server, server_tx) = ChatServer::new(Arc::new(db.clone()));
        let chat_server = spawn(chat_server.run());

        let (tunnel_host, tunnel_join_handle, tunnel_handle) = if let Ok(url) = env::var("WS_HOST")
        {
            let ws_url = url.clone();
            let url = Url::parse(&url).expect("Invalid WS_HOST");
            let hostname = url
                .host_str()
                .map(|s| s.to_string())
                .expect("Invalid WS_HOST");
            let host = format!(
                "{}://{hostname}{}",
                if url.scheme() == "wss" {
                    "https"
                } else {
                    "http"
                },
                if let Some(port) = url.port() {
                    format!(":{port}")
                } else {
                    "".to_string()
                }
            );
            let (client_id, access_token) = {
                let lock = db.library.lock();
                let db = lock.as_ref().unwrap();
                get_client_id_and_access_token(db, &host)
                    .await
                    .map_err(|e| {
                        std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Could not get access token: {e:?}"),
                        )
                    })?
            };
            let (mut tunnel, handle) =
                TunnelSender::new(host.clone(), ws_url, client_id, access_token);

            (
                Some(host),
                Some(spawn(async move {
                    let mut rx = tunnel.start();

                    while let Some(m) = rx.recv().await {
                        match m {
                            TunnelMessage::Text(m) => {
                                debug!("Received text TunnelMessage {}", &m);
                                let tunnel = tunnel.clone();
                                spawn(async move {
                                    println!("received request");
                                    match serde_json::from_str(&m).unwrap() {
                                        TunnelRequest::HttpRequest(request) => tunnel
                                            .tunnel_request(
                                                db,
                                                service_port,
                                                request.request_id,
                                                request.method,
                                                request.path,
                                                request.query,
                                                request.payload,
                                                request.encoding,
                                            )
                                            .await
                                            .unwrap(),
                                        TunnelRequest::WsRequest(request) => {
                                            let sender = CHAT_SERVER_HANDLE
                                                .read()
                                                .unwrap_or_else(|e| e.into_inner())
                                                .as_ref()
                                                .ok_or("Failed to get chat server handle")?
                                                .clone();
                                            if let Err(err) = tunnel
                                                .ws_request(
                                                    db,
                                                    request.request_id,
                                                    request.body.clone(),
                                                    sender,
                                                )
                                                .await
                                            {
                                                error!(
                                                    "Failed to propagate ws request {}: {err:?}",
                                                    request.request_id
                                                );
                                            }
                                        }
                                    }
                                    Ok::<_, String>(())
                                });
                            }
                            TunnelMessage::Binary(_) => todo!(),
                            TunnelMessage::Ping(_) => {}
                            TunnelMessage::Pong(_) => todo!(),
                            TunnelMessage::Close => {
                                info!("Tunnel connection was closed");
                            }
                            TunnelMessage::Frame(_) => todo!(),
                        }
                    }
                    debug!("Exiting tunnel message loop");
                })),
                Some(handle),
            )
        } else {
            (None, None, None)
        };

        let app = move || {
            let app_data = AppState {
                tunnel_host: tunnel_host.clone(),
                service_port,
                db: Some(db.clone()),
            };

            let cors = Cors::default()
                .allow_any_origin()
                .allowed_methods(vec!["GET", "POST"])
                .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
                .allowed_header(http::header::CONTENT_TYPE)
                .supports_credentials()
                .max_age(3600);

            CHAT_SERVER_HANDLE
                .write()
                .unwrap()
                .replace(server_tx.clone());

            let app = App::new().wrap(cors).wrap(middleware::Compress::default());

            #[cfg(feature = "static-token-auth")]
            let app = app.wrap(crate::auth::StaticTokenAuth::new(
                std::env!("STATIC_TOKEN").into(),
            ));

            app.app_data(web::Data::new(app_data))
                .app_data(web::Data::new(server_tx.clone()))
                .service(api::health_endpoint)
                .service(api::websocket)
                .service(api::scan_endpoint)
                .service(moosicbox_auth::api::get_magic_token_endpoint)
                .service(moosicbox_auth::api::create_magic_token_endpoint)
                .service(moosicbox_menu::api::get_artists_endpoint)
                .service(moosicbox_menu::api::get_artist_endpoint)
                .service(moosicbox_menu::api::get_album_endpoint)
                .service(moosicbox_menu::api::get_albums_endpoint)
                .service(moosicbox_menu::api::get_album_tracks_endpoint)
                .service(moosicbox_menu::api::get_artist_albums_endpoint)
                .service(moosicbox_files::api::track_endpoint)
                .service(moosicbox_files::api::track_info_endpoint)
                .service(moosicbox_files::api::artist_cover_endpoint)
                .service(moosicbox_files::api::album_source_artwork_endpoint)
                .service(moosicbox_files::api::album_artwork_endpoint)
                .service(moosicbox_player::api::play_track_endpoint)
                .service(moosicbox_player::api::play_tracks_endpoint)
                .service(moosicbox_player::api::play_album_endpoint)
                .service(moosicbox_player::api::pause_playback_endpoint)
                .service(moosicbox_player::api::resume_playback_endpoint)
                .service(moosicbox_player::api::update_playback_endpoint)
                .service(moosicbox_player::api::next_track_endpoint)
                .service(moosicbox_player::api::previous_track_endpoint)
                .service(moosicbox_player::api::stop_track_endpoint)
                .service(moosicbox_player::api::seek_track_endpoint)
                .service(moosicbox_player::api::player_status_endpoint)
        };

        let mut http_server = actix_web::HttpServer::new(app);

        if let Ok(Ok(workers)) = env::var("ACTIX_WORKERS").map(|w| w.parse::<usize>()) {
            http_server = http_server.workers(workers);
        }

        let http_server = http_server.bind(("0.0.0.0", service_port))?.run();

        try_join!(
            async move {
                let resp = http_server.await;
                CHAT_SERVER_HANDLE.write().unwrap().take();
                if let Some(handle) = tunnel_handle {
                    let _ = handle.close().await;
                }
                resp
            },
            async move { chat_server.await.unwrap() },
            async move {
                if let Some(handle) = tunnel_join_handle {
                    handle.await.unwrap()
                }
                Ok(())
            }
        )?;

        Ok(())
    })
}
