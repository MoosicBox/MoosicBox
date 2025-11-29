//! `MoosicBox` server application.
//!
//! This crate provides the main server implementation for `MoosicBox`, a music streaming and
//! management platform. It orchestrates various components including:
//!
//! * Audio playback and streaming
//! * Music library management with support for multiple sources (local, Tidal, Qobuz, `YouTube` Music)
//! * WebSocket communication for real-time updates
//! * Audio zone management for multi-room audio
//! * Player management (local and UPnP/DLNA devices)
//! * REST API endpoints for client applications
//! * Optional tunneling for remote access
//!
//! # Main Entry Points
//!
//! * [`run`] - Full-featured server startup with all configuration options
//! * [`run_basic`] - Simplified server startup with default settings
//!
//! # Examples
//!
//! ```rust,no_run
//! # use moosicbox_server::{run_basic};
//! # use moosicbox_config::AppType;
//! # async fn example() -> std::io::Result<()> {
//! // Start a basic server on localhost:8080
//! run_basic(
//!     AppType::App,
//!     "0.0.0.0",
//!     8080,
//!     None,
//!     |handle| {
//!         println!("Server started!");
//!         handle
//!     }
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! The server supports numerous optional features including:
//!
//! * `player` - Local audio playback support
//! * `upnp` - UPnP/DLNA player discovery and control
//! * `tunnel` - Remote access tunneling
//! * `telemetry` - Metrics and observability
//! * `sqlite` / `postgres` - Database backend options
//! * Music source integrations: `tidal`, `qobuz`, `yt`
//! * Audio format support: `format-flac`, `format-mp3`, `format-aac`, `format-opus`

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod api;
#[cfg(feature = "static-token-auth")]
mod auth;
#[cfg(feature = "sqlite")]
pub(crate) mod db;
mod events;
#[cfg(feature = "player")]
mod players;
#[cfg(feature = "tunnel")]
mod tunnel;
mod ws;

use actix_cors::Cors;
use actix_web::{App, dev::ServerHandle, http, middleware};
use moosicbox_config::{AppType, get_or_init_server_identity};
use moosicbox_files::files::track_pool::service::Commander as _;
use moosicbox_music_models::ApiSource;
use std::{
    net::TcpListener,
    sync::{Arc, LazyLock},
};
use switchy_async::util::CancellationToken;
use switchy_database::{Database, config::ConfigDatabase, profiles::PROFILES};
use tokio::try_join;

static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
#[cfg(feature = "upnp")]
static UPNP_LISTENER_HANDLE: LazyLock<
    Arc<std::sync::RwLock<Option<switchy_upnp::listener::Handle>>>,
> = LazyLock::new(|| Arc::new(std::sync::RwLock::new(None)));

static WS_SERVER_HANDLE: LazyLock<switchy_async::sync::RwLock<Option<ws::server::WsServerHandle>>> =
    LazyLock::new(|| switchy_async::sync::RwLock::new(None));

#[allow(clippy::type_complexity)]
static CONFIG_DB: LazyLock<std::sync::RwLock<Option<ConfigDatabase>>> =
    LazyLock::new(|| std::sync::RwLock::new(None));

/// Starts the `MoosicBox` server with basic configuration.
///
/// This is a simplified version of [`run`] that uses default settings for optional features.
/// It automatically configures telemetry metrics if the feature is enabled.
///
/// # Parameters
///
/// * `app_type` - The type of application being run
/// * `addr` - The network address to bind the server to
/// * `service_port` - The port number for the service
/// * `actix_workers` - Optional number of Actix worker threads
/// * `on_startup` - Callback function invoked when the server starts, receives the server handle
///
/// # Returns
///
/// Returns the value produced by the `on_startup` callback
///
/// # Errors
///
/// * If the server fails to bind to the specified address and port
/// * If the server fails during initialization or execution
/// * If database initialization or migration fails
/// * If required services fail to start
///
/// # Panics
///
/// * If the config database path cannot be created (with `sqlite` feature, non-simulator mode)
/// * If database initialization fails
/// * If database migration fails (with `sqlite` or `postgres` features)
/// * If the static `CONFIG_DB` lock is poisoned
/// * If server identity cannot be retrieved or initialized
/// * If profile initialization fails
/// * If tunnel setup fails (with `tunnel` feature)
/// * If config directory path cannot be determined (with `tls` feature)
/// * If `TLS` directory creation fails (with `tls` feature)
/// * If `TLS` certificate generation or loading fails (with `tls` feature)
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn run_basic<T>(
    #[allow(unused)] app_type: AppType,
    addr: &str,
    service_port: u16,
    actix_workers: Option<usize>,
    on_startup: impl FnOnce(ServerHandle) -> T + Send,
) -> std::io::Result<T> {
    #[cfg(feature = "telemetry")]
    let request_metrics = std::sync::Arc::new(switchy_telemetry::get_http_metrics_handler());

    run(
        app_type,
        addr,
        service_port,
        actix_workers,
        None,
        #[cfg(feature = "player")]
        false,
        #[cfg(feature = "upnp")]
        false,
        #[cfg(feature = "telemetry")]
        request_metrics,
        on_startup,
    )
    .await
}

