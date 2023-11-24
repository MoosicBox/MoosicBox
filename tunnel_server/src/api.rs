use actix_web::error::ErrorInternalServerError;
use actix_web::http::Method;
use actix_web::web::{self, Json};
use actix_web::{route, HttpResponse};
use actix_web::{HttpRequest, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use log::{debug, info};
use moosicbox_tunnel::tunnel::{TunnelEncoding, TunnelResponse, TunnelStream};
use qstring::QString;
use rand::{thread_rng, Rng as _};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::mpsc::{channel, Receiver};
use uuid::Uuid;

use crate::auth::{hash_token, HeaderAuthorized, SignatureAuthorized};
use crate::ws::db::{insert_signature_token, select_connection};
use crate::CHAT_SERVER_HANDLE;

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({"healthy": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    client_id: String,
}

#[route("/auth/signature-token", method = "POST", method = "HEAD")]
pub async fn auth_signature_token_endpoint(
    query: web::Query<AuthRequest>,
    _: HeaderAuthorized,
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
    handle_request(body, req).await
}

#[route("/artists/{artist_id}/{size}", method = "GET", method = "HEAD")]
pub async fn artist_cover_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    handle_request(body, req).await
}

#[route("/albums/{album_id}/{size}", method = "GET", method = "HEAD")]
pub async fn album_cover_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    _: SignatureAuthorized,
) -> Result<HttpResponse> {
    handle_request(body, req).await
}

#[route("/{path:.*}", method = "GET", method = "POST", method = "HEAD")]
pub async fn tunnel_endpoint(
    body: Option<Bytes>,
    req: HttpRequest,
    _: HeaderAuthorized,
) -> Result<HttpResponse> {
    handle_request(body, req).await
}

#[allow(dead_code)]
enum ResponseType {
    Stream,
    Body,
}

async fn handle_request(body: Option<Bytes>, req: HttpRequest) -> Result<HttpResponse> {
    let request_id = thread_rng().gen::<usize>();

    let method = req.method();
    let path = req.path().strip_prefix('/').expect("Failed to get path");
    let query: Vec<_> = QString::from(req.query_string()).into();
    let query: HashMap<_, _> = query.into_iter().collect();
    let client_id = query
        .get("clientId")
        .cloned()
        .unwrap_or("123123".to_string());
    let query = serde_json::to_value(query).unwrap();

    info!("Received {method} call to {path} with {query} (id {request_id})");

    let body = body
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()?;

    debug!("Starting ws request for {request_id} {method} {path} {query:?}");

    let (mut headers_rx, rx) = request(&client_id, request_id, method, path, query, body).await?;

    let mut builder = HttpResponse::Ok();

    let headers = headers_rx.recv().await.unwrap();
    let response_type = ResponseType::Stream;

    for (key, value) in &headers {
        builder.insert_header((key.clone(), value.clone()));
    }

    let tunnel_stream = TunnelStream::new(request_id, rx, &|request_id| {
        info!("Request {request_id} ended");
        CHAT_SERVER_HANDLE
            .lock()
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
) -> Result<(Receiver<HashMap<String, String>>, Receiver<TunnelResponse>)> {
    let (headers_tx, headers_rx) = channel(64);
    let (tx, rx) = channel(1024);

    debug!("Sending server request {request_id}");
    let chat_server = CHAT_SERVER_HANDLE.lock().unwrap().as_ref().unwrap().clone();
    chat_server.request_start(request_id, tx, headers_tx);

    let conn_id = select_connection(client_id)
        .ok_or(ErrorInternalServerError(
            "Could not get moosicbox server connection",
        ))?
        .tunnel_ws_id
        .parse::<usize>()
        .map_err(|_| ErrorInternalServerError("Failed to parse connection id"))?;

    debug!("Sending server request {request_id} to {conn_id}");
    chat_server
        .send_message(
            conn_id,
            serde_json::json!({
                "type": "TUNNEL_REQUEST",
                "request_id": request_id,
                "method": method.to_string(),
                "path": path,
                "query": query,
                "payload": payload,
                "encoding": TunnelEncoding::Binary
            })
            .to_string(),
        )
        .await;
    debug!("Sent server request {request_id} to {conn_id}");

    Ok((headers_rx, rx))
}
