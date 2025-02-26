use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde_json::Value;

use crate::{ActixApp, ActixResponseProcessor};

#[allow(clippy::future_not_send)]
pub async fn handle_action<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    _req: HttpRequest,
    _app: web::Data<ActixApp<T, R>>,
    action: web::Json<Value>,
) -> impl Responder {
    log::debug!("handle_action: action={action:?}");

    Ok::<_, actix_web::Error>(HttpResponse::Ok())
}
