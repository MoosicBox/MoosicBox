//! Action handling for interactive `HyperChad` user events.
//!
//! This module provides HTTP endpoints for processing user-triggered actions from the frontend.
//! Actions are sent via POST requests to the `/$action` endpoint and forwarded to the application
//! through a channel for processing.
//!
//! This module is only available when the `actions` feature is enabled.

use actix_web::{HttpRequest, HttpResponse, Responder, error::ErrorInternalServerError, web};
use hyperchad_renderer::transformer::actions::logic::Value;
use serde::{Deserialize, Serialize};

use crate::{ActixApp, ActixResponseProcessor};

/// Payload for action requests sent from the frontend.
///
/// This structure represents the data sent in POST requests to the `/$action` endpoint.
/// It contains the action identifier and an optional value associated with the action.
#[derive(Debug, Deserialize, Serialize)]
pub struct ActionPayload {
    /// The action identifier, can be a string or complex JSON value.
    action: serde_json::Value,
    /// Optional value data associated with the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
}

/// Handles POST requests to the `/$action` endpoint for user-triggered actions.
///
/// This function receives action payloads from the frontend, extracts the action name
/// and optional value, and forwards them through the action channel for processing
/// by the application.
///
/// # Errors
///
/// * Returns an error if the action channel fails to send the action
///
/// # Panics
///
/// * Panics if JSON serialization of the action value fails
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
        let action_name = action.0.action.as_str().map_or_else(
            || serde_json::to_string(&action.0.action).unwrap(),
            std::string::ToString::to_string,
        );
        tx.send((action_name, action.0.value))
            .map_err(ErrorInternalServerError)?;
    }

    Ok::<_, actix_web::Error>(HttpResponse::NoContent())
}
