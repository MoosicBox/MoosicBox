use core::fmt;
use std::{
    collections::BTreeMap,
    future::Future,
    num::ParseIntError,
    pin::Pin,
    sync::{Arc, LazyLock, RwLock},
};

use async_trait::async_trait;
use moosicbox_audio_zone::models::CreateAudioZone;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_session::{
    get_session_playlist,
    models::{
        ApiConnection, ApiSessionPlaylist, ApiUpdateSession, ApiUpdateSessionPlaylist, Connection,
        CreateSession, DeleteSession, PlaybackTarget, RegisterConnection, RegisterPlayer,
        UpdateSession,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use switchy_database::{
    config::ConfigDatabase,
    profiles::{LibraryDatabase, PROFILES},
};
use thiserror::Error;

use crate::models::{
    AudioZoneWithSessionsPayload, ConnectionIdPayload, ConnectionsPayload, DownloadEventPayload,
    InboundPayload, OutboundPayload, ScanEventPayload, SessionUpdatedPayload, SessionsPayload,
};

/// Response for websocket operations.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status_code: u16,
    pub body: String,
}

/// Callback function executed when a session update affects a player.
pub type PlayerAction = fn(&UpdateSession) -> Pin<Box<dyn Future<Output = ()> + Send>>;

/// Context for a websocket connection.
#[derive(Clone, Default, Debug)]
pub struct WebsocketContext {
    pub connection_id: String,
    pub profile: Option<String>,
    pub player_actions: Vec<(u64, PlayerAction)>,
}

/// Errors that can occur when sending websocket messages.
#[derive(Debug, Error)]
pub enum WebsocketSendError {
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error("Unknown: {0}")]
    Unknown(String),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Data associated with a websocket connection.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsocketConnectionData {
    pub playing: bool,
}

/// Trait for sending messages via websocket.
#[async_trait]
pub trait WebsocketSender: Send + Sync {
    /// Sends a message to a specific connection.
    ///
    /// # Errors
    ///
    /// * If the websocket message fails to send
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;

    /// Sends a message to all connections.
    ///
    /// # Errors
    ///
    /// * If the websocket message fails to send
    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError>;

    /// Sends a message to all connections except the specified one.
    ///
    /// # Errors
    ///
    /// * If the websocket message fails to send
    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError>;

    /// Sends a ping to all connections.
    ///
    /// # Errors
    ///
    /// * If the websocket ping fails to send
    async fn ping(&self) -> Result<(), WebsocketSendError>;
}

impl core::fmt::Debug for dyn WebsocketSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{WebsocketSender}}")
    }
}

static CONNECTION_DATA: LazyLock<Arc<RwLock<BTreeMap<String, Connection>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(BTreeMap::new())));

/// Errors that can occur when connecting to a websocket.
#[derive(Debug, Error)]
pub enum WebsocketConnectError {
    #[error("Unknown")]
    Unknown,
}

/// Handles a websocket connection.
#[must_use]
pub fn connect(_sender: &impl WebsocketSender, context: &WebsocketContext) -> Response {
    log::debug!("Connected {}", context.connection_id);

    Response {
        status_code: 200,
        body: "Connected".into(),
    }
}

