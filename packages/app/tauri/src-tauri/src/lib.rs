//! `MoosicBox` desktop and mobile application built with Tauri.
//!
//! This crate provides the core Tauri application for the `MoosicBox` music player,
//! supporting desktop (Windows, macOS, Linux) and mobile (Android, iOS) platforms.
//! It manages playback, WebSocket connections to `MoosicBox` servers, mDNS service
//! discovery, and platform-specific integrations.
//!
//! # Features
//!
//! * **Multi-platform support**: Desktop and mobile platforms via Tauri
//! * **Music playback**: Local playback with multiple audio output backends (CPAL, ASIO, JACK)
//! * **Remote control**: WebSocket-based communication with `MoosicBox` servers
//! * **Service discovery**: Automatic discovery of `MoosicBox` servers via mDNS/Zeroconf
//! * **Multiple sources**: Support for local library, Tidal, Qobuz, and `YouTube` Music
//! * **Native UI**: Optional native UI rendering with Hyperchad framework
//! * **Bundled mode**: Optional embedded server for standalone operation
//!
//! # Main Entry Points
//!
//! * [`run`] - Main entry point to start the Tauri application
//! * [`on_playback_event`] - Callback for playback state changes
//! * [`TauriUpdateAppState`] - Structure for updating application state via Tauri commands
//! * [`TauriPlayerError`] - Error type for Tauri player operations
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(all(not(target_os = "android"), not(target_os = "ios")))]
//! moosicbox_lib::run();
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    env,
    fmt::{Debug, Write},
    path::PathBuf,
    sync::{LazyLock, OnceLock},
};

use moosicbox_app_state::{
    AppStateError, UPNP_LISTENER_HANDLE, UpdateAppState, ws::WsConnectMessage,
};
use moosicbox_music_models::{ApiSource, PlaybackQuality, api::ApiTrack, id::Id};
use moosicbox_player::{Playback, PlayerError};
use moosicbox_session::models::{ApiSession, ApiUpdateSession, PlaybackTarget, UpdateSession};
use moosicbox_ws::models::{
    InboundPayload, OutboundPayload, SessionUpdatedPayload, UpdateSessionPayload,
};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use switchy::mdns::scanner::service::Commander;
use tauri::{AppHandle, Emitter, Manager as _};
use tauri_plugin_fs::FsExt as _;
use thiserror::Error;

mod mdns;

/// Error type for Tauri player operations.
///
/// This error type is serializable for transmission across the Tauri IPC boundary.
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum TauriPlayerError {
    /// An unknown error occurred.
    #[error("Unknown({0})")]
    Unknown(String),
}

impl From<AppStateError> for TauriPlayerError {
    fn from(err: AppStateError) -> Self {
        Self::Unknown(err.to_string())
    }
}

impl From<PlayerError> for TauriPlayerError {
    fn from(err: PlayerError) -> Self {
        Self::Unknown(err.to_string())
    }
}

/// Error type for application-level operations.
#[derive(Debug, Error)]
pub enum AppError {
    /// A Tauri framework error occurred.
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    /// An application state error occurred.
    #[error(transparent)]
    AppState(#[from] AppStateError),
    /// An unknown error occurred.
    #[error("Unknown({0})")]
    Unknown(String),
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static LOG_LAYER: OnceLock<moosicbox_logging::free_log_client::FreeLogLayer> = OnceLock::new();

static STATE_LOCK: OnceLock<moosicbox_app_state::AppState> = OnceLock::new();
static STATE: LazyLock<moosicbox_app_state::AppState> =
    LazyLock::new(|| STATE_LOCK.get().unwrap().clone());

#[cfg(feature = "moosicbox-app-native")]
static HTTP_APP: OnceLock<hyperchad::renderer_html_http::HttpApp<native_app::Renderer>> =
    OnceLock::new();

#[cfg(feature = "bundled")]
static THREADS: LazyLock<u16> = LazyLock::new(|| switchy_env::var_parse_or("MAX_THREADS", 64u16));

#[cfg(feature = "bundled")]
static RT: LazyLock<switchy::unsync::runtime::Runtime> = LazyLock::new(|| {
    switchy::unsync::runtime::Builder::new()
        .max_blocking_threads(*THREADS)
        .build()
        .unwrap()
});

/// Tauri command to show the main application window.
///
/// This command makes the main webview window visible. Only available on
/// desktop platforms (not Android).
///
/// # Panics
///
/// * If the main webview window cannot be found
/// * If the window fails to show
#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn show_main_window(window: tauri::Window) {
    use tauri::Manager as _;

    window.get_webview_window("main").unwrap().show().unwrap();
}

/// Tauri command invoked when the application starts up.
///
/// This command handles application startup logic, including reconnecting
/// to existing WebSocket connections if present.
///
/// # Errors
///
/// * If emitting the `ws-connect` event fails
#[tauri::command]
async fn on_startup() -> Result<(), tauri::Error> {
    log::debug!("on_startup");

    let connection_id = { STATE.ws_connection_id.read().await.clone() };

    if let Some(connection_id) = connection_id {
        APP.get().unwrap().emit(
            "ws-connect",
            WsConnectMessage {
                connection_id,
                ws_url: STATE.ws_url.read().await.to_owned().unwrap_or_default(),
            },
        )?;
    }

    Ok(())
}

/// Application state update structure for Tauri IPC.
///
/// This structure is used to update various parts of the application state
/// via Tauri commands. All fields are optional to allow partial updates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TauriUpdateAppState {
    /// The connection ID for the current server connection.
    pub connection_id: Option<String>,
    /// The human-readable name of the current connection.
    pub connection_name: Option<String>,
    /// The API URL for the `MoosicBox` server.
    pub api_url: Option<String>,
    /// The client ID for authentication.
    pub client_id: Option<String>,
    /// The signature token for request signing.
    pub signature_token: Option<String>,
    /// The API token for authentication.
    pub api_token: Option<String>,
    /// The active user profile.
    pub profile: Option<String>,
    /// The target for playback (local or remote).
    pub playback_target: Option<PlaybackTarget>,
    /// The ID of the current playback session.
    pub current_session_id: Option<u64>,
}

