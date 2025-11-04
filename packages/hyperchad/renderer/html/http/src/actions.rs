//! Action request handling for `HyperChad` HTTP applications.
//!
//! This module provides functionality to handle action requests sent from
//! client-side JavaScript. Actions are parsed from request bodies and sent
//! through a channel for processing by the application.

use http::Response;
use hyperchad_renderer::transformer::actions::logic::Value;
use hyperchad_router::RouteRequest;
use serde::{Deserialize, Serialize};

use crate::Error;

/// Payload for action requests from client-side JavaScript.
///
/// This structure represents the parsed request body for action endpoints.
#[derive(Debug, Deserialize, Serialize)]
pub struct ActionPayload {
    /// The action identifier (string or JSON value).
    action: serde_json::Value,
    /// Optional value parameter for the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
}

/// Handles an action request from a client by parsing the payload and sending it to the action channel.
///
/// Parses the request body as an [`ActionPayload`], extracts the action name and value,
/// and sends them through the provided channel for processing by the application.
///
/// # Errors
///
/// * `Error::Http` - If HTTP response construction fails
///
/// # Panics
///
/// * If the channel sender fails to send the action (channel receiver has been dropped)
/// * If JSON serialization of the action name fails (for non-string action values)
#[allow(clippy::future_not_send)]
pub fn handle_action(
    tx: &flume::Sender<(String, Option<Value>)>,
    req: &RouteRequest,
) -> Result<Response<Vec<u8>>, Error> {
    let action = match req.parse_body::<ActionPayload>() {
        Ok(action) => action,
        Err(e) => {
            log::error!("Failed to parse body: {e:?}");
            return Ok(Response::builder().status(400).body(vec![])?);
        }
    };

    log::debug!("handle_action: action={action:?}");
    let action_name = action.action.as_str().map_or_else(
        || serde_json::to_string(&action.action).unwrap(),
        std::string::ToString::to_string,
    );
    tx.send((action_name, action.value)).unwrap();

    Ok(Response::builder().status(204).body(vec![])?)
}
