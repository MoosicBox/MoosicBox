//! HTTP API endpoints for tunnel server operations.
//!
//! This module provides the public HTTP API endpoints for the tunnel server, including:
//! authentication endpoints (client registration, token generation), file proxying endpoints
//! (tracks, album/artist covers), and the main tunnel endpoint that proxies arbitrary HTTP
//! requests through WebSocket connections.

#![allow(clippy::future_not_send)]

use actix_web::error::{
    ErrorBadRequest, ErrorFailedDependency, ErrorInternalServerError, ErrorUnauthorized,
};
use actix_web::http::{StatusCode, header};
use actix_web::web::{self, Json};
use actix_web::{HttpRequest, Result};
use actix_web::{HttpResponse, route};
use bytes::Bytes;
use futures_util::StreamExt;
use log::{debug, info};
use moosicbox_profiles::api::ProfileNameUnverified;
use moosicbox_tunnel::{
    TunnelEncoding, TunnelHttpRequest, TunnelRequest, TunnelResponse, TunnelStream,
};
use qstring::QString;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::str::FromStr as _;
use switchy_async::sync::mpsc::{Receiver, unbounded};
use switchy_async::sync::oneshot;
use switchy_async::util::CancellationToken;
use switchy_http::models::Method;
use switchy_uuid::new_v4_string;
use thiserror::Error;

use crate::WS_SERVER_HANDLE;
use crate::auth::{
    ClientHeaderAuthorized, GeneralHeaderAuthorized, SignatureAuthorized, hash_token,
};
use crate::db::{
    insert_client_access_token, insert_magic_token, insert_signature_token, select_magic_token,
};
use crate::ws::server::service::{Commander, CommanderError};
use crate::ws::server::{ConnectionIdError, RequestHeaders, get_connection_id};

/// Health check endpoint for monitoring server status.
///
/// Returns a JSON response with the server's health status and current git hash.
#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({
        "healthy": true,
        "hash": std::env!("GIT_HASH"),
    })))
}

/// Request body for magic token authentication endpoints.
///
/// Magic tokens provide a temporary authentication mechanism for clients.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthMagicTokenRequest {
    /// The magic token to authenticate with.
    magic_token: String,
}

/// Authenticate and proxy a request using a magic token.
///
/// This endpoint validates the magic token and proxies the request through the
/// tunnel connection associated with the token's client ID. This is typically
/// used for one-time authentication links.
///
/// # Errors
///
/// * Returns [`ErrorUnauthorized`] if the magic token is invalid or expired.
/// * Returns [`DatabaseError`] if database operations fail.
#[route("/auth/magic-token", method = "GET")]
pub async fn auth_get_magic_token_endpoint(
    query: web::Query<AuthMagicTokenRequest>,
    profile: Option<ProfileNameUnverified>,
) -> Result<HttpResponse> {
    let token = &query.magic_token;
    let token_hash = &hash_token(token);

    if let Some(magic_token) = select_magic_token(token_hash).await? {
        handle_request(
            &magic_token.client_id,
            Method::Get,
            "auth/magic-token",
            json!({"magicToken": token}),
            None,
            None,
            profile.map(|x| x.0),
        )
        .await
    } else {
        log::warn!("Unauthorized get magic-token request",);
        Err(ErrorUnauthorized("Unauthorized"))
    }
}

/// Register a new magic token for a client.
///
/// Creates a new magic token in the database for the authenticated client.
/// The client must be authenticated via client access token.
///
/// # Errors
///
/// * Returns [`ErrorBadRequest`] if clientId is missing from query parameters.
/// * Returns [`DatabaseError`] if database operations fail.
#[route("/auth/magic-token", method = "POST")]
pub async fn auth_magic_token_endpoint(
    query: web::Query<AuthMagicTokenRequest>,
    req: HttpRequest,
    _: ClientHeaderAuthorized,
) -> Result<Json<Value>> {
    let token_hash = &hash_token(&query.magic_token);

    let query: Vec<_> = QString::from(req.query_string()).into();
    let client_id = query
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("clientId"))
        .map(|(_, value)| value)
        .ok_or_else(|| ErrorBadRequest("Missing clientId"))?;

    insert_magic_token(client_id, token_hash).await?;

    Ok(Json(json!({"success": true})))
}

/// Request parameters for registering a new client.
///
/// Used to create a new client access token for the specified client ID.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthRegisterClientRequest {
    /// The unique identifier for the client to register.
    client_id: String,
}

/// Register a new client and generate an access token.
///
/// Creates a new client access token for the specified client ID. Requires
/// general authorization via the tunnel access token.
///
/// # Errors
///
/// * Returns [`DatabaseError`] if database operations fail.
#[route("/auth/register-client", method = "POST", method = "HEAD")]
pub async fn auth_register_client_endpoint(
    query: web::Query<AuthRegisterClientRequest>,
    _: GeneralHeaderAuthorized,
) -> Result<Json<Value>> {
    let token = &new_v4_string();
    let token_hash = &hash_token(token);

    insert_client_access_token(&query.client_id, token_hash).await?;

    Ok(Json(json!({"token": token})))
}

