//! Application state management for `MoosicBox`.
//!
//! This crate provides centralized state management for `MoosicBox` applications,
//! including `WebSocket` connections, audio playback sessions, player management,
//! and persistent storage.
//!
//! # Main Components
//!
//! * [`AppState`] - Central application state containing all runtime configuration
//! * [`UpdateAppState`] - Update parameters for modifying application state
//! * [`PlaybackTarget`] - Target destinations for audio playback (re-exported from `moosicbox_session`)
//!
//! # Features
//!
//! * `upnp` - Enables UPnP/DLNA device discovery and playback support
//! * Audio codec features: `aac`, `flac`, `mp3`, `opus`
//! * Music source features: `qobuz`, `tidal`, `yt`
//!
//! # Example
//!
//! ```rust
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use moosicbox_app_state::AppState;
//!
//! // Create new application state
//! let state = AppState::new();
//!
//! // Configure with persistence
//! # #[cfg(feature = "embedded")]
//! # {
//! let state = state.with_persistence_in_memory().await?;
//! # }
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
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
use switchy_async::sync::{RwLock, RwLockReadGuard};
use switchy_async::util::CancellationToken;
use switchy_http::RequestBuilder;
use thiserror::Error;

mod persistence;

/// `WebSocket` connection management and message handling.
///
/// This module provides functionality for establishing and managing `WebSocket`
/// connections to `MoosicBox` servers, handling incoming/outgoing messages, and
/// synchronizing playback state across clients.
pub mod ws;

type ApiPlayersMap = (ApiPlayer, PlayerType, AudioOutputFactory);

static PROXY_CLIENT: LazyLock<switchy_http::Client> = LazyLock::new(switchy_http::Client::new);

/// Global `UPnP` listener handle for managing `UPnP` device discovery and events.
///
/// This handle is initialized once when `UPnP` support is enabled and used throughout
/// the application to interact with `UPnP`/DLNA devices on the network.
#[cfg(feature = "upnp")]
pub static UPNP_LISTENER_HANDLE: std::sync::OnceLock<moosicbox_upnp::listener::Handle> =
    std::sync::OnceLock::new();

/// Adapter for mapping music sources to remote library API instances.
///
/// Used with `UPnP` players to provide access to the music library via
/// the remote library API.
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

/// Errors that can occur during proxied API requests.
#[derive(Debug, Error)]
pub enum ProxyRequestError {
    /// JSON serialization/deserialization error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// HTTP client error
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// Non-success HTTP response from the server
    #[error("Failure response ({status}): {text}")]
    FailureResponse {
        /// HTTP status code
        status: u16,
        /// Response body text
        text: String,
    },
}

/// Errors that can occur when fetching audio zones from the server.
#[derive(Debug, Error)]
pub enum FetchAudioZonesError {
    /// JSON serialization/deserialization error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// Required `MoosicBox` profile is missing
    #[error("Missing profile")]
    MissingProfile,
}

