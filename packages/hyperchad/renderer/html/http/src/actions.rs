use http::Response;
use hyperchad_renderer::transformer::actions::logic::Value;
use hyperchad_router::RouteRequest;
use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Deserialize, Serialize)]
pub struct ActionPayload {
    action: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
}

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
    let action_name = action
        .action
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| serde_json::to_string(&action.action).unwrap());
    tx.send((action_name, action.value)).unwrap();

    Ok(Response::builder().status(204).body(vec![])?)
}
