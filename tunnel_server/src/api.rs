use actix_web::error::ErrorInternalServerError;
use actix_web::http::header::{CacheControl, CacheDirective};
use actix_web::http::Method;
use actix_web::web::Json;
use actix_web::{route, web, HttpResponse};
use actix_web::{HttpRequest, Result};
use bytes::Bytes;
use log::{debug, info};
use moosicbox_tunnel::tunnel::{TunnelEncoding, TunnelResponse, TunnelStream};
use qstring::QString;
use rand::{thread_rng, Rng as _};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::mpsc::{channel, Receiver};

use crate::ws::db::select_connection;
use crate::CHAT_SERVER_HANDLE;

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({"healthy": true})))
}

#[route("/{path:.*}", method = "GET", method = "POST", method = "HEAD")]
pub async fn track_endpoint(
    body: Option<Bytes>,
    path: web::Path<(String,)>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let request_id = thread_rng().gen::<usize>();

    let method = req.method();
    let path = path.into_inner().0;
    let query: Vec<_> = QString::from(req.query_string()).into();
    let query: HashMap<_, _> = query.into_iter().collect();
    let query = serde_json::to_value(query).unwrap();

    info!("Received {method} call to {path} with {query} (id {request_id})");

    let body = body
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()?;

    debug!("Starting ws request for {request_id} {method} {path} {query:?}");

    let rx = request(request_id, method, &path, query, body).await?;

    let mut builder = HttpResponse::Ok();

    builder.insert_header(CacheControl(vec![CacheDirective::MaxAge(86400u32)]));

    Ok(
        HttpResponse::Ok().streaming(TunnelStream::new(request_id, rx, &|request_id| {
            info!("Request {request_id} ended");
            CHAT_SERVER_HANDLE
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .request_end(request_id);
        })),
    )
}

async fn request(
    request_id: usize,
    method: &Method,
    path: &str,
    query: Value,
    payload: Option<Value>,
) -> Result<Receiver<TunnelResponse>> {
    let (tx, rx) = channel(1024);

    debug!("Sending server request {request_id}");
    let chat_server = CHAT_SERVER_HANDLE.lock().unwrap().as_ref().unwrap().clone();
    chat_server.request_start(request_id, tx);

    let conn_id = select_connection("123123")
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

    Ok(rx)
}
