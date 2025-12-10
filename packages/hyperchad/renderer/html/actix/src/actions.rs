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

/// Extracts the action name from a JSON value.
///
/// If the value is a string, returns it directly. Otherwise, serializes the
/// JSON value to a string representation.
///
/// # Panics
///
/// * Panics if JSON serialization fails (should not happen for valid `serde_json::Value`)
#[must_use]
pub fn extract_action_name(action: &serde_json::Value) -> String {
    action.as_str().map_or_else(
        || serde_json::to_string(action).unwrap(),
        std::string::ToString::to_string,
    )
}

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
        let action_name = extract_action_name(&action.0.action);
        tx.send((action_name, action.0.value))
            .map_err(ErrorInternalServerError)?;
    }

    Ok::<_, actix_web::Error>(HttpResponse::NoContent())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test_log::test]
    fn test_extract_action_name_from_string_value() {
        let action = json!("simple-action");
        assert_eq!(extract_action_name(&action), "simple-action");
    }

    #[test_log::test]
    fn test_extract_action_name_from_empty_string() {
        let action = json!("");
        assert_eq!(extract_action_name(&action), "");
    }

    #[test_log::test]
    fn test_extract_action_name_from_string_with_special_chars() {
        let action = json!("action:with:colons");
        assert_eq!(extract_action_name(&action), "action:with:colons");
    }

    #[test_log::test]
    fn test_extract_action_name_from_number() {
        let action = json!(42);
        assert_eq!(extract_action_name(&action), "42");
    }

    #[test_log::test]
    fn test_extract_action_name_from_float() {
        let action = json!(1.5);
        assert_eq!(extract_action_name(&action), "1.5");
    }

    #[test_log::test]
    fn test_extract_action_name_from_boolean_true() {
        let action = json!(true);
        assert_eq!(extract_action_name(&action), "true");
    }

    #[test_log::test]
    fn test_extract_action_name_from_boolean_false() {
        let action = json!(false);
        assert_eq!(extract_action_name(&action), "false");
    }

    #[test_log::test]
    fn test_extract_action_name_from_null() {
        let action = json!(null);
        assert_eq!(extract_action_name(&action), "null");
    }

    #[test_log::test]
    fn test_extract_action_name_from_array() {
        let action = json!(["action1", "action2"]);
        assert_eq!(extract_action_name(&action), r#"["action1","action2"]"#);
    }

    #[test_log::test]
    fn test_extract_action_name_from_object() {
        let action = json!({"type": "click", "target": "button"});
        // Note: JSON object key order may vary, so we parse and compare
        let result = extract_action_name(&action);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, action);
    }

    #[test_log::test]
    fn test_extract_action_name_from_nested_object() {
        let action = json!({
            "action": {
                "type": "navigate",
                "params": {
                    "url": "/home"
                }
            }
        });
        let result = extract_action_name(&action);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, action);
    }

    #[test_log::test]
    fn test_extract_action_name_from_string_with_unicode() {
        let action = json!("action-世界-🚀");
        assert_eq!(extract_action_name(&action), "action-世界-🚀");
    }

    #[test_log::test]
    fn test_extract_action_name_from_string_with_whitespace() {
        let action = json!("action with spaces");
        assert_eq!(extract_action_name(&action), "action with spaces");
    }

    #[test_log::test]
    fn test_extract_action_name_from_string_with_newlines() {
        let action = json!("action\nwith\nnewlines");
        assert_eq!(extract_action_name(&action), "action\nwith\nnewlines");
    }

    #[test_log::test]
    fn test_extract_action_name_preserves_json_escapes_in_object() {
        let action = json!({"key": "value with \"quotes\""});
        let result = extract_action_name(&action);
        // The result should be valid JSON that can be parsed back
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, action);
    }

    #[test_log::test]
    fn test_extract_action_name_from_empty_array() {
        let action = json!([]);
        assert_eq!(extract_action_name(&action), "[]");
    }

    #[test_log::test]
    fn test_extract_action_name_from_empty_object() {
        let action = json!({});
        assert_eq!(extract_action_name(&action), "{}");
    }
}
