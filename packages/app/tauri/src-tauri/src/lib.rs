#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    env,
    fmt::Debug,
    sync::{LazyLock, OnceLock},
};

use moosicbox_app_state::{
    ws::WsConnectMessage, AppStateError, UpdateAppState, UPNP_LISTENER_HANDLE,
};
use moosicbox_logging::free_log_client::DynLayer;
use moosicbox_mdns::scanner::service::Commander;
use moosicbox_music_models::{api::ApiTrack, id::Id, ApiSource, PlaybackQuality};
use moosicbox_player::{Playback, PlayerError};
use moosicbox_session::models::{ApiSession, ApiUpdateSession, UpdateSession};
use moosicbox_ws::models::{
    InboundPayload, OutboundPayload, SessionUpdatedPayload, UpdateSessionPayload,
};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use tauri::{AppHandle, Emitter};
use thiserror::Error;

mod mdns;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum TauriPlayerError {
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

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error(transparent)]
    AppState(#[from] AppStateError),
    #[error("Unknown({0})")]
    Unknown(String),
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static LOG_LAYER: OnceLock<moosicbox_logging::free_log_client::FreeLogLayer> = OnceLock::new();

static STATE: LazyLock<moosicbox_app_state::AppState> = LazyLock::new(|| {
    moosicbox_app_state::AppState::default()
        .with_on_before_handle_playback_update_listener(propagate_state_to_plugin)
        .with_on_after_update_playlist_listener(update_player_plugin_playlist)
        .with_on_before_handle_ws_message_listener(handle_before_ws_message)
        .with_on_after_handle_ws_message_listener(handle_after_ws_message)
        .with_on_before_set_state_listener(update_log_layer)
});

#[cfg(feature = "bundled")]
lazy_static::lazy_static! {
    static ref THREADS: usize =
        moosicbox_env_utils::default_env_usize("MAX_THREADS", 64).unwrap_or(64);
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(*THREADS)
        .build()
        .unwrap();
}

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[cfg(not(all(target_os = "android")))]
#[tauri::command]
async fn show_main_window(window: tauri::Window) {
    use tauri::Manager as _;

    window.get_webview_window("main").unwrap().show().unwrap();
}

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

#[tauri::command]
async fn set_state(state: UpdateAppState) -> Result<(), TauriPlayerError> {
    Ok(STATE.set_state(state).await?)
}

async fn update_log_layer(state: UpdateAppState) {
    log::debug!("update_log_layer: state={state:?}");

    {
        if let Some(connection_id) = &state.connection_id {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("connectionId", connection_id.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("connectionId"));
        }
    }

    {
        if let Some(connection_name) = &state.connection_name {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("connectionName", connection_name.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("connectionName"));
        }
    }

    {
        if let Some(client_id) = &state.client_id {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("clientId", client_id.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("clientId"));
        }
    }

    {
        if let Some(api_url) = &state.api_url {
            LOG_LAYER
                .get()
                .map(|x| x.set_property("apiUrl", api_url.to_owned().into()));
        } else {
            LOG_LAYER.get().map(|x| x.remove_property("apiUrl"));
        }
    }

    {
        if let Some(profile) = &state.profile {
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
        query.push_str(&format!("&clientId={client_id}"));
    }
    if let Some(signature_token) = signature_token {
        query.push_str(&format!("&signature={signature_token}"));
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

#[derive(Copy, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(untagged)]
pub enum TrackId {
    Library(u64),
    Tidal(u64),
    Qobuz(u64),
}

impl From<TrackId> for Id {
    fn from(value: TrackId) -> Self {
        match value {
            TrackId::Library(id) | TrackId::Tidal(id) | TrackId::Qobuz(id) => Self::Number(id),
        }
    }
}

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

#[tauri::command]
async fn propagate_ws_message(message: InboundPayload) -> Result<(), TauriPlayerError> {
    moosicbox_logging::debug_or_trace!(
        ("propagate_ws_message: received ws message from frontend: {message}"),
        ("propagate_ws_message: received ws message from frontend: {message:?}")
    );

    moosicbox_task::spawn("propagate_ws_message", async move {
        STATE.queue_ws_message(message, true).await?;

        Ok::<_, AppStateError>(())
    });

    Ok(())
}

#[tauri::command]
async fn api_proxy_get(
    url: String,
    headers: Option<serde_json::Value>,
) -> Result<serde_json::Value, TauriPlayerError> {
    Ok(STATE.api_proxy_get(url, headers).await?)
}

#[tauri::command]
async fn api_proxy_post(
    url: String,
    body: Option<serde_json::Value>,
    headers: Option<serde_json::Value>,
) -> Result<serde_json::Value, TauriPlayerError> {
    Ok(STATE.api_proxy_post(url, body, headers).await?)
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

pub fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    log::debug!("on_playback_event: received update, spawning task to handle update={update:?}");

    moosicbox_task::spawn(
        "moosicbox_app: on_playback_event",
        propagate_playback_event(update.to_owned(), true),
    );
}

async fn propagate_state_to_plugin(update: ApiUpdateSession) {
    let current_session_id = { *STATE.current_session_id.read().await };

    if current_session_id.is_some_and(|id| update.session_id == id) {
        if let Some((url, query)) = get_url_and_query().await {
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
}

fn album_cover_url(album_id: &str, source: ApiSource, url: &str, query: &str) -> String {
    format!("{url}/files/albums/{album_id}/300x300?source={source}{query}")
}

fn convert_track(track: ApiTrack, url: &str, query: &str) -> app_tauri_plugin_player::Track {
    let api_source = track.api_source;

    let album_cover = if track.contains_cover {
        Some(album_cover_url(
            &track.album_id.to_string(),
            api_source,
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

/// # Panics
///
/// * If the `UPnP` listener fails to initialize
#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::too_many_lines)]
pub fn run() {
    let mut layers = vec![];

    if std::env::var("TOKIO_CONSOLE") == Ok("1".to_string()) {
        layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
    }

    #[cfg(target_os = "android")]
    let filename = None;
    #[cfg(not(target_os = "android"))]
    let filename = Some("moosicbox_app.log");

    let layer =
        moosicbox_logging::init(filename, Some(layers)).expect("Failed to initialize FreeLog");
    LOG_LAYER.set(layer).expect("Failed to set LOG_LAYER");

    let tauri::async_runtime::RuntimeHandle::Tokio(tokio_handle) = tauri::async_runtime::handle();

    #[cfg(feature = "client")]
    {
        moosicbox_app_client::init();
    }

    #[cfg(feature = "bundled")]
    let (join_app_server, app_server_handle) = {
        use moosicbox_app_tauri_bundled::service::Commander as _;

        log::debug!("Starting app server");

        let context = moosicbox_app_tauri_bundled::Context::new(RT.handle());
        let server = moosicbox_app_tauri_bundled::service::Service::new(context);

        let app_server_handle = server.handle();
        let (tx, rx) = tokio::sync::oneshot::channel();

        let join_app_server = server.start_on(RT.handle());

        app_server_handle
            .send_command(moosicbox_app_tauri_bundled::Command::WaitForStartup { sender: tx })
            .expect("Failed to send WaitForStartup command");

        log::debug!("Waiting for app server to start");

        RT.block_on(rx).expect("Failed to start app server");

        log::debug!("App server started");

        (join_app_server, app_server_handle)
    };

    moosicbox_player::on_playback_event(crate::on_playback_event);

    let upnp_service =
        moosicbox_upnp::listener::Service::new(moosicbox_upnp::listener::UpnpContext::new());

    let upnp_service_handle = upnp_service.handle();
    let join_upnp_service = upnp_service.start_on(&tokio_handle);

    UPNP_LISTENER_HANDLE
        .set(upnp_service_handle)
        .unwrap_or_else(|_| panic!("Failed to set UPNP_LISTENER_HANDLE"));

    let (mdns_handle, join_mdns_service) = mdns::spawn_mdns_scanner();

    #[allow(unused_mut)]
    let mut app_builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(app_tauri_plugin_player::init())
        .setup(|app| {
            APP.get_or_init(|| app.handle().clone());

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
            #[cfg(not(all(target_os = "android")))]
            show_main_window,
            set_playback_quality,
            set_state,
            propagate_ws_message,
            api_proxy_get,
            api_proxy_post,
            mdns::fetch_moosicbox_servers,
        ]);

    app_builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run({
            #[cfg(feature = "bundled")]
            let app_server_handle = app_server_handle.clone();
            move |_handle, event| {
                log::trace!("event: {event:?}");

                #[cfg(feature = "bundled")]
                {
                    use moosicbox_app_tauri_bundled::service::Commander as _;

                    let event = std::sync::Arc::new(event);

                    if let Err(e) = app_server_handle
                        .send_command(moosicbox_app_tauri_bundled::Command::RunEvent { event })
                    {
                        log::error!("AppServer failed to handle event: {e:?}");
                    }
                }
            }
        });

    #[cfg(feature = "bundled")]
    {
        use moosicbox_app_tauri_bundled::service::Commander as _;

        log::debug!("Shutting down app server..");
        if let Err(e) = app_server_handle.shutdown() {
            log::error!("AppServer failed to shutdown: {e:?}");
        }
    }

    log::debug!("Shutting down mdns service..");
    if let Err(e) = mdns_handle.shutdown() {
        log::error!("Failed to shutdown mdns service: {e:?}");
    }

    #[cfg(feature = "bundled")]
    {
        log::debug!("Joining app server...");
        match tauri::async_runtime::block_on(join_app_server) {
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
    if let Err(e) = tauri::async_runtime::block_on(join_upnp_service) {
        log::error!("Failed to join UPnP service: {e:?}");
    }

    log::debug!("Joining mdns service...");
    if let Err(e) = tauri::async_runtime::block_on(join_mdns_service) {
        log::error!("Failed to join mdns service: {e:?}");
    }
}
