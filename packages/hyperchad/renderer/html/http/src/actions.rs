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
    ///
    /// Can be either a simple string action name or a more complex JSON value
    /// representing the action.
    pub action: serde_json::Value,
    /// Optional value parameter for the action.
    ///
    /// Additional data to pass along with the action request. If present,
    /// will be sent through the action channel for processing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use super::*;
    use bytes::Bytes;
    use hyperchad_renderer::transformer::actions::logic::Value;
    use hyperchad_router::RequestInfo;
    use switchy::http::models::Method;

    fn create_request_with_body(body: &[u8]) -> RouteRequest {
        RouteRequest {
            path: "/$action".to_string(),
            method: Method::Post,
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: RequestInfo::default(),
            body: Some(Arc::new(Bytes::copy_from_slice(body))),
        }
    }

    fn create_request_without_body() -> RouteRequest {
        RouteRequest {
            path: "/$action".to_string(),
            method: Method::Post,
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: RequestInfo::default(),
            body: None,
        }
    }

    #[test_log::test]
    fn test_handle_action_with_string_action() {
        let (tx, rx) = flume::unbounded();
        let body = br#"{"action":"click"}"#;
        let req = create_request_with_body(body);

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 204);

        let (action_name, value) = rx.try_recv().unwrap();
        assert_eq!(action_name, "click");
        assert!(value.is_none());
    }

    #[test_log::test]
    fn test_handle_action_with_string_action_and_value() {
        let (tx, rx) = flume::unbounded();
        let body = br#"{"action":"setValue","value":42}"#;
        let req = create_request_with_body(body);

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 204);

        let (action_name, value) = rx.try_recv().unwrap();
        assert_eq!(action_name, "setValue");
        assert!(value.is_some());
    }

    #[test_log::test]
    fn test_handle_action_with_complex_json_action() {
        let (tx, rx) = flume::unbounded();
        let body = br#"{"action":{"type":"navigate","path":"/home"}}"#;
        let req = create_request_with_body(body);

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 204);

        let (action_name, value) = rx.try_recv().unwrap();
        // Complex JSON action is serialized to JSON string
        assert!(action_name.contains("navigate"));
        assert!(action_name.contains("/home"));
        assert!(value.is_none());
    }

    #[test_log::test]
    fn test_handle_action_with_missing_body_returns_400() {
        let (tx, _rx) = flume::unbounded();
        let req = create_request_without_body();

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 400);
    }

    #[test_log::test]
    fn test_handle_action_with_invalid_json_returns_400() {
        let (tx, _rx) = flume::unbounded();
        let body = b"not valid json";
        let req = create_request_with_body(body);

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 400);
    }

    #[test_log::test]
    fn test_handle_action_with_missing_action_field_returns_400() {
        let (tx, _rx) = flume::unbounded();
        let body = br#"{"value":42}"#;
        let req = create_request_with_body(body);

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 400);
    }

    #[test_log::test]
    fn test_handle_action_with_string_value() {
        let (tx, rx) = flume::unbounded();
        // Value::String is serialized as {"String": "value"}
        let body = br#"{"action":"update","value":{"String":"hello"}}"#;
        let req = create_request_with_body(body);

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 204);

        let (action_name, value) = rx.try_recv().unwrap();
        assert_eq!(action_name, "update");
        match value {
            Some(Value::String(s)) => assert_eq!(s, "hello"),
            _ => panic!("Expected String value"),
        }
    }

    #[test_log::test]
    fn test_handle_action_with_numeric_value() {
        let (tx, rx) = flume::unbounded();
        // Numeric values are deserialized as Value::Real
        let body = br#"{"action":"setVolume","value":75}"#;
        let req = create_request_with_body(body);

        let response = handle_action(&tx, &req).unwrap();
        assert_eq!(response.status(), 204);

        let (action_name, value) = rx.try_recv().unwrap();
        assert_eq!(action_name, "setVolume");
        match value {
            Some(Value::Real(v)) => assert!((v - 75.0).abs() < f32::EPSILON),
            _ => panic!("Expected Real value, got {value:?}"),
        }
    }

    #[test_log::test]
    fn test_action_payload_deserialize_string_action() {
        let json = r#"{"action":"click"}"#;
        let payload: ActionPayload = serde_json::from_str(json).unwrap();

        assert_eq!(payload.action.as_str(), Some("click"));
        assert!(payload.value.is_none());
    }

    #[test_log::test]
    fn test_action_payload_deserialize_with_value() {
        let json = r#"{"action":"setValue","value":42}"#;
        let payload: ActionPayload = serde_json::from_str(json).unwrap();

        assert_eq!(payload.action.as_str(), Some("setValue"));
        assert!(payload.value.is_some());
    }

    #[test_log::test]
    fn test_action_payload_deserialize_complex_action() {
        let json = r#"{"action":{"type":"navigate","path":"/home"}}"#;
        let payload: ActionPayload = serde_json::from_str(json).unwrap();

        assert!(payload.action.is_object());
        assert_eq!(payload.action["type"].as_str(), Some("navigate"));
        assert_eq!(payload.action["path"].as_str(), Some("/home"));
    }

    #[test_log::test]
    fn test_action_payload_serialize_string_action() {
        let payload = ActionPayload {
            action: serde_json::json!("submit"),
            value: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains(r#""action":"submit""#));
        assert!(!json.contains("value"));
    }

    #[test_log::test]
    fn test_action_payload_serialize_with_value() {
        let payload = ActionPayload {
            action: serde_json::json!("update"),
            value: Some(Value::Real(123.0)),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains(r#""action":"update""#));
        assert!(json.contains(r#""value""#));
    }

    #[test_log::test]
    fn test_action_payload_roundtrip() {
        let original = ActionPayload {
            action: serde_json::json!({"command": "delete", "id": 456}),
            value: Some(Value::String("test".to_string())),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ActionPayload = serde_json::from_str(&json).unwrap();

        assert_eq!(original.action, deserialized.action);
        assert_eq!(
            original
                .value
                .as_ref()
                .and_then(|v| if let Value::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }),
            deserialized
                .value
                .as_ref()
                .and_then(|v| if let Value::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                })
        );
    }

    #[test_log::test]
    fn test_action_payload_missing_action_field() {
        let json = r#"{"value":42}"#;
        let result = serde_json::from_str::<ActionPayload>(json);
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_action_payload_debug_format() {
        let payload = ActionPayload {
            action: serde_json::json!("test"),
            value: Some(Value::String("test_value".to_string())),
        };

        let debug_str = format!("{payload:?}");
        assert!(debug_str.contains("ActionPayload"));
        assert!(debug_str.contains("action"));
    }
}
