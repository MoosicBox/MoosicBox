use crate::ws::handler;
use crate::ws::server::ChatServerHandle;
use actix_web::HttpResponse;
use actix_web::{
    get,
    web::{self},
    Result,
};
use tokio::task::spawn_local;

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
