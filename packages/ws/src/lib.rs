//! WebSocket message handling for `MoosicBox`.
//!
//! This crate provides WebSocket functionality for managing real-time communication
//! between `MoosicBox` clients and servers. It handles session management, audio zone
//! coordination, player registration, and broadcasting updates to connected clients.
//!
//! # Features
//!
//! * `ws` - Enables WebSocket message processing and connection management
//!
//! # Main Components
//!
//! * [`WebsocketSender`] - Trait for sending messages to WebSocket connections
//! * [`WebsocketContext`] - Context information for a WebSocket connection
//! * [`process_message`] - Processes incoming WebSocket messages
//! * [`connect`] and [`disconnect`] - Handle connection lifecycle
//! * [`models`] - Message payload types for inbound and outbound communication
//!
//! # Example
//!
//! ```rust,no_run
//! # use moosicbox_ws::{WebsocketSender, WebsocketContext, WebsocketSendError, connect};
//! # struct MockSender;
//! # #[async_trait::async_trait]
//! # impl WebsocketSender for MockSender {
//! #     async fn send(&self, _: &str, _: &str) -> Result<(), WebsocketSendError> { Ok(()) }
//! #     async fn send_all(&self, _: &str) -> Result<(), WebsocketSendError> { Ok(()) }
//! #     async fn send_all_except(&self, _: &str, _: &str) -> Result<(), WebsocketSendError> { Ok(()) }
//! #     async fn ping(&self) -> Result<(), WebsocketSendError> { Ok(()) }
//! # }
//! # let sender = MockSender;
//! // When a client connects
//! let context = WebsocketContext {
//!     connection_id: "client-123".to_string(),
//!     profile: Some("default".to_string()),
//!     player_actions: vec![],
//! };
//! let response = connect(&sender, &context);
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "ws")]
mod ws;

#[cfg(feature = "ws")]
pub use ws::*;

pub mod models;