/// Request parameters for authentication operations.
///
/// Generic authentication request containing the client ID.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    /// The unique identifier for the client.
    client_id: String,
}

/// Generate a new signature token for a client.
///
/// Creates a temporary signature token (valid for 14 days) for the authenticated
/// client. Signature tokens are used for request signing and short-term access.
///
/// # Errors
///
/// * Returns [`DatabaseError`] if database operations fail.
#[route("/auth/signature-token", method = "POST", method = "HEAD")]
pub async fn auth_signature_token_endpoint(
    query: web::Query<AuthRequest>,
    _: ClientHeaderAuthorized,
) -> Result<Json<Value>> {
    let token = &new_v4_string();
    let token_hash = &hash_token(token);

    insert_signature_token(&query.client_id, token_hash).await?;

    Ok(Json(json!({"token": token})))
}

/// Validate a signature token.
///
/// Checks if the provided signature token is valid. Returns a success response
/// if the token is valid, or an unauthorized error if invalid.
#[route("/auth/validate-signature-token", method = "POST", method = "HEAD")]
pub async fn auth_validate_signature_token_endpoint(_: SignatureAuthorized) -> Result<Json<Value>> {
    Ok(Json(json!({"valid": true})))
}

/// Proxy a request for a music track file through the tunnel.
///
/// This endpoint validates the signature token and proxies the request to the
/// client's tunnel connection to retrieve the track file.
///
/// # Errors
///
/// * Returns [`ErrorBadRequest`] if the request parameters are invalid.
/// * Returns [`ErrorFailedDependency`] if the client is not connected.
/// * Returns [`ErrorUnauthorized`] if the signature token is invalid.
#[route("/files/track", method = "GET", method = "HEAD", method = "OPTIONS")]
pub async fn track_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    profile: Option<ProfileNameUnverified>,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req, profile.map(|x| x.0)).await
}

/// Proxy a request for an artist cover image through the tunnel.
///
/// This endpoint validates the signature token and proxies the request to the
/// client's tunnel connection to retrieve the artist cover image.
///
/// # Errors
///
/// * Returns [`ErrorBadRequest`] if the request parameters are invalid.
/// * Returns [`ErrorFailedDependency`] if the client is not connected.
/// * Returns [`ErrorUnauthorized`] if the signature token is invalid.
#[route("/files/artists/{artist_id}/{size}", method = "GET", method = "HEAD")]
pub async fn artist_cover_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    profile: Option<ProfileNameUnverified>,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req, profile.map(|x| x.0)).await
}

/// Proxy a request for an album cover image through the tunnel.
///
/// This endpoint validates the signature token and proxies the request to the
/// client's tunnel connection to retrieve the album cover image.
///
/// # Errors
///
/// * Returns [`ErrorBadRequest`] if the request parameters are invalid.
/// * Returns [`ErrorFailedDependency`] if the client is not connected.
/// * Returns [`ErrorUnauthorized`] if the signature token is invalid.
#[route("/files/albums/{album_id}/{size}", method = "GET", method = "HEAD")]
pub async fn album_cover_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    profile: Option<ProfileNameUnverified>,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req, profile.map(|x| x.0)).await
}

/// Proxy any HTTP request through the tunnel connection.
///
/// This is the main tunnel endpoint that accepts any HTTP method and path.
/// It validates the client access token and proxies the request to the
/// client's tunnel connection.
///
/// # Errors
///
/// * Returns [`ErrorBadRequest`] if the request parameters are invalid or clientId is missing.
/// * Returns [`ErrorFailedDependency`] if the client is not connected.
/// * Returns [`ErrorUnauthorized`] if the client access token is invalid.
#[route(
    "/{path:.*}",
    method = "GET",
    method = "POST",
    method = "DELETE",
    method = "PUT",
    method = "PATCH",
    method = "HEAD"
)]
pub async fn tunnel_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    profile: Option<ProfileNameUnverified>,
    _: ClientHeaderAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req, profile.map(|x| x.0)).await
}

#[allow(dead_code)]
enum ResponseType {
    Stream,
    Body,
}

#[cfg_attr(feature = "telemetry", tracing::instrument)]
fn get_headers_for_request(req: &HttpRequest) -> Option<Value> {
    let mut headers = BTreeMap::<String, String>::new();

    for (key, value) in req.headers() {
        match *key {
            header::ACCEPT | header::RANGE => {
                headers.insert(key.to_string(), value.to_str().unwrap().to_string());
            }
            _ => {}
        }
    }

    if headers.is_empty() {
        None
    } else {
        Some(serde_json::to_value(headers).unwrap())
    }
}

