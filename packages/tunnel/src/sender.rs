use bytes::Bytes;
use moosicbox_files::api::AudioFormat;
use moosicbox_ws::api::WebsocketMessageError;
use serde::Deserialize;
use serde_aux::prelude::*;
use thiserror::Error;
use tokio_tungstenite::tungstenite::protocol::frame::Frame;

pub mod tunnel_sender;
pub mod tunnel_websocket_sender;

#[derive(Debug, Error)]
pub enum SendBytesError {
    #[error("Unknown {0:?}")]
    Unknown(String),
}

#[derive(Debug, Error)]
pub enum SendMessageError {
    #[error("Unknown {0:?}")]
    Unknown(String),
}

#[derive(Debug, Error)]
pub enum TunnelRequestError {
    #[error("Invalid Query: {0}")]
    InvalidQuery(String),
    #[error("Unsupported Method")]
    UnsupportedMethod,
    #[error("Unsupported Route")]
    UnsupportedRoute,
    #[error("Websocket Message Error")]
    WebsocketMessage(#[from] WebsocketMessageError),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
    format: Option<AudioFormat>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackInfoQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
}

pub enum TunnelMessage {
    Text(String),
    Binary(Bytes),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
    Frame(Frame),
}
