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

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    client_id: String,
    sender: Option<bool>,
}

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
