use crate::{ws::handler, WS_SERVER_HANDLE};
use actix_web::{
    get,
    web::{self, Json},
    Result,
};
use actix_web::{route, HttpResponse};
use log::info;
use moosicbox_database::profiles::api::ProfileName;
use serde_json::{json, Value};

#[cfg(feature = "openapi")]
pub mod openapi;

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({"healthy": true})))
}

#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::future_not_send)]
#[get("/ws")]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    profile_name: ProfileName,
) -> Result<HttpResponse, actix_web::Error> {
    let profile = profile_name.into();
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    moosicbox_task::spawn_local(
        "server: WsClient",
        handler::handle_ws(
            WS_SERVER_HANDLE
                .read()
                .await
                .as_ref()
                .expect("No WsServerHandle available")
                .clone(),
            profile,
            session,
            msg_stream,
        ),
    );

    Ok(response)
}
