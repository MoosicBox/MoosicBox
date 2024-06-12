#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

mod api;
#[cfg(feature = "static-token-auth")]
mod auth;
mod db;
mod playback_session;
mod tunnel;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App};
use futures_util::Future;
use moosicbox_config::get_config_dir_path;
use moosicbox_core::{
    app::AppState,
    sqlite::models::{RegisterConnection, RegisterPlayer, UpdateSession},
};
use moosicbox_database::Database;
use moosicbox_downloader::{api::models::ApiProgressEvent, queue::ProgressEvent};
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_player::{
    api::DEFAULT_PLAYBACK_RETRY_OPTIONS,
    player::{PlayerSource, TrackOrId},
};
use moosicbox_tunnel_sender::sender::TunnelSenderHandle;
use moosicbox_ws::{send_download_event, WebsocketContext, WebsocketSendError};
use once_cell::sync::Lazy;
use std::{
    collections::{hash_map::Entry, HashMap},
    env,
    fs::create_dir_all,
    pin::Pin,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    time::Duration,
};
use throttle::Throttle;
use tokio::try_join;
use tokio_util::sync::CancellationToken;
use ws::server::{ChatServerHandle, WsServer};

use crate::playback_session::PLAYBACK_EVENT_HANDLER;

static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);

static CHAT_SERVER_HANDLE: Lazy<std::sync::RwLock<Option<ws::server::ChatServerHandle>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

#[allow(clippy::type_complexity)]
static DB: Lazy<std::sync::RwLock<Option<Arc<Box<dyn Database>>>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

static WS_SERVER_RT: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap()
});

