#![allow(clippy::module_name_repetitions)]

use moosicbox_app_ws::{
    CloseError, WebsocketSendError, WebsocketSender as _, WsClient, WsHandle, WsMessage,
};
use moosicbox_audio_output::AudioOutputScannerError;
use moosicbox_player::{PlayerError, DEFAULT_PLAYBACK_RETRY_OPTIONS};
use moosicbox_session::models::{ApiSession, ApiUpdateSession};
use moosicbox_ws::models::{EmptyPayload, InboundPayload, OutboundPayload};
use serde::Serialize;
use thiserror::Error;
use tokio::task::JoinError;
use tokio_util::sync::CancellationToken;

use crate::{AppState, AppStateError};

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectMessage {
    pub connection_id: String,
    pub ws_url: String,
}

#[derive(Debug, Error)]
pub enum InitWsError {
    #[error(transparent)]
    AudioOutputScanner(#[from] AudioOutputScannerError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    CloseWs(#[from] CloseWsError),
    #[error("Missing profile")]
    MissingProfile,
}

#[derive(Debug, Error)]
pub enum CloseWsError {
    #[error(transparent)]
    Close(#[from] CloseError),
    #[error(transparent)]
    Join(#[from] JoinError),
}

#[derive(Debug, Error)]
pub enum SendWsMessageError {
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    #[error(transparent)]
    HandleWsMessage(#[from] HandleWsMessageError),
}

#[derive(Debug, Error)]
pub enum HandleWsMessageError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Player(#[from] PlayerError),
}

impl AppState {
    /// # Errors
    ///
    /// * If the existing websocket connection fails to close
    /// * If the request is missing a `MoosicBox` profile
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    #[allow(clippy::too_many_lines)]
    pub async fn init_ws_connection(&self) -> Result<(), AppStateError> {
        self.close_ws_connection().await?;

        log::debug!("init_ws_connection: attempting to connect to ws");
        {
            if self.api_url.as_ref().read().await.is_none() {
                log::debug!("init_ws_connection: missing API_URL");
                return Ok(());
            }
        }
        {
            let token = self.ws_token.read().await.clone();
            if let Some(token) = token {
                token.cancel();
            }
        }
        let token = {
            let token = CancellationToken::new();
            self.ws_token.write().await.replace(token.clone());
            token
        };

        let api_url = self.api_url.read().await.clone().unwrap();
        let profile = self
            .profile
            .read()
            .await
            .clone()
            .ok_or_else(|| InitWsError::MissingProfile)?;

        let client_id = self.client_id.read().await.clone();
        let signature_token = self.signature_token.read().await.clone();

        let ws_url = format!("ws{}/ws", &api_url[4..]);
        {
            *self.ws_url.write().await = Some(ws_url.clone());
        }
        let (client, handle) = WsClient::new(ws_url);

        self.ws_handle.write().await.replace(handle.clone());

        let mut client = client.with_cancellation_token(token.clone());
        let state = self.clone();

        self.ws_join_handle
            .write()
            .await
            .replace(moosicbox_task::spawn("moosicbox_app_state: ws", {
                let state = state.clone();
                async move {
                    let mut rx = client.start(client_id, signature_token, profile, {
                        let handle = handle.clone();
                        let state = state.clone();
                        move || {
                            moosicbox_task::spawn("moosicbox_app_state: ws GetConnectionId", {
                                let handle = handle.clone();
                                let state = state.clone();
                                async move {
                                    log::debug!("Sending GetConnectionId");
                                    if let Err(e) = state
                                        .send_ws_message(
                                            &handle,
                                            InboundPayload::GetConnectionId(EmptyPayload {}),
                                            true,
                                        )
                                        .await
                                    {
                                        log::error!(
                                            "Failed to send GetConnectionId WS message: {e:?}"
                                        );
                                    }
                                    if let Err(e) = state.flush_ws_message_buffer().await {
                                        log::error!("Failed to flush WS message buffer: {e:?}");
                                    }
                                }
                            });
                        }
                    });

                    while let Some(m) = tokio::select! {
                        resp = rx.recv() => {
                            resp
                        }
                        () = token.cancelled() => {
                            None
                        }
                    } {
                        match m {
                            WsMessage::TextMessage(message) => {
                                match serde_json::from_str::<OutboundPayload>(&message) {
                                    Ok(message) => {
                                        if let Err(e) = state.handle_ws_message(message).await {
                                            log::error!("Failed to handle_ws_message: {e:?}");
                                        }
                                    }
                                    Err(e) => {
                                        moosicbox_assert::die_or_error!(
                                            "got invalid message: {message}: {e:?}"
                                        );
                                    }
                                }
                            }
                            WsMessage::Message(bytes) => match String::from_utf8(bytes.into()) {
                                Ok(message) => {
                                    match serde_json::from_str::<OutboundPayload>(&message) {
                                        Ok(message) => {
                                            if let Err(e) = state.handle_ws_message(message).await {
                                                log::error!("Failed to handle_ws_message: {e:?}");
                                            }
                                        }
                                        Err(e) => {
                                            moosicbox_assert::die_or_error!(
                                                "got invalid message: {message}: {e:?}"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to read ws message: {e:?}");
                                }
                            },
                            WsMessage::Ping => {
                                log::debug!("got ping");
                            }
                        }
                    }
                    log::debug!("Exiting ws message loop");
                }
            }));

        Ok(())
    }

    /// # Errors
    ///
    /// * If the websocket connection fails to close
    pub async fn close_ws_connection(&self) -> Result<(), AppStateError> {
        log::debug!("close_ws_connection: attempting to close ws connection");

        let handle = self.ws_handle.read().await.clone();
        if let Some(handle) = handle {
            handle.close().await.map_err(CloseWsError::Close)?;
        }

        let handle = self.ws_join_handle.write().await.take();
        if let Some(handle) = handle {
            handle.abort();
        }

        log::debug!("close_ws_connection: ws connection closed");

        Ok(())
    }

    /// # Errors
    ///
    /// * If fails to handle playback update
    /// * If the websocket message fails to send
    ///
    /// # Panics
    ///
    /// * If the websocket message serialization fails
    pub async fn send_ws_message(
        &self,
        handle: &WsHandle,
        message: InboundPayload,
        handle_update: bool,
    ) -> Result<(), AppStateError> {
        log::debug!("send_ws_message: handle_update={handle_update} message={message:?}");

        if handle_update {
            let message = message.clone();
            let state = self.clone();
            moosicbox_task::spawn("send_ws_message: handle_update", async move {
                match &message {
                    InboundPayload::UpdateSession(payload) => {
                        state
                            .handle_playback_update(&payload.payload.clone().into())
                            .await?;
                    }
                    InboundPayload::SetSeek(payload) => {
                        #[allow(clippy::cast_precision_loss)]
                        state
                            .handle_playback_update(&ApiUpdateSession {
                                session_id: payload.payload.session_id,
                                profile: payload.payload.profile.clone(),
                                playback_target: payload.payload.playback_target.clone(),
                                play: None,
                                stop: None,
                                name: None,
                                active: None,
                                playing: None,
                                position: None,
                                seek: Some(payload.payload.seek as f64),
                                volume: None,
                                playlist: None,
                                quality: None,
                            })
                            .await?;
                    }
                    _ => {}
                }

                Ok::<_, AppStateError>(())
            });
        }

        handle
            .send(&serde_json::to_string(&message).unwrap())
            .await
            .map_err(SendWsMessageError::WebsocketSend)?;

        Ok(())
    }

    /// # Errors
    ///
    /// * If fails to handle playback update
    /// * If fails to update audio zones
    /// * If fails to update connection outputs
    /// * If fails to update playlists
    #[allow(clippy::too_many_lines)]
    pub async fn handle_ws_message(&self, message: OutboundPayload) -> Result<(), AppStateError> {
        log::debug!("handle_ws_message: {message:?}");

        for listener in &self.on_before_handle_ws_message_listeners {
            listener(&message).await;
        }

        let state = self.clone();

        moosicbox_task::spawn("handle_ws_message", {
            let message = message.clone();
            async move {
                match &message {
                    OutboundPayload::SessionUpdated(payload) => {
                        state.handle_playback_update(&payload.payload).await?;
                    }
                    OutboundPayload::SetSeek(payload) => {
                        #[allow(clippy::cast_precision_loss)]
                        state
                            .handle_playback_update(&ApiUpdateSession {
                                session_id: payload.payload.session_id,
                                profile: payload.payload.profile.clone(),
                                playback_target: payload.payload.playback_target.clone(),
                                play: None,
                                stop: None,
                                name: None,
                                active: None,
                                playing: None,
                                position: None,
                                seek: Some(payload.payload.seek as f64),
                                volume: None,
                                playlist: None,
                                quality: None,
                            })
                            .await?;
                    }
                    OutboundPayload::ConnectionId(payload) => {
                        state
                            .ws_connection_id
                            .write()
                            .await
                            .replace(payload.connection_id.clone());
                    }
                    OutboundPayload::Connections(payload) => {
                        (*state.current_connections.write().await).clone_from(&payload.payload);

                        state.update_audio_zones().await?;
                    }
                    OutboundPayload::Sessions(payload) => {
                        let player_ids = {
                            let mut player_ids = vec![];
                            let player_sessions = state
                                .pending_player_sessions
                                .read()
                                .await
                                .iter()
                                .map(|(x, y)| (*x, *y))
                                .collect::<Vec<_>>();

                            let profile = { state.profile.read().await.clone() };

                            if let Some(profile) = profile {
                                for (player_id, session_id) in player_sessions {
                                    if let Some(session) =
                                        payload.payload.iter().find(|x| x.session_id == session_id)
                                    {
                                        let mut binding = state.active_players.write().await;
                                        let player = binding
                                            .iter_mut()
                                            .find(|x| x.player.id as u64 == player_id)
                                            .map(|x| &mut x.player);

                                        if let Some(player) = player {
                                            log::debug!(
                                                "handle_ws_message: init_from_api_session session={session:?}"
                                            );
                                            if let Err(e) = player
                                                .init_from_api_session(
                                                    profile.clone(),
                                                    session.clone(),
                                                )
                                                .await
                                            {
                                                log::error!(
                                                    "Failed to init player from api session: {e:?}"
                                                );
                                            }
                                            drop(binding);
                                            player_ids.push(player_id);
                                        }
                                    }
                                }
                            }

                            player_ids
                        };
                        {
                            state
                                .pending_player_sessions
                                .write()
                                .await
                                .retain(|id, _| !player_ids.contains(id));
                        }
                        {
                            (*state.current_sessions.write().await).clone_from(&payload.payload);

                            for listener in &state.on_current_sessions_updated_listeners {
                                listener(&payload.payload).await;
                            }
                        }

                        state.update_audio_zones().await?;
                        state
                            .update_connection_outputs(
                                &payload
                                    .payload
                                    .iter()
                                    .map(|x| x.session_id)
                                    .collect::<Vec<_>>(),
                            )
                            .await?;
                        state.update_playlist().await;
                    }

                    OutboundPayload::AudioZoneWithSessions(payload) => {
                        (*state.current_audio_zones.write().await).clone_from(&payload.payload);

                        state.update_audio_zones().await?;
                    }
                    _ => {}
                }

                for listener in &state.on_after_handle_ws_message_listeners {
                    listener(&message).await;
                }

                Ok::<_, AppStateError>(())
            }
        });

        Ok(())
    }

    pub async fn update_playlist(&self) {
        log::trace!("update_playlist");

        for listener in &self.on_before_update_playlist_listeners {
            listener().await;
        }

        let current_session_id = { *self.current_session_id.read().await };
        let Some(current_session_id) = current_session_id else {
            log::debug!("update_playlist: no CURRENT_SESSION_ID");
            return;
        };

        log::trace!("update_playlist: current_session_id={current_session_id}");

        let session = {
            let binding = self.current_sessions.read().await;
            let sessions: &[ApiSession] = &binding;
            let session = sessions
                .iter()
                .find(|x| x.session_id == current_session_id)
                .cloned();
            drop(binding);
            session
        };

        let Some(session) = session else {
            log::debug!("update_playlist: no session exists");
            return;
        };

        log::debug!("update_playlist: session={session:?}");

        for listener in &self.on_after_update_playlist_listeners {
            listener(&session).await;
        }
    }

    /// # Errors
    ///
    /// * If any websocket messages fail to send
    pub async fn flush_ws_message_buffer(&self) -> Result<(), AppStateError> {
        let handle = self.ws_handle.read().await.clone();

        if let Some(handle) = handle {
            let mut binding = self.ws_message_buffer.write().await;
            log::debug!(
                "flush_ws_message_buffer: Flushing {} ws messages from buffer",
                binding.len()
            );

            let messages = binding.drain(..);

            for message in messages {
                self.send_ws_message(&handle, message, true).await?;
            }
            drop(binding);
        } else {
            log::debug!("flush_ws_message_buffer: No WS_HANDLE");
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If fails to update playback
    pub async fn handle_playback_update(
        &self,
        update: &ApiUpdateSession,
    ) -> Result<(), AppStateError> {
        log::debug!("handle_playback_update: {update:?}");

        for listener in &self.on_before_handle_playback_update_listeners {
            listener(update).await;
        }

        {
            let mut binding = self.current_sessions.write().await;
            let session = binding
                .iter_mut()
                .find(|x| x.session_id == update.session_id);

            if let Some(session) = session {
                if let Some(seek) = update.seek {
                    #[allow(clippy::cast_precision_loss)]
                    #[allow(clippy::cast_sign_loss)]
                    #[allow(clippy::cast_possible_truncation)]
                    session.seek.replace(seek as u64);
                }
                if let Some(name) = update.name.clone() {
                    session.name = name;
                }
                if let Some(active) = update.active {
                    session.active = active;
                }
                if let Some(playing) = update.playing {
                    session.playing = playing;
                }
                if let Some(volume) = update.volume {
                    session.volume.replace(volume);
                }
                if let Some(position) = update.position {
                    session.position.replace(position);
                }
                if let Some(playlist) = update.playlist.clone() {
                    session.playlist.tracks = playlist.tracks;
                }

                drop(binding);
            }
        }

        let players = self
            .get_players(update.session_id, Some(&update.playback_target))
            .await;

        moosicbox_logging::debug_or_trace!(
            ("handle_playback_update: player count={}", players.len()),
            (
                "handle_playback_update: player count={} players={players:?}",
                players.len()
            )
        );

        for mut player in players {
            let update = self
                .get_session_playback_for_player(update.to_owned(), &player)
                .await;

            log::debug!("handle_playback_update: player={}", player.id);

            if let Some(quality) = update.quality {
                self.playback_quality.write().await.replace(quality);
            }

            player
                .update_playback(
                    true,
                    update.play,
                    update.stop,
                    update.playing,
                    update.position,
                    update.seek,
                    update.volume,
                    update
                        .playlist
                        .map(|x| x.tracks.into_iter().map(Into::into).collect()),
                    update.quality,
                    Some(update.session_id),
                    Some(update.profile.clone()),
                    Some(update.playback_target.into()),
                    false,
                    Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
                )
                .await?;
        }

        for listener in &self.on_after_handle_playback_update_listeners {
            listener(update).await;
        }

        Ok(())
    }
}
