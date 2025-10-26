//! WebSocket server and connection handling.
//!
//! This module provides WebSocket functionality for real-time client-server communication.
//! It manages client connections, message routing, and event broadcasting to subscribed clients.

pub mod handler;
pub mod server;

/// Connection ID.
pub type ConnId = u64;

/// Room ID.
pub type RoomId = String;

/// Message sent to a room/client.
pub type Msg = String;
