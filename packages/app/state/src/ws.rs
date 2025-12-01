//! `WebSocket` connection management for `MoosicBox` application state.
//!
//! This module provides functionality for establishing and managing `WebSocket`
//! connections to `MoosicBox` servers, including message handling, connection
//! lifecycle management, and playback state synchronization.
//!
//! # Features
//!
//! * `WebSocket` connection initialization and teardown
//! * Message queuing and buffering for unreliable connections
//! * Playback state synchronization across clients
//! * Session and audio zone updates via `WebSocket` events
//!
//! # Example
//!
//! ```no_run
//! # use moosicbox_app_state::AppState;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let state = AppState::new();
//!
//! // Start WebSocket connection
//! state.start_ws_connection().await?;
//!
//! // Connection handles messages automatically
//! // Close when done
//! state.close_ws_connection().await?;
//! # Ok(())
//! # }
//! ```

#![allow(clippy::module_name_repetitions)]

use moosicbox_app_ws::{
    CloseError, WebsocketSendError, WebsocketSender as _, WsClient, WsHandle, WsMessage,
};
use moosicbox_audio_output::AudioOutputScannerError;
use moosicbox_player::{DEFAULT_PLAYBACK_RETRY_OPTIONS, PlayerError};
use moosicbox_session::models::{ApiSession, ApiUpdateSession};
use moosicbox_ws::models::{EmptyPayload, InboundPayload, OutboundPayload};
use serde::Serialize;
use switchy_async::{task::JoinError, util::CancellationToken};
use thiserror::Error;

use crate::{AppState, AppStateError};

/// `WebSocket` connection initialization message.
///
/// Contains the connection identifier and `WebSocket` URL needed to establish
/// a connection to the `MoosicBox` server.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectMessage {
    /// Unique identifier for this connection
    pub connection_id: String,
    /// `WebSocket` URL to connect to
    pub ws_url: String,
}

