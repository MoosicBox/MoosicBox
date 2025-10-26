//! WebSocket-based tunnel server for proxying HTTP requests through persistent connections.
//!
//! This crate provides a server that allows clients to establish WebSocket connections
//! and tunnel HTTP requests through them. The server handles authentication, request
//! routing, and bidirectional streaming of request/response data.
//!
//! # Main Features
//!
//! * WebSocket-based client connections with persistent tunnels
//! * HTTP request proxying through tunnel connections
//! * Authentication via client tokens, signature tokens, and magic tokens
//! * Request/response streaming with cancellation support
//! * Profile-based request routing
//!
//! # Architecture
//!
//! The server consists of several components:
//!
//! * WebSocket server handling client connections
//! * HTTP API endpoints for tunneling requests
//! * Authentication layer for client and request validation
//! * Database layer for token storage and client management
//!
//! # Public API
//!
//! The primary public export is [`CANCELLATION_TOKEN`], a global cancellation token
//! used to coordinate graceful shutdown across all server components.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::LazyLock;

use switchy_async::util::CancellationToken;

/// Global cancellation token for coordinating shutdown of the tunnel server.
///
/// This token is used to signal cancellation to all running services and connections
/// when the server is shutting down. Services should clone this token and check it
/// periodically or use it with cancellable operations.
pub static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