#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    #[cfg(debug_assertions)]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=trace";
    #[cfg(not(debug_assertions))]
    const DEFAULT_LOG_LEVEL: &str = "moosicbox=info";

    let mut logs_config = free_log_client::LogsConfig::builder()
        .with_api_writer(
            free_log_client::ApiWriterConfig::builder()
                .user_agent("moosicbox_app")
                .api_url("https://logs.moosicbox.com")
                .log_level(free_log_client::Level::Warn),
        )
        .expect("Failed to initialize api writer");

    if let Some(log_dir) = get_config_dir_path().map(|p| p.join("logs")) {
        if create_dir_all(&log_dir).is_ok() {
            logs_config = logs_config
                .with_file_writer(
                    free_log_client::FileWriterConfig::builder()
                        .file_path(log_dir.join("moosicbox_app.log"))
                        .log_level(free_log_client::Level::Debug),
                )
                .expect("Failed to initialize file writer");
        } else {
            log::warn!("Could not create directory path for logs files at {log_dir:?}");
        }
    } else {
        log::warn!("Could not get config dir to put the logs into");
    }

    free_log_client::init(logs_config.env_filter(default_env!(
        "MOOSICBOX_LOG",
        default_env!("RUST_LOG", DEFAULT_LOG_LEVEL)
    )))
    .expect("Failed to initialize FreeLog");

    let args: Vec<String> = env::args().collect();

    let service_port = if args.len() > 1 {
        args[1].parse::<u16>().expect("Invalid port argument")
    } else {
        default_env_usize("PORT", 8000)
            .unwrap_or(8000)
            .try_into()
            .expect("Invalid PORT environment variable")
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
        #[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
        #[allow(unused)]
        let (db, db_connection) = db::init_postgres_raw_native_tls()
            .await
            .expect("Failed to init postgres DB");
        #[cfg(all(
            not(feature = "postgres-native-tls"),
            feature = "postgres-openssl",
            feature = "postgres-raw"
        ))]
        #[allow(unused)]
        let (db, db_connection) = db::init_postgres_raw_openssl()
            .await
            .expect("Failed to init postgres DB");
        #[cfg(all(
            not(feature = "postgres-native-tls"),
            not(feature = "postgres-openssl"),
            feature = "postgres-raw"
        ))]
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
                let event = if let ProgressEvent::BytesRead { task, read, total } = event {
                    if bytes_throttle.lock().unwrap().accept().is_err() {
                        bytes_buf.fetch_add(*read, std::sync::atomic::Ordering::SeqCst);
                        return;
                    }

                    let bytes = bytes_buf.load(std::sync::atomic::Ordering::SeqCst);
                    bytes_buf.store(0, std::sync::atomic::Ordering::SeqCst);
                    ProgressEvent::BytesRead {
                        task: task.clone(),
                        read: *read + bytes,
                        total: *total,
                    }
                } else {
                    event.clone()
                };

                let api_event: ApiProgressEvent = event.into();

                if let Err(err) = send_download_event(
                    CHAT_SERVER_HANDLE
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .as_ref()
                        .unwrap(),
                    None,
                    api_event,
                ) {
                    log::error!("Failed to broadcast download event: {err:?}");
                }
            },
        ))
        .await;

        let (mut chat_server, server_tx) = WsServer::new(database.clone());
        let handle = server_tx.clone();
        CHAT_SERVER_HANDLE.write().unwrap().replace(server_tx);

        let playback_event_handle = tokio::spawn(PLAYBACK_EVENT_HANDLER.run());

        moosicbox_player::player::set_service_port(service_port);
        moosicbox_player::player::on_playback_event(crate::playback_session::on_playback_event);

        #[cfg(feature = "postgres-raw")]
        let db_connection_handle = tokio::spawn(db_connection);

        let (tunnel_host, tunnel_join_handle, tunnel_handle) =
            crate::tunnel::setup_tunnel(database.clone(), service_port)
                .await
                .expect("Failed to setup tunnel connection");

        if let Some(ref tunnel_handle) = tunnel_handle {
            chat_server.add_sender(Box::new(tunnel_handle.clone()));
        }

        let chat_server_handle = WS_SERVER_RT.spawn(chat_server.run());

        if let Err(err) = register_server_player(&**database.clone(), handle, &tunnel_handle).await
        {
            log::error!("Failed to register server player: {err:?}");
        } else {
            log::debug!("Registered server player");
        }

        #[cfg(feature = "upnp")]
        let upnp_service = moosicbox_upnp::listener::UpnpListener::new();
        #[cfg(feature = "upnp")]
        let upnp_service_handle = upnp_service.handle();
        #[cfg(feature = "upnp")]
        let join_upnp_service = upnp_service.start();

        let app = move || {
            let app_data = AppState {
                tunnel_host: tunnel_host.clone(),
                service_port,
                database: database.clone(),
            };

            let cors = Cors::default()
                .allow_any_origin()
                .allowed_methods(vec!["GET", "POST", "OPTIONS", "DELETE", "PUT", "PATCH"])
                .allowed_headers(vec![
                    http::header::AUTHORIZATION,
                    http::header::ACCEPT,
                    http::header::CONTENT_TYPE,
                ])
                .supports_credentials()
                .max_age(3600);

            let app = App::new()
                .wrap(cors)
                .wrap(moosicbox_middleware::api_logger::ApiLogger::default())
                .wrap(middleware::Compress::default());

            #[cfg(feature = "static-token-auth")]
            let app = app.wrap(crate::auth::StaticTokenAuth::new(
                std::env!("STATIC_TOKEN").into(),
            ));

            #[allow(unused_mut)]
            let mut app = app
                .app_data(web::Data::new(app_data))
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
                .service(moosicbox_qobuz::api::track_endpoint);

            #[cfg(feature = "upnp")]
            {
                app = app.service(moosicbox_upnp::api::scan_devices_endpoint);
            }

            app
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
                if let Some(x) = CHAT_SERVER_HANDLE.write().unwrap().take() {
                    x.shutdown();
                }

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

                log::debug!("Shutting down PlaybackEventHandler...");
                PLAYBACK_EVENT_HANDLER.shutdown();

                log::debug!("Shutting down server players...");
                SERVER_PLAYER
                    .write()
                    .await
                    .drain()
                    .for_each(|(id, player)| {
                        log::debug!("Shutting down player id={}", id);
                        if let Err(err) = player.stop() {
                            log::error!("Failed to stop player id={}: {err:?}", id);
                        } else {
                            log::debug!("Successfully shut down player id={}", id);
                        }
                    });

                #[cfg(feature = "upnp")]
                {
                    use moosicbox_upnp::listener::UpnpCommander as _;

                    log::debug!("Shutting down UpnpListener...");
                    if let Err(e) = upnp_service_handle.shutdown() {
                        log::error!("Failed to shut down UpnpListener: {e:?}");
                    }
                }

                log::trace!("Connections closed");
                resp
            },
            async move {
                let resp = chat_server_handle
                    .await
                    .expect("Failed to shut down chat server");
                log::debug!("Ws server connection closed");
                resp
            },
            async move {
                let resp = playback_event_handle
                    .await
                    .expect("Failed to shut down playback event handler");
                log::debug!("PlaybackEventHandler connection closed");
                resp
            },
            async move {
                #[cfg(feature = "upnp")]
                {
                    let resp = join_upnp_service
                        .await
                        .expect("Failed to shut down UPnP service")
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
                    log::debug!("UPnP service closed");
                    resp
                }
                #[cfg(not(feature = "upnp"))]
                Ok(())
            },
        ) {
            log::error!("Error on shutdown: {err:?}");
            return Err(err);
        }

        log::debug!("Server shut down");

        Ok(())
    })
}