impl From<TauriUpdateAppState> for UpdateAppState {
    fn from(value: TauriUpdateAppState) -> Self {
        Self {
            connection_id: Some(value.connection_id),
            connection_name: Some(value.connection_name),
            api_url: Some(value.api_url),
            client_id: Some(value.client_id),
            signature_token: Some(value.signature_token),
            api_token: Some(value.api_token),
            profile: Some(value.profile),
            playback_target: Some(value.playback_target),
            current_session_id: Some(value.current_session_id),
        }
    }
}

/// Tauri command to update the application state.
///
/// This command allows the frontend to update various aspects of the
/// application state, such as connection details, API credentials, and
/// playback configuration.
///
/// # Errors
///
/// * If the state update fails in the underlying `AppState`
#[tauri::command]
async fn set_state(state: TauriUpdateAppState) -> Result<(), TauriPlayerError> {
    Ok(STATE.set_state(state.into()).await?)
}

async fn update_log_layer(state: UpdateAppState) {
    log::debug!("update_log_layer: state={state:?}");

    {
        if let Some(connection_id) = &state.connection_id.flatten() {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("connectionId", connection_id.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("connectionId"));
        }
    }

    {
        if let Some(connection_name) = &state.connection_name.flatten() {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("connectionName", connection_name.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("connectionName"));
        }
    }

    {
        if let Some(client_id) = &state.client_id.flatten() {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("clientId", client_id.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("clientId"));
        }
    }

    {
        if let Some(api_url) = &state.api_url.flatten() {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("apiUrl", api_url.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("apiUrl"));
        }
    }

    {
        if let Some(profile) = &state.profile.flatten() {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("profile", profile.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("profile"));
        }
    }
}

async fn get_url_and_query() -> Option<(String, String)> {
    let url = { STATE.api_url.read().await.clone() }?;

    let client_id = STATE.client_id.read().await.clone();
    let signature_token = STATE.signature_token.read().await.clone();

    let mut query = String::new();
    if let Some(client_id) = client_id {
        write!(query, "&clientId={client_id}").unwrap();
    }
    if let Some(signature_token) = signature_token {
        write!(query, "&signature={signature_token}").unwrap();
    }

    Some((url, query))
}

async fn update_player_plugin_playlist(session: ApiSession) {
    use app_tauri_plugin_player::PlayerExt;

    let Some((url, query)) = get_url_and_query().await else {
        return;
    };

    match APP
        .get()
        .unwrap()
        .player()
        .update_state(app_tauri_plugin_player::UpdateState {
            playing: Some(session.playing),
            position: session.position,
            seek: session.seek,
            volume: session.volume,
            playlist: Some(app_tauri_plugin_player::Playlist {
                tracks: session
                    .playlist
                    .tracks
                    .into_iter()
                    .map(|x| convert_track(x, &url, &query))
                    .collect::<Vec<_>>(),
            }),
        }) {
        Ok(_resp) => {
            log::debug!("Successfully set state");
        }
        Err(e) => {
            log::error!("Failed to set state: {e:?}");
        }
    }
}

async fn handle_before_ws_message(message: OutboundPayload) {
    if let OutboundPayload::ConnectionId(payload) = &message {
        let ws_url = STATE.ws_url.read().await.to_owned().unwrap_or_default();
        if let Err(e) = APP.get().unwrap().emit(
            "ws-connect",
            WsConnectMessage {
                connection_id: payload.connection_id.clone(),
                ws_url,
            },
        ) {
            log::error!("Failed to emit ws-connect: {e:?}");
        }
    }
}

async fn handle_after_ws_message(message: OutboundPayload) {
    if let Err(e) = APP.get().unwrap().emit("ws-message", message) {
        log::error!("Failed to emit ws-message: {e:?}");
    }
}

/// Track identifier that can reference tracks from different sources.
#[derive(Copy, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(untagged)]
pub enum TrackId {
    /// A track from the local library.
    Library(u64),
    /// A track from the Tidal service.
    #[cfg(feature = "tidal")]
    Tidal(u64),
    /// A track from the Qobuz service.
    #[cfg(feature = "qobuz")]
    Qobuz(u64),
}

impl From<TrackId> for Id {
    fn from(value: TrackId) -> Self {
        match value {
            TrackId::Library(id) => Self::Number(id),
            #[cfg(feature = "tidal")]
            TrackId::Tidal(id) => Self::Number(id),
            #[cfg(feature = "qobuz")]
            TrackId::Qobuz(id) => Self::Number(id),
        }
    }
}

/// Tauri command to set the playback quality for all active players.
///
/// This command updates the playback quality setting and propagates it to
/// all active players in the current session.
///
/// # Errors
///
/// * If updating playback quality on any active player fails
#[tauri::command]
async fn set_playback_quality(quality: PlaybackQuality) -> Result<(), TauriPlayerError> {
    log::debug!("Setting playback quality: {quality:?}");

    STATE.playback_quality.write().await.replace(quality);

    let mut binding = STATE.active_players.write().await;
    let players = binding.iter_mut();

    let profile = { STATE.profile.read().await.clone() };

    for x in players {
        x.player
            .update_playback(
                false,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                *STATE.playback_quality.read().await,
                Some(x.session_id),
                profile.clone(),
                Some(x.playback_target.clone().into()),
                false,
                None,
            )
            .await?;
    }
    drop(binding);

    Ok(())
}

