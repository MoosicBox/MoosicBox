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
/// This module provides the main [`TunnelSender`] type for establishing
/// and maintaining tunnel connections, along with the [`TunnelSenderHandle`]
/// for controlling active connections.
///
/// [`TunnelSender`]: sender::TunnelSender
/// [`TunnelSenderHandle`]: sender::TunnelSenderHandle
pub mod sender;

/// WebSocket message routing through tunnel connections.
///
/// This module provides [`TunnelWebsocketSender`] for routing WebSocket messages
/// through both local and tunnel connections with connection filtering support.
///
/// [`TunnelWebsocketSender`]: websocket_sender::TunnelWebsocketSender
pub mod websocket_sender;

/// Error type for sending bytes through the tunnel.
#[derive(Debug, Error)]
pub enum SendBytesError {
    /// Unknown error occurred during byte transmission.
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// Error type for sending messages through the tunnel.
#[derive(Debug, Error)]
pub enum SendMessageError {
    /// Unknown error occurred during message transmission.
    #[error("Unknown {0:?}")]
    Unknown(String),
}

/// Error type for tunnel request processing.
#[derive(Debug, Error)]
pub enum TunnelRequestError {
    /// Request contained invalid or malformed data.
    #[error("Bad request: {0}")]
    BadRequest(String),
    /// Requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),
    /// Query parameters were invalid or malformed.
    #[error("Invalid Query: {0}")]
    InvalidQuery(String),
    /// Generic request error occurred.
    #[error("Request error: {0}")]
    Request(String),
    /// Other unspecified error occurred.
    #[error("Other: {0}")]
    Other(String),
    /// HTTP method is not supported for this route.
    #[error("Unsupported Method")]
    UnsupportedMethod,
    /// Requested route is not supported.
    #[error("Unsupported Route")]
    UnsupportedRoute,
    /// Required profile was not provided or not found.
    #[error("Missing profile")]
    MissingProfile,
    /// Internal server error with underlying cause.
    #[error("Internal server error: {0:?}")]
    InternalServerError(Box<dyn std::error::Error + Send>),
    /// WebSocket message processing error.
    #[error("Websocket Message Error")]
    WebsocketMessage(#[from] WebsocketMessageError),
    /// I/O operation error.
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Tokio task join error.
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    /// HTTP request error from `switchy_http`.
    #[error(transparent)]
    Reqwest(#[from] switchy_http::Error),
    /// Regular expression parsing or matching error.
    #[error(transparent)]
    Regex(#[from] regex::Error),
    /// JSON serialization/deserialization error.
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    /// Music API operation error.
    #[error(transparent)]
    MusicApi(#[from] Error),
}

/// Query parameters for retrieving track audio data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
    format: Option<AudioFormat>,
    quality: Option<TrackAudioQuality>,
    source: Option<ApiSource>,
}

/// Query parameters for retrieving track metadata information.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTrackInfoQuery {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    track_id: i32,
    source: Option<ApiSource>,
}

/// Message types received from the tunnel WebSocket connection.
///
/// Represents the various types of messages that can be received over
/// the tunnel WebSocket, including text/binary data and control frames.
pub enum TunnelMessage {
    /// Text message with UTF-8 string content.
    Text(String),
    /// Binary message with raw bytes.
    Binary(Bytes),
    /// Ping control frame with optional payload.
    Ping(Vec<u8>),
    /// Pong control frame with optional payload.
    Pong(Vec<u8>),
    /// Close control frame indicating connection closure.
    Close,
    /// Raw WebSocket frame.
    Frame(Frame),
}
