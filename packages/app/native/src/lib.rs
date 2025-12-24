//! Native desktop application for `MoosicBox` music player.
//!
//! This crate provides the core functionality for the `MoosicBox` native desktop application,
//! including routing, UI rendering, state management, and playback visualization. It serves
//! as the main entry point for the desktop GUI application.
//!
//! # Features
//!
//! * **Router Management** - Global router instance for handling application navigation
//! * **UI Rendering** - Support for multiple rendering backends (HTML, FLTK, egui)
//! * **State Management** - Centralized application state with persistence
//! * **Playback Visualization** - Real-time audio waveform visualization (with `_canvas` feature)
//! * **Action Handling** - UI action dispatching for playback control and navigation
//!
//! # Main Entry Points
//!
//! * [`init()`] - Initialize the application router with all route handlers
//! * [`init_app_state()`] - Initialize application state with persistence and event listeners
//! * [`ROUTER`] - Global router instance for handling application routes
//! * [`STATE`] - Global application state
//! * [`actions::handle_action()`] - Handle UI actions (playback, navigation, etc.)

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{LazyLock, OnceLock};

use hyperchad::{renderer::Renderer, router::Router};
use moosicbox_app_native_ui::state;
use moosicbox_config::AppType;
use moosicbox_music_models::ApiSource;

pub mod actions;
mod events;
mod routes;
#[cfg(feature = "_canvas")]
pub mod visualization;

/// Global router instance for handling application routes.
pub static ROUTER: OnceLock<Router> = OnceLock::new();

/// Global renderer instance for rendering UI components.
pub static RENDERER: OnceLock<Box<dyn Renderer>> = OnceLock::new();

/// Global application state, initialized once.
pub static STATE_LOCK: OnceLock<moosicbox_app_state::AppState> = OnceLock::new();

/// Lazily initialized application state, cloned from [`STATE_LOCK`].
pub static STATE: LazyLock<moosicbox_app_state::AppState> =
    LazyLock::new(|| STATE_LOCK.get().unwrap().clone());

/// Application profile identifier.
pub static PROFILE: &str = "master";

#[cfg(feature = "assets")]
pub mod assets {
    //! Static asset configuration for the application.
    //!
    //! This module defines static asset routes for serving application resources
    //! such as JavaScript files, favicons, and public directory contents.
    use std::{path::PathBuf, sync::LazyLock};

    use hyperchad::renderer;

    static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
        LazyLock::new(|| std::option_env!("CARGO_MANIFEST_DIR").map(Into::into));

    static ASSETS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        CARGO_MANIFEST_DIR.as_ref().map_or_else(
            || <PathBuf as std::str::FromStr>::from_str("public").unwrap(),
            |dir| dir.join("public"),
        )
    });

    /// Static asset routes for serving application resources.
    pub static ASSETS: LazyLock<Vec<renderer::assets::StaticAssetRoute>> = LazyLock::new(|| {
        vec![
            #[cfg(feature = "vanilla-js")]
            renderer::assets::StaticAssetRoute {
                route: format!(
                    "/js/{}",
                    hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
                ),
                target: renderer::assets::AssetPathTarget::FileContents(
                    hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
                ),
                not_found_behavior: None,
            },
            renderer::assets::StaticAssetRoute {
                route: "/favicon.ico".to_string(),
                target: ASSETS_DIR.join("favicon.ico").try_into().unwrap(),
                not_found_behavior: None,
            },
            renderer::assets::StaticAssetRoute {
                route: "/public".to_string(),
                target: ASSETS_DIR.clone().try_into().unwrap(),
                not_found_behavior: None,
            },
        ]
    });
}

/// Converts the application state to the UI state representation.
///
/// # Panics
///
/// * If fails to get the current connection from the app state
pub async fn convert_state(app_state: &moosicbox_app_state::AppState) -> state::State {
    let mut state = state::State::default();

    let session = app_state.get_current_session_ref().await;
    if let Some(session) = &session {
        state.player.playback = Some(state::PlaybackState {
            session_id: session.session_id,
            playing: session.playing,
            position: session.position.unwrap_or(0),
            seek: session.seek.unwrap_or(0.0),
            volume: session.volume.unwrap_or(1.0),
            tracks: session.playlist.tracks.clone(),
        });
    }
    drop(session);

    state.connection = STATE.get_current_connection().await.unwrap();

    state
}

