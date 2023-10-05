use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebsocketConnectError {
    #[error("Unknown")]
    Unknown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status_code: u16,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventType {
    Connect,
    Disconnect,
    Message,
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

#[async_trait]
pub trait WebsocketSender {
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;
}

pub async fn connect(context: &WebsocketContext) -> Result<Response, WebsocketConnectError> {
    println!("Connected {}", context.connection_id);
    Ok(Response {
        status_code: 200,
        body: "Connected".into(),
    })
}

pub async fn disconnect(context: &WebsocketContext) -> Result<Response, WebsocketConnectError> {
    println!("Disconnected {}", context.connection_id);
    Ok(Response {
        status_code: 200,
        body: "Disconnected".into(),
    })
}

pub async fn message(
    body: &Value,
    context: &WebsocketContext,
) -> Result<Response, WebsocketConnectError> {
    println!(
        "Received message from {}: {:?}",
        context.connection_id, body
    );
    Ok(Response {
        status_code: 200,
        body: "Connected".into(),
    })
}
