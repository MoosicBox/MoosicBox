#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use core::fmt;
use std::{
    collections::HashMap,
    future::Future,
    num::ParseIntError,
    pin::Pin,
    str::FromStr,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use log::{debug, info, trace};
use moosicbox_core::sqlite::{db::DbError, models::{SetSeek, ToApi as _}};
use moosicbox_database::Database;
use moosicbox_session::{db::get_session_playlist, models::{ApiUpdateSession, ApiUpdateSessionPlaylist, Connection, CreateSession, DeleteSession, RegisterConnection, RegisterPlayer, SetSessionActivePlayers, UpdateSession}};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status_code: u16,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Connect,
    Disconnect,
    Message,
}

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundMessageType {
    Ping,
    GetConnectionId,
    GetSessions,
    CreateSession,
    UpdateSession,
    DeleteSession,
    RegisterConnection,
    RegisterPlayers,
    SetActivePlayers,
    PlaybackAction,
    SetSeek,
}

impl fmt::Display for InboundMessageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutboundMessageType {
    Connect,
    NewConnection,
    ConnectionId,
    Sessions,
    SessionUpdated,
    DownloadEvent,
    Connections,
    SetSeek,
}

pub type PlayerAction = fn(&UpdateSession) -> Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Clone, Default, Debug)]
pub struct WebsocketContext {
    pub connection_id: String,
    pub player_actions: Vec<(i32, PlayerAction)>,
}

#[derive(Debug, Error)]
pub enum WebsocketSendError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("Unknown: {0}")]
    Unknown(String),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
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
    async fn send_all_except(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;
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
    let message_type = InboundMessageType::from_str(
        body.get("type")
            .ok_or(WebsocketMessageError::MissingMessageType)?
            .as_str()
            .ok_or(WebsocketMessageError::InvalidMessageType)?,
    )
    .map_err(|_| WebsocketMessageError::InvalidMessageType)?;

    let payload = body.get("payload");

    message(db, sender, payload, message_type, &context).await
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
    #[error("Unknown {message:?}")]
    Unknown { message: String },
}

pub async fn message(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    payload: Option<&Value>,
    message_type: InboundMessageType,
    context: &WebsocketContext,
) -> Result<Response, WebsocketMessageError> {
    debug!(
        "Received message type {} from {}: {:?}",
        message_type, context.connection_id, payload
    );
    match message_type {
        InboundMessageType::GetConnectionId => {
            get_connection_id(sender, context).await?;
            Ok::<_, WebsocketMessageError>(())
        }
        InboundMessageType::GetSessions => {
            get_sessions(db, sender, context, false).await?;
            Ok(())
        }
        InboundMessageType::RegisterConnection => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload =
                serde_json::from_value::<RegisterConnection>(payload.clone()).map_err(|e| {
                    WebsocketMessageError::Unknown {
                        message: e.to_string(),
                    }
                })?;

            register_connection(db, sender, context, &payload).await?;

            sender.send_all(&get_connections(db).await?).await?;

            Ok(())
        }
        InboundMessageType::RegisterPlayers => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload =
                serde_json::from_value::<Vec<RegisterPlayer>>(payload.clone()).map_err(|e| {
                    WebsocketMessageError::Unknown {
                        message: e.to_string(),
                    }
                })?;

            register_players(db, sender, context, &payload)
                .await
                .map_err(|e| WebsocketMessageError::Unknown {
                    message: e.to_string(),
                })?;

            sender
                .send_all(&get_connections(db).await.map_err(|e| {
                    WebsocketMessageError::Unknown {
                        message: e.to_string(),
                    }
                })?)
                .await
                .map_err(|e| WebsocketMessageError::Unknown {
                    message: e.to_string(),
                })?;

            Ok(())
        }
        InboundMessageType::SetActivePlayers => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload = serde_json::from_value::<SetSessionActivePlayers>(payload.clone())
                .map_err(|e| WebsocketMessageError::Unknown {
                    message: e.to_string(),
                })?;

            set_session_active_players(db, sender, context, &payload).await?;

            sender.send_all_except(&context.connection_id, &get_connections(db).await?).await?;

            Ok(())
        }
        InboundMessageType::CreateSession => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload =
                serde_json::from_value::<CreateSession>(payload.clone()).map_err(|e| {
                    WebsocketMessageError::Unknown {
                        message: e.to_string(),
                    }
                })?;

            create_session(db, sender, context, &payload).await?;
            Ok(())
        }
        InboundMessageType::UpdateSession => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload =
                serde_json::from_value::<UpdateSession>(payload.clone()).map_err(|e| {
                    WebsocketMessageError::Unknown {
                        message: e.to_string(),
                    }
                })?;

            update_session(db, sender, Some(context), &payload).await?;
            Ok(())
        }
        InboundMessageType::DeleteSession => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload =
                serde_json::from_value::<DeleteSession>(payload.clone()).map_err(|e| {
                    WebsocketMessageError::Unknown {
                        message: e.to_string(),
                    }
                })?;

            delete_session(db, sender, context, &payload).await?;
            Ok(())
        }
        InboundMessageType::Ping => {
            trace!("Ping {payload:?}");
            Ok(())
        }
        InboundMessageType::PlaybackAction => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            playback_action(sender, context, payload)?;
            Ok(())
        }
        InboundMessageType::SetSeek => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload = serde_json::from_value::<SetSeek>(payload.clone()).map_err(|e| {
                WebsocketMessageError::Unknown {
                    message: e.to_string(),
                }
            })?;

            sender.send_all_except(
                &context.connection_id,
                &serde_json::json!({
                    "type": OutboundMessageType::SetSeek,
                    "payload": payload,
                })
                .to_string(),
            ).await?;

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