static SERVER_PLAYER: Lazy<tokio::sync::RwLock<HashMap<i32, moosicbox_player::player::Player>>> =
    Lazy::new(|| tokio::sync::RwLock::new(HashMap::new()));

fn handle_server_playback_update(
    update: &UpdateSession,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    let update = update.clone();

    Box::pin(async move {
        log::debug!("Handling server playback update");
        let updated = {
            let mut lock = SERVER_PLAYER.write().await;
            let entry = lock.entry(update.session_id);
            let updated = match entry {
                Entry::Occupied(occ_entry) => occ_entry.into_mut(),
                Entry::Vacant(vac_entry) => {
                    let db = {
                        let lock = DB.read().unwrap();
                        lock.clone().expect("No database")
                    };

                    let player = moosicbox_player::player::Player::new(PlayerSource::Local, None)
                        .init_from_session(&**db, update.session_id)
                        .await
                        .unwrap_or_else(|err| {
                            log::error!("Failed to create new player from session: {err:?}");
                            moosicbox_player::player::Player::new(PlayerSource::Local, None)
                        });

                    vac_entry.insert(player)
                }
            }
            .update_playback(
                update.play,
                update.stop,
                update.playing,
                update.position.map(|x| x.try_into().unwrap()),
                update.seek,
                update.volume,
                update.playlist.as_ref().map(|x| {
                    x.tracks
                        .iter()
                        .map(|t| TrackOrId::Id(t.id.try_into().unwrap(), t.r#type))
                        .collect::<Vec<_>>()
                }),
                None,
                Some(update.session_id.try_into().unwrap()),
                None,
                Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
            );
            drop(lock);
            updated
        };

        match updated {
            Ok(status) => {
                log::debug!("Updated server player playback: {status:?}");
            }
            Err(err) => {
                log::error!("Failed to update server player playback: {err:?}");
            }
        }
    })
}

async fn register_server_player(
    db: &dyn Database,
    mut ws: ChatServerHandle,
    tunnel_handle: &Option<TunnelSenderHandle>,
) -> Result<(), WebsocketSendError> {
    let connection_id = "self";

    let context = WebsocketContext {
        connection_id: connection_id.to_string(),
        ..Default::default()
    };
    let payload = RegisterConnection {
        connection_id: connection_id.to_string(),
        name: "MoosicBox Server".to_string(),
        players: vec![RegisterPlayer {
            name: "MoosicBox Server".into(),
            r#type: "SYMPHONIA".into(),
        }],
    };

    let handle = CHAT_SERVER_HANDLE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .ok_or(WebsocketSendError::Unknown("No chat server handle".into()))?;

    let connection = moosicbox_ws::register_connection(db, &handle, &context, &payload).await?;

    let player = connection
        .players
        .first()
        .ok_or(WebsocketSendError::Unknown(
            "No player on connection".into(),
        ))?;

    ws.add_player_action(player.id, handle_server_playback_update);

    if let Some(handle) = tunnel_handle {
        handle.add_player_action(player.id, handle_server_playback_update);
    }

    moosicbox_ws::get_sessions(db, &handle, &context, true).await
}
