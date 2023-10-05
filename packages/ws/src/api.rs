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
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Connect,
    Disconnect,
    Message,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InputMessageType {
    Ping,
    GetConnectionId,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutputMessageType {
    Connect,
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

pub trait WebsocketSender {
    fn send(&mut self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError>;
}

pub fn connect(context: &WebsocketContext) -> Result<Response, WebsocketConnectError> {
    println!("Connected {}", context.connection_id);
    Ok(Response {
        status_code: 200,
        body: "Connected".into(),
    })
}

pub fn disconnect(context: &WebsocketContext) -> Result<Response, WebsocketConnectError> {
    println!("Disconnected {}", context.connection_id);
    Ok(Response {
        status_code: 200,
        body: "Disconnected".into(),
    })
}

pub fn message(
    sender: &mut impl WebsocketSender,
    payload: Option<&Value>,
    message_type: InputMessageType,
    context: &WebsocketContext,
) -> Result<Response, WebsocketConnectError> {
    println!(
        "Received message from {}: {:?}",
        context.connection_id, payload
    );
    match message_type {
        InputMessageType::GetConnectionId => {
            get_connection_id(sender, context).map_err(|_e| WebsocketConnectError::Unknown)?
        }
        InputMessageType::Ping => {
            println!("Ping {payload:?}");
        }
    }
    Ok(Response {
        status_code: 200,
        body: "Received".into(),
    })
}

fn get_connection_id(
    sender: &mut impl WebsocketSender,
    context: &WebsocketContext,
) -> Result<(), WebsocketSendError> {
    sender.send(
        &context.connection_id,
        &serde_json::json!({
            "connectionId": context.connection_id,
            "type": OutputMessageType::ConnectionId
        })
        .to_string(),
    )
}
