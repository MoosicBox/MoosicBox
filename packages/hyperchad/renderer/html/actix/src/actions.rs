//! Action handling for interactive `HyperChad` user events.
//!
//! This module provides HTTP endpoints for processing user-triggered actions from the frontend.
//! Actions are sent via POST requests to the `/$action` endpoint and forwarded to the application
//! through a channel for processing.
//!
//! This module is only available when the `actions` feature is enabled.

#[cfg(feature = "shared-state-bridge")]
use std::{collections::BTreeMap, str::FromStr as _, sync::Arc};

use actix_web::{HttpRequest, HttpResponse, Responder, error::ErrorInternalServerError, web};
use hyperchad_renderer::transformer::actions::logic::Value;
#[cfg(feature = "shared-state-bridge")]
use hyperchad_router::{ClientInfo, ClientOs, RequestInfo, RouteRequest};
#[cfg(feature = "shared-state-bridge")]
use hyperchad_shared_state_bridge::{
    RouteCommandInput, SharedStateRouteResolver, command_from_route, resolve_route_context,
};
#[cfg(feature = "shared-state-bridge")]
use hyperchad_shared_state_models::CommandEnvelope;
use serde::{Deserialize, Serialize};
#[cfg(feature = "shared-state-bridge")]
use switchy_http_models::Method;

use crate::{ActixApp, ActixResponseProcessor};

#[cfg(feature = "shared-state-bridge")]
/// Type alias for mapping action/value pairs into shared-state command inputs.
pub type SharedStateCommandInputResolver =
    dyn Fn(&str, Option<&Value>) -> Option<RouteCommandInput> + Send + Sync;

/// Shared-state action bridge wiring for Actix action routes.
#[cfg(feature = "shared-state-bridge")]
#[derive(Clone)]
pub struct SharedStateActionBridge {
    /// Command sender used to dispatch shared-state commands.
    pub command_tx: flume::Sender<CommandEnvelope>,
    /// Resolver used to map route requests to shared-state context.
    pub route_resolver: Arc<dyn SharedStateRouteResolver>,
    /// Resolver used to map actions into shared-state command input.
    pub command_input_resolver: Arc<SharedStateCommandInputResolver>,
}

#[cfg(feature = "shared-state-bridge")]
impl SharedStateActionBridge {
    /// Creates a new shared-state action bridge.
    #[must_use]
    pub fn new(
        command_tx: flume::Sender<CommandEnvelope>,
        route_resolver: Arc<dyn SharedStateRouteResolver>,
        command_input_resolver: Arc<SharedStateCommandInputResolver>,
    ) -> Self {
        Self {
            command_tx,
            route_resolver,
            command_input_resolver,
        }
    }
}

