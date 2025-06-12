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

pub static ROUTER: OnceLock<Router> = OnceLock::new();
pub static RENDERER: OnceLock<Box<dyn Renderer>> = OnceLock::new();

pub static STATE_LOCK: OnceLock<moosicbox_app_state::AppState> = OnceLock::new();
pub static STATE: LazyLock<moosicbox_app_state::AppState> =
    LazyLock::new(|| STATE_LOCK.get().unwrap().clone());

pub static PROFILE: &str = "master";

#[cfg(feature = "assets")]
pub mod assets {
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
            },
            renderer::assets::StaticAssetRoute {
                route: "/favicon.ico".to_string(),
                target: ASSETS_DIR.join("favicon.ico").try_into().unwrap(),
            },
            renderer::assets::StaticAssetRoute {
                route: "/public".to_string(),
                target: ASSETS_DIR.clone().try_into().unwrap(),
            },
        ]
    });
}

/// # Panics
///
/// * If the app persistence fails
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

/// # Panics
///
/// * If the `API_SOURCES` `RwLock` is poisoned
pub fn init() -> Router {
    moosicbox_player::on_playback_event(events::on_playback_event);

    let router = Router::new()
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

/// # Errors
///
/// * If fails to initialize the persistence
///
/// # Panics
///
/// * If fails to get the persistence directory
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
