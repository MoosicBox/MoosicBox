use crate::scan::scan;
use crate::ws::handler;
use crate::ws::server::ChatServerHandle;
use crate::CANCELLATION_TOKEN;
use actix_web::error::ErrorInternalServerError;
use actix_web::{
    get, post,
    web::{self, Json},
    Result,
};
use actix_web::{route, HttpResponse};
use log::info;
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::task::spawn_local;

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({"healthy": true})))
}

#[get("/ws")]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    chat_server: web::Data<ChatServerHandle>,
) -> Result<HttpResponse, actix_web::Error> {
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    spawn_local(handler::chat_ws(
        (**chat_server).clone(),
        session,
        msg_stream,
    ));

    Ok(res)
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanQuery {
    location: String,
}

#[post("/scan")]
pub async fn scan_endpoint(
    query: web::Query<ScanQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    scan(&query.location, &data, CANCELLATION_TOKEN.clone())
        .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}
