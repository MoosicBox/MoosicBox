#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{collections::HashMap, sync::Arc};

use moosicbox_app_ws::WsHandle;
use moosicbox_audio_output::AudioOutputFactory;
use moosicbox_audio_zone::models::{ApiAudioZoneWithSession, ApiPlayer};
use moosicbox_core::types::PlaybackQuality;
use moosicbox_player::{
    local::LocalPlayer, PlaybackHandler, PlaybackType, PlayerError, PlayerSource,
};
use moosicbox_session::models::{ApiConnection, ApiPlaybackTarget, ApiSession, PlaybackTarget};
use moosicbox_ws::models::InboundPayload;
use thiserror::Error;
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::sync::CancellationToken;

type ApiPlayersMap = (ApiPlayer, PlayerType, AudioOutputFactory);

#[derive(Debug, Error)]
pub enum AppStateError {
    #[error("Unknown({0})")]
    Unknown(String),
    #[error(transparent)]
    Player(#[from] PlayerError),
}

impl AppStateError {
    fn unknown(message: impl Into<String>) -> Self {
        Self::Unknown(message.into())
    }
}

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

#[derive(Debug, Clone)]
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

impl AppState {
    /// # Errors
    ///
    /// * If there is a `PlayerError`
    /// * If an unknown error occurs
    ///
    /// # Panics
    ///
    /// * If any of the relevant state `RwLock`s are poisoned
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
                let upnp_player = moosicbox_upnp::player::UpnpPlayer::new(
                    source_to_music_api,
                    device,
                    service,
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
                .insert(player.id as u64, session_id);
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

    /// # Errors
    ///
    /// * If there is a `PlayerError`
    /// * If an unknown error occurs
    ///
    /// # Panics
    ///
    /// * If any of the relevant state `RwLock`s are poisoned
    pub async fn get_players(
        &self,
        session_id: u64,
        playback_target: Option<&ApiPlaybackTarget>,
    ) -> Result<Vec<PlaybackHandler>, AppStateError> {
        let players = {
            let mut playback_handlers = vec![];
            let active_players = self.active_players.read().await.clone();

            for player in active_players {
                let target = &player.playback_target;
                log::trace!(
                "get_players: Checking if player is in session: target={target:?} session_id={session_id} player_zone_id={playback_target:?} player={player:?}",
            );
                let same_session = player.player.playback
                .read()
                .unwrap()
                .as_ref()
                .is_some_and(|p| {
                    log::trace!(
                        "get_players: player playback.session_id={} target session_id={session_id} player={player:?}",
                        p.session_id
                    );
                    p.session_id == session_id
                });
                if !same_session {
                    continue;
                }
                log::trace!(
                "get_players: Checking if player is in zone: target={target:?} session_id={session_id} player_zone_id={playback_target:?} player={player:?}",
            );
                if playback_target.is_some_and(|x| x != target) {
                    continue;
                }

                playback_handlers.push(player.player);
            }
            playback_handlers
        };

        Ok(players)
    }
}
