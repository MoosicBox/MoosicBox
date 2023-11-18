use actix_web::error::ErrorInternalServerError;
use actix_web::http::Method;
use actix_web::{route, web, HttpResponse};
use actix_web::{HttpRequest, Result};
use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::{debug, info, warn};
use moosicbox_tunnel::tunnel::{TunnelEncoding, TunnelStream};
use moosicbox_tunnel::ws::sender::TunnelResponse;
use once_cell::sync::Lazy;
use qstring::QString;
use rand::{thread_rng, Rng as _};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::ws::db::select_connection;
use crate::CHAT_SERVER_HANDLE;

pub static TUNNEL_SENDERS: Lazy<Mutex<HashMap<usize, Sender<TunnelResponse>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<HttpResponse> {
    info!("Healthy");
    Ok(HttpResponse::Ok().body(""))
}

#[route("/{path:.*}", method = "GET", method = "POST", method = "HEAD")]
pub async fn track_endpoint(
    body: Option<Bytes>,
    path: web::Path<(String,)>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let id = thread_rng().gen::<usize>();

    let method = req.method();
    let path = path.into_inner().0;
    let query: Vec<_> = QString::from(req.query_string()).into();
    let query: HashMap<String, String> = query.into_iter().collect();
    let query = serde_json::to_value(query).unwrap();

    info!("Received {method} call to {path} with {query} (id {id})");

    let body = body
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()?;

    debug!("Starting ws request for {id} {method} {path} {query:?}");

    let rx = request(id, method, &path, query, body).await?;

    Ok(HttpResponse::Ok().streaming(TunnelStream::new(id, rx)))
}

async fn request(
    id: usize,
    method: &Method,
    path: &str,
    query: Value,
    payload: Option<Value>,
) -> Result<Receiver<TunnelResponse>> {
    let (tx, rx) = bounded(1);

    debug!("Setting sender for request {id}");
    match TUNNEL_SENDERS.lock() {
        Ok(mut lock) => {
            lock.insert(id, tx);
        }
        Err(poison) => {
            warn!("Accessing from poison");
            poison.into_inner().insert(id, tx);
        }
    }

    debug!("Sending server request {id}");
    let chat_server = CHAT_SERVER_HANDLE.lock().unwrap().as_ref().unwrap().clone();
    let conn_id = select_connection("123123")
        .ok_or(ErrorInternalServerError(
            "Could not get moosicbox server connection",
        ))?
        .tunnel_ws_id
        .parse::<usize>()
        .map_err(|_| ErrorInternalServerError("Failed to parse connection id"))?;

    debug!("Sending server request {id} to {conn_id}");
    chat_server
        .send_message(
            conn_id,
            serde_json::json!({
                "type": "TUNNEL_REQUEST",
                "id": id,
                "method": method.to_string(),
                "path": path,
                "query": query,
                "payload": payload,
                "encoding": TunnelEncoding::Binary
            })
            .to_string(),
        )
        .await;
    debug!("Sent server request {id} to {conn_id}");

    Ok(rx)
}
