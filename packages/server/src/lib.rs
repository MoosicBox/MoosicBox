#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

mod api;
#[cfg(feature = "static-token-auth")]
mod auth;
mod events;
#[cfg(feature = "player")]
mod players;
#[cfg(feature = "tunnel")]
mod tunnel;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App};
use moosicbox_config::{
    db::get_or_init_server_identity, get_app_config_dir_path, get_profile_dir_path, AppType,
};
use moosicbox_core::{app::AppState, sqlite::models::ApiSource};
use moosicbox_database::{config::ConfigDatabase, profiles::LibraryDatabase, Database};
use moosicbox_files::files::track_pool::service::Commander as _;
use moosicbox_music_api::{MusicApi, MusicApiState};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, LazyLock},
};
use tokio::try_join;
use tokio_util::sync::CancellationToken;

static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
#[cfg(feature = "upnp")]
static UPNP_LISTENER_HANDLE: std::sync::OnceLock<moosicbox_upnp::listener::Handle> =
    std::sync::OnceLock::new();

static WS_SERVER_HANDLE: LazyLock<tokio::sync::RwLock<Option<ws::server::WsServerHandle>>> =
    LazyLock::new(|| tokio::sync::RwLock::new(None));

#[allow(clippy::type_complexity)]
static CONFIG_DB: LazyLock<std::sync::RwLock<Option<ConfigDatabase>>> =
    LazyLock::new(|| std::sync::RwLock::new(None));

#[allow(clippy::type_complexity)]
static DB: LazyLock<std::sync::RwLock<Option<LibraryDatabase>>> =
    LazyLock::new(|| std::sync::RwLock::new(None));

#[allow(clippy::type_complexity)]
static MUSIC_API_STATE: LazyLock<std::sync::RwLock<Option<MusicApiState>>> =
    LazyLock::new(|| std::sync::RwLock::new(None));

#[cfg(feature = "library")]
#[allow(clippy::type_complexity)]
static LIBRARY_API_STATE: LazyLock<
    std::sync::RwLock<Option<moosicbox_library::LibraryMusicApiState>>,
> = LazyLock::new(|| std::sync::RwLock::new(None));

static SERVER_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[must_use]
pub fn get_config_db_dir_path(app_type: AppType) -> Option<PathBuf> {
    get_app_config_dir_path(app_type).map(|x| x.join("db"))
}

#[must_use]
pub fn make_config_db_dir_path(app_type: AppType) -> Option<PathBuf> {
    if let Some(path) = get_config_db_dir_path(app_type) {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path.join("config.db"));
        }
    }

    None
}

#[must_use]
pub fn get_profile_db_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    get_profile_dir_path(app_type, profile).map(|x| x.join("db"))
}

