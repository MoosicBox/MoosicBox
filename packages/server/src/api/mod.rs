use crate::{WS_SERVER_HANDLE, ws::handler};
use actix_web::{HttpResponse, route};
use actix_web::{
    Result, get,
    web::{self, Json},
};
use log::info;
use serde_json::{Value, json};
use switchy_database::profiles::api::ProfileName;

#[cfg(feature = "openapi")]
pub mod openapi;

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({
        "healthy": true,
        "hash": std::env!("GIT_HASH"),
    })))
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
