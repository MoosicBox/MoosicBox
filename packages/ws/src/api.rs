use core::fmt;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use async_trait::async_trait;
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
        InboundMessageType::Ping => {
            println!("Ping {payload:?}");
            Ok(())
        }
        InboundMessageType::PlaybackAction => {
            let payload = payload.ok_or(WebsocketMessageError::MissingPayload)?;
            println!("Playback Action {payload:?}");
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

    Ok(Response {
        status_code: 200,
        body: "Received".into(),
    })
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
