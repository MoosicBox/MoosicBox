//! HTTP API endpoints for the `MoosicBox` server.
//!
//! This module provides core HTTP endpoints including health checks and WebSocket connections.
//! Additional API endpoints are registered from other crates based on enabled features.

use crate::{WS_SERVER_HANDLE, ws::handler};
use actix_web::{HttpResponse, route};
use actix_web::{
    Result, get,
    web::{self, Json},
};
use log::info;
use moosicbox_profiles::api::ProfileName;
use serde_json::{Value, json};

#[cfg(feature = "openapi")]
pub mod openapi;

/// Health check endpoint for monitoring server status.
///
/// This endpoint is typically used by load balancers and monitoring systems to verify that
/// the server is running and responsive.
///
/// # Returns
///
/// Returns a JSON object containing:
/// * `healthy` - Always `true` when the server is responding
/// * `hash` - The Git commit hash of the running server version
///
/// # Errors
///
/// This function does not currently return errors.
#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({
        "healthy": true,
        "hash": std::env!("GIT_HASH"),
    })))
}

/// WebSocket connection endpoint for real-time client-server communication.
///
/// Upgrades an HTTP request to a WebSocket connection, allowing bidirectional communication
/// for features like playback control, library updates, and player status notifications.
///
/// # Errors
///
/// * If the WebSocket upgrade handshake fails
/// * If the `WS_SERVER_HANDLE` is not initialized
///
/// # Panics
///
/// * If the `WS_SERVER_HANDLE` lock is poisoned
/// * If `WS_SERVER_HANDLE` is not initialized when a connection is attempted
#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::future_not_send)]
#[get("/ws")]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    profile_name: ProfileName,
) -> Result<HttpResponse, actix_web::Error> {
    let profile = profile_name.into();
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    switchy_async::runtime::Handle::current().spawn_local_with_name(
        "server: WsClient",
        handler::handle_ws(
            WS_SERVER_HANDLE
                .read()
                .await
                .as_ref()
                .expect("No WsServerHandle available")
                .clone(),
            profile,
            session,
            msg_stream,
        ),
    );

    Ok(response)
}