/// Tauri command to propagate a WebSocket message from the frontend.
///
/// This command receives WebSocket messages from the frontend and queues them
/// for transmission to the connected `MoosicBox` server.
///
/// # Errors
///
/// * Currently this function always succeeds, but returns a `Result` for
///   consistency with other Tauri commands. Errors in the spawned task are logged.
#[tauri::command]
async fn propagate_ws_message(message: InboundPayload) -> Result<(), TauriPlayerError> {
    moosicbox_logging::debug_or_trace!(
        ("propagate_ws_message: received ws message from frontend: {message}"),
        ("propagate_ws_message: received ws message from frontend: {message:?}")
    );

    switchy::unsync::runtime::Handle::current().spawn_with_name(
        "propagate_ws_message",
        async move {
            STATE.queue_ws_message(message, true).await?;

            Ok::<_, AppStateError>(())
        },
    );

    Ok(())
}

/// Tauri command to proxy a GET request to the `MoosicBox` server.
///
/// This command forwards HTTP GET requests from the frontend to the configured
/// `MoosicBox` server, handling authentication and connection details automatically.
///
/// # Errors
///
/// * If the API proxy GET request fails
///
/// # Panics
///
/// * If the headers JSON value is not an object
/// * If any header value is not a string
#[tauri::command]
async fn api_proxy_get(
    url: String,
    headers: Option<serde_json::Value>,
) -> Result<serde_json::Value, TauriPlayerError> {
    Ok(STATE
        .api_proxy_get(
            url,
            headers.map(|headers| {
                let mut map = BTreeMap::new();
                for (name, value) in headers.as_object().unwrap() {
                    map.insert(name.clone(), value.as_str().unwrap().to_string());
                }
                map
            }),
        )
        .await?)
}

/// Tauri command to proxy a POST request to the `MoosicBox` server.
///
/// This command forwards HTTP POST requests from the frontend to the configured
/// `MoosicBox` server, handling authentication and connection details automatically.
///
/// # Errors
///
/// * If the API proxy POST request fails
///
/// # Panics
///
/// * If the headers JSON value is not an object
/// * If any header value is not a string
#[tauri::command]
async fn api_proxy_post(
    url: String,
    body: Option<serde_json::Value>,
    headers: Option<serde_json::Value>,
) -> Result<serde_json::Value, TauriPlayerError> {
    Ok(STATE
        .api_proxy_post(
            url,
            body,
            headers.map(|headers| {
                let mut map = BTreeMap::new();
                for (name, value) in headers.as_object().unwrap() {
                    map.insert(name.clone(), value.as_str().unwrap().to_string());
                }
                map
            }),
        )
        .await?)
}

async fn propagate_playback_event(update: UpdateSession, to_plugin: bool) -> Result<(), AppError> {
    if to_plugin {
        propagate_state_to_plugin(update.clone().into()).await;
    }

    let handle = STATE.ws_handle.read().await.clone();

    if let Some(handle) = handle {
        log::debug!("on_playback_event: Sending update session: update={update:?}");

        APP.get().unwrap().emit(
            "ws-message",
            OutboundPayload::SessionUpdated(SessionUpdatedPayload {
                payload: update.clone().into(),
            }),
        )?;

        if let Err(e) = STATE
            .send_ws_message(
                &handle,
                InboundPayload::UpdateSession(UpdateSessionPayload { payload: update }),
                false,
            )
            .await
        {
            log::error!("Failed to propagate UpdateSession ws message: {e:?}");
        }
    } else {
        log::debug!("on_playback_event: No WS_HANDLE to send update to");
    }

    Ok(())
}

/// Callback invoked when a playback event occurs in the player.
///
/// This function propagates playback state changes to both the Tauri plugin
/// and connected WebSocket clients. It spawns an asynchronous task to handle
/// the propagation without blocking the caller.
pub fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    log::debug!("on_playback_event: received update, spawning task to handle update={update:?}");

    switchy::unsync::runtime::Handle::current().spawn_with_name(
        "moosicbox_app: on_playback_event",
        propagate_playback_event(update.to_owned(), true),
    );
}

async fn propagate_state_to_plugin(update: ApiUpdateSession) {
    let current_session_id = { *STATE.current_session_id.read().await };

    if current_session_id.is_some_and(|id| update.session_id == id)
        && let Some((url, query)) = get_url_and_query().await
    {
        use app_tauri_plugin_player::PlayerExt;

        let player = APP.get().unwrap().player();

        log::debug!("propagate_state_to_plugin: update={update:?}");
        if let Err(e) = player.update_state(app_tauri_plugin_player::UpdateState {
            playing: update.playing,
            position: update.position,
            seek: update.seek,
            volume: update.volume,
            playlist: update
                .playlist
                .as_ref()
                .map(|x| app_tauri_plugin_player::Playlist {
                    tracks: x
                        .tracks
                        .iter()
                        .map(|x| convert_track(x.clone(), &url, &query))
                        .collect::<Vec<_>>(),
                }),
        }) {
            log::error!("Failed to update_state: {e:?}");
        }
    }
}

fn album_cover_url(album_id: &str, source: &ApiSource, url: &str, query: &str) -> String {
    format!("{url}/files/albums/{album_id}/300x300?source={source}{query}")
}

fn convert_track(track: ApiTrack, url: &str, query: &str) -> app_tauri_plugin_player::Track {
    let api_source = track.api_source;

    let album_cover = if track.contains_cover {
        Some(album_cover_url(
            &track.album_id.to_string(),
            &api_source,
            url,
            query,
        ))
    } else {
        None
    };
    app_tauri_plugin_player::Track {
        id: track.track_id.to_string(),
        number: track.number,
        title: track.title,
        album: track.album,
        album_cover,
        artist: track.artist,
        artist_cover: None,
        duration: track.duration,
    }
}

