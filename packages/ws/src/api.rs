use core::fmt;
use std::{collections::HashMap, str::FromStr, sync::RwLock};

use log::{debug, info, trace};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_session_playlist, DbError},
        models::{
            ApiUpdateSession, ApiUpdateSessionPlaylist, Connection, CreateSession, DeleteSession,
            Player, PlayerType, RegisterConnection, RegisterPlayer, SetSeek,
            SetSessionActivePlayers, ToApi, UpdateSession,
        },
    },
};
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
    Connections,
    SetSeek,
}

pub struct WebsocketContext {
    pub connection_id: String,
}

#[derive(Debug, Error)]
pub enum WebsocketSendError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("Unknown: {0}")]
    Unknown(String),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsocketConnectionData {
    pub playing: bool,
}

pub trait WebsocketSender: Send {
    fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;
    fn send_all(&self, data: &str) -> Result<(), WebsocketSendError>;
    fn send_all_except(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;
}

impl core::fmt::Debug for dyn WebsocketSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{WebsocketSender}}")
    }
}

static CONNECTION_DATA: Lazy<RwLock<HashMap<String, Connection>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Error)]
pub enum WebsocketConnectError {
    #[error("Unknown")]
    Unknown,
}

pub fn connect(
    _db: &Db,
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

pub fn disconnect(
    db: &Db,
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
        .map_err(|_e| WebsocketDisconnectError::Unknown)?;

    sender
        .send_all(&get_connections(db).map_err(|_e| WebsocketDisconnectError::Unknown)?)
        .map_err(|_e| WebsocketDisconnectError::Unknown)?;

    info!("Disconnected {}", context.connection_id);

    Ok(Response {
        status_code: 200,
        body: "Disconnected".into(),
    })
}

pub fn process_message(
    db: &Db,
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

    message(db, sender, payload, message_type, &context)
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

pub fn message(
    db: &Db,
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
            get_connection_id(sender, context)?;
            Ok::<_, WebsocketMessageError>(())
        }
        InboundMessageType::GetSessions => {
            get_sessions(db, sender, context, false)?;
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

            register_connection(db, sender, context, &payload)?;

            sender.send_all(&get_connections(db)?)?;

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

            register_players(db, sender, context, &payload).map_err(|e| {
                WebsocketMessageError::Unknown {
                    message: e.to_string(),
                }
            })?;

            sender
                .send_all(
                    &get_connections(db).map_err(|e| WebsocketMessageError::Unknown {
                        message: e.to_string(),
                    })?,
                )
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

            set_session_active_players(db, sender, context, &payload)?;

            sender.send_all_except(&context.connection_id, &get_connections(db)?)?;

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

            create_session(db, sender, context, &payload)?;
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

            update_session(db, sender, Some(context), &payload)?;
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

            delete_session(db, sender, context, &payload)?;
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
            )?;

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

fn get_sessions(
    db: &Db,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    send_all: bool,
) -> Result<(), WebsocketSendError> {
    let sessions = {
        let library = db.library.lock().unwrap();
        moosicbox_core::sqlite::db::get_sessions(&library.inner)?
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
        sender.send_all(&sessions_json)
    } else {
        sender.send(&context.connection_id, &sessions_json)
    }
}

fn create_session(
    db: &Db,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &CreateSession,
) -> Result<(), WebsocketSendError> {
    {
        let library = db.library.lock().unwrap();
        moosicbox_core::sqlite::db::create_session(&library.inner, payload)?;
    }
    get_sessions(db, sender, context, true)?;
    Ok(())
}

fn get_connections(db: &Db) -> Result<String, WebsocketSendError> {
    let connections = {
        let library = db.library.lock().unwrap();
        let connection_data = CONNECTION_DATA.read().unwrap();
        moosicbox_core::sqlite::db::get_connections(&library.inner)?
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

fn register_connection(
    db: &Db,
    _sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &RegisterConnection,
) -> Result<(), WebsocketSendError> {
    let connection = {
        let library = db.library.lock().unwrap();

        moosicbox_core::sqlite::db::register_connection(&library.inner, payload)?
    };

    let mut connection_data = CONNECTION_DATA.write().unwrap();

    connection_data.insert(context.connection_id.clone(), connection);

    Ok(())
}

fn register_players(
    db: &Db,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &Vec<RegisterPlayer>,
) -> Result<(), WebsocketSendError> {
    {
        let library = db.library.lock().unwrap();

        for player in payload {
            moosicbox_core::sqlite::db::create_player(
                &library.inner,
                &context.connection_id,
                player,
            )?;
        }
    }
    get_sessions(db, sender, context, true)?;
    Ok(())
}

fn set_session_active_players(
    db: &Db,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &SetSessionActivePlayers,
) -> Result<(), WebsocketMessageError> {
    {
        let library = db.library.lock().unwrap();

        moosicbox_core::sqlite::db::set_session_active_players(&library.inner, payload)?;
    }
    get_sessions(db, sender, context, true)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum UpdateSessionError {
    #[error("No session found")]
    NoSessionFound,
    #[error(transparent)]
    WebsocketSend(WebsocketSendError),
    #[error(transparent)]
    Db(#[from] DbError),
}

pub fn update_session(
    db: &Db,
    sender: &impl WebsocketSender,
    context: Option<&WebsocketContext>,
    payload: &UpdateSession,
) -> Result<(), UpdateSessionError> {
    let before_session_playing = moosicbox_core::sqlite::db::get_session_playing(
        &db.library.lock().as_ref().unwrap().inner,
        payload.session_id,
    )?
    .map(Ok)
    .unwrap_or(Err(UpdateSessionError::NoSessionFound))?;

    moosicbox_core::sqlite::db::update_session(
        &db.library.lock().as_ref().unwrap().inner,
        payload,
    )?;

    if let Some(playing) = payload.playing {
        if playing != before_session_playing {
            let active_players = moosicbox_core::sqlite::db::get_session_active_players(
                &db.library.lock().as_ref().unwrap().inner,
                payload.session_id,
            )?;

            match playing {
                true => play_session(&active_players),
                false => pause_session(&active_players),
            }
        }
    }

    let playlist = if payload.playlist.is_some() {
        get_session_playlist(
            &db.library.lock().as_ref().unwrap().inner,
            payload.session_id,
        )?
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
        sender
            .send_all_except(&context.connection_id, &session_updated)
            .map_err(UpdateSessionError::WebsocketSend)?;
    } else {
        sender
            .send_all(&session_updated)
            .map_err(UpdateSessionError::WebsocketSend)?;
    }

    Ok(())
}

fn play_session(active_players: &[Player]) {
    for player in active_players {
        if player.r#type == PlayerType::Symphonia {
            debug!("Playing Symphonia player");
        }
    }
}

fn pause_session(active_players: &[Player]) {
    for player in active_players {
        if player.r#type == PlayerType::Symphonia {
            debug!("Pausing Symphonia player");
        }
    }
}

fn delete_session(
    db: &Db,
    sender: &impl WebsocketSender,
    context: &WebsocketContext,
    payload: &DeleteSession,
) -> Result<(), WebsocketSendError> {
    {
        let library = db.library.lock().unwrap();
        moosicbox_core::sqlite::db::delete_session(&library.inner, payload.session_id)?;
    }

    get_sessions(db, sender, context, true)?;

    Ok(())
}

fn get_connection_id(
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
}

fn playback_action(
    _sender: &impl WebsocketSender,
    _context: &WebsocketContext,
    _payload: &Value,
) -> Result<(), WebsocketSendError> {
    Ok(())
}
