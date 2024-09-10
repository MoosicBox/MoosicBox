#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

mod api;
#[cfg(feature = "static-token-auth")]
mod auth;
mod db;
mod events;
mod tunnel;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App};
use moosicbox_config::db::get_or_init_server_identity;
use moosicbox_core::{app::AppState, sqlite::models::ApiSource};
use moosicbox_database::Database;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_files::files::track_pool::service::Commander as _;
use moosicbox_music_api::{MusicApi, MusicApiState};
use once_cell::sync::Lazy;
use std::{collections::HashMap, env, sync::Arc};
use tokio::try_join;
use tokio_util::sync::CancellationToken;
use ws::server::WsServer;

#[cfg(feature = "player")]
use crate::events::playback_event::{service::Commander, PLAYBACK_EVENT_HANDLE};

static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);
#[cfg(feature = "upnp")]
static UPNP_LISTENER_HANDLE: std::sync::OnceLock<moosicbox_upnp::listener::Handle> =
    std::sync::OnceLock::new();

static WS_SERVER_HANDLE: Lazy<tokio::sync::RwLock<Option<ws::server::WsServerHandle>>> =
    Lazy::new(|| tokio::sync::RwLock::new(None));

#[allow(clippy::type_complexity)]
static DB: Lazy<std::sync::RwLock<Option<Arc<Box<dyn Database>>>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

#[allow(clippy::type_complexity)]
static MUSIC_API_STATE: Lazy<std::sync::RwLock<Option<MusicApiState>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

#[cfg(feature = "library")]
#[allow(clippy::type_complexity)]
static LIBRARY_API_STATE: Lazy<std::sync::RwLock<Option<moosicbox_library::LibraryMusicApiState>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

