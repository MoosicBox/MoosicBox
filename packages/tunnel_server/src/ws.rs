//! WebSocket server components for managing tunnel connections.
//!
//! This module provides the WebSocket server implementation that handles persistent
//! client connections for HTTP request tunneling. It includes connection management,
//! message routing, and the core WebSocket handling logic.

pub mod api;
pub mod handler;
pub mod server;

/// Connection ID.
pub type ConnId = u64;

/// Message sent to a room/client.
pub type Msg = String;