#[cfg(target_os = "android")]
async fn handle_media_event(
    event: app_tauri_plugin_player::MediaEvent,
) -> Result<(), TauriPlayerError> {
    log::trace!("handle_media_event: event={event:?}");
    let Some(current_session_id) = ({ *STATE.current_session_id.read().await }) else {
        log::debug!("handle_media_event: No current_session_id");
        return Ok(());
    };

    let Some(current_profile) = ({ STATE.profile.read().await.clone() }) else {
        log::debug!("handle_media_event: No current_profile");
        return Ok(());
    };

    let Some(current_playback_target) = ({ STATE.current_playback_target.read().await.clone() })
    else {
        log::debug!("handle_media_event: No current_playback_target");
        return Ok(());
    };

    let players = STATE
        .get_players(
            current_session_id,
            Some(&current_playback_target.clone().into()),
        )
        .await;
    log::debug!("handle_media_event: {} player(s)", players.len());

    for mut player in players {
        if let Some(true) = event.next_track {
            let Some(position) = ({
                player
                    .playback
                    .read()
                    .unwrap()
                    .as_ref()
                    .map(|x| std::cmp::min(x.position + 1, x.tracks.len() as u16))
            }) else {
                return Ok(());
            };
            propagate_playback_event(
                UpdateSession {
                    session_id: current_session_id,
                    profile: current_profile.clone(),
                    playback_target: current_playback_target.clone(),
                    play: None,
                    stop: None,
                    name: None,
                    active: None,
                    playing: None,
                    position: Some(position),
                    seek: None,
                    volume: None,
                    playlist: None,
                    quality: None,
                },
                false,
            )
            .await
            .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
            player
                .next_track(None, None)
                .await
                .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
        }
        if let Some(true) = event.prev_track {
            let Some(position) = ({
                player
                    .playback
                    .read()
                    .unwrap()
                    .as_ref()
                    .map(|x| std::cmp::max(x.position - 1, 0))
            }) else {
                return Ok(());
            };
            propagate_playback_event(
                UpdateSession {
                    session_id: current_session_id,
                    profile: current_profile.clone(),
                    playback_target: current_playback_target.clone(),
                    play: None,
                    stop: None,
                    name: None,
                    active: None,
                    playing: None,
                    position: Some(position),
                    seek: None,
                    volume: None,
                    playlist: None,
                    quality: None,
                },
                false,
            )
            .await
            .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
            player
                .previous_track(None, None)
                .await
                .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
        }
        if let Some(true) = event.play {
            propagate_playback_event(
                UpdateSession {
                    session_id: current_session_id,
                    profile: current_profile.clone(),
                    playback_target: current_playback_target.clone(),
                    play: None,
                    stop: None,
                    name: None,
                    active: None,
                    playing: Some(true),
                    position: None,
                    seek: None,
                    volume: None,
                    playlist: None,
                    quality: None,
                },
                false,
            )
            .await
            .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
            player
                .resume(None)
                .await
                .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
        } else if let Some(false) = event.play {
            propagate_playback_event(
                UpdateSession {
                    session_id: current_session_id,
                    profile: current_profile.clone(),
                    playback_target: current_playback_target.clone(),
                    play: None,
                    stop: None,
                    name: None,
                    active: None,
                    playing: Some(false),
                    position: None,
                    seek: None,
                    volume: None,
                    playlist: None,
                    quality: None,
                },
                false,
            )
            .await
            .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
            player
                .pause(None)
                .await
                .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;
        }
    }

    Ok(())
}

#[allow(unused)]
fn get_data_dir() -> Result<PathBuf, TauriPlayerError> {
    log::debug!("get_data_dir");

    let path = APP
        .get()
        .unwrap()
        .path()
        .app_data_dir()
        .map_err(|e| TauriPlayerError::Unknown(e.to_string()))?;

    log::debug!("get_data_dir path={}", path.display());

    let scope = APP.get().unwrap().fs_scope();
    scope.allow_directory(&path, true).unwrap();
    assert!(scope.is_allowed(&path));

    Ok(path)
}

#[cfg(not(feature = "tauri-logger"))]
fn init_log() {
    use moosicbox_logging::free_log_client::DynLayer;

    let mut layers = vec![];

    if matches!(
        switchy_env::var("TOKIO_CONSOLE").as_deref(),
        Ok("1" | "true")
    ) {
        layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
    }

    #[cfg(target_os = "android")]
    let filename = None;
    #[cfg(not(target_os = "android"))]
    let filename = Some("moosicbox_app.log");

    let layer =
        moosicbox_logging::init(filename, Some(layers)).expect("Failed to initialize FreeLog");
    LOG_LAYER.set(layer).expect("Failed to set LOG_LAYER");
}

#[cfg(feature = "moosicbox-app-native")]
/// HTTP request structure for the native app's custom URI scheme handler.
///
/// This type represents an HTTP request received via Tauri's custom protocol
/// handler, which is used to serve the native application UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct HttpRequest {
    /// The HTTP method (GET, POST, etc.).
    pub method: String,
    /// The request path.
    pub path: String,
    /// Query parameters as key-value pairs.
    pub query: std::collections::BTreeMap<String, String>,
    /// HTTP headers as key-value pairs.
    pub headers: std::collections::BTreeMap<String, String>,
    /// Cookies as key-value pairs.
    pub cookies: std::collections::BTreeMap<String, String>,
    /// Optional request body.
    pub body: Option<Vec<u8>>,
}

