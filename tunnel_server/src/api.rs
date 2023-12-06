use actix_web::error::{ErrorBadRequest, ErrorUnauthorized};
use actix_web::web::{self, Json};
use actix_web::{route, HttpResponse};
use actix_web::{HttpRequest, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use log::{debug, info};
use moosicbox_tunnel::tunnel::{
    Method, TunnelEncoding, TunnelHttpRequest, TunnelRequest, TunnelResponse, TunnelStream,
};
use qstring::QString;
use rand::{thread_rng, Rng as _};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::auth::{
    hash_token, ClientHeaderAuthorized, GeneralHeaderAuthorized, SignatureAuthorized,
};
use crate::ws::db::{
    insert_client_access_token, insert_magic_token, insert_signature_token, select_magic_token,
};
use crate::ws::server::ConnectionIdError;
use crate::CHAT_SERVER_HANDLE;

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({"healthy": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthMagicTokenRequest {
    magic_token: String,
}

#[route("/auth/magic-token", method = "GET")]
pub async fn auth_get_magic_token_endpoint(
    query: web::Query<AuthMagicTokenRequest>,
) -> Result<HttpResponse> {
    let token = &query.magic_token;
    let token_hash = &hash_token(token);

    if let Some(magic_token) = select_magic_token(token_hash) {
        handle_request(
            &magic_token.client_id,
            &Method::Get,
            "auth/magic-token",
            json!({"magicToken": token}),
            None,
        )
        .await
    } else {
        Err(ErrorUnauthorized("Unauthorized"))
    }
}

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
        .ok_or(ErrorBadRequest("Missing clientId"))?;

    insert_magic_token(client_id, token_hash);

    Ok(Json(json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthRegisterClientRequest {
    client_id: String,
}

#[route("/auth/register-client", method = "POST", method = "HEAD")]
pub async fn auth_register_client_endpoint(
    query: web::Query<AuthRegisterClientRequest>,
    _: GeneralHeaderAuthorized,
) -> Result<Json<Value>> {
    let token = &Uuid::new_v4().to_string();
    let token_hash = &hash_token(token);

    insert_client_access_token(&query.client_id, token_hash);

    Ok(Json(json!({"token": token})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    client_id: String,
}

#[route("/auth/signature-token", method = "POST", method = "HEAD")]
pub async fn auth_signature_token_endpoint(
    query: web::Query<AuthRequest>,
    _: ClientHeaderAuthorized,
) -> Result<Json<Value>> {
    let token = &Uuid::new_v4().to_string();
    let token_hash = &hash_token(token);

    insert_signature_token(&query.client_id, token_hash);

    Ok(Json(json!({"token": token})))
}

#[route("/auth/validate-signature-token", method = "GET", method = "HEAD")]
pub async fn auth_validate_signature_token_endpoint(_: SignatureAuthorized) -> Result<Json<Value>> {
    Ok(Json(json!({"valid": true})))
}

#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req).await
}

#[route("/artists/{artist_id}/{size}", method = "GET", method = "HEAD")]
pub async fn artist_cover_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req).await
}

#[route("/albums/{album_id}/{size}", method = "GET", method = "HEAD")]
pub async fn album_cover_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req).await
}

#[route("/{path:.*}", method = "GET", method = "POST", method = "HEAD")]
pub async fn tunnel_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    _: ClientHeaderAuthorized,
) -> Result<HttpResponse> {
    proxy_request(body, req).await
}

#[allow(dead_code)]
enum ResponseType {
    Stream,
    Body,
}

async fn proxy_request(body: Option<Bytes>, req: HttpRequest) -> Result<HttpResponse> {
    let method = Method::from_str(&req.method().to_string().to_uppercase()).unwrap();
    let path = req.path().strip_prefix('/').expect("Failed to get path");
    let query: Vec<_> = QString::from(req.query_string()).into();
    let query: HashMap<_, _> = query.into_iter().collect();
    let client_id = query
        .get("clientId")
        .cloned()
        .ok_or(ErrorBadRequest("Missing clientId query param"))?;
    let query = serde_json::to_value(query).unwrap();

    let body = body
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()?;

    handle_request(&client_id, &method, path, query, body).await
}

async fn handle_request(
    client_id: &str,
    method: &Method,
    path: &str,
    query: Value,
    payload: Option<Value>,
) -> Result<HttpResponse> {
    let request_id = thread_rng().gen::<usize>();

    debug!("Starting ws request for {request_id} {method} {path} {query:?} (id {request_id})");

    let (headers_rx, rx) = request(client_id, request_id, method, path, query, payload).await?;

    let mut builder = HttpResponse::Ok();

    let headers = headers_rx.await.unwrap();
    let response_type = ResponseType::Stream;

    for (key, value) in &headers {
        builder.insert_header((key.clone(), value.clone()));
    }

    let tunnel_stream = TunnelStream::new(request_id, rx, &|request_id| {
        debug!("Request {request_id} ended");
        CHAT_SERVER_HANDLE
            .read()
            .unwrap()
            .as_ref()
            .unwrap()
            .request_end(request_id);
    });

    match response_type {
        ResponseType::Stream => Ok(builder.streaming(tunnel_stream)),
        ResponseType::Body => {
            let body: Vec<_> = tunnel_stream
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .filter_map(|bytes| bytes.ok())
                .flatten()
                .collect();

            Ok(builder.body(body))
        }
    }
}

async fn request(
    client_id: &str,
    request_id: usize,
    method: &Method,
    path: &str,
    query: Value,
    payload: Option<Value>,
) -> Result<(
    oneshot::Receiver<HashMap<String, String>>,
    UnboundedReceiver<TunnelResponse>,
)> {
    let (headers_tx, headers_rx) = oneshot::channel();
    let (tx, rx) = unbounded_channel();

    let client_id = client_id.to_string();
    let method = method.clone();
    let path = path.to_string();

    tokio::spawn(async move {
        debug!("Sending server request {request_id}");
        let chat_server = CHAT_SERVER_HANDLE.read().unwrap().as_ref().unwrap().clone();
        chat_server.request_start(request_id, tx, headers_tx);

        let conn_id = chat_server.get_connection_id(&client_id)?;

        debug!("Sending server request {request_id} to {conn_id}");
        chat_server
            .send_message(
                conn_id,
                &serde_json::to_value(TunnelRequest::HttpRequest(TunnelHttpRequest {
                    request_id,
                    method: method.clone(),
                    path: path.to_string(),
                    query,
                    payload,
                    encoding: TunnelEncoding::Binary,
                }))
                .unwrap()
                .to_string(),
            )
            .await;
        debug!("Sent server request {request_id} to {conn_id}");
        Ok::<_, ConnectionIdError>(())
    });

    Ok((headers_rx, rx))
}
