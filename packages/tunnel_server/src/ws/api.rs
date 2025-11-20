//! WebSocket API endpoint for establishing tunnel connections.
//!
//! This module provides the HTTP endpoint that upgrades connections to WebSocket
//! for persistent tunnel communication. It handles the initial handshake and
//! delegates to the handler module for the connection lifecycle.

use crate::WS_SERVER_HANDLE;
use crate::auth::SignatureAuthorized;
use crate::ws::handler;
use actix_web::HttpResponse;
use actix_web::{
    Result, get,
    web::{self},
};
use moosicbox_profiles::api::ProfileNameUnverified;
use serde::Deserialize;

/// WebSocket connection request parameters.
///
/// This struct contains the query parameters for establishing a WebSocket connection.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    /// The unique identifier for the connecting client.
    client_id: String,
    /// Whether this connection is acting as a sender (true) or receiver (false).
    /// Defaults to false if not specified.
    sender: Option<bool>,
}

/// WebSocket endpoint for establishing tunnel connections.
///
/// This endpoint upgrades an HTTP connection to WebSocket and establishes a
/// persistent tunnel connection for the client. The connection can act as either
/// a sender (responding to HTTP requests) or a client (initiating WebSocket requests).
///
/// # Errors
///
/// * Returns an error if the WebSocket handshake fails
/// * Returns [`actix_web::error::ErrorUnauthorized`] if the signature token is invalid
#[get("/ws")]
#[allow(clippy::similar_names, clippy::future_not_send)]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    query: web::Query<ConnectRequest>,
    profile: Option<ProfileNameUnverified>,
    _: SignatureAuthorized,
) -> Result<HttpResponse, actix_web::Error> {
    let ws_server = WS_SERVER_HANDLE.read().await.as_ref().unwrap().clone();
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    switchy_async::runtime::Handle::current().spawn_local_with_name(
        "tunnel_server_websocket",
        handler::handle_ws(
            ws_server,
            session,
            msg_stream,
            query.client_id.clone(),
            query.sender.unwrap_or(false),
            profile.map(|x| x.0),
        ),
    );

    Ok(res)
}