/// Initializes the application router with all route handlers.
///
/// # Panics
///
/// * If the [`ROUTER`] has already been initialized
pub fn init() -> Router {
    moosicbox_player::on_playback_event(events::on_playback_event);

    let router =
        Router::new()
            .with_static_route(&["/", "/home"], |_| async {
                moosicbox_app_native_ui::home(&convert_state(&STATE).await)
            })
            .with_route_result("/downloads", |req| async {
                routes::downloads_route(req).await
            })
            .with_route_result("/settings", |req| async {
                routes::settings_route(req).await
            })
            .with_route_result("/settings/connections", |req| async {
                routes::settings_connections_route(req).await
            })
            .with_route_result("/settings/new-connection", |req| async {
                routes::settings_new_connection_route(req).await
            })
            .with_route_result("/settings/select-connection", |req| async {
                routes::settings_select_connection_route(req).await
            })
            .with_no_content_result("/settings/connection-name", |req| async {
                routes::settings_connection_name_route(req).await
            })
            .with_route_result("/settings/music-api-settings", |req| async {
                routes::settings_music_api_settings_route(req).await
            })
            .with_route_result("/settings/download-settings", |req| async {
                routes::settings_download_settings_route(req).await
            })
            .with_route_result("/settings/downloads/download-location", |req| async move {
                routes::settings_downloads_download_location_route(req).await
            })
            .with_route_result(
                "/settings/downloads/default-download-location",
                |req| async move {
                    routes::settings_downloads_default_download_location_route(req).await
                },
            )
            .with_route_result("/settings/scan-settings", |req| async {
                routes::settings_scan_settings_route(req).await
            })
            .with_route_result("/settings/scan/scan-path", |req| async move {
                routes::settings_scan_scan_path_route(req).await
            })
            .with_route_result("/audio-zones", |req| async {
                routes::audio_zones_route(req).await
            })
            .with_route_result("/playback-sessions", |req| async {
                routes::playback_sessions_route(req).await
            })
            .with_static_route_result(
                "/albums",
                |req| async move { routes::albums_route(req).await },
            )
            .with_route_result("/albums-list-start", |req| async move {
                routes::albums_list_start_route(req).await
            })
            .with_route_result("/albums-list", |req| async move {
                routes::albums_list_route(req).await
            })
            .with_route_result(
                "/artists",
                |req| async move { routes::artist_route(req).await },
            )
            .with_route_result("/artists/albums-list", |req| async move {
                routes::artist_albums_list_route(req).await
            })
            .with_route_result("/music-api/scan", |req| async move {
                routes::music_api_scan_route(req).await
            })
            .with_route_result("/music-api/enable-scan-origin", |req| async move {
                routes::music_api_enable_scan_origin_route(req).await
            })
            .with_route_result("/music-api/auth", |req| async move {
                routes::music_api_auth_route(req).await
            })
            .with_no_content_result(
                "/search",
                |req| async move { routes::search_route(req).await },
            )
            .with_no_content_result(
                "/download",
                |req| async move { routes::download(req).await },
            )
            .with_route_result(
                "/library",
                |req| async move { routes::library_route(req).await },
            );

    moosicbox_assert::assert_or_panic!(ROUTER.set(router.clone()).is_ok(), "Already set ROUTER");

    router
}

/// Initializes the application state with persistence and event listeners.
///
/// # Errors
///
/// * If fails to initialize the persistence database
///
/// # Panics
///
/// * If fails to get or create the persistence directory
pub async fn init_app_state(
    state: moosicbox_app_state::AppState,
) -> Result<moosicbox_app_state::AppState, moosicbox_app_state::AppStateError> {
    ApiSource::register_library();

    #[cfg(feature = "tidal")]
    ApiSource::register("Tidal", "Tidal");

    #[cfg(feature = "qobuz")]
    ApiSource::register("Qobuz", "Qobuz");

    #[cfg(feature = "yt")]
    ApiSource::register("Yt", "YouTube Music");

    let persistence_db = moosicbox_config::get_profile_dir_path(AppType::App, PROFILE)
        .map(|x| x.join("persistence.db"))
        .unwrap();

    switchy::fs::unsync::create_dir_all(persistence_db.parent().unwrap())
        .await
        .unwrap();

    state
        .with_on_current_sessions_updated_listener(events::current_sessions_updated)
        .with_on_audio_zone_with_sessions_updated_listener(events::audio_zone_with_sessions_updated)
        .with_on_connections_updated_listener(events::connections_updated)
        .with_on_after_handle_playback_update_listener(events::handle_playback_update)
        .with_persistence(persistence_db)
        .await
}
