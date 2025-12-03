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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use actix_web::{
        App, HttpResponse,
        http::StatusCode,
        test,
        web::{self, Data},
    };
    use async_trait::async_trait;
    use bytes::Bytes;
    use flume::Receiver;
    use hyperchad_renderer::{Content, RendererEvent};

    #[derive(Clone)]
    struct TestProcessor;

    #[async_trait]
    impl ActixResponseProcessor<()> for TestProcessor {
        fn prepare_request(
            &self,
            _req: actix_web::HttpRequest,
            _body: Option<Arc<Bytes>>,
        ) -> Result<(), actix_web::Error> {
            Ok(())
        }

        async fn to_response(&self, _data: ()) -> Result<HttpResponse, actix_web::Error> {
            Ok(HttpResponse::Ok().finish())
        }

        async fn to_body(
            &self,
            _content: Content,
            _data: (),
        ) -> Result<(Bytes, String), actix_web::Error> {
            Ok((Bytes::new(), "text/html".to_string()))
        }
    }

    fn create_test_app(
        renderer_rx: Receiver<RendererEvent>,
        action_tx: Option<flume::Sender<(String, Option<Value>)>>,
    ) -> ActixApp<(), TestProcessor> {
        let mut app = ActixApp::new(TestProcessor, renderer_rx);
        if let Some(tx) = action_tx {
            app = app.with_action_tx(tx);
        }
        app
    }

    #[actix_web::test]
    async fn test_handle_action_with_string_action() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": "click"}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        assert_eq!(action_name, "click");
        assert!(value.is_none());
    }

    #[actix_web::test]
    async fn test_handle_action_with_complex_json_action() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": {"type": "navigate", "path": "/home"}}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        // Complex JSON action is serialized to JSON string
        assert!(action_name.contains("navigate"));
        assert!(action_name.contains("/home"));
        assert!(value.is_none());
    }

    #[actix_web::test]
    async fn test_handle_action_with_value() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": "setVolume", "value": 75}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        assert_eq!(action_name, "setVolume");
        match value {
            Some(Value::Real(v)) => assert!((v - 75.0).abs() < f32::EPSILON),
            _ => panic!("Expected Real value, got {value:?}"),
        }
    }

    #[actix_web::test]
    async fn test_handle_action_without_action_tx_returns_no_content() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        // No action_tx configured
        let actix_app = create_test_app(renderer_rx, None);

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": "click"}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        // Should still return NoContent even without action_tx
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[actix_web::test]
    async fn test_handle_action_with_string_value() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": "update", "value": {"String": "hello"}}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        assert_eq!(action_name, "update");
        match value {
            Some(Value::String(s)) => assert_eq!(s, "hello"),
            _ => panic!("Expected String value"),
        }
    }

    #[actix_web::test]
    async fn test_handle_action_with_numeric_action_identifier() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        // Action as a number (non-string JSON value)
        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": 42}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        // Numeric action is serialized to JSON string
        assert_eq!(action_name, "42");
        assert!(value.is_none());
    }

    #[actix_web::test]
    async fn test_handle_action_with_array_action_identifier() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        // Action as an array (non-string JSON value)
        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": ["a", "b", "c"]}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        // Array action is serialized to JSON string
        assert_eq!(action_name, r#"["a","b","c"]"#);
        assert!(value.is_none());
    }

    #[actix_web::test]
    async fn test_handle_action_with_boolean_action_identifier() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        // Action as a boolean (non-string JSON value)
        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": true}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        // Boolean action is serialized to JSON string
        assert_eq!(action_name, "true");
        assert!(value.is_none());
    }

    #[actix_web::test]
    async fn test_handle_action_with_null_action_identifier() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        // Action as null (non-string JSON value)
        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": null}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        // Null action is serialized to JSON string
        assert_eq!(action_name, "null");
        assert!(value.is_none());
    }

    #[actix_web::test]
    async fn test_handle_action_with_empty_string_action() {
        let (_renderer_tx, renderer_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let actix_app = create_test_app(renderer_rx, Some(action_tx));

        let app = test::init_service(App::new().app_data(Data::new(actix_app)).service(
            web::resource("/$action").route(web::post().to(handle_action::<(), TestProcessor>)),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/$action")
            .set_json(serde_json::json!({"action": ""}))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let (action_name, value) = action_rx.try_recv().unwrap();
        assert_eq!(action_name, "");
        assert!(value.is_none());
    }
}
