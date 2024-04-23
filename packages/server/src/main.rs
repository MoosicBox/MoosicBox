#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

mod api;
#[cfg(feature = "static-token-auth")]
mod auth;
mod db;
mod playback_session;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App};
use log::{debug, error, info};
use moosicbox_auth::get_client_id_and_access_token;
use moosicbox_core::app::AppState;
use moosicbox_database::Database;
use moosicbox_downloader::{api::models::ApiProgressEvent, queue::ProgressEvent};
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_tunnel::TunnelRequest;
use moosicbox_tunnel_sender::{sender::TunnelSender, TunnelMessage};
use moosicbox_ws::api::send_download_event;
use once_cell::sync::Lazy;
use std::{
    env,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    time::Duration,
};
use throttle::Throttle;
use tokio::{task::spawn, try_join};
use tokio_util::sync::CancellationToken;
use url::Url;
use ws::server::ChatServer;

static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);

static CHAT_SERVER_HANDLE: Lazy<std::sync::RwLock<Option<ws::server::ChatServerHandle>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

static DB: Lazy<std::sync::RwLock<Option<Arc<Box<dyn Database>>>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

fn main() -> std::io::Result<()> {
    #[cfg(debug_assertions)]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=trace";
    #[cfg(not(debug_assertions))]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=info";

    free_log_client::init(
        free_log_client::LogsConfig::builder()
            .user_agent("moosicbox_server")
            .log_writer_api_url("https://logs.moosicbox.com")
            .log_level(free_log_client::Level::Warn)
            .env_filter(default_env!(
                "MOOSICBOX_LOG",
                default_env!("RUST_LOG", DEFAULT_LOG_LEVEL)
            )),
    )
    .expect("Failed to initialize FreeLog client");

    let args: Vec<String> = env::args().collect();

    let service_port = if args.len() > 1 {
        args[1].parse::<u16>().expect("Invalid port argument")
    } else {
        default_env_usize("PORT", 8000)
            .unwrap_or(8000)
            .try_into()
            .expect("Invalid PORT environment variable")
    };

    moosicbox_player::player::set_service_port(service_port);
    moosicbox_player::player::on_playback_event(crate::playback_session::on_playback_event);

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
        #[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
        #[allow(unused)]
        let (db, db_connection) = db::init_postgres_raw_native_tls()
            .await
            .expect("Failed to init postgres DB");
        #[cfg(all(not(feature = "postgres-native-tls"), feature = "postgres-openssl", feature = "postgres-raw"))]
        #[allow(unused)]
        let (db, db_connection) = db::init_postgres_raw_openssl()
            .await
            .expect("Failed to init postgres DB");
        #[cfg(all(not(feature = "postgres-native-tls"), not(feature = "postgres-openssl"), feature = "postgres-raw"))]
        #[allow(unused)]
        let (db, db_connection) = db::init_postgres_raw_no_tls()
            .await
            .expect("Failed to init postgres DB");
        #[cfg(feature = "postgres-sqlx")]
        let db = db::init_postgres_sqlx()
            .await
            .expect("Failed to init postgres DB");
        #[cfg(not(feature = "postgres"))]
        #[allow(unused_variables)]
        let db = db::init_sqlite().await.expect("Failed to init sqlite DB");

        let database: Arc<Box<dyn Database>> = Arc::new(db);
        DB.write().unwrap().replace(database.clone());

        let bytes_throttle = Arc::new(Mutex::new(Throttle::new(Duration::from_millis(200), 1)));
        let bytes_buf = AtomicUsize::new(0);

        moosicbox_downloader::api::add_progress_listener_to_download_queue(Box::new(
            move |event| {
                let binding = CHAT_SERVER_HANDLE.read().unwrap_or_else(|e| e.into_inner());
                let sender = binding.as_ref().unwrap();

                let event = if let ProgressEvent::BytesRead { task, read, total } = event {
                    if let Err(_) = bytes_throttle.lock().unwrap().accept() {
                        bytes_buf.fetch_add(*read, std::sync::atomic::Ordering::SeqCst);
                        return;
                    } else {
                        let bytes = bytes_buf.load(std::sync::atomic::Ordering::SeqCst);
                        bytes_buf.store(0, std::sync::atomic::Ordering::SeqCst);
                        ProgressEvent::BytesRead {
                            task: task.clone(),
                            read: *read + bytes,
                            total: *total,
                        }
                    }
                } else {
                    event.clone()
                };

                let api_event: ApiProgressEvent = event.into();

                if let Err(err) = send_download_event(sender, None, api_event) {
                    log::error!("Failed to broadcast download event: {err:?}");
                }
            },
        ))
        .await;

        let (mut chat_server, server_tx) = ChatServer::new(database.clone());
        CHAT_SERVER_HANDLE.write().unwrap().replace(server_tx);

        #[cfg(feature = "postgres-raw")]
        let db_connection_handle = tokio::spawn(async { db_connection.await });

        let (tunnel_host, tunnel_join_handle, tunnel_handle) = if let Ok(url) = env::var("WS_HOST")
        {
            if !url.is_empty() {
                log::debug!("Using WS_HOST: {url}");
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
                // FIXME: Handle retry
                let (client_id, access_token) = {
                    get_client_id_and_access_token(&**database, &host)
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

                tunnel = tunnel.with_cancellation_token(CANCELLATION_TOKEN.clone());

                let database_send = database.clone();
                (
                    Some(host),
                    Some(spawn(async move {
                        let mut rx = tunnel.start();

                        while let Some(m) = rx.recv().await {
                            match m {
                                TunnelMessage::Text(m) => {
                                    debug!("Received text TunnelMessage {}", &m);
                                    let tunnel = tunnel.clone();
                                    let database_send = database_send.clone();
                                    spawn(async move {
                                        match serde_json::from_str(&m).unwrap() {
                                            TunnelRequest::Http(request) => {
                                                if let Err(err) = tunnel
                                                    .tunnel_request(
                                                        database_send.clone(),
                                                        service_port,
                                                        request.request_id,
                                                        request.method,
                                                        request.path,
                                                        request.query,
                                                        request.payload,
                                                        request.headers,
                                                        request.encoding,
                                                    )
                                                    .await
                                                {
                                                    log::error!("Tunnel request failed: {err:?}");
                                                }
                                            }
                                            TunnelRequest::Ws(request) => {
                                                let sender = CHAT_SERVER_HANDLE
                                                    .read()
                                                    .unwrap_or_else(|e| e.into_inner())
                                                    .as_ref()
                                                    .ok_or("Failed to get chat server handle")?
                                                    .clone();
                                                if let Err(err) = tunnel
                                                    .ws_request(
                                                        &**database_send,
                                                        request.conn_id,
                                                        request.request_id,
                                                        request.body.clone(),
                                                        sender,
                                                    )
                                                    .await
                                                {
                                                    error!(
                                                        "Failed to propagate ws request {} from conn_id {}: {err:?}",
                                                        request.request_id,
                                                        request.conn_id
                                                    );
                                                }
                                            }
                                            TunnelRequest::Abort(request) => {
                                                log::debug!("Aborting request {}", request.request_id);
                                                tunnel.abort_request(request.request_id);
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
            }
        } else {
            (None, None, None)
        };

        if let Some(ref tunnel_handle) = tunnel_handle {
            chat_server.add_sender(Box::new(tunnel_handle.clone()));
        }

        let chat_server_handle = spawn(async move { chat_server.run().await });

        let app = move || {
            let app_data = AppState {
                tunnel_host: tunnel_host.clone(),
                service_port,
                database: database.clone(),
            };

            let cors = Cors::default()
                .allow_any_origin()
                .allowed_methods(vec!["GET", "POST", "OPTIONS", "DELETE", "PUT", "PATCH"])
                .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT, http::header::CONTENT_TYPE])
                .supports_credentials()
                .max_age(3600);

            let app = App::new().wrap(cors).wrap(middleware::Compress::default());

            #[cfg(feature = "static-token-auth")]
            let app = app.wrap(crate::auth::StaticTokenAuth::new(
                std::env!("STATIC_TOKEN").into(),
            ));

            app.app_data(web::Data::new(app_data))
                .service(api::health_endpoint)
                .service(api::websocket)
                .service(moosicbox_scan::api::run_scan_endpoint)
                .service(moosicbox_scan::api::run_scan_path_endpoint)
                .service(moosicbox_scan::api::get_scan_origins_endpoint)
                .service(moosicbox_scan::api::enable_scan_origin_endpoint)
                .service(moosicbox_scan::api::disable_scan_origin_endpoint)
                .service(moosicbox_scan::api::get_scan_paths_endpoint)
                .service(moosicbox_scan::api::add_scan_path_endpoint)
                .service(moosicbox_scan::api::remove_scan_path_endpoint)
                .service(moosicbox_auth::api::get_magic_token_endpoint)
                .service(moosicbox_auth::api::create_magic_token_endpoint)
                .service(moosicbox_downloader::api::download_endpoint)
                .service(moosicbox_downloader::api::download_tasks_endpoint)
                .service(moosicbox_menu::api::get_artists_endpoint)
                .service(moosicbox_menu::api::get_artist_endpoint)
                .service(moosicbox_menu::api::get_album_endpoint)
                .service(moosicbox_menu::api::add_album_endpoint)
                .service(moosicbox_menu::api::remove_album_endpoint)
                .service(moosicbox_menu::api::refavorite_album_endpoint)
                .service(moosicbox_menu::api::get_albums_endpoint)
                .service(moosicbox_menu::api::get_tracks_endpoint)
                .service(moosicbox_menu::api::get_album_tracks_endpoint)
                .service(moosicbox_menu::api::get_album_versions_endpoint)
                .service(moosicbox_menu::api::get_artist_albums_endpoint)
                .service(moosicbox_files::api::track_endpoint)
                .service(moosicbox_files::api::track_visualization_endpoint)
                .service(moosicbox_files::api::track_info_endpoint)
                .service(moosicbox_files::api::tracks_info_endpoint)
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
                .service(moosicbox_search::api::reindex_endpoint)
                .service(moosicbox_search::api::search_global_search_endpoint)
                .service(moosicbox_tidal::api::device_authorization_endpoint)
                .service(moosicbox_tidal::api::device_authorization_token_endpoint)
                .service(moosicbox_tidal::api::track_file_url_endpoint)
                .service(moosicbox_tidal::api::track_playback_info_endpoint)
                .service(moosicbox_tidal::api::favorite_artists_endpoint)
                .service(moosicbox_tidal::api::add_favorite_artist_endpoint)
                .service(moosicbox_tidal::api::remove_favorite_artist_endpoint)
                .service(moosicbox_tidal::api::favorite_albums_endpoint)
                .service(moosicbox_tidal::api::add_favorite_album_endpoint)
                .service(moosicbox_tidal::api::remove_favorite_album_endpoint)
                .service(moosicbox_tidal::api::favorite_tracks_endpoint)
                .service(moosicbox_tidal::api::add_favorite_track_endpoint)
                .service(moosicbox_tidal::api::remove_favorite_track_endpoint)
                .service(moosicbox_tidal::api::artist_albums_endpoint)
                .service(moosicbox_tidal::api::album_tracks_endpoint)
                .service(moosicbox_tidal::api::album_endpoint)
                .service(moosicbox_tidal::api::artist_endpoint)
                .service(moosicbox_tidal::api::track_endpoint)
                .service(moosicbox_qobuz::api::user_login_endpoint)
                .service(moosicbox_qobuz::api::track_file_url_endpoint)
                .service(moosicbox_qobuz::api::favorite_artists_endpoint)
                .service(moosicbox_qobuz::api::favorite_albums_endpoint)
                .service(moosicbox_qobuz::api::favorite_tracks_endpoint)
                .service(moosicbox_qobuz::api::artist_albums_endpoint)
                .service(moosicbox_qobuz::api::album_tracks_endpoint)
                .service(moosicbox_qobuz::api::album_endpoint)
                .service(moosicbox_qobuz::api::artist_endpoint)
                .service(moosicbox_qobuz::api::track_endpoint)
        };

        let mut http_server = actix_web::HttpServer::new(app);

        if let Ok(Some(workers)) = option_env_usize("ACTIX_WORKERS") {
            log::debug!("Running with {workers} Actix workers");
            http_server = http_server.workers(workers);
        }

        tokio::spawn(async move {
            tokio::signal::ctrl_c().await?;
            log::debug!("Received ctrl-c");
            Ok::<_, std::io::Error>(())
        });

        let http_server = http_server
            .bind((default_env("BIND_ADDR", "0.0.0.0"), service_port))?
            .run();

        if let Err(err) = try_join!(
            async move {
                let resp = http_server.await;
                log::debug!("Shutting down ws server...");
                CHAT_SERVER_HANDLE.write().unwrap().take();
                log::debug!("Shutting down db client...");
                DB.write().unwrap().take();
                log::debug!("Cancelling scan...");
                moosicbox_scan::cancel();
                CANCELLATION_TOKEN.cancel();
                if let Some(handle) = tunnel_handle {
                    log::debug!("Closing tunnel connection...");
                    let _ = handle.close().await;
                }
                if let Some(handle) = tunnel_join_handle {
                    log::debug!("Closing tunnel join handle connection...");
                    handle.await.unwrap();
                } else {
                    log::trace!("No tunnel handle connection to close");
                }
                #[cfg(feature = "postgres-raw")]
                {
                    log::debug!("Aborting database connection...");
                    db_connection_handle.abort();
                }
                log::trace!("Connections closed");
                resp
            },
            async move {
                let resp = chat_server_handle.await.unwrap();
                log::debug!("Ws server connection closed");
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
