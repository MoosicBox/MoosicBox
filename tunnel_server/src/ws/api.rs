use crate::ws::handler;
use crate::CHAT_SERVER_HANDLE;
use actix_web::HttpResponse;
use actix_web::{
    get,
    web::{self},
    Result,
};
use serde::Deserialize;
use tokio::task::spawn_local;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    client_id: String,
}

#[get("/ws")]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    query: web::Query<ConnectRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let chat_server = CHAT_SERVER_HANDLE.lock().unwrap().as_ref().unwrap().clone();
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    spawn_local(handler::chat_ws(
        chat_server,
        session,
        msg_stream,
        query.client_id.clone(),
    ));

    Ok(res)
}
