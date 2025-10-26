//! Tunnel sender implementation for `MoosicBox` remote connectivity.
//!
//! This crate provides the client-side functionality for establishing and maintaining
//! tunnel connections to remote `MoosicBox` servers. It handles bidirectional communication
//! over WebSocket connections, forwarding HTTP requests and WebSocket messages through
//! the tunnel.
//!
//! # Main Components
//!
//! * [`TunnelSender`](sender::TunnelSender) - Main tunnel client that manages WebSocket
//!   connections and request forwarding
//! * [`TunnelSenderHandle`](sender::TunnelSenderHandle) - Handle for controlling active
//!   tunnel connections
//! * [`TunnelWebsocketSender`](websocket_sender::TunnelWebsocketSender) - Routes WebSocket
//!   messages through tunnel connections
//!
//! # Example
//!
//! ```rust,no_run
//! # use moosicbox_tunnel_sender::sender::TunnelSender;
//! # use switchy_database::config::ConfigDatabase;
//! # use std::sync::Arc;
//! # async fn example(config_db: ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! let (sender, handle) = TunnelSender::new(
//!     "https://example.com".to_string(),
//!     "wss://example.com/tunnel".to_string(),
//!     "client-id".to_string(),
//!     "access-token".to_string(),
//!     config_db,
//! );
//!
//! // Start receiving messages from the tunnel
//! let mut receiver = sender.start();
//!
//! // Process incoming messages
//! while let Some(message) = receiver.recv().await {
//!     // Handle tunnel messages
//! }
//!
//! // Close the tunnel when done
//! handle.close();
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use bytes::Bytes;
use moosicbox_music_api::{Error, models::TrackAudioQuality};
use moosicbox_music_models::{ApiSource, AudioFormat};
use moosicbox_ws::WebsocketMessageError;
use serde::Deserialize;
use serde_aux::prelude::*;
use thiserror::Error;
use tokio_tungstenite::tungstenite::protocol::frame::Frame;

/// Core tunnel sender implementation and connection management.
///
/// This module provides the main [`TunnelSender`](sender::TunnelSender) type for establishing
/// and maintaining tunnel connections, along with the [`TunnelSenderHandle`](sender::TunnelSenderHandle)
/// for controlling active connections.
pub mod sender;

/// WebSocket message routing through tunnel connections.
///
/// This module provides [`TunnelWebsocketSender`](websocket_sender::TunnelWebsocketSender)
/// for routing WebSocket messages through both local and tunnel connections with connection
/// filtering support.
pub mod websocket_sender;

/// Error type for sending bytes through the tunnel.
#[derive(Debug, Error)]
pub enum SendBytesError {
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// Error type for sending messages through the tunnel.
#[derive(Debug, Error)]
pub enum SendMessageError {
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// Error type for tunnel request processing.
#[derive(Debug, Error)]
pub enum TunnelRequestError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Not found: {0}")]
    NotFound(String),
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
    #[error("Missing profile")]
    MissingProfile,
    #[error("Internal server error: {0:?}")]
    InternalServerError(Box<dyn std::error::Error + Send>),
    #[error("Websocket Message Error")]
    WebsocketMessage(#[from] WebsocketMessageError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Reqwest(#[from] switchy_http::Error),
    #[error(transparent)]
    Regex(#[from] regex::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    MusicApi(#[from] Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
    format: Option<AudioFormat>,
    quality: Option<TrackAudioQuality>,
    source: Option<ApiSource>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackInfoQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
    source: Option<ApiSource>,
}

/// Message type received from the tunnel websocket.
pub enum TunnelMessage {
    Text(String),
    Binary(Bytes),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
    Frame(Frame),
}