/// Errors that can occur during `WebSocket` initialization.
#[derive(Debug, Error)]
pub enum InitWsError {
    /// Audio output scanner error
    #[error(transparent)]
    AudioOutputScanner(#[from] AudioOutputScannerError),
    /// JSON serialization/deserialization error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// Error closing existing `WebSocket` connection
    #[error(transparent)]
    CloseWs(#[from] CloseWsError),
    /// Required `MoosicBox` profile is missing
    #[error("Missing profile")]
    MissingProfile,
}

/// Errors that can occur when closing a `WebSocket` connection.
#[derive(Debug, Error)]
pub enum CloseWsError {
    /// Error from the underlying `WebSocket` close operation
    #[error(transparent)]
    Close(#[from] CloseError),
    /// Error joining async task handles
    #[error(transparent)]
    Join(#[from] JoinError),
}

/// Errors that can occur when sending a `WebSocket` message.
#[derive(Debug, Error)]
pub enum SendWsMessageError {
    /// Error from the underlying `WebSocket` send operation
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    /// Error handling the `WebSocket` message
    #[error(transparent)]
    HandleWsMessage(#[from] HandleWsMessageError),
}

/// Errors that can occur when handling incoming `WebSocket` messages.
#[derive(Debug, Error)]
pub enum HandleWsMessageError {
    /// JSON serialization/deserialization error
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    /// Audio player error
    #[error(transparent)]
    Player(#[from] PlayerError),
}

impl AppState {
    /// Establishes a `WebSocket` connection to the `MoosicBox` server.
    ///
    /// Creates and starts a `WebSocket` client connection, spawns a message handling loop,
    /// and sends an initial connection ID request. The connection runs until cancelled
    /// or an error occurs.
    ///
    /// # Errors
    ///
    /// * If the existing websocket connection fails to close
    /// * If the websocket connection is `UNAUTHORIZED`
    /// * If the request is missing a `MoosicBox` profile
    ///
    /// # Panics
    ///
    /// * If any of the required state properties are missing
    #[allow(clippy::too_many_lines)]
    pub async fn start_ws_connection(&self) -> Result<(), AppStateError> {
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
            .ok_or(InitWsError::MissingProfile)?;

        let client_id = self.client_id.read().await.clone();
        let signature_token = self.signature_token.read().await.clone();

        let ws_url = format!("ws{}/ws", &api_url[4..]);
        {
            *self.ws_url.write().await = Some(ws_url.clone());
        }
        let (client, handle) = WsClient::new(ws_url);

        self.ws_handle.write().await.replace(handle.clone());

        let client = client.with_cancellation_token(token.clone());
        let state = self.clone();

        let (tx, mut rx) = switchy_async::sync::mpsc::unbounded();

        switchy_async::runtime::Handle::current().spawn_with_name("ws message loop", async move {
            while let Some(m) = switchy_async::select! {
                resp = rx.recv_async() => {
                    resp.ok()
                }
                () = token.cancelled() => {
                    log::debug!("message loop cancelled");
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
                        Ok(message) => match serde_json::from_str::<OutboundPayload>(&message) {
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
                        },
                        Err(e) => {
                            log::error!("Failed to read ws message: {e:?}");
                        }
                    },
                    WsMessage::Ping => {
                        log::trace!("got ping");
                    }
                }
            }
            log::debug!("Exiting ws message loop");
        });

        Ok(client
            .start(
                client_id,
                signature_token,
                profile,
                {
                    let handle = handle.clone();
                    let state = self.clone();
                    move || {
                        switchy_async::runtime::Handle::current().spawn_with_name(
                            "moosicbox_app_state: ws GetConnectionId",
                            {
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
                            },
                        );
                    }
                },
                tx,
            )
            .await?)
    }

    /// Closes the active `WebSocket` connection.
    ///
    /// Terminates the `WebSocket` connection and aborts the message handling task.
    /// Safe to call even if no connection is active.
    ///
    /// # Errors
    ///
    /// * If the websocket connection fails to close
    pub async fn close_ws_connection(&self) -> Result<(), AppStateError> {
        log::debug!("close_ws_connection: attempting to close ws connection");

        let handle = self.ws_handle.read().await.clone();
        if let Some(handle) = handle {
            handle.close();
        }

        let handle = self.ws_join_handle.write().await.take();
        if let Some(handle) = handle {
            handle.abort();
        }

        log::debug!("close_ws_connection: ws connection closed");

        Ok(())
    }

    /// Queues a `WebSocket` message for sending or buffers it if not connected.
    ///
    /// Sends the message immediately if a connection exists, otherwise adds it to the
    /// message buffer to be sent when the connection is established.
    ///
    /// # Errors
    ///
    /// * If fails to handle playback update
    /// * If the websocket message fails to send
    ///
    /// # Panics
    ///
    /// * If the websocket message serialization fails
    pub async fn queue_ws_message(
        &self,
        message: InboundPayload,
        handle_update: bool,
    ) -> Result<(), AppStateError> {
        let handle = { self.ws_handle.read().await.clone() };

        if let Some(handle) = handle {
            self.send_ws_message(&handle, message, handle_update)
                .await?;
        } else {
            moosicbox_logging::debug_or_trace!(
                ("queue_ws_message: pushing message to buffer: {message}"),
                ("queue_ws_message: pushing message to buffer: {message:?}")
            );
            self.ws_message_buffer.write().await.push(message);
        }

        Ok(())
    }

    /// Sends a `WebSocket` message to the server.
    ///
    /// Serializes and sends the message over the `WebSocket` connection. If `handle_update`
    /// is true, also applies the update to local players asynchronously.
    ///
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
            switchy_async::runtime::Handle::current().spawn_with_name(
                "send_ws_message: handle_update",
                async move {
                    match &message {
                        InboundPayload::UpdateSession(payload) => {
                            state
                                .handle_playback_update(&payload.payload.clone().into(), true)
                                .await?;
                        }
                        InboundPayload::SetSeek(payload) => {
                            #[allow(clippy::cast_precision_loss)]
                            state
                                .handle_playback_update(
                                    &ApiUpdateSession {
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
                                    },
                                    true,
                                )
                                .await?;
                        }
                        _ => {}
                    }

                    Ok::<_, AppStateError>(())
                },
            );
        }

        handle
            .send(&serde_json::to_string(&message).unwrap())
            .await
            .map_err(SendWsMessageError::WebsocketSend)?;

        Ok(())
    }

    /// Processes an incoming `WebSocket` message from the server.
    ///
    /// Handles various message types including session updates, connection info, audio
    /// zones, and playback commands. Triggers appropriate state updates and notifies
    /// registered listeners.
    ///
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

        switchy_async::runtime::Handle::current().spawn_with_name("handle_ws_message", {
            let message = message.clone();
            async move {
                match &message {
                    OutboundPayload::SessionUpdated(payload) => {
                        state.handle_playback_update(&payload.payload, true).await?;
                    }
                    OutboundPayload::SetSeek(payload) => {
                        #[allow(clippy::cast_precision_loss)]
                        state
                            .handle_playback_update(
                                &ApiUpdateSession {
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
                                },
                                true,
                            )
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

                        for listener in &state.on_connections_updated_listeners {
                            listener(&payload.payload).await;
                        }
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
                                            .find(|x| x.player.id == player_id)
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

                        for listener in &state.on_audio_zone_with_sessions_updated_listeners {
                            listener(&payload.payload).await;
                        }
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

    /// Applies an update to a session's state fields.
    ///
    /// This method updates the session matching `update.session_id` in `current_sessions`
    /// with any non-`None` fields from the update. This is a pure state mutation
    /// without player interaction or event triggering.
    pub async fn apply_session_update(&self, update: &ApiUpdateSession) {
        let mut binding = self.current_sessions.write().await;
        let session = binding
            .iter_mut()
            .find(|x| x.session_id == update.session_id);

        if let Some(session) = session {
            if let Some(seek) = update.seek {
                session.seek.replace(seek);
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
        }
        drop(binding);
    }

    /// Updates the playlist for the current session.
    ///
    /// Fetches the current session and triggers all registered playlist update
    /// listeners with the session data. If no current session is set or the
    /// session doesn't exist, the method returns early.
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

    /// Sends all buffered `WebSocket` messages to the server.
    ///
    /// Processes all messages that were queued while the `WebSocket` connection was
    /// not available. Called automatically when a connection is established.
    ///
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

    /// Applies a playback update to local players and session state.
    ///
    /// Updates the local session state and applies the changes to all matching players.
    /// Optionally triggers before/after event listeners based on `trigger_events`.
    ///
    /// # Errors
    ///
    /// * If fails to update playback
    #[allow(clippy::cognitive_complexity)]
    pub async fn handle_playback_update(
        &self,
        update: &ApiUpdateSession,
        trigger_events: bool,
    ) -> Result<(), AppStateError> {
        log::debug!("handle_playback_update: {update:?}");

        if trigger_events {
            for listener in &self.on_before_handle_playback_update_listeners {
                listener(update).await;
            }
        }

        self.apply_session_update(update).await;

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

        if trigger_events {
            for listener in &self.on_after_handle_playback_update_listeners {
                listener(update).await;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    use moosicbox_session::models::{
        ApiPlaybackTarget, ApiSessionPlaylist, ApiUpdateSessionPlaylist,
    };
    use moosicbox_ws::models::EmptyPayload;

    use super::*;

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

    fn create_update(session_id: u64) -> ApiUpdateSession {
        ApiUpdateSession {
            session_id,
            profile: "test".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            play: None,
            stop: None,
            name: None,
            active: None,
            playing: None,
            position: None,
            seek: None,
            volume: None,
            playlist: None,
            quality: None,
        }
    }

    // Tests for queue_ws_message

    #[test_log::test(switchy_async::test)]
    async fn test_queue_ws_message_buffers_when_no_handle() {
        let state = crate::AppState::new();

        // Ensure no ws_handle is set (default state)
        assert!(state.ws_handle.read().await.is_none());

        // Queue a message
        let message = InboundPayload::Ping(EmptyPayload {});
        state
            .queue_ws_message(message, false)
            .await
            .expect("queue_ws_message should succeed");

        // Verify message was buffered
        let buffer = state.ws_message_buffer.read().await;
        assert_eq!(buffer.len(), 1);
        // Verify it's the right type of message
        assert!(matches!(buffer[0], InboundPayload::Ping(_)));
        drop(buffer);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_queue_ws_message_multiple_messages_buffer() {
        let state = crate::AppState::new();

        // Queue multiple messages
        let message1 = InboundPayload::Ping(EmptyPayload {});
        let message2 = InboundPayload::GetConnectionId(EmptyPayload {});
        let message3 = InboundPayload::GetSessions(EmptyPayload {});

        state
            .queue_ws_message(message1, false)
            .await
            .expect("queue_ws_message should succeed");
        state
            .queue_ws_message(message2, false)
            .await
            .expect("queue_ws_message should succeed");
        state
            .queue_ws_message(message3, false)
            .await
            .expect("queue_ws_message should succeed");

        // Verify all messages were buffered in order
        let buffer = state.ws_message_buffer.read().await;
        assert_eq!(buffer.len(), 3);
        assert!(matches!(buffer[0], InboundPayload::Ping(_)));
        assert!(matches!(buffer[1], InboundPayload::GetConnectionId(_)));
        assert!(matches!(buffer[2], InboundPayload::GetSessions(_)));
        drop(buffer);
    }

    // Tests for update_playlist

    #[test_log::test(switchy_async::test)]
    async fn test_update_playlist_calls_before_listeners_even_without_session_id() {
        let before_count = Arc::new(AtomicU32::new(0));
        let before_count_clone = before_count.clone();

        let state = crate::AppState::new().with_on_before_update_playlist_listener(move || {
            let count = before_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
            }
        });

        // No session ID is set, but before listener should still be called
        state.update_playlist().await;

        assert_eq!(before_count.load(Ordering::SeqCst), 1);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_update_playlist_does_not_call_after_listeners_without_session_id() {
        let after_count = Arc::new(AtomicU32::new(0));
        let after_count_clone = after_count.clone();

        let state =
            crate::AppState::new().with_on_after_update_playlist_listener(move |_session| {
                let count = after_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                }
            });

        // No session ID is set
        state.update_playlist().await;

        // After listener should NOT be called when no session ID
        assert_eq!(after_count.load(Ordering::SeqCst), 0);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_update_playlist_does_not_call_after_listeners_when_session_not_found() {
        let after_count = Arc::new(AtomicU32::new(0));
        let after_count_clone = after_count.clone();

        let state =
            crate::AppState::new().with_on_after_update_playlist_listener(move |_session| {
                let count = after_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                }
            });

        // Set session ID but don't add any sessions
        *state.current_session_id.write().await = Some(999);

        state.update_playlist().await;

        // After listener should NOT be called when session not found
        assert_eq!(after_count.load(Ordering::SeqCst), 0);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_update_playlist_calls_after_listeners_when_session_exists() {
        let before_count = Arc::new(AtomicU32::new(0));
        let before_count_clone = before_count.clone();
        let after_count = Arc::new(AtomicU32::new(0));
        let after_count_clone = after_count.clone();
        let received_session_id = Arc::new(AtomicU32::new(0));
        let received_session_id_clone = received_session_id.clone();

        let state = crate::AppState::new()
            .with_on_before_update_playlist_listener(move || {
                let count = before_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                }
            })
            .with_on_after_update_playlist_listener(move |session| {
                let count = after_count_clone.clone();
                let session_id_store = received_session_id_clone.clone();
                let session_id = session.session_id;
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    #[allow(clippy::cast_possible_truncation)]
                    session_id_store.store(session_id as u32, Ordering::SeqCst);
                }
            });

        // Add a session and set it as current
        let session = create_test_session(42, "Test Session");
        *state.current_sessions.write().await = vec![session];
        *state.current_session_id.write().await = Some(42);

        state.update_playlist().await;

        // Both listeners should be called
        assert_eq!(before_count.load(Ordering::SeqCst), 1);
        assert_eq!(after_count.load(Ordering::SeqCst), 1);
        // And should receive the correct session
        assert_eq!(received_session_id.load(Ordering::SeqCst), 42);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_update_playlist_finds_correct_session_among_multiple() {
        let received_session_name = Arc::new(std::sync::RwLock::new(String::new()));
        let received_session_name_clone = received_session_name.clone();

        let state = crate::AppState::new().with_on_after_update_playlist_listener(move |session| {
            let name_store = received_session_name_clone.clone();
            let name = session.name;
            async move {
                *name_store.write().unwrap() = name;
            }
        });

        // Add multiple sessions
        let session1 = create_test_session(1, "First");
        let session2 = create_test_session(2, "Target");
        let session3 = create_test_session(3, "Third");
        *state.current_sessions.write().await = vec![session1, session2, session3];
        *state.current_session_id.write().await = Some(2);

        state.update_playlist().await;

        // Should receive the correct session
        assert_eq!(*received_session_name.read().unwrap(), "Target");
    }

    // Tests for apply_session_update

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_does_nothing_when_session_not_found() {
        let state = crate::AppState::new();

        // Add a session with ID 1
        let session = create_test_session(1, "Session 1");
        *state.current_sessions.write().await = vec![session];

        // Try to update session with ID 999 (doesn't exist)
        let update = create_update(999);
        state.apply_session_update(&update).await;

        // Session 1 should remain unchanged
        let sessions = state.current_sessions.read().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, 1);
        assert_eq!(sessions[0].name, "Session 1");
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_seek() {
        let state = crate::AppState::new();

        let session = create_test_session(1, "Test Session");
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        update.seek = Some(42.5);
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert_eq!(sessions[0].seek, Some(42.5));
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_name() {
        let state = crate::AppState::new();

        let session = create_test_session(1, "Original Name");
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        update.name = Some("New Name".to_string());
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert_eq!(sessions[0].name, "New Name");
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_active() {
        let state = crate::AppState::new();

        let mut session = create_test_session(1, "Test Session");
        session.active = true;
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        update.active = Some(false);
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert!(!sessions[0].active);
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_playing() {
        let state = crate::AppState::new();

        let mut session = create_test_session(1, "Test Session");
        session.playing = false;
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        update.playing = Some(true);
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert!(sessions[0].playing);
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_volume() {
        let state = crate::AppState::new();

        let session = create_test_session(1, "Test Session");
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        update.volume = Some(0.8);
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert_eq!(sessions[0].volume, Some(0.8));
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_position() {
        let state = crate::AppState::new();

        let session = create_test_session(1, "Test Session");
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        update.position = Some(5);
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert_eq!(sessions[0].position, Some(5));
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_playlist_tracks() {
        let state = crate::AppState::new();

        let session = create_test_session(1, "Test Session");
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        let track = moosicbox_music_models::api::ApiTrack::default();
        update.playlist = Some(ApiUpdateSessionPlaylist {
            session_playlist_id: 1,
            tracks: vec![track.clone()],
        });
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert_eq!(sessions[0].playlist.tracks.len(), 1);
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_does_not_update_unset_fields() {
        let state = crate::AppState::new();

        let mut session = create_test_session(1, "Original");
        session.seek = Some(10.0);
        session.volume = Some(0.5);
        session.position = Some(3);
        session.active = true;
        session.playing = true;
        *state.current_sessions.write().await = vec![session];

        // Update with only name set, others None
        let mut update = create_update(1);
        update.name = Some("Updated Name".to_string());
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        // Name should be updated
        assert_eq!(sessions[0].name, "Updated Name");
        // Other fields should remain unchanged
        assert_eq!(sessions[0].seek, Some(10.0));
        assert_eq!(sessions[0].volume, Some(0.5));
        assert_eq!(sessions[0].position, Some(3));
        assert!(sessions[0].active);
        assert!(sessions[0].playing);
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_multiple_fields_at_once() {
        let state = crate::AppState::new();

        let session = create_test_session(1, "Original");
        *state.current_sessions.write().await = vec![session];

        let mut update = create_update(1);
        update.name = Some("New Name".to_string());
        update.seek = Some(25.0);
        update.volume = Some(0.9);
        update.playing = Some(true);
        update.active = Some(false);
        update.position = Some(7);
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        assert_eq!(sessions[0].name, "New Name");
        assert_eq!(sessions[0].seek, Some(25.0));
        assert_eq!(sessions[0].volume, Some(0.9));
        assert!(sessions[0].playing);
        assert!(!sessions[0].active);
        assert_eq!(sessions[0].position, Some(7));
        drop(sessions);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_apply_session_update_updates_correct_session_among_multiple() {
        let state = crate::AppState::new();

        let session1 = create_test_session(1, "Session 1");
        let session2 = create_test_session(2, "Session 2");
        let session3 = create_test_session(3, "Session 3");
        *state.current_sessions.write().await = vec![session1, session2, session3];

        // Update session 2 only
        let mut update = create_update(2);
        update.name = Some("Updated Session 2".to_string());
        update.volume = Some(0.3);
        state.apply_session_update(&update).await;

        let sessions = state.current_sessions.read().await;
        // Session 1 unchanged
        assert_eq!(sessions[0].name, "Session 1");
        assert_eq!(sessions[0].volume, Some(0.5));
        // Session 2 updated
        assert_eq!(sessions[1].name, "Updated Session 2");
        assert_eq!(sessions[1].volume, Some(0.3));
        // Session 3 unchanged
        assert_eq!(sessions[2].name, "Session 3");
        assert_eq!(sessions[2].volume, Some(0.5));
        drop(sessions);
    }
}
