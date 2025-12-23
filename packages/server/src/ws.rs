//! WebSocket server and connection handling.
//!
//! This module provides WebSocket functionality for real-time client-server communication.
//! It manages client connections, message routing, and event broadcasting to subscribed clients.

pub mod handler;
pub mod server;

/// Connection ID type for identifying WebSocket clients.
///
/// Each connected client is assigned a unique numeric identifier.
pub type ConnId = u64;

/// Room ID type for identifying WebSocket rooms.
///
/// Rooms are named groups that clients can join for receiving broadcast messages.
pub type RoomId = String;

/// Message type for WebSocket communication.
///
/// All WebSocket messages are transmitted as JSON-encoded strings.
pub type Msg = String;
