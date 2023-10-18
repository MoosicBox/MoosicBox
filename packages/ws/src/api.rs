use core::fmt;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use async_trait::async_trait;
use moosicbox_core::{
    app::Db,
    sqlite::models::{CreateSession, DeleteSession, UpdateSession},
    ToApi,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status_code: u16,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Connect,
    Disconnect,
    Message,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundMessageType {
    Ping,
    GetConnectionId,
    GetSessions,
    CreateSession,
    UpdateSession,
    DeleteSession,
    SyncConnectionData,
    PlaybackAction,
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
}

pub struct WebsocketContext {
    pub connection_id: String,
    pub event_type: EventType,
}

#[derive(Debug, Error)]
pub enum WebsocketSendError {
    #[error("Unknown")]
    Unknown,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsocketConnectionData {
    pub playing: bool,
}

#[async_trait]
pub trait WebsocketSender {
    async fn send(&mut self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;
    async fn send_all(&mut self, data: &str) -> Result<(), WebsocketSendError>;
    async fn send_all_except(
        &mut self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError>;
}

static CONNECTION_DATA: OnceLock<Mutex<HashMap<String, WebsocketConnectionData>>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum WebsocketConnectError {
    #[error("Unknown")]
    Unknown,
}

pub async fn connect(
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<Response, WebsocketConnectError> {
    println!("Connected {}", context.connection_id);
    sender
        .send(
            &context.connection_id,
            &serde_json::json!({
                "connectionId": context.connection_id,
                "type": OutboundMessageType::ConnectionId
            })
            .to_string(),
        )
        .await
        .map_err(|_| WebsocketConnectError::Unknown)?;
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
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<Response, WebsocketDisconnectError> {
    let connections = {
        let mut connection_data = CONNECTION_DATA
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap();

        connection_data.remove(&context.connection_id);

        &serde_json::to_string(&connection_data.values().collect::<Vec<_>>()).unwrap()
    };

    sender
        .send(&context.connection_id, connections)
        .await
        .map_err(|_e| WebsocketDisconnectError::Unknown)?;

    println!("Disconnected {}", context.connection_id);

    Ok(Response {
        status_code: 200,
        body: "Disconnected".into(),
    })
}

#[derive(Debug, Error)]
pub enum WebsocketMessageError {
    #[error("Missing message type")]
    MissingMessageType,
    #[error("Invalid message type")]
    InvalidMessageType,
    #[error("Missing payload")]
    MissingPayload,
    #[error("Unknown")]
    Unknown,
}

pub async fn message(
    db: Arc<Mutex<Db>>,
    sender: &mut impl WebsocketSender,
    payload: Option<&Value>,
    message_type: InboundMessageType,
    context: &WebsocketContext,
) -> Result<Response, WebsocketMessageError> {
    println!(
        "Received message type {} from {}: {:?}",
        message_type, context.connection_id, payload
    );
    match message_type {
        InboundMessageType::GetConnectionId => {
            get_connection_id(sender, context)
                .await
                .map_err(|_e| WebsocketMessageError::Unknown)?;
            Ok(())
        }
        InboundMessageType::GetSessions => {
            get_sessions(db, sender, context, false)
                .await
                .map_err(|_e| WebsocketMessageError::Unknown)?;
            Ok(())
        }
        InboundMessageType::CreateSession => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload = serde_json::from_value::<CreateSession>(payload.clone())
                .map_err(|_| WebsocketMessageError::Unknown)?;

            create_session(db, sender, context, &payload)
                .await
                .map_err(|_e| WebsocketMessageError::Unknown)?;
            Ok(())
        }
        InboundMessageType::UpdateSession => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload = serde_json::from_value::<UpdateSession>(payload.clone())
                .map_err(|_| WebsocketMessageError::Unknown)?;

            update_session(db, sender, context, &payload)
                .await
                .map_err(|_e| WebsocketMessageError::Unknown)?;
            Ok(())
        }
        InboundMessageType::DeleteSession => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            let payload = serde_json::from_value::<DeleteSession>(payload.clone())
                .map_err(|_| WebsocketMessageError::Unknown)?;

            delete_session(db, sender, context, &payload)
                .await
                .map_err(|_e| WebsocketMessageError::Unknown)?;
            Ok(())
        }
        InboundMessageType::Ping => {
            println!("Ping {payload:?}");
            Ok(())
        }
        InboundMessageType::PlaybackAction => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            playback_action(sender, context, payload)
                .await
                .map_err(|_e| WebsocketMessageError::Unknown)?;
            Ok(())
        }
        InboundMessageType::SyncConnectionData => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            sync_connection_data(sender, context, payload)
                .await
                .map_err(|_e| WebsocketMessageError::Unknown)?;
            Ok(())
        }
    }?;

    println!(
        "Successfully processed message type {} from {}",
        message_type, context.connection_id
    );
    Ok(Response {
        status_code: 200,
        body: "Received".into(),
    })
}

async fn get_sessions(
    db: Arc<Mutex<Db>>,
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
    send_all: bool,
) -> Result<(), WebsocketSendError> {
    let sessions = {
        let db = db.lock();
        let library = db.as_ref().unwrap().library.lock().unwrap();
        moosicbox_core::sqlite::db::get_sessions(&library)
            .map_err(|_| WebsocketSendError::Unknown)?
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
    db: Arc<Mutex<Db>>,
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
    payload: &CreateSession,
) -> Result<(), WebsocketSendError> {
    println!("create session.... 0");
    {
        let db = db.lock();
        let library = db.as_ref().unwrap().library.lock().unwrap();
        moosicbox_core::sqlite::db::create_session(&library, payload)
            .map_err(|_| WebsocketSendError::Unknown)?;
    }
    get_sessions(db, sender, context, true).await?;
    Ok(())
}

async fn update_session(
    db: Arc<Mutex<Db>>,
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
    payload: &UpdateSession,
) -> Result<(), WebsocketSendError> {
    {
        let db = db.lock();
        let library = db.as_ref().unwrap().library.lock().unwrap();
        moosicbox_core::sqlite::db::update_session(&library, payload)
            .map_err(|_| WebsocketSendError::Unknown)?;
    }

    get_sessions(db, sender, context, true).await?;
    Ok(())
}

async fn delete_session(
    db: Arc<Mutex<Db>>,
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
    payload: &DeleteSession,
) -> Result<(), WebsocketSendError> {
    {
        let db = db.lock();
        let library = db.as_ref().unwrap().library.lock().unwrap();
        moosicbox_core::sqlite::db::delete_session(&library, payload.session_id)
            .map_err(|_| WebsocketSendError::Unknown)?;
    }

    get_sessions(db, sender, context, true).await?;

    Ok(())
}

async fn get_connection_id(
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<(), WebsocketSendError> {
    sender
        .send(
            &context.connection_id,
            &serde_json::json!({
                "connectionId": context.connection_id,
                "type": OutboundMessageType::ConnectionId
            })
            .to_string(),
        )
        .await
}

async fn playback_action(
    _sender: &mut impl WebsocketSender,
    _context: &WebsocketContext,
    _payload: &Value,
) -> Result<(), WebsocketSendError> {
    Ok(())
}

async fn sync_connection_data(
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
    payload: &Value,
) -> Result<(), WebsocketSendError> {
    let connections = {
        let mut connection_data = CONNECTION_DATA
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap();

        connection_data.insert(
            context.connection_id.clone(),
            serde_json::from_value(payload.clone()).unwrap(),
        );
        &serde_json::to_string(&connection_data.values().collect::<Vec<_>>()).unwrap()
    };

    sender.send(&context.connection_id, connections).await?;

    Ok(())
}
