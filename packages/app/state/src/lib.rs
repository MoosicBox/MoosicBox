#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{collections::HashMap, sync::Arc};

use moosicbox_app_ws::WsHandle;
use moosicbox_audio_output::AudioOutputFactory;
use moosicbox_audio_zone::models::{ApiAudioZoneWithSession, ApiPlayer};
use moosicbox_core::types::PlaybackQuality;
use moosicbox_player::PlaybackHandler;
use moosicbox_session::models::{ApiConnection, ApiPlaybackTarget, ApiSession, PlaybackTarget};
use moosicbox_ws::models::InboundPayload;
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::sync::CancellationToken;

type ApiPlayersMap = (ApiPlayer, PlayerType, AudioOutputFactory);

#[derive(Clone)]
pub enum PlayerType {
    Local,
    #[cfg(feature = "upnp")]
    Upnp {
        source_to_music_api: Arc<Box<dyn moosicbox_music_api::SourceToMusicApi + Send + Sync>>,
        device: moosicbox_upnp::Device,
        service: moosicbox_upnp::Service,
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

#[derive(Debug)]
pub struct PlaybackTargetSessionPlayer {
    pub playback_target: ApiPlaybackTarget,
    pub session_id: u64,
    pub player: PlaybackHandler,
    pub player_type: PlayerType,
}

#[derive(Default)]
pub struct AppState {
    pub api_url: Arc<RwLock<Option<String>>>,
    pub profile: Arc<RwLock<Option<String>>>,
    pub ws_url: Arc<RwLock<Option<String>>>,
    pub ws_connection_id: Arc<RwLock<Option<String>>>,
    pub connection_id: Arc<RwLock<Option<String>>>,
    pub signature_token: Arc<RwLock<Option<String>>>,
    pub client_id: Arc<RwLock<Option<String>>>,
    pub api_token: Arc<RwLock<Option<String>>>,
    pub ws_token: Arc<RwLock<Option<CancellationToken>>>,
    pub ws_handle: Arc<RwLock<Option<WsHandle>>>,
    pub ws_join_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    pub audio_zone_active_api_players: Arc<RwLock<HashMap<u64, Vec<ApiPlayersMap>>>>,
    pub active_players: Arc<RwLock<Vec<PlaybackTargetSessionPlayer>>>,
    pub playback_quality: Arc<RwLock<Option<PlaybackQuality>>>,
    pub ws_message_buffer: Arc<RwLock<Vec<InboundPayload>>>,
    pub current_playback_target: Arc<RwLock<Option<PlaybackTarget>>>,
    pub current_connections: Arc<RwLock<Vec<ApiConnection>>>,
    pub pending_player_sessions: Arc<RwLock<HashMap<u64, u64>>>,
    pub current_sessions: Arc<RwLock<Vec<ApiSession>>>,
    pub current_session_id: Arc<RwLock<Option<u64>>>,
    pub current_audio_zones: Arc<RwLock<Vec<ApiAudioZoneWithSession>>>,
    #[allow(clippy::type_complexity)]
    pub current_players: Arc<RwLock<Vec<ApiPlayersMap>>>,
    #[cfg(feature = "upnp")]
    pub upnp_av_transport_services:
        Arc<RwLock<Vec<moosicbox_upnp::player::UpnpAvTransportService>>>,
}