#[cfg_attr(feature = "telemetry", tracing::instrument)]
async fn proxy_request(
    body: Option<Bytes>,
    req: HttpRequest,
    profile: Option<String>,
) -> Result<HttpResponse> {
    let method = Method::from_str(&req.method().to_string().to_uppercase()).map_err(|e| {
        ErrorBadRequest(format!(
            "Failed to parse method: '{:?}': {e:?}",
            req.method()
        ))
    })?;
    let path = req.path().strip_prefix('/').expect("Failed to get path");
    let query: Vec<_> = QString::from(req.query_string()).into();
    let query: BTreeMap<_, _> = query.into_iter().collect();
    let client_id = query
        .get("clientId")
        .cloned()
        .ok_or_else(|| ErrorBadRequest("Missing clientId query param"))?;
    let query = serde_json::to_value(query).unwrap();

    let body = body
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()?;

    let headers = get_headers_for_request(&req);

    handle_request(&client_id, method, path, query, body, headers, profile).await
}

#[cfg_attr(feature = "telemetry", tracing::instrument)]
async fn handle_request(
    client_id: &str,
    method: Method,
    path: &str,
    query: Value,
    payload: Option<Value>,
    headers: Option<Value>,
    profile: Option<String>,
) -> Result<HttpResponse> {
    let request_id = switchy_random::rng().next_u64();
    let abort_token = CancellationToken::new();

    debug!(
        "Starting ws request for {request_id} method={method} path={path} query={query:?} headers={headers:?} profile={profile:?} (id {request_id})"
    );

    let (headers_rx, rx) = request(
        client_id,
        request_id,
        method,
        path,
        query,
        payload,
        headers,
        profile,
        &abort_token,
    );

    let mut builder = HttpResponse::Ok();

    let headers = match headers_rx.await {
        Ok(headers) => headers,
        Err(err) => {
            log::error!(
                "Failed to receive headers for request_id={request_id} client_id={client_id} ({err:?})"
            );
            return Err(ErrorFailedDependency("Client with ID is not connected"));
        }
    };

    let response_type = ResponseType::Stream;

    builder.status(StatusCode::from_u16(headers.status).map_err(|e| {
        ErrorInternalServerError(format!(
            "Received invalid status code {}: {e:?}",
            headers.status
        ))
    })?);

    for (key, value) in &headers.headers {
        builder.insert_header((key.clone(), value.clone()));
    }

    let tunnel_stream = TunnelStream::new(request_id, rx, abort_token, &|request_id| async move {
        debug!("Request {request_id} ended");
        WS_SERVER_HANDLE
            .read()
            .await
            .as_ref()
            .unwrap()
            .send_command_async(crate::ws::server::Command::RequestEnd { request_id })
            .await?;
        Ok(())
    });

    match response_type {
        ResponseType::Stream => Ok(builder.streaming(tunnel_stream)),
        ResponseType::Body => {
            let body: Vec<_> = tunnel_stream
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .filter_map(Result::ok)
                .flatten()
                .collect();

            Ok(builder.body(body))
        }
    }
}

/// Errors that can occur when processing HTTP requests through the tunnel.
#[derive(Error, Debug)]
pub enum RequestError {
    /// Failed to look up or parse a connection ID.
    #[error(transparent)]
    ConnectionId(#[from] ConnectionIdError),
    /// Failed to send a command to the WebSocket server.
    #[error(transparent)]
    Commander(#[from] CommanderError),
}

#[cfg_attr(feature = "telemetry", tracing::instrument)]
#[allow(clippy::too_many_arguments)]
fn request(
    client_id: &str,
    request_id: u64,
    method: Method,
    path: &str,
    query: Value,
    payload: Option<Value>,
    headers: Option<Value>,
    profile: Option<String>,
    abort_token: &CancellationToken,
) -> (oneshot::Receiver<RequestHeaders>, Receiver<TunnelResponse>) {
    let (headers_tx, headers_rx) = oneshot::channel();
    let (tx, rx) = unbounded();

    let client_id = client_id.to_string();
    let path = path.to_string();
    let abort_token = abort_token.clone();

    switchy_async::runtime::Handle::current().spawn_with_name("tunnel_server_request", async move {
        debug!("Sending server request {request_id}");
        let ws_server = WS_SERVER_HANDLE.read().await.as_ref().unwrap().clone();
        ws_server
            .send_command_async(crate::ws::server::Command::RequestStart {
                request_id,
                sender: tx,
                headers_sender: headers_tx,
                abort_request_token: abort_token,
            })
            .await?;

        let conn_id = match get_connection_id(&client_id).await {
            Ok(conn_id) => conn_id,
            Err(err) => {
                log::error!(
                    "Failed to get connection id for request_id={request_id} client_id={client_id}: {err:?}"
                );
                ws_server
                    .send_command_async(crate::ws::server::Command::RequestEnd { request_id })
                    .await?;
                return Err(err.into());
            }
        };

        debug!("Sending server request {request_id} to {conn_id}");
        ws_server
            .send_command_async(crate::ws::server::Command::Message {
                msg: serde_json::to_value(TunnelRequest::Http(TunnelHttpRequest {
                    request_id,
                    method,
                    path: path.clone(),
                    query,
                    payload,
                    headers,
                    encoding: TunnelEncoding::Binary,
                    profile,
                }))
                .unwrap()
                .to_string(),
                conn: conn_id,
            })
            .await?;
        debug!("Sent server request {request_id} to {conn_id}");
        Ok::<_, RequestError>(())
    });

    (headers_rx, rx)
}