#[must_use]
pub fn make_profile_db_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf> {
    if let Some(path) = get_profile_db_dir_path(app_type, profile) {
        if path.is_dir() || std::fs::create_dir_all(&path).is_ok() {
            return Some(path.join("library.db"));
        }
    }

    None
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::missing_panics_doc)]
#[allow(clippy::missing_errors_doc)]
pub async fn run(
    #[allow(unused)] app_type: AppType,
    addr: &str,
    service_port: u16,
    actix_workers: Option<usize>,
    #[cfg(feature = "player")] local_players: bool,
    #[cfg(feature = "upnp")] upnp_players: bool,
    on_startup: impl FnOnce() + Send,
) -> std::io::Result<()> {
    #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
    let config_db_profile_path = {
        let path = make_config_db_dir_path(app_type).expect("Failed to get DB config path");

        let path_str = path.to_str().expect("Failed to get DB path_str");
        if let Err(e) = moosicbox_schema::migrate_config(path_str) {
            moosicbox_assert::die_or_panic!("Failed to migrate database: {e:?}");
        };

        path
    };

    let config_db = moosicbox_database_connection::init(
        #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
        &config_db_profile_path,
        None,
    )
    .await
    .expect("Failed to initialize database");

    #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
    let library_db_profile_path = {
        let profile = "master";
        let path =
            make_profile_db_dir_path(app_type, profile).expect("Failed to get DB profile path");

        let path_str = path.to_str().expect("Failed to get DB path_str");
        if let Err(e) = moosicbox_schema::migrate_library(path_str) {
            moosicbox_assert::die_or_panic!("Failed to migrate database: {e:?}");
        };

        path
    };

    let library_db = moosicbox_database_connection::init(
        #[cfg(all(not(feature = "postgres"), feature = "sqlite"))]
        &library_db_profile_path,
        None,
    )
    .await
    .expect("Failed to initialize database");

    let config_database: Arc<Box<dyn Database>> = Arc::new(config_db);
    let config_database = ConfigDatabase {
        database: config_database,
    };

    CONFIG_DB.write().unwrap().replace(config_database.clone());

    SERVER_ID
        .set(
            get_or_init_server_identity(&config_database)
                .await
                .expect("Failed to get or init server identity"),
        )
        .expect("Failed to set SERVER_ID");

    moosicbox_database::config::init(config_database.clone().into()).unwrap();

    let library_database: Arc<Box<dyn Database>> = Arc::new(library_db);

    let library_database =
        moosicbox_database::profiles::PROFILES.fetch_add("master", library_database.clone());

    DB.write().unwrap().replace(library_database.clone());

    #[cfg(feature = "library")]
    let library_music_api = moosicbox_library::LibraryMusicApi::new(library_database.clone());
    #[cfg(feature = "library")]
    let library_api_state = moosicbox_library::LibraryMusicApiState::new(library_music_api.clone());

    #[allow(unused_mut)]
    let mut apis_map: HashMap<ApiSource, Box<dyn MusicApi>> = HashMap::new();
    #[cfg(feature = "library")]
    apis_map.insert(
        ApiSource::Library,
        Box::new(moosicbox_music_api::CachedMusicApi::new(library_music_api)),
    );
    #[cfg(feature = "tidal")]
    apis_map.insert(
        ApiSource::Tidal,
        Box::new(moosicbox_music_api::CachedMusicApi::new(
            moosicbox_tidal::TidalMusicApi::new(library_database.clone()),
        )),
    );
    #[cfg(feature = "qobuz")]
    apis_map.insert(
        ApiSource::Qobuz,
        Box::new(moosicbox_music_api::CachedMusicApi::new(
            moosicbox_qobuz::QobuzMusicApi::new(library_database.clone()),
        )),
    );
    #[cfg(feature = "yt")]
    apis_map.insert(
        ApiSource::Yt,
        Box::new(moosicbox_music_api::CachedMusicApi::new(
            moosicbox_yt::YtMusicApi::new(library_database.clone()),
        )),
    );
    let music_api_state = MusicApiState::new(apis_map);
    MUSIC_API_STATE
        .write()
        .unwrap()
        .replace(music_api_state.clone());

    #[cfg(feature = "library")]
    LIBRARY_API_STATE
        .write()
        .unwrap()
        .replace(library_api_state.clone());

    #[cfg(feature = "tunnel")]
    let (mut ws_server, server_tx) = ws::server::WsServer::new(config_database.clone());
    #[cfg(not(feature = "tunnel"))]
    let (ws_server, server_tx) = ws::server::WsServer::new(config_database.clone());
    #[cfg(feature = "player")]
    let handle = server_tx.clone();
    WS_SERVER_HANDLE.write().await.replace(server_tx);

    #[cfg(feature = "player")]
    {
        moosicbox_player::set_service_port(service_port);
        moosicbox_player::on_playback_event(crate::events::playback_event::on_event);
    }

    #[cfg(feature = "downloader")]
    events::download_event::init().await;

    #[cfg(feature = "scan")]
    events::scan_event::init().await;

    events::audio_zone_event::init(&config_database).await;
    events::session_event::init().await;

    #[cfg(feature = "tunnel")]
    let (tunnel_host, tunnel_join_handle, tunnel_handle) = crate::tunnel::setup_tunnel(
        config_database.clone(),
        music_api_state.clone(),
        service_port,
    )
    .await
    .expect("Failed to setup tunnel connection");

    #[cfg(feature = "tunnel")]
    if let Some(ref tunnel_handle) = tunnel_handle {
        ws_server.add_sender(Box::new(tunnel_handle.clone()));
    }

    let ws_server_handle = moosicbox_task::spawn("server: WsServer", ws_server.run());

    let (track_pool_handle, track_pool_join_handle) = {
        use moosicbox_files::files::track_pool::{service::Service, Context, HANDLE};

        let service = Service::new(Context::new());
        let handle = service.handle();
        let join_handle = service.start();

        HANDLE
            .set(handle.clone())
            .unwrap_or_else(|_| panic!("Failed to set TrackPool HANDLE"));

        (handle, join_handle)
    };

    #[cfg(feature = "upnp")]
    let (upnp_service_handle, join_upnp_service) = if upnp_players {
        let upnp_service =
            moosicbox_upnp::listener::Service::new(moosicbox_upnp::listener::UpnpContext::new());
        let upnp_service_handle = upnp_service.handle();
        let join_upnp_service = upnp_service.start();

        UPNP_LISTENER_HANDLE
            .set(upnp_service_handle.clone())
            .unwrap_or_else(|_| panic!("Failed to set UPNP_LISTENER_HANDLE"));

        #[cfg(feature = "upnp")]
        moosicbox_task::spawn(
            "server: register upnp players",
            players::upnp::init(
                handle.clone(),
                #[cfg(feature = "tunnel")]
                tunnel_handle.clone(),
            ),
        );

        (Some(upnp_service_handle), Some(join_upnp_service))
    } else {
        (None, None)
    };

    #[cfg(feature = "player")]
    let (playback_event_handle, playback_join_handle) = if local_players {
        let playback_event_service = events::playback_event::service::Service::new(
            events::playback_event::Context::new(handle.clone()),
        );
        let playback_event_handle = playback_event_service.handle();
        let playback_join_handle = playback_event_service
            .with_name("PlaybackEventService")
            .start();
        events::playback_event::PLAYBACK_EVENT_HANDLE
            .set(playback_event_handle.clone())
            .unwrap_or_else(|_| panic!("Failed to set PLAYBACK_EVENT_HANDLE"));

        let config_database = config_database.clone();
        let library_database = library_database.clone();
        #[cfg(feature = "tunnel")]
        let tunnel_handle = tunnel_handle.clone();

        moosicbox_task::spawn("server: scan outputs", async move {
            players::local::init(
                &config_database,
                &library_database,
                #[cfg(feature = "tunnel")]
                tunnel_handle,
            )
            .await
        });

        (Some(playback_event_handle), Some(playback_join_handle))
    } else {
        (None, None)
    };

    #[cfg(feature = "openapi")]
    let openapi = api::openapi::init();

    let app = {
        move || {
            let app_data = AppState {
                #[cfg(feature = "tunnel")]
                tunnel_host: tunnel_host.clone(),
                #[cfg(not(feature = "tunnel"))]
                tunnel_host: None,
                service_port,
            };

            let music_api_state = MUSIC_API_STATE.read().unwrap().as_ref().unwrap().clone();

            #[cfg(feature = "library")]
            let library_api_state = LIBRARY_API_STATE.read().unwrap().as_ref().unwrap().clone();

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

            let app = App::new().wrap(cors);

            #[cfg(feature = "static-token-auth")]
            let app = app.wrap(crate::auth::StaticTokenAuth::new(
                std::env!("STATIC_TOKEN").into(),
            ));

            let app = app
                .wrap(middleware::Compress::default())
                .wrap(moosicbox_middleware::api_logger::ApiLogger::default())
                .app_data(web::Data::new(app_data))
                .app_data(web::Data::new(music_api_state))
                .service(api::health_endpoint)
                .service(api::websocket);

            #[cfg(feature = "library")]
            let app = app.app_data(web::Data::new(library_api_state));

            #[cfg(feature = "openapi")]
            let app = app.service(api::openapi::bind_services(
                web::scope("/openapi"),
                &openapi,
            ));

            #[cfg(feature = "admin-htmx-api")]
            let app = app.wrap(actix_htmx::HtmxMiddleware {});

            #[cfg(feature = "admin-htmx-api")]
            let app = app.service(moosicbox_admin_htmx::api::bind_services(web::scope(
                "/admin",
            )));

            #[cfg(feature = "audio-output-api")]
            let app = app.service(moosicbox_audio_output::api::bind_services(web::scope(
                "/audio-output",
            )));

            #[cfg(feature = "audio-zone-api")]
            let app = app.service(moosicbox_audio_zone::api::bind_services(web::scope(
                "/audio-zone",
            )));

            #[cfg(feature = "auth-api")]
            let app = app.service(moosicbox_auth::api::bind_services(web::scope("/auth")));

            #[cfg(feature = "config-api")]
            let app = app.service(moosicbox_config::api::bind_services(web::scope("/config")));

            #[cfg(feature = "downloader-api")]
            let app = app.service(moosicbox_downloader::api::bind_services(web::scope(
                "/downloader",
            )));

            #[cfg(feature = "files-api")]
            let app = app.service(moosicbox_files::api::bind_services(web::scope("/files")));

            #[cfg(feature = "menu-api")]
            let app = app.service(moosicbox_menu::api::bind_services(web::scope("/menu")));

            #[cfg(feature = "player-api")]
            let app = app.service(moosicbox_player::api::bind_services(web::scope("/player")));

            #[cfg(feature = "search-api")]
            let app = app.service(moosicbox_search::api::bind_services(web::scope("/search")));

            #[cfg(feature = "library-api")]
            let app = app.service(moosicbox_library::api::bind_services(web::scope(
                "/library",
            )));

            #[cfg(feature = "tidal-api")]
            let app = app.service(moosicbox_tidal::api::bind_services(web::scope("/tidal")));

            #[cfg(feature = "qobuz-api")]
            let app = app.service(moosicbox_qobuz::api::bind_services(web::scope("/qobuz")));

            #[cfg(feature = "session-api")]
            let app = app.service(moosicbox_session::api::bind_services(web::scope(
                "/session",
            )));

            #[cfg(feature = "scan-api")]
            let app = app.service(moosicbox_scan::api::bind_services(web::scope("/scan")));

            #[cfg(feature = "upnp-api")]
            let app = app.service(moosicbox_upnp::api::bind_services(web::scope("/upnp")));

            #[cfg(feature = "yt-api")]
            let app = app.service(moosicbox_yt::api::bind_services(web::scope("/yt")));

            app
        }
    };

    let http_server = {
        let mut http_server = actix_web::HttpServer::new(app);

        #[cfg(feature = "tls")]
        {
            use std::io::Write as _;

            use openssl::ssl::{SslAcceptor, SslMethod};

            let config_dir =
                moosicbox_config::get_config_dir_path().expect("Failed to get config dir");

            let tls_dir = config_dir.join("tls");
            let cert_path = tls_dir.join("cert.pem");
            let key_path = tls_dir.join("key.pem");

            if !tls_dir.is_dir() {
                std::fs::create_dir_all(&tls_dir).expect("Failed to create tls dir");
            }

            if !cert_path.is_file() || !key_path.is_file() {
                use rcgen::{generate_simple_self_signed, CertifiedKey};

                let subject_alt_names = vec!["localhost".to_string()];

                let CertifiedKey { cert, key_pair } =
                    generate_simple_self_signed(subject_alt_names).unwrap();

                let mut cert_file = std::fs::OpenOptions::new()
                    .create(true) // To create a new file
                    .truncate(true)
                    .write(true)
                    .open(&cert_path)
                    .unwrap();
                cert_file
                    .write_all(cert.pem().as_bytes())
                    .expect("Failed to create cert file");

                let mut key_file = std::fs::OpenOptions::new()
                    .create(true) // To create a new file
                    .truncate(true)
                    .write(true)
                    .open(&key_path)
                    .unwrap();
                key_file
                    .write_all(key_pair.serialize_pem().as_bytes())
                    .expect("Failed to create key file");
            }

            let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();

            builder
                .set_private_key_file(&key_path, openssl::ssl::SslFiletype::PEM)
                .unwrap();

            builder.set_certificate_chain_file(&cert_path).unwrap();

            http_server = http_server.bind_openssl((addr, service_port), builder)?;
        }
        #[cfg(not(feature = "tls"))]
        {
            http_server = http_server.bind((addr, service_port))?;
        }

        if let Some(workers) = actix_workers {
            log::debug!("Running with {workers} Actix workers");
            http_server = http_server.workers(workers);
        }

        moosicbox_task::spawn("server: ctrl-c", async move {
            tokio::signal::ctrl_c().await?;
            log::debug!("Received ctrl-c");
            Ok::<_, std::io::Error>(())
        });

        http_server.run()
    };

    let ip = local_ip_address::local_ip().map_or_else(
        |e| {
            moosicbox_assert::die_or_warn!("Failed to get local ip address: {e:?}");
            "127.0.0.1".to_string()
        },
        |x| x.to_string(),
    );

    if let Err(e) =
        moosicbox_mdns::register_service(SERVER_ID.get().expect("No SERVER_ID"), &ip, service_port)
    {
        moosicbox_assert::die_or_error!("Failed to register mdns service: {e:?}");
    }

    on_startup();

    let config_db = config_database.clone();
    let library_db = library_database.clone();

    if let Err(err) = try_join!(
        async move {
            let resp = http_server.await;

            #[cfg(feature = "player")]
            {
                log::debug!("Shutting down server players...");
                let players = players::local::SERVER_PLAYERS
                    .write()
                    .await
                    .drain()
                    .collect::<Vec<_>>();
                for (id, (_, mut player)) in players {
                    log::debug!("Shutting down player id={}", id);
                    if let Err(err) = player
                        .update_playback(
                            true,
                            None,
                            Some(true),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            true,
                            None,
                        )
                        .await
                    {
                        log::error!("Failed to stop player id={}: {err:?}", id);
                    } else {
                        log::debug!("Successfully shut down player id={}", id);
                    }
                }
            }

            #[cfg(feature = "upnp")]
            {
                log::debug!("Shutting down UPnP players...");
                let players = {
                    let mut binding = players::upnp::UPNP_PLAYERS.write().await;
                    binding.drain(..).collect::<Vec<_>>()
                };

                for (_output, upnp_player, mut player) in players {
                    log::debug!("Shutting down player id={}", upnp_player.id);
                    if let Err(err) = player
                        .update_playback(
                            true,
                            None,
                            Some(true),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            true,
                            None,
                        )
                        .await
                    {
                        log::error!("Failed to stop player id={}: {err:?}", upnp_player.id);
                    } else {
                        log::debug!("Successfully shut down player id={}", upnp_player.id);
                    }
                }
            }

            log::debug!("Shutting down ws server...");
            let server = WS_SERVER_HANDLE.write().await.take();
            if let Some(x) = server {
                x.shutdown();
            }

            log::debug!("Shutting down db client...");
            DB.write().unwrap().take();

            log::debug!("Cancelling scan...");
            #[cfg(feature = "scan")]
            moosicbox_scan::cancel();
            CANCELLATION_TOKEN.cancel();

            #[cfg(feature = "tunnel")]
            if let Some(handle) = tunnel_handle {
                log::debug!("Closing tunnel connection...");
                let _ = handle.close().await;
            }

            #[cfg(feature = "tunnel")]
            if let Some(handle) = tunnel_join_handle {
                log::debug!("Closing tunnel join handle connection...");
                handle.await.unwrap();
            } else {
                log::trace!("No tunnel handle connection to close");
            }

            {
                log::debug!("Closing config database connection...");
                if let Err(e) = config_db.close().await {
                    log::error!("Failed to shut down database connection: {e:?}");
                }
            }
            {
                log::debug!("Closing library database connection...");
                if let Err(e) = library_db.close().await {
                    log::error!("Failed to shut down database connection: {e:?}");
                }
            }

            #[cfg(feature = "player")]
            if let Some(playback_event_handle) = playback_event_handle {
                use crate::events::playback_event::service::Commander as _;

                log::debug!("Shutting down PlaybackEventHandler...");
                if let Err(e) = playback_event_handle.shutdown() {
                    log::error!("Failed to shut down PlaybackEventHandler: {e:?}");
                }
            }

            log::debug!("Shutting down TrackPool...");
            if let Err(e) = track_pool_handle.shutdown() {
                log::error!("Failed to shut down TrackPool: {e:?}");
            }

            #[cfg(feature = "upnp")]
            if let Some(upnp_service_handle) = upnp_service_handle {
                use moosicbox_upnp::listener::Commander as _;

                log::debug!("Shutting down UpnpListener...");
                if let Err(e) = upnp_service_handle.shutdown() {
                    log::error!("Failed to shut down UpnpListener: {e:?}");
                }
            }

            log::trace!("Connections closed");
            resp
        },
        async move {
            let resp = ws_server_handle
                .await
                .expect("Failed to shut down ws server");
            log::debug!("Ws server connection closed");
            resp
        },
        async move {
            #[cfg(feature = "player")]
            if let Some(playback_join_handle) = playback_join_handle {
                let resp = playback_join_handle
                    .await
                    .expect("Failed to shut down playback event handler")
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
                log::debug!("PlaybackEventHandler connection closed");
                resp
            } else {
                Ok(())
            }
            #[cfg(not(feature = "player"))]
            Ok(())
        },
        async move {
            let resp = track_pool_join_handle
                .await
                .expect("Failed to shut down track_pool event handler")
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
            log::debug!("PlaybackEventHandler connection closed");
            resp
        },
        async move {
            #[cfg(feature = "upnp")]
            if let Some(join_upnp_service) = join_upnp_service {
                let resp = join_upnp_service
                    .await
                    .expect("Failed to shut down UPnP service")
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
                log::debug!("UPnP service closed");
                resp
            } else {
                Ok(())
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
}