#[cfg(feature = "moosicbox-app-native")]
async fn handle_http_request(
    request: HttpRequest,
) -> Result<http::Response<Vec<u8>>, TauriPlayerError> {
    use std::{str::FromStr as _, sync::Arc};

    use hyperchad::router::{DEFAULT_CLIENT_INFO, RequestInfo, RouteRequest};

    log::debug!("handle_http_request: request={request:?}");

    let app = HTTP_APP
        .get()
        .ok_or_else(|| TauriPlayerError::Unknown("HttpApp not initialized".to_string()))?;

    if request.path.as_str() == "/$sse" {
        return http::Response::builder()
            .status(204)
            .body(vec![])
            .map_err(|e| TauriPlayerError::Unknown(e.to_string()));
    }

    let req = RouteRequest {
        path: request.path,
        method: switchy::http::models::Method::from_str(request.method.as_str()).unwrap(),
        query: request.query,
        headers: request.headers,
        cookies: request.cookies,
        info: RequestInfo {
            client: DEFAULT_CLIENT_INFO.clone(),
        },
        body: request.body.map(|x| Arc::new(x.into())),
    };

    app.process(&req)
        .await
        .map_err(|e| TauriPlayerError::Unknown(e.to_string()))
}

/// Runs the `MoosicBox` Tauri application.
///
/// This is the main entry point for the application. It initializes all services,
/// configures plugins, sets up the application state, and starts the Tauri runtime.
///
/// # Panics
///
/// * If the data directory cannot be obtained
/// * If the application state fails to initialize (with `moosicbox-app-native` feature)
/// * If the `STATE_LOCK` has already been set
/// * If the `HTTP_APP` has already been set (with `moosicbox-app-native` feature)
/// * If sending the `WaitForStartup` command fails (with `bundled` feature)
/// * If the bundled app server fails to start (with `bundled` feature)
/// * If the `UPNP_LISTENER_HANDLE` has already been set
/// * If the Tauri application builder fails to build
#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::too_many_lines)]
pub fn run() {
    #[cfg(feature = "bundled")]
    #[allow(clippy::type_complexity)]
    static APP_SERVER_HANDLE: LazyLock<
        std::sync::Mutex<Option<moosicbox_app_tauri_bundled::service::Handle>>,
    > = LazyLock::new(|| std::sync::Mutex::new(None));

    #[cfg(feature = "bundled")]
    #[allow(clippy::type_complexity)]
    static JOIN_APP_SERVER: LazyLock<
        std::sync::Mutex<
            Option<
                switchy::unsync::task::JoinHandle<
                    Result<(), moosicbox_app_tauri_bundled::service::Error>,
                >,
            >,
        >,
    > = LazyLock::new(|| std::sync::Mutex::new(None));

    #[allow(clippy::type_complexity)]
    static JOIN_MDNS_SERVICE: LazyLock<
        std::sync::Mutex<
            Option<
                switchy::unsync::task::JoinHandle<
                    Result<(), switchy::mdns::scanner::service::Error>,
                >,
            >,
        >,
    > = LazyLock::new(|| std::sync::Mutex::new(None));

    #[allow(clippy::type_complexity)]
    static JOIN_UPNP_SERVICE: LazyLock<
        std::sync::Mutex<
            Option<switchy::unsync::task::JoinHandle<Result<(), switchy::upnp::listener::Error>>>,
        >,
    > = LazyLock::new(|| std::sync::Mutex::new(None));

    #[allow(clippy::type_complexity)]
    static MDNS_HANDLE: LazyLock<
        std::sync::Mutex<Option<switchy::mdns::scanner::service::Handle>>,
    > = LazyLock::new(|| std::sync::Mutex::new(None));

    #[cfg(not(feature = "tauri-logger"))]
    init_log();

    #[allow(unused_mut)]
    let mut app_builder = tauri::Builder::default().plugin(tauri_plugin_fs::init());

    #[cfg(feature = "tauri-logger")]
    {
        app_builder = app_builder.plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .build(),
        );
    }

    #[cfg(feature = "moosicbox-app-native")]
    {
        app_builder = app_builder.register_asynchronous_uri_scheme_protocol(
            "tauri",
            move |_app, request, responder| {
                fn parse_cookies(header: &str) -> Vec<(String, String)> {
                    header
                        .split(';')
                        .filter_map(|part| {
                            let mut parts = part.trim().splitn(2, '=');
                            let key = parts.next()?.trim();
                            let value = parts.next()?.trim();
                            Some((key.to_string(), value.to_string()))
                        })
                        .collect()
                }

                log::debug!("handle_tauri_request: request={request:?}");

                let path = request.uri().path().to_string();
                let method = request.method().to_string();
                let headers = request
                    .headers()
                    .iter()
                    .map(|(name, value)| {
                        (
                            name.to_string(),
                            value.to_str().unwrap_or_default().to_string(),
                        )
                    })
                    .collect();

                let cookies = request
                    .headers()
                    .get("Cookie")
                    .map(|x| parse_cookies(x.to_str().unwrap_or_default()))
                    .unwrap_or_default()
                    .into_iter()
                    .collect();

                let query = request
                    .uri()
                    .query()
                    .map(qstring::QString::from)
                    .unwrap_or_default();

                let query = std::collections::BTreeMap::from_iter(query.into_pairs());

                let body = request.into_body();
                let body = if body.is_empty() { None } else { Some(body) };
                let http_request = HttpRequest {
                    method,
                    path,
                    query,
                    headers,
                    cookies,
                    body,
                };

                tauri::async_runtime::spawn(async move {
                    let response = match handle_http_request(http_request).await {
                        Ok(response) => response,
                        Err(e) => http::response::Response::builder()
                            .status(500)
                            .body(format!("Error: {e}").into_bytes())
                            .unwrap(),
                    };

                    responder.respond(response);
                });
            },
        );
    }

    app_builder = app_builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(app_tauri_plugin_player::init())
        .setup(move |app| {
            APP.get_or_init(|| app.handle().clone());

            let runtime_handle = tauri::async_runtime::block_on(async move {
                switchy::unsync::runtime::Handle::current()
            });

            moosicbox_config::set_root_dir(get_data_dir().unwrap());

            let state = moosicbox_app_state::AppState::new()
                .with_on_before_handle_playback_update_listener(propagate_state_to_plugin)
                .with_on_after_update_playlist_listener(update_player_plugin_playlist)
                .with_on_before_handle_ws_message_listener(handle_before_ws_message)
                .with_on_after_handle_ws_message_listener(handle_after_ws_message)
                .with_on_before_set_state_listener(update_log_layer);

            #[cfg(feature = "moosicbox-app-native")]
            let state = runtime_handle.block_on(async move { moosicbox_app_native::init_app_state(state).await }).unwrap();
            #[cfg(not(feature = "moosicbox-app-native"))]
            {
                ApiSource::register_library();

                #[cfg(feature = "tidal")]
                ApiSource::register("Tidal", "Tidal");

                #[cfg(feature = "qobuz")]
                ApiSource::register("Qobuz", "Qobuz");

                #[cfg(feature = "yt")]
                ApiSource::register("Yt", "YouTube Music");
            }

            STATE_LOCK.set(state).unwrap();

            #[cfg(feature = "moosicbox-app-native")]
            {
                use hyperchad::{
                    color::Color, renderer_html_http::HttpApp,
                    renderer_vanilla_js::VanillaJsTagRenderer,
                };
                use moosicbox_app_native::RENDERER;
                use moosicbox_app_native_ui::Action;

                moosicbox_app_native::STATE_LOCK.set(STATE.clone()).unwrap();

                let router = moosicbox_app_native::init();

                let (action_tx, action_rx) = flume::unbounded();

                let tag_renderer = VanillaJsTagRenderer::default();
                let renderer = native_app::Renderer::new(tag_renderer, app.handle().clone());

                moosicbox_assert::assert_or_panic!(
                    RENDERER.set(Box::new(renderer.clone())).is_ok(),
                    "Already set RENDERER"
                );

                let app = HttpApp::new(renderer, router)
                    .with_title("MoosicBox")
                    .with_description("A music app for cows")
                    .with_background(Color::from_hex("#181a1b"))
                    .with_action_tx(action_tx)
                    .with_static_asset_route_handler(|req| {
                        static SCRIPT_PATH: LazyLock<String> = LazyLock::new(|| format!(
                            "/js/{}",
                            hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
                        ));
                        log::debug!("static_asset_route_handler: path={}", req.path);
                        let script_path = SCRIPT_PATH.as_str();
                        match req.path.as_str() {
                            "/favicon.ico" => {
                                let favicon_path = "/public/favicon.ico";
                                moosicbox_app_native_image::Asset::get(favicon_path).map(|x| {
                                    log::debug!("static_asset_route_handler (favicon): found image at favicon_path={favicon_path}");
                                    hyperchad::renderer::assets::AssetPathTarget::FileContents(
                                        x.data.to_vec().into(),
                                    )
                                })
                            }
                            _ if req.path == script_path => {
                                log::debug!("static_asset_route_handler (script): found for script_path={script_path}");
                                Some(hyperchad::renderer::assets::AssetPathTarget::FileContents(
                                    hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
                                ))
                            }
                            path =>  {
                                moosicbox_app_native_image::Asset::get(path).map(|x| {
                                    log::debug!("static_asset_route_handler: found image at path={path}");
                                    hyperchad::renderer::assets::AssetPathTarget::FileContents(
                                        x.data.to_vec().into(),
                                    )
                                })
                            }
                        }
                    });

                runtime_handle.spawn(async move {
                    while let Ok((action, value)) = action_rx.recv_async().await {
                        log::debug!("Received action: action={action} value={value:?}");
                        match Action::try_from(action) {
                            Ok(action) => {
                                moosicbox_app_native::actions::handle_action(action, value).await?;
                            }
                            Err(e) => {
                                log::error!("Failed to handle action: {e:?}");
                            }
                        }
                    }
                    Ok::<_, AppStateError>(())
                });

                HTTP_APP.set(app).unwrap();

                runtime_handle.spawn(async move {
                    let api_url = STATE
                        .get_current_connection()
                        .await
                        .unwrap()
                        .map(|x| x.api_url);
                    let connection_name = STATE.get_connection_name().await.unwrap();
                    let connection_id = STATE.get_or_init_connection_id().await.unwrap();

                    STATE
                        .set_state(moosicbox_app_state::UpdateAppState {
                            connection_id: Some(Some(connection_id)),
                            connection_name: Some(connection_name),
                            api_url: Some(api_url),
                            profile: Some(Some(moosicbox_app_native::PROFILE.to_string())),
                            ..Default::default()
                        })
                        .await?;

                    Ok::<_, moosicbox_app_state::AppStateError>(())
                });
            }

            #[cfg(feature = "client")]
            {
                moosicbox_app_client::init();
            }

            #[cfg(feature = "bundled")]
            {
                use moosicbox_app_tauri_bundled::service::Commander as _;

                log::debug!("Starting app server");

                let context = moosicbox_app_tauri_bundled::Context::new(&RT.handle());
                let server = moosicbox_app_tauri_bundled::service::Service::new(context);

                let app_server_handle = server.handle();
                let (tx, rx) = switchy::unsync::sync::oneshot::channel();

                let join_app_server = server.start_on(&RT.handle());

                app_server_handle
                    .send_command(moosicbox_app_tauri_bundled::Command::WaitForStartup {
                        sender: tx,
                    })
                    .expect("Failed to send WaitForStartup command");

                log::debug!("Waiting for app server to start");

                RT.block_on(rx).expect("Failed to start app server");

                log::debug!("App server started");

                *JOIN_APP_SERVER.lock().unwrap() = Some(join_app_server);
                *APP_SERVER_HANDLE.lock().unwrap() = Some(app_server_handle);
            };

            moosicbox_player::on_playback_event(crate::on_playback_event);

            let upnp_service =
                switchy::upnp::listener::Service::new(switchy::upnp::listener::UpnpContext::new());

            let upnp_service_handle = upnp_service.handle();
            *JOIN_UPNP_SERVICE.lock().unwrap() = Some(upnp_service.start_on(&runtime_handle));

            UPNP_LISTENER_HANDLE
                .set(upnp_service_handle)
                .unwrap_or_else(|_| panic!("Failed to set UPNP_LISTENER_HANDLE"));

            let (mdns_handle, join_mdns_service) = mdns::spawn_mdns_scanner(&runtime_handle);
            *MDNS_HANDLE.lock().unwrap() = Some(mdns_handle);
            *JOIN_MDNS_SERVICE.lock().unwrap() = Some(join_mdns_service);

            #[cfg(target_os = "android")]
            {
                use app_tauri_plugin_player::PlayerExt as _;

                let player = app.player();

                let channel = tauri::ipc::Channel::new(|event| {
                    tauri::async_runtime::spawn(async move {
                        log::trace!("Received event from channel: {event:?}");
                        let event: app_tauri_plugin_player::MediaEvent =
                            event.deserialize().map_err(|x| x.to_string())?;
                        log::debug!("Received media event from channel: {event:?}");

                        handle_media_event(event).await.map_err(|x| x.to_string())?;

                        Ok::<_, String>(())
                    });
                    Ok(())
                });

                log::debug!("moosicbox_app: init_channel");
                if let Err(e) =
                    player.init_channel(app_tauri_plugin_player::InitChannel { channel })
                {
                    log::error!("Failed to init_channel: {e:?}");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            on_startup,
            #[cfg(not(target_os = "android"))]
            show_main_window,
            set_playback_quality,
            set_state,
            propagate_ws_message,
            api_proxy_get,
            api_proxy_post,
            mdns::fetch_moosicbox_servers,
        ]);

    app_builder
        .build(
            #[allow(clippy::large_stack_frames)]
            {
                tauri::generate_context!()
            },
        )
        .expect("error while running tauri application")
        .run(move |_handle, event| {
            log::trace!("event: {event:?}");

            #[cfg(feature = "bundled")]
            {
                use moosicbox_app_tauri_bundled::service::Commander as _;

                static BUFFER: LazyLock<std::sync::RwLock<Vec<tauri::RunEvent>>> =
                    LazyLock::new(|| std::sync::RwLock::new(vec![]));

                let value = APP_SERVER_HANDLE.lock().unwrap().clone();

                if let Some(value) = value {
                    let mut buffer = BUFFER.write().unwrap();

                    for event in buffer.drain(..) {
                        let event = std::sync::Arc::new(event);
                        if let Err(e) = value
                            .send_command(moosicbox_app_tauri_bundled::Command::RunEvent { event })
                        {
                            log::error!("AppServer failed to handle event: {e:?}");
                        }
                    }

                    drop(buffer);

                    let event = std::sync::Arc::new(event);
                    if let Err(e) =
                        value.send_command(moosicbox_app_tauri_bundled::Command::RunEvent { event })
                    {
                        log::error!("AppServer failed to handle event: {e:?}");
                    }
                } else {
                    BUFFER.write().unwrap().push(event);
                }
            }
        });

    #[cfg(feature = "bundled")]
    {
        use moosicbox_app_tauri_bundled::service::Commander as _;

        log::debug!("Shutting down app server..");
        let handle = APP_SERVER_HANDLE.lock().unwrap().take();
        if let Err(e) = handle.unwrap().shutdown() {
            log::error!("AppServer failed to shutdown: {e:?}");
        }
    }

    log::debug!("Shutting down mdns service..");
    {
        let handle = MDNS_HANDLE.lock().unwrap().take();
        if let Err(e) = handle.unwrap().shutdown() {
            log::error!("Failed to shutdown mdns service: {e:?}");
        }
    }

    #[cfg(feature = "bundled")]
    {
        log::debug!("Joining app server...");
        let server = JOIN_APP_SERVER.lock().unwrap().take();
        match tauri::async_runtime::block_on(server.unwrap()) {
            Err(e) => {
                log::error!("Failed to join app server: {e:?}");
            }
            Ok(Err(e)) => {
                log::error!("Failed to join app server: {e:?}");
            }
            _ => {}
        }
    }

    log::debug!("Joining UPnP service..");
    {
        let handle = JOIN_UPNP_SERVICE.lock().unwrap().take();
        if let Err(e) = tauri::async_runtime::block_on(handle.unwrap()) {
            log::error!("Failed to join UPnP service: {e:?}");
        }
    }

    log::debug!("Joining mdns service...");
    {
        let handle = JOIN_MDNS_SERVICE.lock().unwrap().take();
        if let Err(e) = tauri::async_runtime::block_on(handle.unwrap()) {
            log::error!("Failed to join mdns service: {e:?}");
        }
    }
}

#[cfg(feature = "moosicbox-app-native")]
mod native_app {
    use std::{
        collections::BTreeMap,
        sync::{Arc, LazyLock, Mutex},
    };

    use async_trait::async_trait;
    use hyperchad::{
        renderer::{Color, Handle, HtmlTagRenderer, ToRenderRunner, View},
        renderer_html::html::container_element_to_html,
        renderer_vanilla_js::VanillaJsTagRenderer,
        transformer::{Container, ResponsiveTrigger},
    };
    use tauri::Emitter as _;

    static HEADERS: LazyLock<BTreeMap<String, String>> = LazyLock::new(BTreeMap::new);

    #[derive(Debug, Clone)]
    pub struct Renderer {
        tag_renderer: Arc<Mutex<VanillaJsTagRenderer>>,
        app_handle: tauri::AppHandle,
    }

    impl Renderer {
        pub fn new(tag_renderer: VanillaJsTagRenderer, app_handle: tauri::AppHandle) -> Self {
            Self {
                tag_renderer: Arc::new(Mutex::new(tag_renderer)),
                app_handle,
            }
        }
    }

    impl HtmlTagRenderer for Renderer {
        fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
            self.tag_renderer
                .lock()
                .unwrap()
                .add_responsive_trigger(name, trigger);
        }

        fn element_attrs_to_html(
            &self,
            f: &mut dyn std::io::Write,
            container: &Container,
            is_flex_child: bool,
        ) -> Result<(), std::io::Error> {
            self.tag_renderer
                .lock()
                .unwrap()
                .element_attrs_to_html(f, container, is_flex_child)
        }

        fn reactive_conditions_to_css(
            &self,
            f: &mut dyn std::io::Write,
            container: &Container,
        ) -> Result<(), std::io::Error> {
            self.tag_renderer
                .lock()
                .unwrap()
                .reactive_conditions_to_css(f, container)
        }

        fn partial_html(
            &self,
            headers: &std::collections::BTreeMap<String, String>,
            container: &Container,
            content: String,
            viewport: Option<&str>,
            background: Option<Color>,
        ) -> String {
            self.tag_renderer
                .lock()
                .unwrap()
                .partial_html(headers, container, content, viewport, background)
        }

        fn root_html(
            &self,
            headers: &std::collections::BTreeMap<String, String>,
            container: &Container,
            content: String,
            viewport: Option<&str>,
            background: Option<Color>,
            title: Option<&str>,
            description: Option<&str>,
            css_urls: &[String],
            css_paths: &[String],
            inline_css: &[String],
        ) -> String {
            self.tag_renderer.lock().unwrap().root_html(
                headers,
                container,
                content,
                viewport,
                background,
                title,
                description,
                css_urls,
                css_paths,
                inline_css,
            )
        }
    }

    struct RenderRunnner;

    impl hyperchad::renderer::RenderRunner for RenderRunnner {
        fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }
    }

    impl ToRenderRunner for Renderer {
        /// # Errors
        ///
        /// * If failed to convert the value to a `RenderRunner`
        fn to_runner(
            self,
            _handle: Handle,
        ) -> Result<Box<dyn hyperchad::renderer::RenderRunner>, Box<dyn std::error::Error + Send>>
        {
            Ok(Box::new(RenderRunnner))
        }
    }

    #[derive(Debug, Clone, serde::Serialize)]
    struct EventData {
        id: Option<String>,
        event: String,
        data: String,
    }

    #[async_trait]
    impl hyperchad::renderer::Renderer for Renderer {
        async fn init(
            &mut self,
            _width: f32,
            _height: f32,
            _x: Option<i32>,
            _y: Option<i32>,
            _background: Option<Color>,
            _title: Option<&str>,
            _description: Option<&str>,
            _viewport: Option<&str>,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            Ok(())
        }

        fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
            self.tag_renderer
                .lock()
                .unwrap()
                .add_responsive_trigger(name, trigger);
        }

        /// # Errors
        ///
        /// Will error if `Renderer` implementation fails to emit the event.
        async fn emit_event(
            &self,
            event_name: String,
            event_value: Option<String>,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            log::trace!("emit_event");
            let event_value = EventData {
                id: None,
                event: "event".to_string(),
                data: format!("{event_name}:{}", event_value.unwrap_or_default()),
            };
            self.app_handle
                .emit("sse-event", event_value)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            Ok(())
        }

        /// # Errors
        ///
        /// Will error if `Renderer` implementation fails to render the view.
        async fn render(
            &self,
            view: View,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            log::trace!("render");

            // Handle primary content
            if let Some(container) = view.primary {
                let tag_renderer = self.tag_renderer.lock().unwrap();
                let content = container_element_to_html(&container, &*tag_renderer)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
                let content = tag_renderer.partial_html(&HEADERS, &container, content, None, None);
                drop(tag_renderer);

                let event_value = EventData {
                    id: None,
                    event: "view".to_string(),
                    data: content,
                };
                self.app_handle
                    .emit("sse-event", event_value)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            }

            // Handle fragments
            for container in view.fragments {
                let tag_renderer = self.tag_renderer.lock().unwrap();
                let content = container_element_to_html(&container.container, &*tag_renderer)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
                let content =
                    tag_renderer.partial_html(&HEADERS, &container.container, content, None, None);
                drop(tag_renderer);

                let event_value = EventData {
                    id: container.container.str_id.clone(),
                    event: "fragment".to_string(),
                    data: content,
                };
                self.app_handle
                    .emit("sse-event", event_value)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            }

            Ok(())
        }

        /// # Errors
        ///
        /// Will error if `Renderer` implementation fails to render the canvas update.
        async fn render_canvas(
            &self,
            update: hyperchad::renderer::canvas::CanvasUpdate,
        ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
            log::trace!("render_canvas");
            let event_value = EventData {
                id: Some(update.target.clone()),
                event: "canvas_update".to_string(),
                data: serde_json::to_string(&update)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?,
            };
            self.app_handle
                .emit("sse-event", event_value)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            Ok(())
        }
    }
}