/// Errors that can occur when disconnecting from a websocket.
#[derive(Debug, Error)]
pub enum WebsocketDisconnectError {
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// # Errors
///
/// * If the list of connections fails to serialize
/// * If a database error occurs when trying to delete the connection
/// * If a `WebsocketSendError` error occurs
///
/// # Panics
///
/// * If the connection data `RwLock` panics
pub async fn disconnect(
    db: &ConfigDatabase,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<Response, WebsocketDisconnectError> {
    let connections = {
        let mut connection_data = CONNECTION_DATA.write().unwrap();

        connection_data.remove(&context.connection_id);

        &serde_json::to_string(&connection_data.values().collect::<Vec<_>>())?
    };

    moosicbox_session::delete_connection(db, &context.connection_id).await?;

    sender.send(&context.connection_id, connections).await?;

    sender.send_all(&get_connections(db).await?).await?;

    log::debug!("Disconnected {}", context.connection_id);

    Ok(Response {
        status_code: 200,
        body: "Disconnected".into(),
    })
}

/// # Errors
///
/// * If the message is an invalid type
/// * If the message fails to process
pub async fn process_message(
    config_db: &ConfigDatabase,
    body: Value,
    context: WebsocketContext,
    sender: &impl WebsocketSender,
) -> Result<Response, WebsocketMessageError> {
    let payload: InboundPayload = serde_json::from_value(body).map_err(|e| {
        moosicbox_assert::die_or_error!("Invalid message type: {e:?}");
        WebsocketMessageError::InvalidMessageType
    })?;

    message(config_db, sender, payload, &context).await
}

/// Errors that can occur when processing a websocket message.
#[derive(Debug, Error)]
pub enum WebsocketMessageError {
    #[error("Missing message type")]
    MissingMessageType,
    #[error("Invalid message type")]
    InvalidMessageType,
    #[error("Invalid payload: '{0}' ({1})")]
    InvalidPayload(String, String),
    #[error("Missing payload")]
    MissingPayload,
    #[error("Missing profile")]
    MissingProfile,
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    #[error(transparent)]
    UpdateSession(#[from] UpdateSessionError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error("Unknown {message:?}")]
    Unknown { message: String },
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// # Errors
///
/// * If the message fails to process
pub async fn message(
    config_db: &ConfigDatabase,
    sender: &impl WebsocketSender,
    message: InboundPayload,
    context: &WebsocketContext,
) -> Result<Response, WebsocketMessageError> {
    let message_type = message.as_ref().to_string();
    log::debug!(
        "Received message type {} from {}: {:?}",
        message_type,
        context.connection_id,
        message
    );
    let db = context.profile.as_ref().and_then(|x| PROFILES.get(x));
    match message {
        InboundPayload::GetConnectionId(_) => {
            get_connection_id(sender, context).await?;
            let db = db.ok_or(WebsocketMessageError::MissingProfile)?;
            broadcast_sessions(&db, sender, context, false).await?;
            Ok::<_, WebsocketMessageError>(())
        }
        InboundPayload::GetSessions(_) => {
            let db = db.ok_or(WebsocketMessageError::MissingProfile)?;
            broadcast_sessions(&db, sender, context, false).await?;
            Ok(())
        }
        InboundPayload::RegisterConnection(payload) => {
            register_connection(config_db, sender, context, &payload.payload).await?;

            sender.send_all(&get_connections(config_db).await?).await?;

            Ok(())
        }
        InboundPayload::RegisterPlayers(payload) => {
            register_players(config_db, sender, context, &payload.payload)
                .await
                .map_err(|e| WebsocketMessageError::Unknown {
                    message: e.to_string(),
                })?;

            broadcast_connections(config_db, sender)
                .await
                .map_err(|e| WebsocketMessageError::Unknown {
                    message: e.to_string(),
                })?;

            Ok(())
        }
        InboundPayload::CreateAudioZone(payload) => {
            let db = db.ok_or(WebsocketMessageError::MissingProfile)?;
            create_audio_zone(config_db, &db, sender, context, &payload.payload).await?;

            sender
                .send_all_except(&context.connection_id, &get_connections(config_db).await?)
                .await?;

            Ok(())
        }
        InboundPayload::CreateSession(payload) => {
            let db = db.ok_or(WebsocketMessageError::MissingProfile)?;
            create_session(&db, sender, context, &payload.payload).await?;
            Ok(())
        }
        InboundPayload::UpdateSession(payload) => {
            let db = db.ok_or(WebsocketMessageError::MissingProfile)?;
            update_session(config_db, &db, sender, Some(context), &payload.payload).await?;
            Ok(())
        }
        InboundPayload::DeleteSession(payload) => {
            let db = db.ok_or(WebsocketMessageError::MissingProfile)?;
            delete_session(&db, sender, context, &payload.payload).await?;
            Ok(())
        }
        InboundPayload::Ping(_) => {
            log::trace!("Ping");
            Ok(())
        }
        InboundPayload::SetSeek(payload) => {
            sender
                .send_all_except(
                    &context.connection_id,
                    &serde_json::to_value(OutboundPayload::SetSeek(payload))?.to_string(),
                )
                .await?;

            Ok(())
        }
    }?;

    log::debug!(
        "Successfully processed message type {} from {}",
        message_type,
        context.connection_id
    );
    Ok(Response {
        status_code: 200,
        body: "Received".into(),
    })
}

/// # Errors
///
/// * If the db fails to return the zones with sessions
/// * If the json fails to serialize
/// * If the ws message fails to broadcast
pub async fn broadcast_audio_zones(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    send_all: bool,
) -> Result<(), WebsocketSendError> {
    let audio_zones = {
        moosicbox_audio_zone::zones_with_sessions(config_db, library_db)
            .await?
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
    };

    let audio_zones_json = serde_json::to_value(OutboundPayload::AudioZoneWithSessions(
        AudioZoneWithSessionsPayload {
            payload: audio_zones,
        },
    ))?
    .to_string();

    if send_all {
        sender.send_all(&audio_zones_json).await
    } else {
        sender.send(&context.connection_id, &audio_zones_json).await
    }
}

/// # Errors
///
/// * If the db fails to return the sessions
/// * If the json fails to serialize
/// * If the ws message fails to broadcast
pub async fn broadcast_sessions(
    db: &LibraryDatabase,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    send_all: bool,
) -> Result<(), WebsocketSendError> {
    let sessions = {
        moosicbox_session::get_sessions(db)
            .await?
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
    };

    let sessions_json = serde_json::to_value(OutboundPayload::Sessions(SessionsPayload {
        payload: sessions,
    }))?
    .to_string();

    if send_all {
        sender.send_all(&sessions_json).await
    } else {
        sender.send(&context.connection_id, &sessions_json).await
    }
}

async fn create_session(
    db: &LibraryDatabase,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &CreateSession,
) -> Result<(), WebsocketSendError> {
    moosicbox_session::create_session(db, payload).await?;
    broadcast_sessions(db, sender, context, true).await?;
    Ok(())
}

async fn get_connections(db: &ConfigDatabase) -> Result<String, WebsocketSendError> {
    let connection_data = CONNECTION_DATA.as_ref().read().unwrap().clone();
    let connections = {
        moosicbox_session::get_connections(db)
            .await?
            .into_iter()
            .map(|connection| {
                let id = connection.id.clone();
                let mut api: ApiConnection = connection.into();

                api.alive = connection_data.values().any(|c| c.id == id);

                api
            })
            .collect::<Vec<_>>()
    };

    let connections_json =
        serde_json::to_value(OutboundPayload::Connections(ConnectionsPayload {
            payload: connections,
        }))?
        .to_string();

    Ok(connections_json)
}

/// # Errors
///
/// * If the db fails to register the connection
///
/// # Panics
///
/// * If the connection data `RwLock` panics
pub async fn register_connection(
    db: &ConfigDatabase,
    _sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &RegisterConnection,
) -> Result<Connection, WebsocketSendError> {
    let connection = moosicbox_session::register_connection(db, payload).await?;

    let mut connection_data = CONNECTION_DATA.write().unwrap();
    connection_data.insert(context.connection_id.clone(), connection.clone());
    drop(connection_data);

    Ok(connection)
}

/// # Errors
///
/// * If the db fails to create the players
pub async fn register_players(
    db: &ConfigDatabase,
    _sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &Vec<RegisterPlayer>,
) -> Result<Vec<moosicbox_audio_zone::models::Player>, WebsocketSendError> {
    let mut players = vec![];
    for player in payload {
        players.push(moosicbox_session::create_player(db, &context.connection_id, player).await?);
    }

    Ok(players)
}

/// # Errors
///
/// * If the db fails to get the connections
/// * If the ws message fails to broadcast
pub async fn broadcast_connections(
    db: &ConfigDatabase,
    sender: &impl WebsocketSender,
) -> Result<(), WebsocketSendError> {
    sender.send_all(&get_connections(db).await?).await?;

    Ok(())
}

async fn create_audio_zone(
    config_db: &ConfigDatabase,
    db: &LibraryDatabase,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &CreateAudioZone,
) -> Result<(), WebsocketMessageError> {
    moosicbox_audio_zone::create_audio_zone(config_db, payload).await?;
    broadcast_sessions(db, sender, context, true).await?;
    Ok(())
}

/// # Errors
///
/// * If the `OutboundPayload::DownloadEvent` fails to serialize
/// * If the ws message fails to broadcast
pub async fn send_download_event<ProgressEvent: Serialize + Send>(
    sender: &impl WebsocketSender,
    context: Option<&WebsocketContext>,
    payload: ProgressEvent,
) -> Result<(), WebsocketSendError> {
    let download_even =
        serde_json::to_value(OutboundPayload::DownloadEvent(DownloadEventPayload {
            payload: serde_json::to_value(payload)?,
        }))?
        .to_string();

    if let Some(context) = context {
        sender
            .send_all_except(&context.connection_id, &download_even)
            .await?;
    } else {
        sender.send_all(&download_even).await?;
    }

    Ok(())
}

/// # Errors
///
/// * If the `OutboundPayload::ScanEvent` fails to serialize
/// * If the ws message fails to broadcast
pub async fn send_scan_event<ProgressEvent: Serialize + Send>(
    sender: &impl WebsocketSender,
    context: Option<&WebsocketContext>,
    payload: ProgressEvent,
) -> Result<(), WebsocketSendError> {
    let scan_even = serde_json::to_value(OutboundPayload::ScanEvent(ScanEventPayload {
        payload: serde_json::to_value(payload)?,
    }))?
    .to_string();

    if let Some(context) = context {
        sender
            .send_all_except(&context.connection_id, &scan_even)
            .await?;
    } else {
        sender.send_all(&scan_even).await?;
    }

    Ok(())
}

/// Errors that can occur when updating a session.
#[derive(Debug, Error)]
pub enum UpdateSessionError {
    #[error("No session found")]
    NoSessionFound,
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// # Errors
///
/// * If the db fails to update the session
/// * If the db fails get the players that were updated
/// * If the ws message fails to broadcast
pub async fn update_session(
    config_db: &ConfigDatabase,
    db: &LibraryDatabase,
    sender: &impl WebsocketSender,
    context: Option<&WebsocketContext>,
    payload: &UpdateSession,
) -> Result<(), UpdateSessionError> {
    moosicbox_logging::debug_or_trace!(
        ("Updating session id={}", payload.session_id),
        (
            "Updating session id={} payload={payload:?}",
            payload.session_id
        )
    );
    moosicbox_session::update_session(db, payload).await?;

    if let Some(actions) = context.map(|x| &x.player_actions)
        && payload.playback_updated()
        && let Some(session) = moosicbox_session::get_session(db, payload.session_id).await?
    {
        let funcs = if let Some(PlaybackTarget::AudioZone { audio_zone_id }) =
            session.playback_target
        {
            let players = moosicbox_audio_zone::db::get_players(config_db, audio_zone_id).await?;

            players
                .iter()
                .filter_map(|p| {
                    actions.iter().find_map(|(player_id, action)| {
                        if *player_id == p.id {
                            Some(action)
                        } else {
                            None
                        }
                    })
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        if log::log_enabled!(log::Level::Trace) {
            log::trace!(
                "Running player actions on existing session id={} count_of_funcs={} payload={payload:?} session={session:?} playback_target={:?} action_player_ids={:?}",
                session.id,
                funcs.len(),
                session.playback_target,
                actions.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            );
        } else {
            log::debug!(
                "Running player actions on existing id={} count_of_funcs={}",
                session.id,
                funcs.len(),
            );
        }

        for func in funcs {
            func(payload).await;
        }
    }

    let playlist = if payload.playlist.is_some() {
        get_session_playlist(db, payload.session_id)
            .await?
            .map(Into::into)
            .map(|playlist: ApiSessionPlaylist| ApiUpdateSessionPlaylist {
                session_playlist_id: playlist.session_playlist_id,
                tracks: playlist.tracks,
            })
    } else {
        None
    };

    let response = ApiUpdateSession {
        session_id: payload.session_id,
        profile: payload.profile.clone(),
        play: payload.play,
        stop: payload.stop,
        name: payload.name.clone(),
        active: payload.active,
        playing: payload.playing,
        position: payload.position,
        seek: payload.seek,
        volume: payload.volume,
        playback_target: payload.playback_target.clone().into(),
        playlist,
        quality: payload.quality,
    };

    let session_updated =
        serde_json::to_value(OutboundPayload::SessionUpdated(SessionUpdatedPayload {
            payload: response,
        }))?
        .to_string();

    if let Some(context) = context {
        sender
            .send_all_except(&context.connection_id, &session_updated)
            .await?;
    } else {
        sender.send_all(&session_updated).await?;
    }

    Ok(())
}

async fn delete_session(
    db: &LibraryDatabase,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &DeleteSession,
) -> Result<(), WebsocketSendError> {
    moosicbox_session::delete_session(db, payload.session_id).await?;

    broadcast_sessions(db, sender, context, true).await?;

    Ok(())
}

async fn get_connection_id(
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<(), WebsocketSendError> {
    sender
        .send(
            &context.connection_id,
            &serde_json::to_value(OutboundPayload::ConnectionId(ConnectionIdPayload {
                connection_id: context.connection_id.clone(),
            }))?
            .to_string(),
        )
        .await
}
