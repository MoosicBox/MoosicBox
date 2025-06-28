#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, LazyLock},
};

use hyperchad::state::sqlite::SqlitePersistence;
use moosicbox_app_ws::{ConnectWsError, WsHandle};
use moosicbox_audio_output::{AudioOutputFactory, AudioOutputScannerError};
use moosicbox_audio_zone::models::{ApiAudioZoneWithSession, ApiPlayer};
use moosicbox_music_models::PlaybackQuality;
use moosicbox_paging::Page;
use moosicbox_player::{
    PlaybackHandler, PlaybackType, PlayerError, PlayerSource, local::LocalPlayer,
};
pub use moosicbox_session::models::PlaybackTarget;
use moosicbox_session::models::{
    ApiConnection, ApiPlaybackTarget, ApiSession, ApiUpdateSession, ApiUpdateSessionPlaylist,
    RegisterConnection, RegisterPlayer,
};
use moosicbox_ws::models::{InboundPayload, OutboundPayload};
use serde::{Deserialize, Serialize};
use switchy_http::RequestBuilder;
use thiserror::Error;
use tokio::{
    sync::{RwLock, RwLockReadGuard},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

mod persistence;
pub mod ws;

type ApiPlayersMap = (ApiPlayer, PlayerType, AudioOutputFactory);

static PROXY_CLIENT: LazyLock<switchy_http::Client> = LazyLock::new(switchy_http::Client::new);

#[cfg(feature = "upnp")]
pub static UPNP_LISTENER_HANDLE: std::sync::OnceLock<switchy_upnp::listener::Handle> =
    std::sync::OnceLock::new();

#[cfg(feature = "upnp")]
pub struct SourceToRemoteLibrary {
    host: String,
    profile: String,
}

#[cfg(feature = "upnp")]
impl moosicbox_music_api::SourceToMusicApi for SourceToRemoteLibrary {
    fn get(
        &self,
        source: &moosicbox_music_models::ApiSource,
    ) -> Option<Arc<Box<dyn moosicbox_music_api::MusicApi>>> {
        Some(Arc::new(Box::new(
            moosicbox_remote_library::RemoteLibraryMusicApi::new(
                self.host.clone(),
                source.clone(),
                self.profile.clone(),
            ),
        )))
    }
}

#[derive(Debug, Error)]
pub enum ProxyRequestError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    #[error("Failure response ({status}): {text}")]
    FailureResponse { status: u16, text: String },
}

#[derive(Debug, Error)]
pub enum FetchAudioZonesError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Missing profile")]
    MissingProfile,
}

