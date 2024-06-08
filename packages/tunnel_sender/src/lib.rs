#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use bytes::Bytes;
use moosicbox_core::{sqlite::models::TrackApiSource, types::AudioFormat};
use moosicbox_files::files::track::TrackAudioQuality;
use moosicbox_ws::WebsocketMessageError;
use serde::Deserialize;
use serde_aux::prelude::*;
use thiserror::Error;
use tokio_tungstenite::tungstenite::protocol::frame::Frame;

pub mod sender;
pub mod websocket_sender;

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
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Invalid Query: {0}")]
    InvalidQuery(String),
    #[error("Request error: {0}")]
    Request(String),
    #[error("Other: {0}")]
    Other(String),
    #[error("Unsupported Method")]
    UnsupportedMethod,
    #[error("Unsupported Route")]
    UnsupportedRoute,
    #[error("Internal server error: {0:?}")]
    InternalServerError(Box<dyn std::error::Error>),
    #[error("Websocket Message Error")]
    WebsocketMessage(#[from] WebsocketMessageError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Regex(#[from] regex::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
    format: Option<AudioFormat>,
    quality: Option<TrackAudioQuality>,
    source: Option<TrackApiSource>,
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