/// Errors that can occur when scanning for audio outputs.
#[derive(Debug, Error)]
pub enum ScanOutputsError {
    /// Audio output scanner error
    #[error(transparent)]
    AudioOutputScanner(#[from] AudioOutputScannerError),
    /// JSON serialization/deserialization error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Errors that can occur during `UPnP` player initialization.
#[cfg(feature = "upnp")]
#[derive(Debug, Error)]
pub enum InitUpnpError {
    /// `UPnP` device scanner error
    #[error(transparent)]
    UpnpDeviceScanner(#[from] switchy_upnp::UpnpDeviceScannerError),
    /// Audio output configuration error
    #[error(transparent)]
    AudioOutput(#[from] moosicbox_audio_output::AudioOutputError),
    /// Error registering players with the server
    #[error(transparent)]
    RegisterPlayers(#[from] RegisterPlayersError),
}

/// Errors that can occur when registering players with the server.
#[derive(Debug, Error)]
pub enum RegisterPlayersError {
    /// JSON serialization/deserialization error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// Required `MoosicBox` profile is missing
    #[error("Missing profile")]
    MissingProfile,
}

/// Primary error type for application state operations.
///
/// This error type encompasses all possible errors that can occur during
/// application state management, including player operations, `WebSocket`
/// communication, persistence, and API requests.
#[derive(Debug, Error)]
pub enum AppStateError {
    /// Unknown error with custom message
    #[error("Unknown({0})")]
    Unknown(String),
    /// Action is missing required parameter
    #[error("Action missing param")]
    ActionMissingParam,
    /// Action has invalid parameter value
    #[error("Action invalid param")]
    ActionInvalidParam,
    /// Audio player error
    #[error(transparent)]
    Player(#[from] PlayerError),
    /// `UPnP` initialization error
    #[cfg(feature = "upnp")]
    #[error(transparent)]
    InitUpnp(#[from] InitUpnpError),
    /// Player registration error
    #[error(transparent)]
    RegisterPlayers(#[from] RegisterPlayersError),
    /// Audio output scanning error
    #[error(transparent)]
    ScanOutputs(#[from] ScanOutputsError),
    /// `WebSocket` initialization error
    #[error(transparent)]
    InitWs(#[from] ws::InitWsError),
    /// `WebSocket` close error
    #[error(transparent)]
    CloseWs(#[from] ws::CloseWsError),
    /// `WebSocket` message send error
    #[error(transparent)]
    SendWsMessage(#[from] ws::SendWsMessageError),
    /// Audio zones fetch error
    #[error(transparent)]
    FetchAudioZones(#[from] FetchAudioZonesError),
    /// Proxied API request error
    #[error(transparent)]
    ProxyRequest(#[from] ProxyRequestError),
    /// `WebSocket` connection error
    #[error(transparent)]
    ConnectWs(#[from] ConnectWsError),
    /// Async task join error
    #[error(transparent)]
    Join(#[from] switchy_async::task::JoinError),
    /// Persistence layer error
    #[error(transparent)]
    Persistence(#[from] hyperchad::state::Error),
}

impl AppStateError {
    /// Creates an unknown error with a custom message.
    ///
    /// Used for errors that don't fit into other error categories or when
    /// wrapping error messages from external sources.
    #[must_use]
    pub fn unknown(message: impl Into<String>) -> Self {
        Self::Unknown(message.into())
    }
}

/// Parameters for updating application state.
///
/// This struct contains optional fields for updating various aspects of the application
/// state. Each field is doubly-optional (`Option<Option<T>>`) where:
/// * `None` means don't update this field
/// * `Some(None)` means clear/remove the value
/// * `Some(Some(value))` means set to the specified value
#[derive(Debug, Clone, Default, Error, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppState {
    /// Connection identifier
    pub connection_id: Option<Option<String>>,
    /// Connection display name
    pub connection_name: Option<Option<String>>,
    /// `MoosicBox` API server URL
    pub api_url: Option<Option<String>>,
    /// Client identifier for authentication
    pub client_id: Option<Option<String>>,
    /// Signature token for authentication
    pub signature_token: Option<Option<String>>,
    /// API authentication token
    pub api_token: Option<Option<String>>,
    /// `MoosicBox` profile name
    pub profile: Option<Option<String>>,
    /// Target for audio playback
    pub playback_target: Option<Option<PlaybackTarget>>,
    /// Currently active session identifier
    pub current_session_id: Option<Option<u64>>,
}

impl std::fmt::Display for UpdateAppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{self:?}"))
    }
}

/// Type of audio player.
///
/// Distinguishes between local audio players and remote `UPnP`/DLNA players,
/// each with their own configuration and requirements.
#[derive(Clone)]
pub enum PlayerType {
    /// Local audio player using system audio outputs
    Local,
    /// `UPnP`/DLNA network player
    #[cfg(feature = "upnp")]
    Upnp {
        /// Music API adapter for accessing the library
        source_to_music_api: Arc<Box<dyn moosicbox_music_api::SourceToMusicApi + Send + Sync>>,
        /// `UPnP` device information
        device: Box<switchy_upnp::Device>,
        /// `UPnP` service interface
        service: Box<switchy_upnp::Service>,
        /// Handle for `UPnP` event listener
        handle: moosicbox_upnp::listener::Handle,
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

/// Association between a playback target, session, and player.
///
/// Represents an active player instance that is bound to a specific playback
/// target (audio zone or connection output) and session.
#[derive(Debug, Clone)]
pub struct PlaybackTargetSessionPlayer {
    /// Target destination for audio playback
    pub playback_target: ApiPlaybackTarget,
    /// Session identifier
    pub session_id: u64,
    /// Player handler for controlling playback
    pub player: PlaybackHandler,
    /// Type of player (local or `UPnP`)
    pub player_type: PlayerType,
}

/// Central application state container.
///
/// This struct holds all runtime state for a `MoosicBox` application, including:
/// * Server connection configuration (API URL, tokens, profile)
/// * `WebSocket` connection state and message handling
/// * Active audio players and playback sessions
/// * Audio zones and connection outputs
/// * Persistence layer for storing configuration
/// * Event listeners for state changes
///
/// All fields are wrapped in `Arc<RwLock<_>>` for thread-safe shared access.
#[derive(Clone, Default)]
pub struct AppState {
    /// `SQLite` persistence layer for storing configuration
    pub persistence: Arc<RwLock<Option<Arc<SqlitePersistence>>>>,
    /// `MoosicBox` API server URL
    pub api_url: Arc<RwLock<Option<String>>>,
    /// `MoosicBox` profile name
    pub profile: Arc<RwLock<Option<String>>>,
    /// `WebSocket` server URL
    pub ws_url: Arc<RwLock<Option<String>>>,
    /// `WebSocket` connection identifier
    pub ws_connection_id: Arc<RwLock<Option<String>>>,
    /// Connection identifier
    pub connection_id: Arc<RwLock<Option<String>>>,
    /// Connection display name
    pub connection_name: Arc<RwLock<Option<String>>>,
    /// Signature token for authentication
    pub signature_token: Arc<RwLock<Option<String>>>,
    /// Client identifier for authentication
    pub client_id: Arc<RwLock<Option<String>>>,
    /// API authentication token
    pub api_token: Arc<RwLock<Option<String>>>,
    /// Cancellation token for `WebSocket` connection
    pub ws_token: Arc<RwLock<Option<CancellationToken>>>,
    /// Handle for `WebSocket` connection
    pub ws_handle: Arc<RwLock<Option<WsHandle>>>,
    /// Join handle for `WebSocket` task
    #[allow(clippy::type_complexity)]
    pub ws_join_handle:
        Arc<RwLock<Option<switchy_async::task::JoinHandle<Result<(), AppStateError>>>>>,
    /// Active players for each audio zone
    pub audio_zone_active_api_players: Arc<RwLock<BTreeMap<u64, Vec<ApiPlayersMap>>>>,
    /// Currently active players
    pub active_players: Arc<RwLock<Vec<PlaybackTargetSessionPlayer>>>,
    /// Current playback quality setting
    pub playback_quality: Arc<RwLock<Option<PlaybackQuality>>>,
    /// Buffer for `WebSocket` messages sent before connection is established
    pub ws_message_buffer: Arc<RwLock<Vec<InboundPayload>>>,
    /// Currently selected playback target
    pub current_playback_target: Arc<RwLock<Option<PlaybackTarget>>>,
    /// List of all connections
    pub current_connections: Arc<RwLock<Vec<ApiConnection>>>,
    /// Mapping of player IDs to session IDs for players awaiting session info
    pub pending_player_sessions: Arc<RwLock<BTreeMap<u64, u64>>>,
    /// List of all active sessions
    pub current_sessions: Arc<RwLock<Vec<ApiSession>>>,
    /// Default location for downloaded files
    pub default_download_location: Arc<std::sync::RwLock<Option<String>>>,
    /// Listeners called when sessions are updated
    #[allow(clippy::type_complexity)]
    pub on_current_sessions_updated_listeners: Vec<
        Arc<Box<dyn Fn(&[ApiSession]) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    >,
    /// Listeners called when audio zones are updated
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
    /// Listeners called when connections are updated
    #[allow(clippy::type_complexity)]
    pub on_connections_updated_listeners: Vec<
        Arc<
            Box<dyn Fn(&[ApiConnection]) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
        >,
    >,
    /// Currently selected session identifier
    pub current_session_id: Arc<RwLock<Option<u64>>>,
    /// List of all audio zones with their sessions
    pub current_audio_zones: Arc<RwLock<Vec<ApiAudioZoneWithSession>>>,
    /// List of all available players
    #[allow(clippy::type_complexity)]
    pub current_players: Arc<RwLock<Vec<ApiPlayersMap>>>,
    /// `UPnP` AV transport services for DLNA playback
    #[cfg(feature = "upnp")]
    pub upnp_av_transport_services:
        Arc<RwLock<Vec<moosicbox_upnp::player::UpnpAvTransportService>>>,
    /// Listeners called before handling playback updates
    #[allow(clippy::type_complexity)]
    pub on_before_handle_playback_update_listeners: Vec<
        Arc<
            Box<
                dyn Fn(&ApiUpdateSession) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
            >,
        >,
    >,
    /// Listeners called after handling playback updates
    #[allow(clippy::type_complexity)]
    pub on_after_handle_playback_update_listeners: Vec<
        Arc<
            Box<
                dyn Fn(&ApiUpdateSession) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
            >,
        >,
    >,
    /// Listeners called before updating playlist
    #[allow(clippy::type_complexity)]
    pub on_before_update_playlist_listeners:
        Vec<Arc<Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>>,
    /// Listeners called after updating playlist
    #[allow(clippy::type_complexity)]
    pub on_after_update_playlist_listeners: Vec<
        Arc<Box<dyn Fn(&ApiSession) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    >,
    /// Listeners called before handling `WebSocket` messages
    #[allow(clippy::type_complexity)]
    pub on_before_handle_ws_message_listeners: Vec<
        Arc<
            Box<dyn Fn(&OutboundPayload) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
        >,
    >,
    /// Listeners called after handling `WebSocket` messages
    #[allow(clippy::type_complexity)]
    pub on_after_handle_ws_message_listeners: Vec<
        Arc<
            Box<dyn Fn(&OutboundPayload) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
        >,
    >,
    /// Listeners called before updating application state
    #[allow(clippy::type_complexity)]
    pub on_before_set_state_listeners: Vec<
        Arc<Box<dyn Fn(&UpdateAppState) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    >,
    /// Listeners called after updating application state
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
            .field("ws_join_handle", &"<JoinHandle>")
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
    /// Creates a new application state with default values.
    ///
    /// All fields are initialized to empty/default values. Use builder methods
    /// like `with_persistence` and `with_on_*_listener` to configure the state
    /// before use.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a listener to be called before handling playback updates.
    ///
    /// The listener receives the update session information and can perform
    /// pre-processing or validation before the update is applied.
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

    /// Registers a listener to be called after handling playback updates.
    ///
    /// The listener receives the update session information and can perform
    /// post-processing tasks after the update has been applied to players.
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

    /// Registers a listener to be called before updating the playlist.
    ///
    /// The listener can perform pre-processing tasks before the playlist
    /// update is applied.
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

    /// Registers a listener to be called after updating the playlist.
    ///
    /// The listener receives the updated session and can perform post-processing
    /// tasks such as UI updates or analytics tracking.
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

    /// Registers a listener to be called before handling `WebSocket` messages.
    ///
    /// The listener receives the incoming message payload and can perform
    /// validation or logging before the message is processed.
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

    /// Registers a listener to be called after handling `WebSocket` messages.
    ///
    /// The listener receives the processed message payload and can perform
    /// post-processing tasks such as UI updates or state synchronization.
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

    /// Registers a listener to be called before updating application state.
    ///
    /// The listener receives the state update parameters and can perform
    /// validation or side effects before the state is modified.
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

    /// Registers a listener to be called after updating application state.
    ///
    /// The listener receives the state update parameters and can perform
    /// post-processing tasks such as persistence or event propagation.
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

    /// Registers a listener to be called when the current sessions are updated.
    ///
    /// The listener receives the complete list of current sessions and can
    /// update UI or perform other tasks in response to session changes.
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

    /// Registers a listener to be called when audio zones with sessions are updated.
    ///
    /// The listener receives the complete list of audio zones with their
    /// associated sessions, useful for updating zone-based UI components.
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

    /// Registers a listener to be called when connections are updated.
    ///
    /// The listener receives the complete list of connections and can update
    /// UI or perform other tasks in response to connection changes.
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

    /// Gets the currently active session if one is selected.
    ///
    /// Returns a cloned copy of the session data. Use `get_current_session_ref`
    /// if you need a reference without cloning.
    #[must_use]
    pub async fn get_current_session(&self) -> Option<ApiSession> {
        self.get_current_session_ref().await.map(|x| x.clone())
    }

    /// Gets a read lock reference to the currently active session if one is selected.
    ///
    /// Returns a lock guard that provides efficient read access without cloning.
    /// The guard will block other writers until dropped.
    #[must_use]
    pub async fn get_current_session_ref(&self) -> Option<RwLockReadGuard<'_, ApiSession>> {
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

    /// Sets the default download location path and persists it to storage.
    ///
    /// The path will be saved to the persistence layer and loaded on future
    /// application starts.
    ///
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

    /// Gets the default download location path.
    ///
    /// Returns the configured path for file downloads, or `None` if no default
    /// location has been set.
    ///
    /// # Panics
    ///
    /// * If the `default_download_location` `RwLock` is poisoned
    #[must_use]
    pub fn get_default_download_location(&self) -> Option<String> {
        self.default_download_location.read().unwrap().clone()
    }

    /// Creates a new audio player for the specified session and playback target.
    ///
    /// Initializes either a local or `UPnP` player depending on the player type, configures
    /// it with the appropriate authentication headers and endpoints, and initializes it
    /// with the current session's playback state.
    ///
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

        let mut headers = BTreeMap::new();
        headers.insert("moosicbox-profile".to_string(), profile);

        if self.api_token.read().await.is_some() {
            headers.insert(
                "Authorization".to_string(),
                self.api_token.read().await.as_ref().unwrap().clone(),
            );
        }

        let query = if self.client_id.read().await.is_some()
            && self.signature_token.read().await.is_some()
        {
            let mut query = BTreeMap::new();
            query.insert(
                "clientId".to_string(),
                self.client_id.read().await.as_ref().unwrap().clone(),
            );
            query.insert(
                "signature".to_string(),
                self.signature_token.read().await.as_ref().unwrap().clone(),
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

                let handler = PlaybackHandler::new(local_player.clone())
                    .with_playback(playback)
                    .with_output(Some(Arc::new(std::sync::Mutex::new(output))));

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
                let upnp_player = moosicbox_upnp::player::UpnpPlayer::new(
                    source_to_music_api,
                    *device,
                    *service,
                    player_source,
                    handle,
                );

                let playback = upnp_player.playback.clone();

                let handler = PlaybackHandler::new(upnp_player.clone())
                    .with_playback(playback)
                    .with_output(Some(Arc::new(std::sync::Mutex::new(output))));

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

    /// Gets all active players for a specific session and optional playback target.
    ///
    /// Returns players whose session matches `session_id`, optionally filtered by
    /// `playback_target` if provided. Used to get the players that should receive
    /// playback updates.
    ///
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

    /// Reinitializes all active players with fresh instances.
    ///
    /// Creates new player instances for each active player while preserving their
    /// current playback state. This is useful when connection parameters or
    /// authentication credentials have changed.
    ///
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

    /// Sets the active players for a specific audio zone.
    ///
    /// Creates or updates player instances for the given audio zone, associating them
    /// with the specified session. Existing players for the zone are reused if they
    /// match the session and output configuration.
    ///
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

    /// Updates all audio zones with their current players.
    ///
    /// Iterates through all known audio zones and creates/updates players for each zone
    /// based on the zone's configured players. This syncs the active players list with
    /// the current audio zone configuration.
    ///
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

    /// Initializes `UPnP`/DLNA players by scanning for network devices.
    ///
    /// Scans the network for `UPnP` devices with AV Transport services, registers
    /// them as players with the server, and adds them to the current players list.
    ///
    /// # Errors
    ///
    /// * If the `UpnpDeviceScanner` fails
    /// * If the `UpnpAvTransportService` fails to convert into an `AudioOutput`
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    #[cfg(feature = "upnp")]
    #[allow(clippy::too_many_lines)]
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
                        .push(moosicbox_upnp::player::UpnpAvTransportService { device, service });
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

    /// Registers audio players with the `MoosicBox` server.
    ///
    /// Sends player information to the server to register them under the current
    /// connection. If the connection doesn't exist on the server, it will be
    /// created automatically.
    ///
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

    /// Makes a GET request to the `MoosicBox` API.
    ///
    /// Proxies a GET request to the configured API server, automatically adding
    /// authentication headers and profile information.
    ///
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
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<serde_json::Value, AppStateError> {
        self.api_proxy("get", url, None, headers).await
    }

    /// Makes a POST request to the `MoosicBox` API.
    ///
    /// Proxies a POST request to the configured API server, automatically adding
    /// authentication headers and profile information.
    ///
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
        headers: Option<BTreeMap<String, String>>,
    ) -> Result<serde_json::Value, AppStateError> {
        self.api_proxy("post", url, body, headers).await
    }

    /// Makes an HTTP request to the `MoosicBox` API with the specified method.
    ///
    /// Core method for making authenticated API requests. Adds required headers
    /// (profile, authorization, content-type) and client parameters automatically.
    ///
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
        headers: Option<BTreeMap<String, String>>,
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

    /// Sends an HTTP request and parses the JSON response.
    ///
    /// Executes the request configured in the builder, validates the response status,
    /// and deserializes the response body as JSON.
    ///
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

    /// Adds new players to the current players list.
    ///
    /// Only players not already in the list (by player ID) will be added.
    /// This prevents duplicate player entries.
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

    /// Creates players for all connection outputs across specified sessions.
    ///
    /// Iterates through all available audio outputs (both local and `UPnP`) and creates
    /// players for each output/session combination. This allows individual outputs on
    /// this connection to be controlled independently.
    ///
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

    /// Scans for available audio outputs and registers them with the server.
    ///
    /// Discovers local audio outputs on the system, registers them as players with
    /// the `MoosicBox` server, and creates player instances for all sessions. This
    /// should be called when the connection is established or when outputs change.
    ///
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

    /// Gets the session playback state for a specific player.
    ///
    /// If the player's session differs from the update's session, this method
    /// will fetch the player's actual session data and merge it with the update.
    ///
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

        if let Some(session_id) = session_id
            && session_id != update.session_id
        {
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

        update
    }

    /// Updates application state with the provided parameters.
    ///
    /// Applies the state update to the application, persisting changes and triggering
    /// reconnection if connection details changed. Notifies all registered before/after
    /// state change listeners.
    ///
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

    /// Re-establishes the connection to the `MoosicBox` server.
    ///
    /// Closes existing `WebSocket` connections, scans for outputs, initializes `UPnP`
    /// players if enabled, reinitializes all players, and establishes a new `WebSocket`
    /// connection. Called after connection details (URL, auth tokens) change.
    ///
    /// # Errors
    ///
    /// * If any of the connection state fails to update
    pub async fn update_connection_state(&self) -> Result<(), AppStateError> {
        let has_connection_id = { self.connection_id.read().await.is_some() };
        log::debug!("update_connection_state: has_connection_id={has_connection_id}");

        if has_connection_id {
            switchy_async::runtime::Handle::current().spawn_with_name("set_state: scan_outputs", {
                let state = self.clone();
                async move {
                    log::debug!("Attempting to scan_outputs...");
                    state.scan_outputs().await
                }
            });

            #[cfg(feature = "upnp")]
            let inited_upnp_players = switchy_async::runtime::Handle::current().spawn_with_name(
                "set_state: init_upnp_players",
                {
                    let state = self.clone();
                    async move {
                        log::debug!("Attempting to init_upnp_players...");
                        state.init_upnp_players().await
                    }
                },
            );

            let reinited_players = switchy_async::runtime::Handle::current().spawn_with_name(
                "set_state: reinit_players",
                {
                    let state = self.clone();
                    async move {
                        #[cfg(feature = "upnp")]
                        inited_upnp_players
                            .await
                            .map_err(|e| AppStateError::unknown(e.to_string()))??;
                        log::debug!("Attempting to reinit_players...");
                        state.reinit_players().await
                    }
                },
            );

            switchy_async::runtime::Handle::current().spawn_with_name(
                "set_state: fetch_audio_zones",
                {
                    let state = self.clone();
                    async move {
                        reinited_players
                            .await
                            .map_err(|e| AppStateError::unknown(e.to_string()))??;
                        log::debug!("Attempting to fetch_audio_zones...");
                        state.fetch_audio_zones().await
                    }
                },
            );
        }

        self.close_ws_connection().await?;

        let ws = switchy_async::runtime::Handle::current().spawn_with_name(
            "set_state: init_ws_connection",
            {
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
                                if matches!(
                                    e,
                                    AppStateError::ConnectWs(ConnectWsError::Unauthorized)
                                ) {
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
            },
        );

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

    /// Fetches audio zones from the server and updates local state.
    ///
    /// Retrieves all audio zones with their current sessions from the server,
    /// updates the local audio zones list, and triggers a player update for each zone.
    ///
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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_session::models::{ApiSession, ApiSessionPlaylist};

    fn create_test_session(session_id: u64, name: &str) -> ApiSession {
        ApiSession {
            session_id,
            name: name.to_string(),
            active: true,
            playing: false,
            position: None,
            seek: None,
            volume: Some(0.5),
            playback_target: None,
            playlist: ApiSessionPlaylist {
                session_playlist_id: 1,
                tracks: vec![],
            },
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_returns_none_when_no_session_id_set() {
        let state = AppState::new();

        // No current_session_id is set
        let result = state.get_current_session().await;

        assert!(result.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_returns_none_when_session_id_not_found() {
        let state = AppState::new();

        // Set a session ID that doesn't exist in current_sessions
        *state.current_session_id.write().await = Some(999);

        // Add some sessions, but not one with ID 999
        let session1 = create_test_session(1, "Session 1");
        let session2 = create_test_session(2, "Session 2");
        *state.current_sessions.write().await = vec![session1, session2];

        let result = state.get_current_session().await;

        assert!(result.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_returns_matching_session() {
        let state = AppState::new();

        let session1 = create_test_session(1, "Session 1");
        let session2 = create_test_session(2, "Session 2");
        let session3 = create_test_session(3, "Session 3");

        *state.current_sessions.write().await = vec![session1, session2.clone(), session3];
        *state.current_session_id.write().await = Some(2);

        let result = state.get_current_session().await;

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.session_id, 2);
        assert_eq!(result.name, "Session 2");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_ref_returns_none_when_no_session_id_set() {
        let state = AppState::new();

        assert!(state.get_current_session_ref().await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_ref_returns_none_when_session_id_not_found() {
        let state = AppState::new();

        *state.current_session_id.write().await = Some(999);

        let session1 = create_test_session(1, "Session 1");
        *state.current_sessions.write().await = vec![session1];

        assert!(state.get_current_session_ref().await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_ref_returns_matching_session() {
        let state = AppState::new();

        let session1 = create_test_session(1, "Session 1");
        let session2 = create_test_session(2, "Target Session");

        *state.current_sessions.write().await = vec![session1, session2];
        *state.current_session_id.write().await = Some(2);

        let session_ref = state.get_current_session_ref().await.unwrap();
        assert_eq!(session_ref.session_id, 2);
        assert_eq!(session_ref.name, "Target Session");
        drop(session_ref);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_first_session_in_list() {
        let state = AppState::new();

        let session1 = create_test_session(1, "First Session");
        let session2 = create_test_session(2, "Second Session");

        *state.current_sessions.write().await = vec![session1, session2];
        *state.current_session_id.write().await = Some(1);

        let result = state.get_current_session().await;

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.session_id, 1);
        assert_eq!(result.name, "First Session");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_current_session_last_session_in_list() {
        let state = AppState::new();

        let session1 = create_test_session(1, "First Session");
        let session2 = create_test_session(2, "Second Session");
        let session3 = create_test_session(3, "Last Session");

        *state.current_sessions.write().await = vec![session1, session2, session3];
        *state.current_session_id.write().await = Some(3);

        let result = state.get_current_session().await;

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.session_id, 3);
        assert_eq!(result.name, "Last Session");
    }

    // Tests for get_players

    #[test_log::test(switchy_async::test)]
    async fn test_get_players_returns_empty_when_no_active_players() {
        let state = AppState::new();

        // No active players set (default state)
        assert!(state.active_players.read().await.is_empty());

        // Should return empty vec regardless of session_id or playback_target
        let result = state.get_players(1, None).await;
        assert!(result.is_empty());

        let target = moosicbox_session::models::ApiPlaybackTarget::AudioZone { audio_zone_id: 1 };
        let result = state.get_players(999, Some(&target)).await;
        assert!(result.is_empty());
    }
}