/// Starts the `MoosicBox` server with full configuration options.
///
/// This function initializes and runs the complete `MoosicBox` server, including database setup,
/// service initialization, `API` endpoint registration, and optional features like players,
/// `UPnP` discovery, tunneling, and telemetry.
///
/// # Parameters
///
/// * `app_type` - The type of application being run
/// * `addr` - The network address to bind the server to
/// * `service_port` - The port number for the service
/// * `actix_workers` - Optional number of Actix worker threads
/// * `listener` - Optional pre-configured TCP listener to use instead of binding to addr/port
/// * `local_players` - Whether to enable local audio players (requires `player` feature)
/// * `upnp_players` - Whether to enable `UPnP` player discovery (requires `upnp` feature)
/// * `metrics_handler` - `HTTP` metrics handler for telemetry (requires `telemetry` feature)
/// * `on_startup` - Callback function invoked when the server starts, receives the server handle
///
/// # Returns
///
/// Returns the value produced by the `on_startup` callback
///
/// # Errors
///
/// * If the server fails to bind to the specified address and port
/// * If the server fails during initialization or execution
/// * If database initialization or migration fails
/// * If required services fail to start
/// * If `TLS` certificate operations fail (with `tls` feature)
///
/// # Panics
///
/// * If the config database path cannot be created (with `sqlite` feature, non-simulator mode)
/// * If database initialization fails
/// * If database migration fails (with `sqlite` or `postgres` features)
/// * If the static `CONFIG_DB` lock is poisoned
/// * If server identity cannot be retrieved or initialized
/// * If profile initialization fails
/// * If tunnel setup fails (with `tunnel` feature)
/// * If config directory path cannot be determined (with `tls` feature)
/// * If `TLS` directory creation fails (with `tls` feature)
/// * If `TLS` certificate generation or loading fails (with `tls` feature)
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub async fn run<T>(
    #[allow(unused)] app_type: AppType,
    addr: &str,
    service_port: u16,
    actix_workers: Option<usize>,
    listener: Option<TcpListener>,
    #[cfg(feature = "player")] local_players: bool,
    #[cfg(feature = "upnp")] upnp_players: bool,
    #[cfg(feature = "telemetry")] metrics_handler: Arc<
        Box<dyn switchy_telemetry::HttpMetricsHandler>,
    >,
    on_startup: impl FnOnce(ServerHandle) -> T + Send,
) -> std::io::Result<T> {
    #[cfg(feature = "profiling-tracing")]
    tracing_subscriber::fmt::init();
    #[cfg(feature = "profiling-puffin")]
    start_puffin_server();

    let _ = ApiSource::register_library();

    #[cfg(feature = "tidal")]
    ApiSource::register("Tidal", "Tidal");

    #[cfg(feature = "qobuz")]
    ApiSource::register("Qobuz", "Qobuz");

    #[cfg(feature = "yt")]
    ApiSource::register("Yt", "YouTube Music");

    #[cfg(feature = "sqlite")]
    let config_db_path = {
        if cfg!(feature = "simulator") {
            None
        } else {
            Some(crate::db::make_config_db_path(app_type).expect("Failed to get DB config path"))
        }
    };

    let config_db = switchy_database_connection::init(
        #[cfg(feature = "sqlite")]
        config_db_path.as_deref(),
        None,
    )
    .await
    .expect("Failed to initialize database");

    #[cfg(any(feature = "sqlite", feature = "postgres"))]
    if let Err(e) = moosicbox_schema::migrate_config(&*config_db).await {
        moosicbox_assert::die_or_panic!("Failed to migrate database: {e:?}");
    }

    let config_database: Arc<Box<dyn Database>> = Arc::new(config_db);
    let config_database = ConfigDatabase {
        database: config_database,
    };

    CONFIG_DB.write().unwrap().replace(config_database.clone());

    let server_id = get_or_init_server_identity(&config_database)
        .await
        .expect("Failed to get or init server identity");

    switchy_database::config::init(config_database.clone().into());

    events::profiles_event::init(app_type, config_database.clone())
        .await
        .expect("Failed to initialize profiles");

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
    let (tunnel_host, tunnel_join_handle, tunnel_handle) =
        crate::tunnel::setup_tunnel(config_database.clone(), service_port)
            .await
            .expect("Failed to setup tunnel connection");

    #[cfg(feature = "tunnel")]
    if let Some(tunnel_handle) = &tunnel_handle {
        ws_server.add_sender(Box::new(tunnel_handle.clone()));
    }

    let ws_server_handle = switchy_async::runtime::Handle::current()
        .spawn_with_name("server: WsServer", ws_server.run());

    let (track_pool_handle, track_pool_join_handle) = switchy_async::runtime::Handle::current()
        .spawn_with_name("server: init TrackPool", async move {
            use moosicbox_files::files::track_pool::{Context, HANDLE, service::Service};

            let service = Service::new(Context::new());
            let handle = service.handle();
            let join_handle = service.start();

            *HANDLE.write().await = Some(handle.clone());

            (handle, join_handle)
        })
        .await
        .map_err(std::io::Error::other)?;

    #[cfg(feature = "upnp")]
    let (upnp_service_handle, join_upnp_service) = if upnp_players {
        let upnp_service =
            switchy_upnp::listener::Service::new(switchy_upnp::listener::UpnpContext::new());
        let upnp_service_handle = upnp_service.handle();
        let join_upnp_service = upnp_service.start();

        *UPNP_LISTENER_HANDLE.write().unwrap() = Some(upnp_service_handle.clone());

        #[cfg(feature = "upnp")]
        switchy_async::runtime::Handle::current().spawn_with_name(
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
        *events::playback_event::PLAYBACK_EVENT_HANDLE
            .write()
            .unwrap() = Some(playback_event_handle.clone());

        let config_database = config_database.clone();
        #[cfg(feature = "tunnel")]
        let tunnel_handle = tunnel_handle.clone();

        switchy_async::runtime::Handle::current().spawn_with_name(
            "server: scan outputs",
            async move {
                players::local::init(
                    &config_database,
                    #[cfg(feature = "tunnel")]
                    tunnel_handle,
                )
                .await
            },
        );

        (Some(playback_event_handle), Some(playback_join_handle))
    } else {
        (None, None)
    };

    #[cfg(feature = "openapi")]
    let openapi = api::openapi::init();

    let app = {
        move || {
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

            #[cfg(feature = "telemetry")]
            let app = app
                .app_data(actix_web::web::Data::new(metrics_handler.clone()))
                .service(switchy_telemetry::metrics)
                .wrap(metrics_handler.request_middleware())
                .wrap(switchy_telemetry::RequestTracing::new());

            #[cfg(feature = "static-token-auth")]
            let app = app.wrap(crate::auth::StaticTokenAuth::new(
                std::env!("STATIC_TOKEN").into(),
            ));

            #[cfg(feature = "tunnel")]
            let app = app.app_data(moosicbox_middleware::tunnel_info::init(
                moosicbox_middleware::tunnel_info::TunnelInfo {
                    host: Arc::new(tunnel_host.clone()),
                },
            ));

            let app = app.app_data(moosicbox_middleware::service_info::init(
                moosicbox_middleware::service_info::ServiceInfo { port: service_port },
            ));

            let app = app
                .wrap(middleware::Compress::default())
                .wrap(moosicbox_middleware::api_logger::ApiLogger::default())
                .service(api::health_endpoint)
                .service(api::websocket);

            #[cfg(feature = "openapi")]
            let app = app.service(api::openapi::bind_services(
                actix_web::web::scope("/openapi"),
                &openapi,
            ));

            #[cfg(feature = "admin-htmx-api")]
            let app = app.wrap(actix_htmx::HtmxMiddleware {});

            #[cfg(feature = "admin-htmx-api")]
            let app = app.service(moosicbox_admin_htmx::api::bind_services(
                actix_web::web::scope("/admin"),
            ));

            #[cfg(feature = "audio-output-api")]
            let app = app.service(moosicbox_audio_output::api::bind_services(
                actix_web::web::scope("/audio-output"),
            ));

            #[cfg(feature = "audio-zone-api")]
            let app = app.service(moosicbox_audio_zone::api::bind_services(
                actix_web::web::scope("/audio-zone"),
            ));

            #[cfg(feature = "auth-api")]
            let app = app.service(moosicbox_auth::api::bind_services(actix_web::web::scope(
                "/auth",
            )));

            #[cfg(feature = "config-api")]
            let app = app.service(moosicbox_config::api::bind_services(actix_web::web::scope(
                "/config",
            )));

            #[cfg(feature = "downloader-api")]
            let app = app.service(moosicbox_downloader::api::bind_services(
                actix_web::web::scope("/downloader"),
            ));

            #[cfg(feature = "files-api")]
            let app = app.service(moosicbox_files::api::bind_services(actix_web::web::scope(
                "/files",
            )));

            #[cfg(feature = "menu-api")]
            let app = app.service(moosicbox_menu::api::bind_services(actix_web::web::scope(
                "/menu",
            )));

            #[cfg(feature = "music-api-api")]
            let app = app.service(moosicbox_music_api_api::api::bind_services(
                actix_web::web::scope("/music-api"),
            ));

            #[cfg(feature = "player-api")]
            let app = app.service(moosicbox_player::api::bind_services(actix_web::web::scope(
                "/player",
            )));

            #[cfg(feature = "search-api")]
            let app = app.service(moosicbox_search::api::bind_services(actix_web::web::scope(
                "/search",
            )));

            #[cfg(feature = "library-api")]
            let app = app.service(moosicbox_library::api::bind_services(
                actix_web::web::scope("/library"),
            ));

            #[cfg(all(feature = "tidal", feature = "tidal-api"))]
            let app = app.service(moosicbox_tidal::api::bind_services(actix_web::web::scope(
                "/tidal",
            )));

            #[cfg(all(feature = "qobuz", feature = "qobuz-api"))]
            let app = app.service(moosicbox_qobuz::api::bind_services(actix_web::web::scope(
                "/qobuz",
            )));

            #[cfg(feature = "session-api")]
            let app = app.service(moosicbox_session::api::bind_services(
                actix_web::web::scope("/session"),
            ));

            #[cfg(feature = "scan-api")]
            let app = app.service(moosicbox_scan::api::bind_services(actix_web::web::scope(
                "/scan",
            )));

            #[cfg(feature = "upnp-api")]
            let app = app.service(switchy_upnp::api::bind_services(actix_web::web::scope(
                "/upnp",
            )));

            #[cfg(all(feature = "yt", feature = "yt-api"))]
            let app = app.service(moosicbox_yt::api::bind_services(actix_web::web::scope(
                "/yt",
            )));

            app
        }
    };

    let http_server = {
        let mut http_server = actix_web::HttpServer::new(app);

        #[cfg(feature = "simulator")]
        {
            log::debug!("run: starting http_server listening on {addr}:{service_port}...");
            http_server = http_server.disable_signals();
            log::debug!("run: started http_server listening on {addr}:{service_port}");
        }

        if let Some(listener) = listener {
            http_server = http_server.listen(listener)?;
        } else {
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
                    use rcgen::{CertifiedKey, generate_simple_self_signed};

                    let subject_alt_names = vec!["localhost".to_string()];

                    let CertifiedKey { cert, signing_key } =
                        generate_simple_self_signed(subject_alt_names).unwrap();

                    let mut cert_file = switchy_fs::sync::OpenOptions::new()
                        .create(true) // To create a new file
                        .truncate(true)
                        .write(true)
                        .open(&cert_path)
                        .unwrap();
                    cert_file
                        .write_all(cert.pem().as_bytes())
                        .expect("Failed to create cert file");

                    let mut key_file = switchy_fs::sync::OpenOptions::new()
                        .create(true) // To create a new file
                        .truncate(true)
                        .write(true)
                        .open(&key_path)
                        .unwrap();
                    key_file
                        .write_all(signing_key.serialize_pem().as_bytes())
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
        }

        if let Some(workers) = actix_workers {
            log::debug!("Running with {workers} Actix workers");
            http_server = http_server.workers(workers);
        }

        switchy_async::runtime::Handle::current().spawn_with_name("server: ctrl-c", async move {
            #[cfg(feature = "simulator")]
            {
                Ok::<_, std::io::Error>(())
            }
            #[cfg(not(feature = "simulator"))]
            {
                tokio::signal::ctrl_c().await?;
                log::debug!("Received ctrl-c");
                Ok::<_, std::io::Error>(())
            }
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

    if let Err(e) = switchy_mdns::register_service(&server_id, &ip, service_port).await {
        moosicbox_assert::die_or_error!("Failed to register mdns service: {e:?}");
    }

    let resp = on_startup(http_server.handle());

    log::info!("MoosicBox Server started on {ip}:{service_port}");

    let config_db = config_database.clone();

    if let Err(err) = try_join!(
        async move {
            let resp = http_server.await;

            #[cfg(feature = "player")]
            {
                fn drain_btreemap<K: Ord, V>(
                    map: &mut std::collections::BTreeMap<K, V>,
                ) -> Vec<(K, V)> {
                    let mut values = Vec::new();

                    while let Some((key, value)) = map.pop_first() {
                        values.push((key, value));
                    }

                    values
                }

                log::debug!("Shutting down server players...");
                let players = drain_btreemap(&mut *players::local::SERVER_PLAYERS.write().await);
                for (id, (_, mut player)) in players {
                    log::debug!("Shutting down player id={id}");
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
                            None,
                            true,
                            None,
                        )
                        .await
                    {
                        log::error!("Failed to stop player id={id}: {err:?}");
                    } else {
                        log::debug!("Successfully shut down player id={id}");
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

            log::debug!("Cancelling scan...");
            #[cfg(feature = "scan")]
            moosicbox_scan::cancel();
            CANCELLATION_TOKEN.cancel();

            #[cfg(feature = "tunnel")]
            if let Some(handle) = tunnel_handle {
                log::debug!("Closing tunnel connection...");
                handle.close();
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
                for profile in PROFILES.names() {
                    if let Some(library_db) = PROFILES.get(&profile) {
                        log::debug!("Closing library database connection...");
                        if let Err(e) = library_db.close().await {
                            log::error!("Failed to shut down database connection: {e:?}");
                        }
                    }
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
                use switchy_upnp::listener::Commander as _;

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
                    .map_err(std::io::Error::other);
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
                .map_err(std::io::Error::other);
            log::debug!("PlaybackEventHandler connection closed");
            resp
        },
        async move {
            #[cfg(feature = "upnp")]
            if let Some(join_upnp_service) = join_upnp_service {
                let resp = join_upnp_service
                    .await
                    .expect("Failed to shut down UPnP service")
                    .map_err(std::io::Error::other);
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

    Ok(resp)
}

#[cfg(feature = "profiling-puffin")]
fn start_puffin_server() {
    puffin::set_scopes_on(true);

    match puffin_http::Server::new("127.0.0.1:8586") {
        Ok(puffin_server) => {
            log::info!("Run: cargo install puffin_viewer && puffin_viewer --url 127.0.0.1:8586");

            std::process::Command::new("puffin_viewer")
                .arg("--url")
                .arg("127.0.0.1:8586")
                .spawn()
                .ok();

            #[allow(clippy::mem_forget)]
            std::mem::forget(puffin_server);
        }
        Err(err) => {
            log::error!("Failed to start puffin server: {err}");
        }
    }
}
