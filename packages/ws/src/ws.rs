use core::fmt;
use std::{
    collections::HashMap,
    future::Future,
    num::ParseIntError,
    pin::Pin,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use log::{debug, info, trace};
use moosicbox_audio_zone::models::CreateAudioZone;
use moosicbox_core::sqlite::{db::DbError, models::ToApi as _};
use moosicbox_database::Database;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_session::{
    get_session_playlist,
    models::{
        ApiUpdateSession, ApiUpdateSessionPlaylist, Connection, CreateSession, DeleteSession,
        PlaybackTarget, RegisterConnection, RegisterPlayer, UpdateSession,
    },
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::models::{
    AudioZoneWithSessionsPayload, ConnectionIdPayload, ConnectionsPayload, DownloadEventPayload,
    InboundPayload, OutboundPayload, ScanEventPayload, SessionUpdatedPayload, SessionsPayload,
    SetSeekPayload,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status_code: u16,
    pub body: String,
}

pub type PlayerAction = fn(&UpdateSession) -> Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Clone, Default, Debug)]
pub struct WebsocketContext {
    pub connection_id: String,
    pub player_actions: Vec<(u64, PlayerAction)>,
}

#[derive(Debug, Error)]
pub enum WebsocketSendError {
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("Unknown: {0}")]
    Unknown(String),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsocketConnectionData {
    pub playing: bool,
}

#[async_trait]
pub trait WebsocketSender: Send + Sync {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;
    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError>;
    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError>;
    async fn ping(&self) -> Result<(), WebsocketSendError>;
}

impl core::fmt::Debug for dyn WebsocketSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{WebsocketSender}}")
    }
}

static CONNECTION_DATA: Lazy<Arc<RwLock<HashMap<String, Connection>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

#[derive(Debug, Error)]
pub enum WebsocketConnectError {
    #[error("Unknown")]
    Unknown,
}