#[cfg(feature = "shared-state-bridge")]
fn route_request_from_http_request(req: &HttpRequest) -> Result<RouteRequest, actix_web::Error> {
    let query = qstring::QString::from(req.query_string())
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let headers = req
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let cookies = req
        .cookies()
        .inspect_err(|e| {
            log::debug!("Failed to parse cookies for shared-state bridge: {e}");
        })
        .map(|cookies| {
            cookies
                .iter()
                .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    Ok(RouteRequest {
        path: req.path().to_string(),
        method: Method::from_str(req.method().as_str()).map_err(ErrorInternalServerError)?,
        query,
        headers,
        cookies,
        info: RequestInfo {
            client: Arc::new(ClientInfo {
                os: ClientOs {
                    name: "unknown".to_string(),
                },
            }),
        },
        body: None,
    })
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
///
/// # Examples
///
/// ```rust,ignore
/// use actix_web::web;
///
/// // Register the action endpoint in your Actix app configuration.
/// let _route = web::resource("/$action").route(web::post().to(handle_action::<(), _>));
/// ```
#[allow(clippy::future_not_send)]
pub async fn handle_action<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    req: HttpRequest,
    app: web::Data<ActixApp<T, R>>,
    action: web::Json<ActionPayload>,
) -> impl Responder {
    log::debug!("handle_action: action={action:?}");

    #[cfg(not(feature = "shared-state-bridge"))]
    let _ = &req;

    let action_name = action.0.action.as_str().map_or_else(
        || serde_json::to_string(&action.0.action).unwrap(),
        std::string::ToString::to_string,
    );

    #[cfg(feature = "shared-state-bridge")]
    if let Some(shared_state_bridge) = &app.shared_state_bridge {
        let command_input = (shared_state_bridge.command_input_resolver)(
            action_name.as_str(),
            action.0.value.as_ref(),
        );

        if let Some(command_input) = command_input {
            let route_request = route_request_from_http_request(&req)?;
            let context =
                resolve_route_context(shared_state_bridge.route_resolver.as_ref(), &route_request)
                    .map_err(ErrorInternalServerError)?;
            let command =
                command_from_route(context, command_input).map_err(ErrorInternalServerError)?;

            shared_state_bridge
                .command_tx
                .send(command)
                .map_err(ErrorInternalServerError)?;
        }
    }

    if let Some(tx) = &app.action_tx {
        tx.send((action_name, action.0.value))
            .map_err(ErrorInternalServerError)?;
    }

    Ok::<_, actix_web::Error>(HttpResponse::NoContent())
}

#[cfg(all(test, feature = "shared-state-bridge"))]
mod tests {
    use std::sync::Arc;

    use actix_web::{HttpResponse, Responder, http::StatusCode, test, web};
    use async_trait::async_trait;
    use bytes::Bytes;
    use hyperchad_renderer::RendererEvent;
    use hyperchad_shared_state_bridge::{BridgeError, RouteCommandInput, SharedStateRouteResolver};
    use hyperchad_shared_state_models::{
        ChannelId, CommandId, IdempotencyKey, ParticipantId, PayloadBlob, Revision,
    };
    use switchy_http_models::Method;

    use super::{ActionPayload, handle_action};
    use crate::{ActixApp, ActixResponseProcessor};

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
            _content: hyperchad_renderer::Content,
            _data: (),
        ) -> Result<(Bytes, String), actix_web::Error> {
            Ok((Bytes::from_static(b""), "text/plain".to_string()))
        }
    }

    #[derive(Debug)]
    struct RequestAssertingResolver;

    impl SharedStateRouteResolver for RequestAssertingResolver {
        fn resolve_channel_id(
            &self,
            request: &hyperchad_router::RouteRequest,
        ) -> Result<ChannelId, BridgeError> {
            assert_eq!(request.path, "/$action");
            assert_eq!(request.method, Method::Post);
            assert_eq!(request.query.get("room"), Some(&"alpha".to_string()));
            assert_eq!(
                request.headers.get("x-test-header"),
                Some(&"header-value".to_string())
            );
            assert_eq!(
                request.cookies.get("session"),
                Some(&"cookie-1".to_string())
            );

            Ok(ChannelId::new("room:alpha"))
        }

        fn resolve_participant_id(
            &self,
            _request: &hyperchad_router::RouteRequest,
        ) -> Result<ParticipantId, BridgeError> {
            Ok(ParticipantId::new("participant-1"))
        }
    }

    #[actix_web::test]
    async fn handle_action_sends_action_and_shared_state_command() {
        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let (command_tx, command_rx) = flume::unbounded();

        let payload = PayloadBlob::from_serializable(&7_u32).expect("payload should serialize");
        let app = ActixApp::new(TestProcessor, renderer_event_rx)
            .with_action_tx(action_tx)
            .with_shared_state_bridge(
                command_tx,
                Arc::new(RequestAssertingResolver),
                move |action: &str, _value| {
                    if action != "increment" {
                        return None;
                    }

                    Some(RouteCommandInput {
                        command_id: CommandId::new("command-1"),
                        idempotency_key: IdempotencyKey::new("idem-1"),
                        expected_revision: Revision::new(3),
                        command_name: "INCREMENT".to_string(),
                        payload: payload.clone(),
                        metadata: std::collections::BTreeMap::new(),
                    })
                },
            );

        let req = test::TestRequest::post()
            .uri("/$action?room=alpha")
            .insert_header(("x-test-header", "header-value"))
            .cookie(actix_web::cookie::Cookie::new("session", "cookie-1"))
            .to_http_request();
        let payload = web::Json(ActionPayload {
            action: serde_json::Value::String("increment".to_string()),
            value: None,
        });

        let response = handle_action(req.clone(), web::Data::new(app), payload)
            .await
            .respond_to(&req);

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let sent_action = action_rx.try_recv().expect("action should be emitted");
        assert_eq!(sent_action.0, "increment");
        assert!(sent_action.1.is_none());

        let sent_command = command_rx
            .try_recv()
            .expect("shared-state command should be emitted");
        assert_eq!(sent_command.channel_id, ChannelId::new("room:alpha"));
        assert_eq!(
            sent_command.participant_id,
            ParticipantId::new("participant-1")
        );
        assert_eq!(sent_command.command_id, CommandId::new("command-1"));
        assert_eq!(sent_command.idempotency_key, IdempotencyKey::new("idem-1"));
        assert_eq!(sent_command.expected_revision, Revision::new(3));
        assert_eq!(sent_command.command_name, "INCREMENT");
    }

    #[actix_web::test]
    async fn handle_action_skips_shared_state_when_resolver_returns_none() {
        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let (action_tx, action_rx) = flume::unbounded();
        let (command_tx, command_rx) = flume::unbounded();

        let app = ActixApp::new(TestProcessor, renderer_event_rx)
            .with_action_tx(action_tx)
            .with_shared_state_bridge(
                command_tx,
                Arc::new(RequestAssertingResolver),
                |_action: &str, _value| None,
            );

        let req = test::TestRequest::post()
            .uri("/$action?room=alpha")
            .insert_header(("x-test-header", "header-value"))
            .cookie(actix_web::cookie::Cookie::new("session", "cookie-1"))
            .to_http_request();
        let payload = web::Json(ActionPayload {
            action: serde_json::Value::String("noop".to_string()),
            value: None,
        });

        let response = handle_action(req.clone(), web::Data::new(app), payload)
            .await
            .respond_to(&req);

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let sent_action = action_rx.try_recv().expect("action should be emitted");
        assert_eq!(sent_action.0, "noop");
        assert!(sent_action.1.is_none());

        assert!(command_rx.is_empty());
    }
}
