use actix_web::{HttpRequest, HttpResponse, Responder, error::ErrorInternalServerError, web};
use hyperchad_renderer::transformer::actions::logic::Value;
use serde::{Deserialize, Serialize};

use crate::{ActixApp, ActixResponseProcessor};

#[derive(Debug, Deserialize, Serialize)]
pub struct ActionPayload {
    action: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
}

#[allow(clippy::future_not_send)]
pub async fn handle_action<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    _req: HttpRequest,
    app: web::Data<ActixApp<T, R>>,
    action: web::Json<ActionPayload>,
) -> impl Responder {
    log::debug!("handle_action: action={action:?}");
    if let Some(tx) = &app.action_tx {
        tx.send((
            serde_json::to_string(&action.0.action).unwrap(),
            action.0.value,
        ))
        .map_err(ErrorInternalServerError)?;
    }

    Ok::<_, actix_web::Error>(HttpResponse::NoContent())
}