pub fn connect(
    _db: &dyn Database,
    _sender: &impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<Response, WebsocketConnectError> {
    info!("Connected {}", context.connection_id);

    Ok(Response {
        status_code: 200,
        body: "Connected".into(),
    })
}

#[derive(Debug, Error)]
pub enum WebsocketDisconnectError {
    #[error("Unknown")]
    Unknown,
}

pub async fn disconnect(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<Response, WebsocketDisconnectError> {
    let connections = {
        let mut connection_data = CONNECTION_DATA.write().unwrap();

        connection_data.remove(&context.connection_id);

        &serde_json::to_string(&connection_data.values().collect::<Vec<_>>()).unwrap()
    };

    sender
        .send(&context.connection_id, connections)
        .await
        .map_err(|_e| WebsocketDisconnectError::Unknown)?;

    sender
        .send_all(
            &get_connections(db)
                .await
                .map_err(|_e| WebsocketDisconnectError::Unknown)?,
        )
        .await
        .map_err(|_e| WebsocketDisconnectError::Unknown)?;

    info!("Disconnected {}", context.connection_id);

    Ok(Response {
        status_code: 200,
        body: "Disconnected".into(),
    })
}

pub async fn process_message(
    db: &dyn Database,
    body: Value,
    context: WebsocketContext,
    sender: &impl WebsocketSender,
) -> Result<Response, WebsocketMessageError> {
    let payload: InboundPayload = serde_json::from_value(body).map_err(|e| {
        moosicbox_assert::die_or_error!("Invalid message type: {e:?}");
        WebsocketMessageError::InvalidMessageType
    })?;

    message(db, sender, payload, &context).await
}

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
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    #[error(transparent)]
    UpdateSession(#[from] UpdateSessionError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error("Unknown {message:?}")]
    Unknown { message: String },
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

pub async fn message(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    message: InboundPayload,
    context: &WebsocketContext,
) -> Result<Response, WebsocketMessageError> {
    let message_type = message.as_ref().to_string();
    debug!(
        "Received message type {} from {}: {:?}",
        message_type, context.connection_id, message
    );
    match message {
        InboundPayload::GetConnectionId(_) => {
            get_connection_id(sender, context).await?;
            Ok::<_, WebsocketMessageError>(())
        }
        InboundPayload::GetSessions(_) => {
            get_sessions(db, sender, context, false).await?;
            Ok(())
        }
        InboundPayload::RegisterConnection(payload) => {
            register_connection(db, sender, context, &payload.payload).await?;

            sender.send_all(&get_connections(db).await?).await?;

            Ok(())
        }
        InboundPayload::RegisterPlayers(payload) => {
            register_players(db, sender, context, &payload.payload)
                .await
                .map_err(|e| WebsocketMessageError::Unknown {
                    message: e.to_string(),
                })?;

            broadcast_connections(db, sender).await.map_err(|e| {
                WebsocketMessageError::Unknown {
                    message: e.to_string(),
                }
            })?;

            Ok(())
        }
        InboundPayload::CreateAudioZone(payload) => {
            create_audio_zone(db, sender, context, &payload.payload).await?;

            sender
                .send_all_except(&context.connection_id, &get_connections(db).await?)
                .await?;

            Ok(())
        }
        InboundPayload::CreateSession(payload) => {
            create_session(db, sender, context, &payload.payload).await?;
            Ok(())
        }
        InboundPayload::UpdateSession(payload) => {
            update_session(db, sender, Some(context), &payload.payload).await?;
            Ok(())
        }
        InboundPayload::DeleteSession(payload) => {
            delete_session(db, sender, context, &payload.payload).await?;
            Ok(())
        }
        InboundPayload::Ping(_) => {
            trace!("Ping");
            Ok(())
        }
        InboundPayload::PlaybackAction(payload) => {
            playback_action(sender, context, &payload.payload)?;
            Ok(())
        }
        InboundPayload::SetSeek(payload) => {
            sender
                .send_all_except(
                    &context.connection_id,
                    &serde_json::to_value(OutboundPayload::SetSeek(SetSeekPayload {
                        payload: payload.payload,
                    }))?
                    .to_string(),
                )
                .await?;

            Ok(())
        }
    }?;

    debug!(
        "Successfully processed message type {} from {}",
        message_type, context.connection_id
    );
    Ok(Response {
        status_code: 200,
        body: "Received".into(),
    })
}

pub async fn broadcast_audio_zones(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    send_all: bool,
) -> Result<(), WebsocketSendError> {
    let audio_zones = {
        moosicbox_audio_zone::zones_with_sessions(db)
            .await?
            .into_iter()
            .map(|zone| zone.into())
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

pub async fn get_sessions(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    send_all: bool,
) -> Result<(), WebsocketSendError> {
    let sessions = {
        moosicbox_session::get_sessions(db)
            .await?
            .into_iter()
            .map(|session| session.to_api())
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
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &CreateSession,
) -> Result<(), WebsocketSendError> {
    moosicbox_session::create_session(db, payload).await?;
    get_sessions(db, sender, context, true).await?;
    Ok(())
}

async fn get_connections(db: &dyn Database) -> Result<String, WebsocketSendError> {
    let connection_data = CONNECTION_DATA.as_ref().read().unwrap().clone();
    let connections = {
        moosicbox_session::get_connections(db)
            .await?
            .into_iter()
            .map(|connection| {
                let id = connection.id.clone();
                let mut api = connection.to_api();

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

pub async fn register_connection(
    db: &dyn Database,
    _sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &RegisterConnection,
) -> Result<Connection, WebsocketSendError> {
    let connection = moosicbox_session::register_connection(db, payload).await?;

    let mut connection_data = CONNECTION_DATA.write().unwrap();

    connection_data.insert(context.connection_id.clone(), connection.clone());

    Ok(connection)
}

pub async fn register_players(
    db: &dyn Database,
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

pub async fn broadcast_connections(
    db: &dyn Database,
    sender: &impl WebsocketSender,
) -> Result<(), WebsocketSendError> {
    sender.send_all(&get_connections(db).await?).await?;

    Ok(())
}

async fn create_audio_zone(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &CreateAudioZone,
) -> Result<(), WebsocketMessageError> {
    moosicbox_audio_zone::create_audio_zone(db, payload).await?;
    get_sessions(db, sender, context, true).await?;
    Ok(())
}

pub async fn send_download_event<ProgressEvent: Serialize>(
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

pub async fn send_scan_event<ProgressEvent: Serialize>(
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

#[derive(Debug, Error)]
pub enum UpdateSessionError {
    #[error("No session found")]
    NoSessionFound,
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

pub async fn update_session(
    db: &dyn Database,
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

    if let Some(actions) = context.map(|x| &x.player_actions) {
        if payload.playback_updated() {
            if let Some(session) = moosicbox_session::get_session(db, payload.session_id).await? {
                let funcs = if let Some(PlaybackTarget::AudioZone { audio_zone_id }) =
                    session.playback_target
                {
                    let players = moosicbox_audio_zone::db::get_players(db, audio_zone_id).await?;

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
        }
    }

    let playlist = if payload.playlist.is_some() {
        get_session_playlist(db, payload.session_id)
            .await?
            .map(|playlist| playlist.to_api())
            .map(|playlist| ApiUpdateSessionPlaylist {
                session_playlist_id: playlist.session_playlist_id,
                tracks: playlist.tracks,
            })
    } else {
        None
    };

    let response = ApiUpdateSession {
        session_id: payload.session_id,
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
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &DeleteSession,
) -> Result<(), WebsocketSendError> {
    moosicbox_session::delete_session(db, payload.session_id).await?;

    get_sessions(db, sender, context, true).await?;

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

fn playback_action(
    _sender: &impl WebsocketSender,
    _context: &WebsocketContext,
    _payload: &Value,
) -> Result<(), WebsocketSendError> {
    Ok(())
}
