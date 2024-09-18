use crate::auth::SignatureAuthorized;
use crate::ws::handler;
use crate::WS_SERVER_HANDLE;
use actix_web::error::ErrorBadGateway;
use actix_web::HttpResponse;
use actix_web::{
    get,
    web::{self},
    Result,
};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    client_id: String,
    sender: Option<bool>,
}

#[get("/ws")]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    query: web::Query<ConnectRequest>,
    _: SignatureAuthorized,
) -> Result<HttpResponse, actix_web::Error> {
    let profile = req
        .headers()
        .get("moosicbox-profile")
        .and_then(|x| x.to_str().ok())
        .ok_or_else(|| ErrorBadGateway("Missing profile"))?
        .to_string();

    let ws_server = WS_SERVER_HANDLE.read().await.as_ref().unwrap().clone();
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    moosicbox_task::spawn_local(
        "tunnel_server_websocket",
        handler::handle_ws(
            ws_server,
            session,
            msg_stream,
            query.client_id.clone(),
            query.sender.unwrap_or(false),
            profile,
        ),
    );

    Ok(res)
}