static SERVER_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    if std::env::var("TOKIO_CONSOLE") == Ok("1".to_string()) {
        console_subscriber::init();
    } else {
        moosicbox_logging::init(Some("moosicbox_server.log"))
            .expect("Failed to initialize FreeLog");
    }

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
        let db_profile_dir_path = moosicbox_config::make_db_profile_dir_path("master")
            .expect("Failed to get DB profile dir path");
        let db_profile_path = db_profile_dir_path.join("library.db");
        let db_profile_path_str = db_profile_path
            .to_str()
            .expect("Failed to get DB profile path");
        if let Err(e) = moosicbox_schema::migrate_library(db_profile_path_str) {
            moosicbox_assert::die_or_panic!("Failed to migrate database: {e:?}");
        };

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
        #[cfg(feature = "sqlite-rusqlite")]
        #[allow(unused_variables)]
        let db = db::init_sqlite(&db_profile_path).expect("Failed to init sqlite DB");
        #[cfg(all(
            not(feature = "postgres"),
            not(feature = "postgres-sqlx"),
            not(feature = "sqlite-rusqlite")
        ))]
        #[allow(unused_variables)]
        let db = db::init_sqlite_sqlx(&db_profile_path)
            .await
            .expect("Failed to init sqlite DB");

        SERVER_ID
            .set(
                get_or_init_server_identity(&*db)
                    .await
                    .expect("Failed to get or init server identity"),
            )
            .expect("Failed to set SERVER_ID");

        let database: Arc<Box<dyn Database>> = Arc::new(db);
        DB.write().unwrap().replace(database.clone());

        #[cfg(feature = "library")]
        let library_music_api = moosicbox_library::LibraryMusicApi::new(database.clone());
        #[cfg(feature = "library")]
        let library_api_state =
            moosicbox_library::LibraryMusicApiState::new(library_music_api.clone());

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
                moosicbox_tidal::TidalMusicApi::new(database.clone()),
            )),
        );
        #[cfg(feature = "qobuz")]
        apis_map.insert(
            ApiSource::Qobuz,
            Box::new(moosicbox_music_api::CachedMusicApi::new(
                moosicbox_qobuz::QobuzMusicApi::new(database.clone()),
            )),
        );
        #[cfg(feature = "yt")]
        apis_map.insert(
            ApiSource::Yt,
            Box::new(moosicbox_music_api::CachedMusicApi::new(
                moosicbox_yt::YtMusicApi::new(database.clone()),
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

        let (mut ws_server, server_tx) = WsServer::new(database.clone());
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

        events::audio_zone_event::init(database.clone()).await;
        events::session_event::init(database.clone()).await;

        #[cfg(feature = "player")]
        let playback_event_service = events::playback_event::service::Service::new(
            events::playback_event::Context::new(handle.clone()),
        );
        #[cfg(feature = "player")]
        let playback_event_handle = playback_event_service.handle();
        #[cfg(feature = "player")]
        let playback_join_handle = playback_event_service
            .with_name("PlaybackEventService")
            .start();
        #[cfg(feature = "player")]
        PLAYBACK_EVENT_HANDLE
            .set(playback_event_handle.clone())
            .unwrap_or_else(|_| panic!("Failed to set PLAYBACK_EVENT_HANDLE"));

        #[cfg(feature = "postgres-raw")]
        let db_connection_handle = moosicbox_task::spawn("server: postgres", db_connection);

        let (tunnel_host, tunnel_join_handle, tunnel_handle) =
            crate::tunnel::setup_tunnel(database.clone(), music_api_state.clone(), service_port)
                .await
                .expect("Failed to setup tunnel connection");

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
        let upnp_service =
            moosicbox_upnp::listener::Service::new(moosicbox_upnp::listener::UpnpContext::new());

        #[cfg(feature = "upnp")]
        let upnp_service_handle = upnp_service.handle();
        #[cfg(feature = "upnp")]
        let join_upnp_service = upnp_service.start();
        #[cfg(feature = "upnp")]
        UPNP_LISTENER_HANDLE
            .set(upnp_service_handle.clone())
            .unwrap_or_else(|_| panic!("Failed to set UPNP_LISTENER_HANDLE"));

        #[cfg(feature = "player")]
        moosicbox_task::spawn("server: scan outputs", {
            let db = database.clone();
            let tunnel_handle = tunnel_handle.clone();
            async move {
                moosicbox_audio_output::scan_outputs()
                    .await
                    .map_err(|e| e.to_string())?;

                let handle = WS_SERVER_HANDLE
                    .read()
                    .await
                    .clone()
                    .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
                        "No ws server handle".into(),
                    ))
                    .map_err(|e| e.to_string())?;

                for audio_output in moosicbox_audio_output::output_factories().await {
                    if let Err(err) = register_server_player(
                        &**db,
                        handle.clone(),
                        &tunnel_handle,
                        audio_output.clone(),
                    )
                    .await
                    {
                        log::error!("Failed to register server player: {err:?}");
                    } else {
                        log::debug!("Registered server player audio_output={audio_output:?}");
                    }
                }

                Ok::<_, String>(())
            }
        });

        #[cfg(feature = "upnp")]
        {
            moosicbox_task::spawn("server: register upnp players", {
                let db = database.clone();
                let tunnel_handle = tunnel_handle.clone();
                async move {
                    load_upnp_players().await.map_err(|e| e.to_string())?;

                    let upnp_players = {
                        let binding = UPNP_PLAYERS.read().await;
                        binding.iter().cloned().collect::<Vec<_>>()
                    };

                    log::debug!("register_upnp_player: players={}", upnp_players.len());

                    for (output, _player, _) in upnp_players {
                        if let Err(err) =
                            register_upnp_player(&**db, handle.clone(), &tunnel_handle, output)
                                .await
                        {
                            log::error!("Failed to register server player: {err:?}");
                        } else {
                            log::debug!("Registered server player");
                        }
                    }

                    Ok::<_, String>(())
                }
            });
        }

        #[cfg(feature = "openapi")]
        let openapi = api::openapi::init();

        let app = move || {
            let app_data = AppState {
                tunnel_host: tunnel_host.clone(),
                service_port,
                database: database.clone(),
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
                    http::header::HeaderName::from_static("hx-boosted"),
                    http::header::HeaderName::from_static("hx-current-url"),
                    http::header::HeaderName::from_static("hx-history-restore-request"),
                    http::header::HeaderName::from_static("hx-prompt"),
                    http::header::HeaderName::from_static("hx-request"),
                    http::header::HeaderName::from_static("hx-target"),
                    http::header::HeaderName::from_static("hx-trigger-name"),
                    http::header::HeaderName::from_static("hx-trigger"),
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
            let app = app.service(api::openapi::bind_services(web::scope("/"), &openapi));

            #[cfg(feature = "admin-htmx-api")]
            let app = app.service(moosicbox_admin_htmx::api::bind_services(web::scope(
                "/admin",
            )));

            #[cfg(feature = "audio-output-api")]
            let app = app.service(
                web::scope("/audio-output")
                    .service(moosicbox_audio_output::api::audio_outputs_endpoint),
            );

            #[cfg(feature = "audio-zone-api")]
            let app = app.service(
                web::scope("/audio-zone")
                    .service(moosicbox_audio_zone::api::audio_zones_endpoint)
                    .service(moosicbox_audio_zone::api::audio_zone_with_sessions_endpoint)
                    .service(moosicbox_audio_zone::api::create_audio_zone_endpoint)
                    .service(moosicbox_audio_zone::api::update_audio_zone_endpoint)
                    .service(moosicbox_audio_zone::api::delete_audio_zone_endpoint),
            );

            #[cfg(feature = "auth-api")]
            let app = app.service(
                web::scope("/auth")
                    .service(moosicbox_auth::api::get_magic_token_endpoint)
                    .service(moosicbox_auth::api::create_magic_token_endpoint),
            );

            #[cfg(feature = "downloader-api")]
            let app = app.service(
                web::scope("/downloader")
                    .service(moosicbox_downloader::api::download_endpoint)
                    .service(moosicbox_downloader::api::retry_download_endpoint)
                    .service(moosicbox_downloader::api::download_tasks_endpoint),
            );

            #[cfg(feature = "files-api")]
            let app = app.service(
                web::scope("/files")
                    .service(moosicbox_files::api::get_silence_endpoint)
                    .service(moosicbox_files::api::track_endpoint)
                    .service(moosicbox_files::api::track_visualization_endpoint)
                    .service(moosicbox_files::api::track_info_endpoint)
                    .service(moosicbox_files::api::tracks_info_endpoint)
                    .service(moosicbox_files::api::track_urls_endpoint)
                    .service(moosicbox_files::api::artist_source_artwork_endpoint)
                    .service(moosicbox_files::api::artist_cover_endpoint)
                    .service(moosicbox_files::api::album_source_artwork_endpoint)
                    .service(moosicbox_files::api::album_artwork_endpoint),
            );

            #[cfg(feature = "menu-api")]
            let app = app.service(
                web::scope("/menu")
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
                    .service(moosicbox_menu::api::get_artist_albums_endpoint),
            );

            #[cfg(feature = "player-api")]
            let app = app.service(
                web::scope("/player")
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
                    .service(moosicbox_player::api::player_status_endpoint),
            );

            #[cfg(feature = "search-api")]
            let app = app.service(
                web::scope("/search")
                    .service(moosicbox_search::api::search_global_search_endpoint)
                    .service(moosicbox_search::api::search_raw_global_search_endpoint),
            );

            #[cfg(feature = "library-api")]
            let app = app.service(
                web::scope("/library")
                    .service(moosicbox_library::api::track_file_url_endpoint)
                    .service(moosicbox_library::api::favorite_artists_endpoint)
                    .service(moosicbox_library::api::add_favorite_artist_endpoint)
                    .service(moosicbox_library::api::remove_favorite_artist_endpoint)
                    .service(moosicbox_library::api::favorite_albums_endpoint)
                    .service(moosicbox_library::api::add_favorite_album_endpoint)
                    .service(moosicbox_library::api::remove_favorite_album_endpoint)
                    .service(moosicbox_library::api::favorite_tracks_endpoint)
                    .service(moosicbox_library::api::add_favorite_track_endpoint)
                    .service(moosicbox_library::api::remove_favorite_track_endpoint)
                    .service(moosicbox_library::api::artist_albums_endpoint)
                    .service(moosicbox_library::api::album_tracks_endpoint)
                    .service(moosicbox_library::api::album_endpoint)
                    .service(moosicbox_library::api::artist_endpoint)
                    .service(moosicbox_library::api::track_endpoint)
                    .service(moosicbox_library::api::search_endpoint)
                    .service(moosicbox_library::api::reindex_endpoint),
            );

            #[cfg(feature = "tidal-api")]
            let app = app.service(
                web::scope("/tidal")
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
                    .service(moosicbox_tidal::api::search_endpoint),
            );

            #[cfg(feature = "qobuz-api")]
            let app = app.service(
                web::scope("/qobuz")
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
                    .service(moosicbox_qobuz::api::search_endpoint),
            );

            #[cfg(feature = "session-api")]
            let app = app.service(
                web::scope("/session")
                    .service(moosicbox_session::api::session_playlist_endpoint)
                    .service(moosicbox_session::api::session_playlist_tracks_endpoint)
                    .service(moosicbox_session::api::session_audio_zone_endpoint)
                    .service(moosicbox_session::api::session_playing_endpoint)
                    .service(moosicbox_session::api::session_endpoint)
                    .service(moosicbox_session::api::sessions_endpoint)
                    .service(moosicbox_session::api::register_players_endpoint),
            );

            #[cfg(feature = "scan-api")]
            let app = app.service(
                web::scope("/scan")
                    .service(moosicbox_scan::api::run_scan_endpoint)
                    .service(moosicbox_scan::api::start_scan_endpoint)
                    .service(moosicbox_scan::api::run_scan_path_endpoint)
                    .service(moosicbox_scan::api::get_scan_origins_endpoint)
                    .service(moosicbox_scan::api::enable_scan_origin_endpoint)
                    .service(moosicbox_scan::api::disable_scan_origin_endpoint)
                    .service(moosicbox_scan::api::get_scan_paths_endpoint)
                    .service(moosicbox_scan::api::add_scan_path_endpoint)
                    .service(moosicbox_scan::api::remove_scan_path_endpoint),
            );

            #[cfg(feature = "upnp-api")]
            let app = app.service(
                web::scope("/upnp")
                    .service(moosicbox_upnp::api::scan_devices_endpoint)
                    .service(moosicbox_upnp::api::get_transport_info_endpoint)
                    .service(moosicbox_upnp::api::get_media_info_endpoint)
                    .service(moosicbox_upnp::api::get_position_info_endpoint)
                    .service(moosicbox_upnp::api::get_volume_endpoint)
                    .service(moosicbox_upnp::api::set_volume_endpoint)
                    .service(moosicbox_upnp::api::subscribe_endpoint)
                    .service(moosicbox_upnp::api::pause_endpoint)
                    .service(moosicbox_upnp::api::play_endpoint)
                    .service(moosicbox_upnp::api::seek_endpoint),
            );

            #[cfg(feature = "yt-api")]
            let app = app.service(
                web::scope("/yt")
                    .service(moosicbox_yt::api::device_authorization_endpoint)
                    .service(moosicbox_yt::api::device_authorization_token_endpoint)
                    .service(moosicbox_yt::api::track_file_url_endpoint)
                    .service(moosicbox_yt::api::track_playback_info_endpoint)
                    .service(moosicbox_yt::api::favorite_artists_endpoint)
                    .service(moosicbox_yt::api::add_favorite_artist_endpoint)
                    .service(moosicbox_yt::api::remove_favorite_artist_endpoint)
                    .service(moosicbox_yt::api::favorite_albums_endpoint)
                    .service(moosicbox_yt::api::add_favorite_album_endpoint)
                    .service(moosicbox_yt::api::remove_favorite_album_endpoint)
                    .service(moosicbox_yt::api::favorite_tracks_endpoint)
                    .service(moosicbox_yt::api::add_favorite_track_endpoint)
                    .service(moosicbox_yt::api::remove_favorite_track_endpoint)
                    .service(moosicbox_yt::api::artist_albums_endpoint)
                    .service(moosicbox_yt::api::album_tracks_endpoint)
                    .service(moosicbox_yt::api::album_endpoint)
                    .service(moosicbox_yt::api::artist_endpoint)
                    .service(moosicbox_yt::api::track_endpoint)
                    .service(moosicbox_yt::api::search_endpoint),
            );

            app
        };

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

            http_server = http_server
                .bind_openssl((default_env("BIND_ADDR", "0.0.0.0"), service_port), builder)?;
        }
        #[cfg(not(feature = "tls"))]
        {
            http_server = http_server.bind((default_env("BIND_ADDR", "0.0.0.0"), service_port))?;
        }

        if let Ok(Some(workers)) = option_env_usize("ACTIX_WORKERS") {
            log::debug!("Running with {workers} Actix workers");
            http_server = http_server.workers(workers);
        }

        moosicbox_task::spawn("server: ctrl-c", async move {
            tokio::signal::ctrl_c().await?;
            log::debug!("Received ctrl-c");
            Ok::<_, std::io::Error>(())
        });

        let http_server = http_server.run();

        let ip = local_ip_address::local_ip().map_or_else(
            |e| {
                moosicbox_assert::die_or_warn!("Failed to get local ip address: {e:?}");
                "127.0.0.1".to_string()
            },
            |x| x.to_string(),
        );

        if let Err(e) = moosicbox_mdns::register_service(
            SERVER_ID.get().expect("No SERVER_ID"),
            &ip,
            service_port,
        ) {
            moosicbox_assert::die_or_error!("Failed to register mdns service: {e:?}");
        }

        if let Err(err) = try_join!(
            async move {
                let resp = http_server.await;

                #[cfg(feature = "player")]
                {
                    log::debug!("Shutting down server players...");
                    let players = SERVER_PLAYERS.write().await.drain().collect::<Vec<_>>();
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
                        let mut binding = UPNP_PLAYERS.write().await;
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

                #[cfg(feature = "player")]
                {
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
                {
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
                {
                    let resp = playback_join_handle
                        .await
                        .expect("Failed to shut down playback event handler")
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
                    log::debug!("PlaybackEventHandler connection closed");
                    resp
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

#[cfg(feature = "player")]
static SERVER_PLAYERS: Lazy<
    tokio::sync::RwLock<
        HashMap<
            u64,
            (
                moosicbox_player::local::LocalPlayer,
                moosicbox_player::PlaybackHandler,
            ),
        >,
    >,
> = Lazy::new(|| tokio::sync::RwLock::new(HashMap::new()));

#[cfg(feature = "player")]
#[allow(clippy::too_many_lines)]
fn handle_server_playback_update(
    update: &moosicbox_session::models::UpdateSession,
) -> std::pin::Pin<Box<dyn futures_util::Future<Output = ()> + Send>> {
    use moosicbox_core::sqlite::models::Id;
    use moosicbox_player::PlaybackHandler;
    use moosicbox_session::get_session;

    let update = update.clone();
    let db = DB.read().unwrap().clone().unwrap().clone();

    Box::pin(async move {
        log::debug!("Handling server playback update");

        let update = update;

        let updated = {
            {
                let audio_zone =
                    match moosicbox_session::get_session_audio_zone(&**db, update.session_id).await
                    {
                        Ok(players) => players,
                        Err(e) => moosicbox_assert::die_or_panic!(
                            "Failed to get session active players: {e:?}"
                        ),
                    };

                let Some(audio_zone) = audio_zone else {
                    return;
                };

                let existing = { SERVER_PLAYERS.read().await.get(&update.session_id).cloned() };
                let existing = existing.filter(|(player, _)| {
                    player.output.as_ref().is_some_and(|output| {
                        !audio_zone
                            .players
                            .iter()
                            .any(|p| p.audio_output_id != output.lock().unwrap().id)
                    })
                });

                if let Some((_, player)) = existing {
                    player
                } else {
                    let outputs = moosicbox_audio_output::output_factories().await;

                    // TODO: handle more than one output
                    let output = audio_zone
                        .players
                        .into_iter()
                        .find_map(|x| outputs.iter().find(|output| output.id == x.audio_output_id))
                        .cloned();

                    let Some(output) = output else {
                        moosicbox_assert::die_or_panic!("No output available");
                    };

                    let mut players = SERVER_PLAYERS.write().await;

                    let db = { DB.read().unwrap().clone().expect("No database") };

                    let local_player = match moosicbox_player::local::LocalPlayer::new(
                        moosicbox_player::PlayerSource::Local,
                        None,
                    )
                    .await
                    {
                        Ok(player) => player,
                        Err(e) => {
                            moosicbox_assert::die_or_panic!("Failed to create new player: {e:?}")
                        }
                    }
                    .with_output(output);

                    let playback = local_player.playback.clone();
                    let output = local_player.output.clone();
                    let receiver = local_player.receiver.clone();

                    let mut player = PlaybackHandler::new(local_player.clone())
                        .with_playback(playback)
                        .with_output(output)
                        .with_receiver(receiver);

                    local_player
                        .playback_handler
                        .write()
                        .unwrap()
                        .replace(player.clone());

                    if let Ok(Some(session)) = get_session(&**db, update.session_id).await {
                        if let Err(e) = player.init_from_session(session, &update).await {
                            moosicbox_assert::die_or_error!(
                                "Failed to create new player from session: {e:?}"
                            );
                        }
                    }

                    players.insert(update.session_id, (local_player, player.clone()));

                    player
                }
            }
            .update_playback(
                true,
                update.play,
                update.stop,
                update.playing,
                update.position,
                update.seek,
                update.volume,
                if let Some(playlist) = update.playlist {
                    let track_ids = playlist
                        .tracks
                        .iter()
                        .filter_map(|x| x.id.parse::<u64>().ok())
                        .map(std::convert::Into::into)
                        .collect::<Vec<Id>>();

                    let tracks = match moosicbox_library::db::get_tracks(&**db, Some(&track_ids))
                        .await
                    {
                        Ok(tracks) => tracks,
                        Err(e) => moosicbox_assert::die_or_panic!("Failed to get tracks: {e:?}"),
                    };

                    Some(
                        playlist
                            .tracks
                            .iter()
                            .map(|x| {
                                let data = if x.r#type == ApiSource::Library {
                                    tracks
                                        .iter()
                                        .find(|track| x.id == track.id.to_string())
                                        .and_then(|x| serde_json::to_value(x).ok())
                                } else {
                                    x.data.clone().and_then(|x| serde_json::to_value(x).ok())
                                };

                                moosicbox_player::Track {
                                    id: x.id.clone().into(),
                                    source: x.r#type,
                                    data,
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                },
                None,
                Some(update.session_id),
                Some(update.playback_target),
                false,
                Some(moosicbox_player::DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await
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

#[cfg(feature = "player")]
async fn register_server_player(
    db: &dyn Database,
    ws: ws::server::WsServerHandle,
    tunnel_handle: &Option<moosicbox_tunnel_sender::sender::TunnelSenderHandle>,
    audio_output: moosicbox_audio_output::AudioOutputFactory,
) -> Result<(), moosicbox_ws::WebsocketSendError> {
    let connection_id = "self";

    let context = moosicbox_ws::WebsocketContext {
        connection_id: connection_id.to_string(),
        ..Default::default()
    };
    let payload = moosicbox_session::models::RegisterConnection {
        connection_id: connection_id.to_string(),
        name: "MoosicBox Server".to_string(),
        players: vec![moosicbox_session::models::RegisterPlayer {
            name: audio_output.name,
            audio_output_id: audio_output.id.clone(),
        }],
    };

    let handle =
        WS_SERVER_HANDLE
            .read()
            .await
            .clone()
            .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
                "No ws server handle".into(),
            ))?;

    let connection = moosicbox_ws::register_connection(db, &handle, &context, &payload).await?;

    let player = connection
        .players
        .iter()
        .find(|x| x.audio_output_id == audio_output.id)
        .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
            "No player on connection".into(),
        ))?;

    ws.add_player_action(player.id, handle_server_playback_update)
        .await;

    if let Some(handle) = tunnel_handle {
        handle.add_player_action(player.id, handle_server_playback_update);
    }

    moosicbox_ws::get_sessions(db, &handle, &context, true).await
}

#[cfg(feature = "upnp")]
static UPNP_PLAYERS: Lazy<
    tokio::sync::RwLock<
        Vec<(
            moosicbox_audio_output::AudioOutputFactory,
            moosicbox_upnp::player::UpnpPlayer,
            moosicbox_player::PlaybackHandler,
        )>,
    >,
> = Lazy::new(|| tokio::sync::RwLock::new(vec![]));

#[cfg(feature = "upnp")]
static SESSION_UPNP_PLAYERS: Lazy<
    tokio::sync::RwLock<
        HashMap<
            u64,
            (
                moosicbox_audio_output::AudioOutputFactory,
                moosicbox_player::PlaybackHandler,
            ),
        >,
    >,
> = Lazy::new(|| tokio::sync::RwLock::new(HashMap::new()));

#[cfg(feature = "upnp")]
async fn load_upnp_players() -> Result<(), moosicbox_upnp::UpnpDeviceScannerError> {
    use moosicbox_audio_output::AudioOutputFactory;
    use moosicbox_player::{PlaybackHandler, PlayerSource};

    moosicbox_upnp::scan_devices().await?;

    {
        for device in moosicbox_upnp::devices().await {
            let mut players = UPNP_PLAYERS.write().await;

            if !players.iter().any(|(_, x, _)| x.device.udn() == device.udn) {
                let service_id = "urn:upnp-org:serviceId:AVTransport";
                if let Ok((device, service)) =
                    moosicbox_upnp::get_device_and_service(&device.udn, service_id)
                {
                    let player = moosicbox_upnp::player::UpnpPlayer::new(
                        Arc::new(Box::new(
                            MUSIC_API_STATE.read().unwrap().clone().unwrap().apis,
                        )),
                        device,
                        service,
                        PlayerSource::Local,
                        UPNP_LISTENER_HANDLE.get().unwrap().clone(),
                    );

                    let playback = player.playback.clone();
                    let receiver = player.receiver.clone();

                    let output: AudioOutputFactory = player
                        .clone()
                        .try_into()
                        .expect("Failed to create audio output factory for UpnpPlayer");

                    let handler = PlaybackHandler::new(player.clone())
                        .with_playback(playback)
                        .with_output(Some(Arc::new(std::sync::Mutex::new(output.clone()))))
                        .with_receiver(receiver);

                    player
                        .playback_handler
                        .write()
                        .unwrap()
                        .replace(handler.clone());

                    players.push((output.clone(), player.clone(), handler));
                }
            }
        }
    }

    Ok(())
}

#[cfg(feature = "upnp")]
fn handle_upnp_playback_update(
    update: &moosicbox_session::models::UpdateSession,
) -> std::pin::Pin<Box<dyn futures_util::Future<Output = ()> + Send>> {
    use moosicbox_player::{Track, DEFAULT_PLAYBACK_RETRY_OPTIONS};
    use moosicbox_session::get_session;

    let update = update.clone();
    let db = DB.read().unwrap().clone().unwrap().clone();

    Box::pin(async move {
        log::debug!("Handling UPnP playback update={update:?}");
        let updated = {
            {
                let existing = {
                    SESSION_UPNP_PLAYERS
                        .read()
                        .await
                        .get(&update.session_id)
                        .cloned()
                };
                let audio_output_ids = match update.audio_output_ids(&**db).await {
                    Ok(ids) => ids,
                    Err(e) => {
                        log::error!("Failed to get audio output IDs: {e:?}");
                        return;
                    }
                };
                let existing = existing
                    .filter(|(output, _)| !audio_output_ids.iter().any(|p| p != &output.id));

                if let Some((_, player)) = existing {
                    log::debug!(
                        "handle_upnp_playback_update: Using existing player for session_id={}",
                        update.session_id
                    );
                    player
                } else {
                    log::debug!(
                        "handle_upnp_playback_update: No existing player for session_id={}",
                        update.session_id
                    );
                    if let Err(e) = load_upnp_players().await {
                        log::error!("Failed to load upnp players: {e:?}");
                        return;
                    }

                    let binding = UPNP_PLAYERS.read().await;
                    let existing = binding
                        .iter()
                        .filter(|(output, _, _)| !audio_output_ids.iter().any(|p| p != &output.id));

                    // TODO: This needs to handle multiple players
                    if let Some((output, _upnp_player, player)) = existing.into_iter().next() {
                        let mut player = player.clone();
                        let output = output.clone();
                        drop(binding);

                        if let Ok(Some(session)) = get_session(&**db, update.session_id).await {
                            if let Err(e) = player.init_from_session(session, &update).await {
                                moosicbox_assert::die_or_error!(
                                    "Failed to create new player from session: {e:?}"
                                );
                            }

                            SESSION_UPNP_PLAYERS
                                .write()
                                .await
                                .insert(update.session_id, (output, player.clone()));
                        }

                        player
                    } else {
                        moosicbox_assert::die_or_panic!("No UPNP player found");
                    }
                }
            }
            .update_playback(
                true,
                update.play,
                update.stop,
                update.playing,
                update.position,
                update.seek,
                update.volume,
                update.playlist.as_ref().map(|x| {
                    x.tracks
                        .iter()
                        .map(|t| Track {
                            id: t.id.clone().into(),
                            source: t.r#type,
                            data: t.data.clone().and_then(|x| serde_json::to_value(x).ok()),
                        })
                        .collect::<Vec<_>>()
                }),
                None,
                Some(update.session_id),
                Some(update.playback_target),
                false,
                Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await
        };

        match updated {
            Ok(()) => {
                log::debug!("Updated UPnP player playback");
            }
            Err(err) => {
                log::error!("Failed to update UPnP player playback: {err:?}");
            }
        }
    })
}

#[cfg(feature = "upnp")]
#[allow(unused)]
async fn register_upnp_player(
    db: &dyn Database,
    ws: ws::server::WsServerHandle,
    tunnel_handle: &Option<moosicbox_tunnel_sender::sender::TunnelSenderHandle>,
    audio_output: moosicbox_audio_output::AudioOutputFactory,
) -> Result<(), moosicbox_ws::WebsocketSendError> {
    log::debug!("register_upnp_player: Registering audio_output={audio_output:?}");
    let connection_id = "self";

    let context = moosicbox_ws::WebsocketContext {
        connection_id: connection_id.to_string(),
        ..Default::default()
    };
    let payload = vec![moosicbox_session::models::RegisterPlayer {
        name: audio_output.name,
        audio_output_id: audio_output.id,
    }];

    let handle =
        WS_SERVER_HANDLE
            .read()
            .await
            .clone()
            .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
                "No ws server handle".into(),
            ))?;

    let players = moosicbox_ws::register_players(db, &handle, &context, &payload).await?;

    for player in players {
        ws.add_player_action(player.id, handle_upnp_playback_update)
            .await;

        if let Some(handle) = tunnel_handle {
            handle.add_player_action(player.id, handle_server_playback_update);
        }
    }

    moosicbox_ws::get_sessions(db, &handle, &context, true).await
}