pub async fn get_sessions(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    send_all: bool,
) -> Result<(), WebsocketSendError> {
    let sessions = {
        moosicbox_session::db::get_sessions(db)
            .await?
            .iter()
            .map(|session| session.to_api())
            .collect::<Vec<_>>()
    };

    let sessions_json = serde_json::json!({
        "type": OutboundMessageType::Sessions,
        "payload": sessions,
    })
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
    moosicbox_session::db::create_session(db, payload).await?;
    get_sessions(db, sender, context, true).await?;
    Ok(())
}

async fn get_connections(db: &dyn Database) -> Result<String, WebsocketSendError> {
    let connection_data = CONNECTION_DATA.as_ref().read().unwrap().clone();
    let connections = {
        moosicbox_session::db::get_connections(db)
            .await?
            .iter()
            .map(|connection| {
                let mut api = connection.to_api();

                api.alive = connection_data.values().any(|c| c.id == connection.id);

                api
            })
            .collect::<Vec<_>>()
    };

    let connections_json = serde_json::json!({
        "type": OutboundMessageType::Connections,
        "payload": connections,
    })
    .to_string();

    Ok(connections_json)
}

pub async fn register_connection(
    db: &dyn Database,
    _sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &RegisterConnection,
) -> Result<Connection, WebsocketSendError> {
    let connection = moosicbox_session::db::register_connection(db, payload).await?;

    let mut connection_data = CONNECTION_DATA.write().unwrap();

    connection_data.insert(context.connection_id.clone(), connection.clone());

    Ok(connection)
}

pub async fn register_players(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &Vec<RegisterPlayer>,
) -> Result<Vec<moosicbox_session::models::Player>, WebsocketSendError> {
    let mut players = vec![];
    for player in payload {
        players.push(
            moosicbox_session::db::create_player(db, &context.connection_id, player).await?,
        );
    }

    get_sessions(db, sender, context, true).await?;

    Ok(players)
}

async fn set_session_active_players(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &SetSessionActivePlayers,
) -> Result<(), WebsocketMessageError> {
    moosicbox_session::db::set_session_active_players(db, payload).await?;
    get_sessions(db, sender, context, true).await?;
    Ok(())
}

pub async fn send_download_event<ProgressEvent: Serialize>(
    sender: &impl WebsocketSender,
    context: Option<&WebsocketContext>,
    payload: ProgressEvent,
) -> Result<(), WebsocketSendError> {
    let session_updated = serde_json::json!({
        "type": OutboundMessageType::DownloadEvent,
        "payload": payload,
    })
    .to_string();

    if let Some(context) = context {
        sender.send_all_except(&context.connection_id, &session_updated).await?;
    } else {
        sender.send_all(&session_updated).await?;
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
}

pub async fn update_session(
    db: &dyn Database,
    sender: &impl WebsocketSender,
    context: Option<&WebsocketContext>,
    payload: &UpdateSession,
) -> Result<(), UpdateSessionError> {
    if let Some(actions) = context.map(|x| &x.player_actions) {
        if payload.playback_updated() {
            if let Some(session) =
                moosicbox_session::db::get_session(db, payload.session_id).await?
            {
                let funcs = session
                    .active_players
                    .iter()
                    .filter_map(|p| {
                        actions
                            .iter()
                            .find_map(|x| if x.0 == p.id { Some(x.1) } else { None })
                    })
                    .collect::<Vec<_>>();

                if log::log_enabled!(log::Level::Trace) {
                    log::trace!(
                        "Running player actions on existing session id={} count_of_funcs={} payload={payload:?} session={session:?}",
                        session.id, 
                        funcs.len(),
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

    if log::log_enabled!(log::Level::Trace) {
        log::trace!("Updating session id={} payload={payload:?}", payload.session_id);
    } else {
        log::debug!("Updating session id={}", payload.session_id);
    }
    moosicbox_session::db::update_session(db, payload).await?;

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
        playlist,
    };

    let session_updated = serde_json::json!({
        "type": OutboundMessageType::SessionUpdated,
        "payload": response,
    })
    .to_string();

    if let Some(context) = context {
        sender.send_all_except(&context.connection_id, &session_updated).await?;
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
    moosicbox_session::db::delete_session(db, payload.session_id).await?;

    get_sessions(db, sender, context, true).await?;

    Ok(())
}

async fn get_connection_id(
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<(), WebsocketSendError> {
    sender.send(
        &context.connection_id,
        &serde_json::json!({
            "connectionId": context.connection_id,
            "type": OutboundMessageType::ConnectionId
        })
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
