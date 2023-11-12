use actix_web::{route, HttpResponse};
use actix_web::{
    web::{self},
    HttpRequest, Result,
};
use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::debug;
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Mutex;

pub static TUNNEL_SENDERS: Lazy<Mutex<HashMap<usize, Sender<Bytes>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    track_id: i32,
}

#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_server_endpoint(
    _req: HttpRequest,
    query: web::Query<GetTrackQuery>,
) -> Result<HttpResponse> {
    let id = thread_rng().gen::<usize>();

    debug!("Starting ws request for {id}");
    let start = std::time::SystemTime::now();
    let rx = request(
        id,
        "track",
        serde_json::to_value(query.deref().clone()).unwrap(),
    )
    .await?;

    let mut time_to_first_byte = None;
    let mut packet_count = 0;
    let mut byte_count = 0;
    loop {
        let bytes = rx.recv().unwrap();
        if time_to_first_byte.is_none() {
            time_to_first_byte = Some(std::time::SystemTime::now());
        }
        packet_count += 1;
        debug!("Received packet for {id} {packet_count}");

        if bytes.is_empty() {
            break;
        }

        byte_count += bytes.len();
    }
    let end = std::time::SystemTime::now();

    debug!(
        "Byte count: {byte_count} (received {} packets, took {}ms total, {}ms to first byte)",
        packet_count,
        end.duration_since(start).unwrap().as_millis(),
        time_to_first_byte
            .map(|t| t.duration_since(start).unwrap().as_millis())
            .map(|t| t.to_string())
            .unwrap_or("N/A".into())
    );

    Ok(HttpResponse::Ok().body(""))
}

async fn request(id: usize, path: &str, payload: Value) -> Result<Receiver<Bytes>> {
    let (tx, rx) = bounded(1);

    TUNNEL_SENDERS.lock().unwrap().insert(id, tx);

    #[cfg(feature = "server")]
    request_server(id, path, payload).await?;
    #[cfg(all(not(feature = "server"), feature = "serverless"))]
    request_serverless(id, path, payload).await?;

    Ok(rx)
}

#[cfg(feature = "server")]
async fn request_server(id: usize, path: &str, payload: Value) -> Result<()> {
    use crate::{CHAT_SERVER_HANDLE, CONN_ID};

    let chat_server = CHAT_SERVER_HANDLE.lock().unwrap().as_ref().unwrap().clone();
    let conn_id = CONN_ID.lock().unwrap().unwrap();

    chat_server
        .send_message(
            conn_id,
            serde_json::json!({
                "type": "TUNNEL_REQUEST",
                "id": id,
                "path": path,
                "payload": payload
            })
            .to_string(),
        )
        .await;

    Ok(())
}

#[cfg(feature = "serverless")]
#[allow(dead_code)]
async fn request_serverless(id: usize, path: &str, payload: Value) -> Result<()> {
    use actix_web::error::ErrorInternalServerError;
    use moosicbox_tunnel::ws::sender::send_message;

    send_message(
        serde_json::json!({
            "type": "TUNNEL_REQUEST",
            "id": id,
            "path": path,
            "payload": payload
        })
        .to_string(),
    )
    .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(())
}