#[derive(Debug, Error)]
pub enum ScanOutputsError {
    #[error(transparent)]
    AudioOutputScanner(#[from] AudioOutputScannerError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

#[cfg(feature = "upnp")]
#[derive(Debug, Error)]
pub enum InitUpnpError {
    #[error(transparent)]
    UpnpDeviceScanner(#[from] switchy_upnp::UpnpDeviceScannerError),
    #[error(transparent)]
    AudioOutput(#[from] moosicbox_audio_output::AudioOutputError),
    #[error(transparent)]
    RegisterPlayers(#[from] RegisterPlayersError),
}

#[derive(Debug, Error)]
pub enum RegisterPlayersError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Missing profile")]
    MissingProfile,
}

#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("Unknown({0})")]
    Unknown(String),
    #[error("Action missing param")]
    ActionMissingParam,
    #[error("Action invalid param")]
    ActionInvalidParam,
    #[error(transparent)]
    Player(#[from] PlayerError),
    #[cfg(feature = "upnp")]
    #[error(transparent)]
    InitUpnp(#[from] InitUpnpError),
    #[error(transparent)]
    RegisterPlayers(#[from] RegisterPlayersError),
    #[error(transparent)]
    ScanOutputs(#[from] ScanOutputsError),
    #[error(transparent)]
    InitWs(#[from] ws::InitWsError),
    #[error(transparent)]
    CloseWs(#[from] ws::CloseWsError),
    #[error(transparent)]
    SendWsMessage(#[from] ws::SendWsMessageError),
    #[error(transparent)]
    FetchAudioZones(#[from] FetchAudioZonesError),
    #[error(transparent)]
    ProxyRequest(#[from] ProxyRequestError),
    #[error(transparent)]
    ConnectWs(#[from] ConnectWsError),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Persistence(#[from] hyperchad::state::Error),
}

impl AppStateError {
    pub fn unknown(message: impl Into<String>) -> Self {
        Self::Unknown(message.into())
    }
}

#[derive(Debug, Clone, Default, Error, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppState {
    pub connection_id: Option<Option<String>>,
    pub connection_name: Option<Option<String>>,
    pub api_url: Option<Option<String>>,
    pub client_id: Option<Option<String>>,
    pub signature_token: Option<Option<String>>,
    pub api_token: Option<Option<String>>,
    pub profile: Option<Option<String>>,
    pub playback_target: Option<Option<PlaybackTarget>>,
    pub current_session_id: Option<Option<u64>>,
}

impl std::fmt::Display for UpdateAppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{self:?}"))
    }
}

#[derive(Clone)]
pub enum PlayerType {
    Local,
    #[cfg(feature = "upnp")]
    Upnp {
        source_to_music_api: Arc<Box<dyn moosicbox_music_api::SourceToMusicApi + Send + Sync>>,
        device: Box<switchy_upnp::Device>,
        service: Box<switchy_upnp::Service>,
        handle: switchy_upnp::listener::Handle,
    },
}

impl std::fmt::Debug for PlayerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "Local"),
            #[cfg(feature = "upnp")]
            Self::Upnp {
                device, service, ..
            } => f
                .debug_struct("Upnp")
                .field("device", device)
                .field("service", service)
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaybackTargetSessionPlayer {
    pub playback_target: ApiPlaybackTarget,
    pub session_id: u64,
    pub player: PlaybackHandler,
    pub player_type: PlayerType,
}

#[derive(Clone, Default)]
pub struct AppState {
    pub persistence: Arc<RwLock<Option<Arc<SqlitePersistence>>>>,
    pub api_url: Arc<RwLock<Option<String>>>,
    pub profile: Arc<RwLock<Option<String>>>,
    pub ws_url: Arc<RwLock<Option<String>>>,
    pub ws_connection_id: Arc<RwLock<Option<String>>>,
    pub connection_id: Arc<RwLock<Option<String>>>,
    pub connection_name: Arc<RwLock<Option<String>>>,
    pub signature_token: Arc<RwLock<Option<String>>>,
    pub client_id: Arc<RwLock<Option<String>>>,
    pub api_token: Arc<RwLock<Option<String>>>,
    pub ws_token: Arc<RwLock<Option<CancellationToken>>>,
    pub ws_handle: Arc<RwLock<Option<WsHandle>>>,
    #[allow(clippy::type_complexity)]
    pub ws_join_handle: Arc<RwLock<Option<JoinHandle<Result<(), AppStateError>>>>>,
    pub audio_zone_active_api_players: Arc<RwLock<HashMap<u64, Vec<ApiPlayersMap>>>>,
    pub active_players: Arc<RwLock<Vec<PlaybackTargetSessionPlayer>>>,
    pub playback_quality: Arc<RwLock<Option<PlaybackQuality>>>,
    pub ws_message_buffer: Arc<RwLock<Vec<InboundPayload>>>,
    pub current_playback_target: Arc<RwLock<Option<PlaybackTarget>>>,
    pub current_connections: Arc<RwLock<Vec<ApiConnection>>>,
    pub pending_player_sessions: Arc<RwLock<HashMap<u64, u64>>>,
    pub current_sessions: Arc<RwLock<Vec<ApiSession>>>,
    pub default_download_location: Arc<std::sync::RwLock<Option<String>>>,
    #[allow(clippy::type_complexity)]
    pub on_current_sessions_updated_listeners: Vec<
        Arc<Box<dyn Fn(&[ApiSession]) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    >,
    #[allow(clippy::type_complexity)]
    pub on_audio_zone_with_sessions_updated_listeners: Vec<
        Arc<
            Box<
                dyn Fn(&[ApiAudioZoneWithSession]) -> Pin<Box<dyn Future<Output = ()> + Send>>
                    + Send
                    + Sync,
            >,
        >,
    >,
    #[allow(clippy::type_complexity)]
    pub on_connections_updated_listeners: Vec<
        Arc<
            Box<dyn Fn(&[ApiConnection]) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
        >,
    >,
    pub current_session_id: Arc<RwLock<Option<u64>>>,
    pub current_audio_zones: Arc<RwLock<Vec<ApiAudioZoneWithSession>>>,
    #[allow(clippy::type_complexity)]
    pub current_players: Arc<RwLock<Vec<ApiPlayersMap>>>,
    #[cfg(feature = "upnp")]
    pub upnp_av_transport_services: Arc<RwLock<Vec<switchy_upnp::player::UpnpAvTransportService>>>,
    #[allow(clippy::type_complexity)]
    pub on_before_handle_playback_update_listeners: Vec<
        Arc<
            Box<
                dyn Fn(&ApiUpdateSession) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
            >,
        >,
    >,
    #[allow(clippy::type_complexity)]
    pub on_after_handle_playback_update_listeners: Vec<
        Arc<
            Box<
                dyn Fn(&ApiUpdateSession) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
            >,
        >,
    >,
    #[allow(clippy::type_complexity)]
    pub on_before_update_playlist_listeners:
        Vec<Arc<Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>>,
    #[allow(clippy::type_complexity)]
    pub on_after_update_playlist_listeners: Vec<
        Arc<Box<dyn Fn(&ApiSession) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    >,
    #[allow(clippy::type_complexity)]
    pub on_before_handle_ws_message_listeners: Vec<
        Arc<
            Box<dyn Fn(&OutboundPayload) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
        >,
    >,
    #[allow(clippy::type_complexity)]
    pub on_after_handle_ws_message_listeners: Vec<
        Arc<
            Box<dyn Fn(&OutboundPayload) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
        >,
    >,
    #[allow(clippy::type_complexity)]
    pub on_before_set_state_listeners: Vec<
        Arc<Box<dyn Fn(&UpdateAppState) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    >,
    #[allow(clippy::type_complexity)]
    pub on_after_set_state_listeners: Vec<
        Arc<Box<dyn Fn(&UpdateAppState) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    >,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("api_url", &self.api_url)
            .field("profile", &self.profile)
            .field("ws_url", &self.ws_url)
            .field("ws_connection_id", &self.ws_connection_id)
            .field("connection_id", &self.connection_id)
            .field("connection_name", &self.connection_name)
            .field("signature_token", &self.signature_token)
            .field("client_id", &self.client_id)
            .field("api_token", &self.api_token)
            .field("ws_token", &self.ws_token)
            .field("ws_join_handle", &self.ws_join_handle)
            .field(
                "audio_zone_active_api_players",
                &self.audio_zone_active_api_players,
            )
            .field("active_players", &self.active_players)
            .field("playback_quality", &self.playback_quality)
            .field("ws_message_buffer", &self.ws_message_buffer)
            .field("current_playback_target", &self.current_playback_target)
            .field("current_connections", &self.current_connections)
            .field("pending_player_sessions", &self.pending_player_sessions)
            .field("current_sessions", &self.current_sessions)
            .field("default_download_location", &self.default_download_location)
            .field("current_session_id", &self.current_session_id)
            .field("current_audio_zones", &self.current_audio_zones)
            .field("current_players", &self.current_players)
            .finish_non_exhaustive()
    }
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_on_before_handle_playback_update_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(ApiUpdateSession) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_before_handle_playback_update_listeners
            .push(Arc::new(Box::new(move |update_session| {
                let listener = listener.clone();
                let update_session = update_session.to_owned();
                Box::pin(async move { listener(update_session).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_after_handle_playback_update_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(ApiUpdateSession) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_after_handle_playback_update_listeners
            .push(Arc::new(Box::new(move |update_session| {
                let listener = listener.clone();
                let update_session = update_session.to_owned();
                Box::pin(async move { listener(update_session).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_before_update_playlist_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn() -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_before_update_playlist_listeners
            .push(Arc::new(Box::new(move || {
                let listener = listener.clone();
                Box::pin(async move { listener().await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_after_update_playlist_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(ApiSession) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_after_update_playlist_listeners
            .push(Arc::new(Box::new(move |session| {
                let listener = listener.clone();
                let session = session.to_owned();
                Box::pin(async move { listener(session).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_before_handle_ws_message_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(OutboundPayload) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_before_handle_ws_message_listeners
            .push(Arc::new(Box::new(move |message| {
                let listener = listener.clone();
                let message = message.to_owned();
                Box::pin(async move { listener(message).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_after_handle_ws_message_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(OutboundPayload) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_after_handle_ws_message_listeners
            .push(Arc::new(Box::new(move |message| {
                let listener = listener.clone();
                let message = message.to_owned();
                Box::pin(async move { listener(message).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_before_set_state_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(UpdateAppState) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_before_set_state_listeners
            .push(Arc::new(Box::new(move |message| {
                let listener = listener.clone();
                let message = message.to_owned();
                Box::pin(async move { listener(message).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_after_set_state_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(UpdateAppState) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_after_set_state_listeners
            .push(Arc::new(Box::new(move |message| {
                let listener = listener.clone();
                let message = message.to_owned();
                Box::pin(async move { listener(message).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_current_sessions_updated_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(Vec<ApiSession>) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_current_sessions_updated_listeners
            .push(Arc::new(Box::new(move |message| {
                let listener = listener.clone();
                let message = message.to_owned();
                Box::pin(async move { listener(message).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_audio_zone_with_sessions_updated_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(Vec<ApiAudioZoneWithSession>) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_audio_zone_with_sessions_updated_listeners
            .push(Arc::new(Box::new(move |message| {
                let listener = listener.clone();
                let message = message.to_owned();
                Box::pin(async move { listener(message).await })
            })));
        self
    }

    #[must_use]
    pub fn with_on_connections_updated_listener<F: Future<Output = ()> + Send>(
        mut self,
        listener: impl Fn(Vec<ApiConnection>) -> F + Send + Sync + 'static,
    ) -> Self {
        let listener = Arc::new(Box::new(listener));
        self.on_connections_updated_listeners
            .push(Arc::new(Box::new(move |message| {
                let listener = listener.clone();
                let message = message.to_owned();
                Box::pin(async move { listener(message).await })
            })));
        self
    }

    pub async fn get_current_session(&self) -> Option<ApiSession> {
        self.get_current_session_ref().await.map(|x| x.clone())
    }

    pub async fn get_current_session_ref(&self) -> Option<RwLockReadGuard<ApiSession>> {
        let session_id = (*self.current_session_id.read().await)?;
        let binding = self.current_sessions.read().await;
        if !binding.iter().any(|x| x.session_id == session_id) {
            return None;
        }

        let binding: RwLockReadGuard<ApiSession> = RwLockReadGuard::map(binding, |x| {
            for session in x {
                if session.session_id == session_id {
                    return session;
                }
            }
            unreachable!();
        });

        Some(binding)
    }

    /// # Errors
    ///
    /// * If the persistence fails
    ///
    /// # Panics
    ///
    /// * If the `default_download_location` `RwLock` is poisoned
    pub async fn set_default_download_location(&self, path: String) -> Result<(), AppStateError> {
        self.persist_default_download_location(path.as_str())
            .await?;

        *self.default_download_location.write().unwrap() = Some(path);

        Ok(())
    }

    /// # Panics
    ///
    /// * If the `default_download_location` `RwLock` is poisoned
    #[must_use]
    pub fn get_default_download_location(&self) -> Option<String> {
        self.default_download_location.read().unwrap().clone()
    }

    /// # Errors
    ///
    /// * If there is a `PlayerError`
    /// * If the request is missing a `MoosicBox` profile
    /// * If an unknown error occurs
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    #[allow(clippy::too_many_lines)]
    pub async fn new_player(
        &self,
        session_id: u64,
        playback_target: ApiPlaybackTarget,
        output: AudioOutputFactory,
        player_type: PlayerType,
    ) -> Result<PlaybackHandler, AppStateError> {
        let profile = { self.profile.read().await.clone() };
        let Some(profile) = profile else {
            return Err(AppStateError::unknown("Missing profile"));
        };

        let mut headers = HashMap::new();
        headers.insert("moosicbox-profile".to_string(), profile);

        if self.api_token.read().await.is_some() {
            headers.insert(
                "Authorization".to_string(),
                self.api_token.read().await.as_ref().unwrap().to_string(),
            );
        }

        let query = if self.client_id.read().await.is_some()
            && self.signature_token.read().await.is_some()
        {
            let mut query = HashMap::new();
            query.insert(
                "clientId".to_string(),
                self.client_id.read().await.as_ref().unwrap().to_string(),
            );
            query.insert(
                "signature".to_string(),
                self.signature_token
                    .read()
                    .await
                    .as_ref()
                    .unwrap()
                    .to_string(),
            );
            Some(query)
        } else {
            None
        };

        let host = self
            .api_url
            .read()
            .await
            .clone()
            .ok_or_else(|| AppStateError::unknown("API_URL not set"))?;

        let player_source = PlayerSource::Remote {
            host: host.clone(),
            headers: Some(headers),
            query,
        };

        let mut player = match player_type {
            PlayerType::Local => {
                let local_player = LocalPlayer::new(player_source, Some(PlaybackType::Stream))
                    .await
                    .map_err(|e| {
                        AppStateError::unknown(format!(
                            "Failed to initialize new local player: {e:?}"
                        ))
                    })?
                    .with_output(output.clone());

                let playback = local_player.playback.clone();
                let receiver = local_player.receiver.clone();

                let handler = PlaybackHandler::new(local_player.clone())
                    .with_playback(playback)
                    .with_output(Some(Arc::new(std::sync::Mutex::new(output))))
                    .with_receiver(receiver);

                local_player
                    .playback_handler
                    .write()
                    .unwrap()
                    .replace(handler.clone());

                handler
            }
            #[cfg(feature = "upnp")]
            PlayerType::Upnp {
                source_to_music_api,
                device,
                service,
                handle,
            } => {
                let upnp_player = switchy_upnp::player::UpnpPlayer::new(
                    source_to_music_api,
                    *device,
                    *service,
                    player_source,
                    handle,
                );

                let playback = upnp_player.playback.clone();
                let receiver = upnp_player.receiver.clone();

                let handler = PlaybackHandler::new(upnp_player.clone())
                    .with_playback(playback)
                    .with_output(Some(Arc::new(std::sync::Mutex::new(output))))
                    .with_receiver(receiver);

                upnp_player
                    .playback_handler
                    .write()
                    .unwrap()
                    .replace(handler.clone());

                handler
            }
        };

        let session = {
            self.current_sessions
                .read()
                .await
                .iter()
                .find(|x| x.session_id == session_id)
                .cloned()
        };

        let profile = { self.profile.read().await.clone() };

        if let (Some(profile), Some(session)) = (profile.clone(), session) {
            log::debug!("new_player: init_from_api_session session={session:?}");
            if let Err(e) = player.init_from_api_session(profile, session).await {
                log::error!("Failed to init player from api session: {e:?}");
            }
        } else {
            log::debug!("new_player: No session info available for player yet");
            self.pending_player_sessions
                .write()
                .await
                .insert(player.id, session_id);
        }

        player
            .update_playback(
                false,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                *self.playback_quality.read().await,
                Some(session_id),
                profile,
                Some(playback_target.into()),
                false,
                None,
            )
            .await?;

        Ok(player)
    }

    /// # Panics
    ///
    /// * If any of the required state properties are missing
    pub async fn get_players(
        &self,
        session_id: u64,
        playback_target: Option<&ApiPlaybackTarget>,
    ) -> Vec<PlaybackHandler> {
        let mut playback_handlers = vec![];
        let active_players = self.active_players.read().await.clone();

        for player in active_players {
            let target = &player.playback_target;
            moosicbox_logging::debug_or_trace!(
                ("get_players: Checking if player is in session: target={target:?} session_id={session_id} player_zone_id={playback_target:?}"),
                ("get_players: Checking if player is in session: target={target:?} session_id={session_id} player_zone_id={playback_target:?} player={player:?}")
            );
            let same_session = player.player.playback
                .read()
                .unwrap()
                .as_ref()
                .is_some_and(|p| {
                    moosicbox_logging::debug_or_trace!(
                        (
                            "get_players: player playback.session_id={} target session_id={session_id}",
                            p.session_id
                        ),
                        (
                            "get_players: player playback.session_id={} target session_id={session_id} player={player:?}",
                            p.session_id
                        )
                    );
                    log::trace!(
                        "get_players: player playback.session_id={} target session_id={session_id} player={player:?}",
                        p.session_id
                    );
                    p.session_id == session_id
                });
            if !same_session {
                continue;
            }
            moosicbox_logging::debug_or_trace!(
                ("get_players: Checking if player is in zone: target={target:?} session_id={session_id} player_zone_id={playback_target:?}"),
                ("get_players: Checking if player is in zone: target={target:?} session_id={session_id} player_zone_id={playback_target:?} player={player:?}")
            );
            if playback_target.is_some_and(|x| x != target) {
                continue;
            }

            playback_handlers.push(player.player);
        }
        playback_handlers
    }

    /// # Errors
    ///
    /// * If there is a `PlayerError`
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    pub async fn reinit_players(&self) -> Result<(), AppStateError> {
        let mut players_map = self.active_players.write().await;
        let ids = {
            players_map
                .iter()
                .map(|x| {
                    (
                        x.playback_target.clone(),
                        x.session_id,
                        x.player.clone(),
                        x.player_type.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };

        for (i, (playback_target, session_id, player, ptype)) in ids.into_iter().enumerate() {
            let output = player.output.as_ref().unwrap().lock().unwrap().clone();
            log::debug!(
                "reinit_players: playback_target={playback_target:?} session_id={session_id} output={output:?}"
            );
            let mut created_player = self
                .new_player(session_id, playback_target.clone(), output, ptype.clone())
                .await?;

            let playback = player.playback.read().unwrap().clone();

            if let Some(playback) = playback {
                created_player
                    .update_playback(
                        false,
                        None,
                        None,
                        Some(playback.playing),
                        Some(playback.position),
                        Some(playback.progress),
                        Some(playback.volume.load(std::sync::atomic::Ordering::SeqCst)),
                        Some(playback.tracks.clone()),
                        Some(playback.quality),
                        Some(playback.session_id),
                        Some(playback.profile),
                        Some(playback_target.clone().into()),
                        false,
                        None,
                    )
                    .await?;
            }

            players_map[i] = PlaybackTargetSessionPlayer {
                playback_target,
                session_id,
                player: created_player,
                player_type: ptype,
            };
        }
        drop(players_map);

        Ok(())
    }

    /// # Errors
    ///
    /// * If a new player fails to be created
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    pub async fn set_audio_zone_active_players(
        &self,
        session_id: u64,
        audio_zone_id: u64,
        players: Vec<(ApiPlayer, PlayerType, AudioOutputFactory)>,
    ) -> Result<(), AppStateError> {
        log::debug!(
            "set_audio_zone_active_players: session_id={session_id} audio_zone_id={audio_zone_id} {:?}",
            players.iter().map(|(x, _, _)| x).collect::<Vec<_>>()
        );

        let mut api_players_map = self.audio_zone_active_api_players.write().await;
        api_players_map.insert(audio_zone_id, players.clone());

        {
            let mut players_map = self.active_players.write().await;
            for (player, ptype, output) in &players {
                if let Some(existing) = players_map.iter().find(|x| match x.playback_target {
                    ApiPlaybackTarget::AudioZone { audio_zone_id: id } => id == audio_zone_id,
                    ApiPlaybackTarget::ConnectionOutput { .. } => false,
                }) {
                    let different_session = {
                        existing
                            .player
                            .playback
                            .read()
                            .unwrap()
                            .as_ref()
                            .is_none_or(|p| p.session_id != session_id)
                    };

                    let same_output =
                        existing.player.output.as_ref().is_some_and(|output| {
                            output.lock().unwrap().id == player.audio_output_id
                        });

                    if !different_session && same_output {
                        log::debug!(
                            "Skipping existing player for audio_zone_id={audio_zone_id} audio_output_id={}",
                            player.audio_output_id
                        );
                        continue;
                    }
                }

                let playback_target = ApiPlaybackTarget::AudioZone { audio_zone_id };
                let player = self
                    .new_player(
                        session_id,
                        playback_target.clone(),
                        output.clone(),
                        ptype.clone(),
                    )
                    .await?;
                log::debug!(
                    "set_audio_zone_active_players: audio_zone_id={audio_zone_id} session_id={session_id:?}"
                );
                let playback_target_session_player = PlaybackTargetSessionPlayer {
                    playback_target,
                    session_id,
                    player,
                    player_type: ptype.clone(),
                };
                if let Some((i, _)) =
                    players_map
                        .iter()
                        .enumerate()
                        .find(|(_, x)| match x.playback_target {
                            ApiPlaybackTarget::AudioZone { audio_zone_id: id } => {
                                id == audio_zone_id && x.session_id == session_id
                            }
                            ApiPlaybackTarget::ConnectionOutput { .. } => false,
                        })
                {
                    players_map[i] = playback_target_session_player;
                } else {
                    players_map.push(playback_target_session_player);
                }
            }
        }

        drop(api_players_map);

        Ok(())
    }

    /// # Errors
    ///
    /// * If `set_audio_zone_active_players` fails
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    pub async fn update_audio_zones(&self) -> Result<(), AppStateError> {
        let audio_zones_binding = self.current_audio_zones.read().await;
        let audio_zones: &[ApiAudioZoneWithSession] = audio_zones_binding.as_ref();
        let players_binding = self.current_players.read().await;
        let players: &[(ApiPlayer, PlayerType, AudioOutputFactory)] = players_binding.as_ref();

        log::debug!(
            "\
            Updating audio zones\n\t\
            audio_zones={audio_zones:?}\n\t\
            players={:?}\n\t\
            ",
            players.iter().map(|(x, _, _)| x).collect::<Vec<_>>()
        );

        for audio_zone in audio_zones {
            let players = audio_zone
                .players
                .clone()
                .into_iter()
                .filter_map(|x| {
                    players
                        .iter()
                        .find(|(p, _, _)| p.player_id == x.player_id)
                        .map(|(_, ptype, output)| (x, ptype.clone(), output.clone()))
                })
                .collect::<Vec<_>>();

            if !players.is_empty() {
                self.set_audio_zone_active_players(audio_zone.session_id, audio_zone.id, players)
                    .await?;
            }
        }

        drop(audio_zones_binding);
        drop(players_binding);

        Ok(())
    }

    /// # Errors
    ///
    /// * If the `UpnpDeviceScanner` fails
    /// * If the `UpnpAvTransportService` fails to convert into an `AudioOutput`
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    #[cfg(feature = "upnp")]
    pub async fn init_upnp_players(&self) -> Result<(), AppStateError> {
        use moosicbox_session::models::RegisterPlayer;

        switchy_upnp::scan_devices()
            .await
            .map_err(InitUpnpError::UpnpDeviceScanner)?;

        let services = {
            let mut av_transport_services = self.upnp_av_transport_services.write().await;
            av_transport_services.clear();

            for device in switchy_upnp::devices().await {
                let service_id = "urn:upnp-org:serviceId:AVTransport";
                if let Ok((device, service)) =
                    switchy_upnp::get_device_and_service(&device.udn, service_id)
                {
                    av_transport_services
                        .push(switchy_upnp::player::UpnpAvTransportService { device, service });
                }
            }

            av_transport_services.clone()
        };

        let mut outputs = Vec::with_capacity(services.len());

        let url_string = { self.api_url.read().await.clone() };
        let url = url_string.as_deref();

        let Some(url) = url else {
            return Ok(());
        };

        let Some(profile) = self.profile.read().await.clone() else {
            return Ok(());
        };

        for service in services {
            let player_type = PlayerType::Upnp {
                source_to_music_api: Arc::new(Box::new(SourceToRemoteLibrary {
                    host: url.to_owned(),
                    profile: profile.clone(),
                })),
                device: Box::new(service.device.clone()),
                service: Box::new(service.service.clone()),
                handle: UPNP_LISTENER_HANDLE.get().unwrap().clone(),
            };
            let output: AudioOutputFactory =
                service.try_into().map_err(InitUpnpError::AudioOutput)?;

            outputs.push((output, player_type));
        }

        if outputs.is_empty() {
            log::debug!("No players to register");
            return Ok(());
        }

        let register_players_payload = outputs
            .iter()
            .map(|(x, _)| RegisterPlayer {
                audio_output_id: x.id.clone(),
                name: x.name.clone(),
            })
            .collect::<Vec<_>>();

        let api_players = self.register_players(&register_players_payload).await?;

        log::debug!("init_upnp_players: players={api_players:?}");

        let api_players = api_players
            .into_iter()
            .filter_map(|p| {
                if let Some((output, ptype)) = outputs
                    .iter()
                    .find(|(output, _ptype)| output.id == p.audio_output_id)
                {
                    Some((p, ptype.clone(), output.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        self.add_players_to_current_players(api_players).await;

        let ids = {
            self.current_sessions
                .read()
                .await
                .iter()
                .map(|x| x.session_id)
                .collect::<Vec<_>>()
        };

        self.update_connection_outputs(&ids).await?;

        Ok(())
    }

    /// # Errors
    ///
    /// * If the request is missing a `MoosicBox` profile
    /// * If the `RegisterPlayer` `players` fail to serialize
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    pub async fn register_players(
        &self,
        players: &[RegisterPlayer],
    ) -> Result<Vec<ApiPlayer>, AppStateError> {
        let connection_id = self.connection_id.read().await.clone().unwrap();

        let url = format!("session/register-players?connectionId={connection_id}");
        let body = Some(serde_json::to_value(players).map_err(RegisterPlayersError::Serde)?);

        let response = match self.api_proxy_post(url.clone(), body.clone(), None).await {
            Ok(value) => serde_json::from_value(value).unwrap(),
            Err(e) => {
                let AppStateError::ProxyRequest(ProxyRequestError::FailureResponse {
                    status, ..
                }) = e
                else {
                    return Err(e);
                };
                if status != 404 {
                    return Err(e);
                }

                let Some(name) = self.connection_name.read().await.clone() else {
                    return Err(AppStateError::unknown(
                        "Connection name required to create a connection",
                    ));
                };

                let response = self
                    .api_proxy_post(
                        "session/register-connection".to_string(),
                        Some(
                            serde_json::to_value(RegisterConnection {
                                connection_id,
                                name,
                                players: players.to_vec(),
                            })
                            .map_err(RegisterPlayersError::Serde)?,
                        ),
                        None,
                    )
                    .await?;

                let connection: ApiConnection = serde_json::from_value(response).unwrap();

                connection.players
            }
        };

        Ok(response)
    }

    /// # Errors
    ///
    /// * If the `api_url` is not set in the state
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    /// * If the headers object is not a valid JSON object
    pub async fn api_proxy_get(
        &self,
        url: String,
        headers: Option<HashMap<String, String>>,
    ) -> Result<serde_json::Value, AppStateError> {
        self.api_proxy("get", url, None, headers).await
    }

    /// # Errors
    ///
    /// * If the `api_url` is not set in the state
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    /// * If the headers object is not a valid JSON object
    pub async fn api_proxy_post(
        &self,
        url: String,
        body: Option<serde_json::Value>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<serde_json::Value, AppStateError> {
        self.api_proxy("post", url, body, headers).await
    }

    /// # Errors
    ///
    /// * If the `api_url` is not set in the state
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    /// * If the headers object is not a valid JSON object
    pub async fn api_proxy(
        &self,
        method: &str,
        url: String,
        body: Option<serde_json::Value>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<serde_json::Value, AppStateError> {
        let mut headers = headers.unwrap_or_default();

        if !headers.contains_key("moosicbox-profile") {
            let profile = { self.profile.read().await.clone() };
            let Some(profile) = profile else {
                return Err(RegisterPlayersError::MissingProfile.into());
            };
            headers.insert("moosicbox-profile".to_string(), profile);
        }
        if matches!(method, "post" | "put") && !headers.contains_key("content-type") {
            headers.insert("content-type".to_string(), "application/json".to_string());
        }

        let api_token = self.api_token.read().await;
        if let Some(api_token) = api_token.as_ref() {
            headers.insert("Authorization".to_string(), format!("bearer {api_token}"));
        }
        drop(api_token);

        let client_id = self
            .client_id
            .read()
            .await
            .clone()
            .filter(|x| !x.is_empty())
            .map(|x| format!("{}clientId={x}", if url.contains('?') { '&' } else { '?' }))
            .unwrap_or_default();

        let url = format!(
            "{}/{url}{client_id}",
            self.api_url
                .read()
                .await
                .clone()
                .ok_or_else(|| AppStateError::unknown(format!("API_URL not set ({url})")))?
        );
        log::info!("Posting url from proxy: {url}");

        let mut builder = match method {
            "get" => PROXY_CLIENT.get(&url),
            "post" => PROXY_CLIENT.post(&url),
            _ => return Err(AppStateError::unknown(format!("Invalid method '{method}'"))),
        };

        for header in headers {
            builder = builder.header(&header.0, &header.1);
        }

        if let Some(body) = body {
            builder = builder.json(&body);
        }

        Ok(self.send_request_builder(builder).await?)
    }

    /// # Errors
    ///
    /// * If failed to parse the JSON response
    /// * If the HTTP request fails
    pub async fn send_request_builder(
        &self,
        builder: RequestBuilder,
    ) -> Result<serde_json::Value, ProxyRequestError> {
        log::debug!("send_request_builder: Sending request");
        let resp = builder.send().await?;
        log::debug!("send_request_builder: status_code={}", resp.status());
        let status = resp.status();
        let success = status.is_success();
        let text = resp.text().await?;
        if success {
            Ok(serde_json::from_str(&text)?)
        } else {
            log::error!("Failure response: ({text:?})");
            Err(ProxyRequestError::FailureResponse {
                status: status.into(),
                text,
            })
        }
    }

    pub async fn add_players_to_current_players(&self, players: Vec<ApiPlayersMap>) {
        let mut existing_players = self.current_players.write().await;

        let new_players = players
            .into_iter()
            .filter(|(p, _, _)| {
                !existing_players
                    .iter()
                    .any(|(existing, _, _)| existing.player_id == p.player_id)
            })
            .collect::<Vec<_>>();

        log::debug!(
            "add_players_to_current_players: Adding new_players={:?}",
            new_players.iter().map(|(x, _, _)| x).collect::<Vec<_>>()
        );

        existing_players.extend(new_players);
    }

    /// # Errors
    ///
    /// * If any `UpnpAvTransportService`s fail to convert to `AudioOutputFactory`s
    /// * If there is a `PlayerError`
    pub async fn update_connection_outputs(
        &self,
        session_ids: &[u64],
    ) -> Result<(), AppStateError> {
        let Some(current_connection_id) = ({ self.connection_id.read().await.clone() }) else {
            return Ok(());
        };

        let local_outputs = moosicbox_audio_output::output_factories().await;
        #[cfg(feature = "upnp")]
        let upnp_outputs = self
            .upnp_av_transport_services
            .read()
            .await
            .iter()
            .cloned()
            .map(TryInto::try_into)
            .collect::<Result<Vec<AudioOutputFactory>, moosicbox_audio_output::AudioOutputError>>()
            .map_err(|e| AppStateError::unknown(format!("Error: {e:?}")))?;

        #[cfg(not(feature = "upnp"))]
        let upnp_outputs = vec![];

        let outputs = [local_outputs, upnp_outputs].concat();

        for output in outputs {
            let playback_target = ApiPlaybackTarget::ConnectionOutput {
                connection_id: current_connection_id.clone(),
                output_id: output.id.clone(),
            };
            let output_id = &output.id;
            log::debug!(
                "update_connection_outputs: ApiPlaybackTarget::ConnectionOutput current_connection_id={current_connection_id} output_id={output_id}"
            );

            let binding = self.current_players.read().await;
            let current_players: &[(ApiPlayer, PlayerType, AudioOutputFactory)] = binding.as_ref();

            if let Some((_player, ptype, output)) = current_players.iter().find(|(x, _, _)| {
                log::trace!(
                    "update_connection_outputs: ApiPlaybackTarget::ConnectionOutput checking '{}' == '{output_id}'",
                    x.audio_output_id
                );
                &x.audio_output_id == output_id
            }) {
                for session_id in session_ids {
                    let session_id = *session_id;
                    log::debug!("update_connection_outputs: ApiPlaybackTarget::ConnectionOutput creating player for output_id={output_id} session_id={session_id} playback_target={playback_target:?}");

                    let player = self.new_player(
                        session_id,
                        playback_target.clone(),
                        output.clone(),
                        ptype.clone(),
                    )
                    .await?;

                    moosicbox_logging::debug_or_trace!(
                        ("update_connection_outputs: ApiPlaybackTarget::ConnectionOutput created new player={}", player.id),
                        ("update_connection_outputs: ApiPlaybackTarget::ConnectionOutput created new player={:?}", player)
                    );
                    let player = PlaybackTargetSessionPlayer {
                        playback_target: playback_target.clone(),
                        session_id,
                        player,
                        player_type: ptype.clone(),
                    };

                    let mut players = self.active_players.write().await;

                    if !players.iter().any(|x| x.session_id == session_id && x.playback_target == playback_target) {
                        players.push(player);
                    }
                }
            }

            drop(binding);
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If failed to scan outputs
    /// * If failed to update audio zones
    /// * If failed to update connection outputs
    pub async fn scan_outputs(&self) -> Result<(), AppStateError> {
        log::debug!("scan_outputs: attempting to scan outputs");
        {
            if self.api_url.as_ref().read().await.is_none()
                || self.connection_id.as_ref().read().await.is_none()
            {
                log::debug!("scan_outputs: missing API_URL or CONNECTION_ID, not scanning");
                return Ok(());
            }
        }

        if moosicbox_audio_output::output_factories().await.is_empty() {
            moosicbox_audio_output::scan_outputs()
                .await
                .map_err(ScanOutputsError::AudioOutputScanner)?;
        }

        let outputs = moosicbox_audio_output::output_factories().await;
        log::debug!("scan_outputs: scanned outputs={outputs:?}");

        let players = outputs
            .iter()
            .map(|x| RegisterPlayer {
                audio_output_id: x.id.clone(),
                name: x.name.clone(),
            })
            .collect::<Vec<_>>();

        if players.is_empty() {
            log::debug!("No players to register");
            return Ok(());
        }

        let players = self.register_players(&players).await?;

        log::debug!("scan_outputs: players={players:?}");

        let players = players
            .into_iter()
            .filter_map(|p| {
                outputs
                    .iter()
                    .find(|output| output.id == p.audio_output_id)
                    .map(|output| (p, PlayerType::Local, output.clone()))
            })
            .collect::<Vec<_>>();

        self.add_players_to_current_players(players).await;

        self.update_audio_zones().await?;
        let ids = {
            self.current_sessions
                .read()
                .await
                .iter()
                .map(|x| x.session_id)
                .collect::<Vec<_>>()
        };
        self.update_connection_outputs(&ids).await?;

        Ok(())
    }

    /// # Panics
    ///
    /// * If the `Playback` `RwLock` is poisoned
    pub async fn get_session_playback_for_player(
        &self,
        mut update: ApiUpdateSession,
        player: &PlaybackHandler,
    ) -> ApiUpdateSession {
        let session_id = {
            player
                .playback
                .read()
                .unwrap()
                .as_ref()
                .map(|x| x.session_id)
        };

        if let Some(session_id) = session_id {
            if session_id != update.session_id {
                let session = {
                    self.current_sessions
                        .read()
                        .await
                        .iter()
                        .find(|s| s.session_id == session_id)
                        .cloned()
                };

                if let Some(session) = session {
                    update.session_id = session_id;

                    if update.position.is_none() {
                        update.position = session.position;
                    }
                    if update.seek.is_none() {
                        update.seek = session.seek;
                    }
                    if update.volume.is_none() {
                        update.volume = session.volume;
                    }
                    if update.playlist.is_none() {
                        update.playlist = Some(ApiUpdateSessionPlaylist {
                            session_playlist_id: session.playlist.session_playlist_id,
                            tracks: session.playlist.tracks,
                        });
                    }
                }
            }
        }

        update
    }

    /// # Errors
    ///
    /// * If fails to `updated_connection_details`
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub async fn set_state(&self, state: UpdateAppState) -> Result<(), AppStateError> {
        log::debug!("set_state: state={state:?}");

        for listener in &self.on_before_set_state_listeners {
            listener(&state).await;
        }

        let mut updated_connection_details = false;

        if let Some(state_connection_id) = &state.connection_id {
            let mut connection_id = self.connection_id.write().await;
            let is_empty = state_connection_id.as_ref().is_some_and(String::is_empty);

            if connection_id.as_ref() == state_connection_id.as_ref()
                || is_empty && connection_id.is_none()
            {
                log::debug!("set_state: no update to CONNECTION_ID");
            } else if is_empty {
                log::debug!("set_state: empty CONNECTION_ID, removing value");
                connection_id.take();
                drop(connection_id);
                updated_connection_details = true;
            } else {
                log::debug!(
                    "set_state: updating CONNECTION_ID from '{:?}' -> '{:?}'",
                    connection_id.as_ref(),
                    state_connection_id
                );
                (*connection_id).clone_from(state_connection_id);
                drop(connection_id);
                updated_connection_details = true;
            }
        }

        if let Some(state_connection_name) = &state.connection_name {
            let mut connection_name = self.connection_name.write().await;
            let is_empty = state_connection_name.as_ref().is_some_and(String::is_empty);

            if connection_name.as_ref() == state_connection_name.as_ref()
                || is_empty && connection_name.is_none()
            {
                log::debug!("set_state: no update to CONNECTION_NAME");
            } else if is_empty {
                log::debug!("set_state: empty CONNECTION_NAME, removing value");
                connection_name.take();
                drop(connection_name);
                updated_connection_details = true;
            } else {
                log::debug!(
                    "set_state: updating CONNECTION_NAME from '{:?}' -> '{:?}'",
                    connection_name.as_ref(),
                    state_connection_name
                );
                (*connection_name).clone_from(state_connection_name);
                drop(connection_name);
                updated_connection_details = true;
            }
        }

        if let Some(state_client_id) = &state.client_id {
            let mut client_id = self.client_id.write().await;
            let is_empty = state_client_id.as_ref().is_some_and(String::is_empty);

            if client_id.as_ref() == state_client_id.as_ref() || is_empty && client_id.is_none() {
                log::debug!("set_state: no update to CLIENT_ID");
            } else if is_empty {
                log::debug!("set_state: empty CLIENT_ID, removing value");
                client_id.take();
                drop(client_id);
                updated_connection_details = true;
            } else {
                log::debug!(
                    "set_state: updating CLIENT_ID from '{:?}' -> '{:?}'",
                    client_id.as_ref(),
                    state_client_id
                );
                (*client_id).clone_from(state_client_id);
                drop(client_id);
                updated_connection_details = true;
            }
        }

        if let Some(state_signature_token) = &state.signature_token {
            let mut signature_token = self.signature_token.write().await;
            let is_empty = state_signature_token.as_ref().is_some_and(String::is_empty);

            if signature_token.as_ref() == state_signature_token.as_ref()
                || is_empty && signature_token.is_none()
            {
                log::debug!("set_state: no update to SIGNATURE_TOKEN");
            } else if is_empty {
                log::debug!("set_state: empty SIGNATURE_TOKEN, removing value");
                signature_token.take();
                drop(signature_token);
                updated_connection_details = true;
            } else {
                log::debug!(
                    "set_state: updating SIGNATURE_TOKEN from '{:?}' -> '{:?}'",
                    signature_token.as_ref(),
                    state_signature_token
                );
                (*signature_token).clone_from(state_signature_token);
                drop(signature_token);
                updated_connection_details = true;
            }
        }

        if let Some(state_api_token) = &state.api_token {
            let mut api_token = self.api_token.write().await;
            let is_empty = state_api_token.as_ref().is_some_and(String::is_empty);

            if api_token.as_ref() == state_api_token.as_ref() || is_empty && api_token.is_none() {
                log::debug!("set_state: no update to API_TOKEN");
            } else if is_empty {
                log::debug!("set_state: empty API_TOKEN, removing value");
                api_token.take();
                drop(api_token);
                updated_connection_details = true;
            } else {
                log::debug!(
                    "set_state: updating API_TOKEN from '{:?}' -> '{:?}'",
                    api_token.as_ref(),
                    state_api_token
                );
                (*api_token).clone_from(state_api_token);
                drop(api_token);
                updated_connection_details = true;
            }
        }

        if let Some(state_api_url) = &state.api_url {
            let mut api_url = self.api_url.write().await;
            let is_empty = state_api_url.as_ref().is_some_and(String::is_empty);

            if api_url.as_ref() == state_api_url.as_ref() || is_empty && api_url.is_none() {
                log::debug!("set_state: no update to API_URL");
            } else if is_empty {
                log::debug!("set_state: empty API_URL, removing value");
                api_url.take();
                drop(api_url);
                updated_connection_details = true;
            } else {
                log::debug!(
                    "set_state: updating API_URL from '{:?}' -> '{:?}'",
                    api_url.as_ref(),
                    state_api_url
                );
                (*api_url).clone_from(state_api_url);
                drop(api_url);
                updated_connection_details = true;
            }
        }

        if let Some(state_profile) = &state.profile {
            let mut profile = self.profile.write().await;
            let is_empty = state_profile.as_ref().is_some_and(String::is_empty);

            if profile.as_ref() == state_profile.as_ref() || is_empty && profile.is_none() {
                log::debug!("set_state: no update to PROFILE");
            } else if is_empty {
                log::debug!("set_state: empty PROFILE, removing value");
                profile.take();
                drop(profile);
                updated_connection_details = true;
            } else {
                log::debug!(
                    "set_state: updating PROFILE from '{:?}' -> '{:?}'",
                    profile.as_ref(),
                    state_profile
                );
                (*profile).clone_from(state_profile);
                drop(profile);
                updated_connection_details = true;
            }
        }

        if let Some(state_playback_target) = &state.playback_target {
            (*self.current_playback_target.write().await).clone_from(state_playback_target);
        }

        if let Some(state_current_session_id) = state.current_session_id {
            *self.current_session_id.write().await = state_current_session_id;
        }

        if state.current_session_id.is_some_and(|x| x.is_some()) {
            self.update_playlist().await;
        }

        if updated_connection_details {
            self.update_connection_state().await?;
        }

        for listener in &self.on_after_set_state_listeners {
            listener(&state).await;
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If any of the connection state fails to update
    pub async fn update_connection_state(&self) -> Result<(), AppStateError> {
        let has_connection_id = { self.connection_id.read().await.is_some() };
        log::debug!("update_connection_state: has_connection_id={has_connection_id}");

        if has_connection_id {
            moosicbox_task::spawn("set_state: scan_outputs", {
                let state = self.clone();
                async move {
                    log::debug!("Attempting to scan_outputs...");
                    state.scan_outputs().await
                }
            });

            #[cfg(feature = "upnp")]
            let inited_upnp_players = moosicbox_task::spawn("set_state: init_upnp_players", {
                let state = self.clone();
                async move {
                    log::debug!("Attempting to init_upnp_players...");
                    state.init_upnp_players().await
                }
            });

            let reinited_players = moosicbox_task::spawn("set_state: reinit_players", {
                let state = self.clone();
                async move {
                    #[cfg(feature = "upnp")]
                    inited_upnp_players
                        .await
                        .map_err(|e| AppStateError::unknown(e.to_string()))??;
                    log::debug!("Attempting to reinit_players...");
                    state.reinit_players().await
                }
            });

            moosicbox_task::spawn("set_state: fetch_audio_zones", {
                let state = self.clone();
                async move {
                    reinited_players
                        .await
                        .map_err(|e| AppStateError::unknown(e.to_string()))??;
                    log::debug!("Attempting to fetch_audio_zones...");
                    state.fetch_audio_zones().await
                }
            });
        }

        self.close_ws_connection().await?;

        let ws = moosicbox_task::spawn("set_state: init_ws_connection", {
            let state = self.clone();
            async move {
                loop {
                    log::debug!("Attempting to init_ws_connection...");
                    match state.start_ws_connection().await {
                        Ok(()) => {
                            log::debug!("ws connection closed");
                            break;
                        }
                        Err(e) => {
                            if matches!(e, AppStateError::ConnectWs(ConnectWsError::Unauthorized)) {
                                if state.signature_token.read().await.is_none() {
                                    state.fetch_signature_token().await?;
                                    continue;
                                }

                                log::error!("ws connection Unauthorized: {e:?}");
                                return Err(e);
                            }

                            log::error!("ws connection error: {e:?}");

                            return Err(e);
                        }
                    }
                }

                Ok::<_, AppStateError>(())
            }
        });

        self.ws_join_handle.write().await.replace(ws);

        Ok(())
    }

    async fn fetch_signature_token(&self) -> Result<(), AppStateError> {
        if self.signature_token.read().await.is_none() {
            let response = self
                .api_proxy_post("auth/signature-token".to_string(), None, None)
                .await?;
            let token = response
                .get("token")
                .ok_or_else(|| AppStateError::unknown("Missing token"))?
                .as_str()
                .ok_or_else(|| AppStateError::unknown("Invalid token"))?
                .to_string();
            let mut signature_token = self.signature_token.write().await;

            *signature_token = Some(token);
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If the http proxy request fails
    /// * If the http response fails to deserialize
    /// * If the audio zones fail to update
    pub async fn fetch_audio_zones(&self) -> Result<(), AppStateError> {
        let response = self
            .api_proxy_get("audio-zone/with-session".to_string(), None)
            .await?;

        log::debug!("fetch_audio_zones: audio_zones={response}");

        let zones: Page<ApiAudioZoneWithSession> =
            serde_json::from_value(response).map_err(FetchAudioZonesError::Serde)?;

        *self.current_audio_zones.write().await = zones.into_items();

        self.update_audio_zones().await?;

        Ok(())
    }
}
