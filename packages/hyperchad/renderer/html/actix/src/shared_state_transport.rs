use std::sync::Arc;

use actix_web::{
    HttpRequest, HttpResponse,
    error::ErrorInternalServerError,
    http::header::{CacheControl, CacheDirective},
    web,
};
use bytes::Bytes;
use flume::Receiver;
use futures_util::StreamExt as _;
use hyperchad_shared_state_models::{TransportInbound, TransportOutbound};

use crate::{ActixApp, ActixResponseProcessor};

pub type SharedStateInboundReceiverFactory = dyn Fn() -> Receiver<TransportInbound> + Send + Sync;

#[derive(Clone)]
pub struct SharedStateTransportBridge {
    pub outbound_tx: flume::Sender<TransportOutbound>,
    pub inbound_receiver_factory: Arc<SharedStateInboundReceiverFactory>,
}

impl SharedStateTransportBridge {
    #[must_use]
    pub fn new(
        outbound_tx: flume::Sender<TransportOutbound>,
        inbound_receiver_factory: Arc<SharedStateInboundReceiverFactory>,
    ) -> Self {
        Self {
            outbound_tx,
            inbound_receiver_factory,
        }
    }
}

pub async fn handle_shared_state_transport_post<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    app: web::Data<ActixApp<T, R>>,
    outbound: web::Json<TransportOutbound>,
) -> Result<HttpResponse, actix_web::Error> {
    let Some(shared_state_transport) = &app.shared_state_transport else {
        return Ok(HttpResponse::ServiceUnavailable().finish());
    };

    shared_state_transport
        .outbound_tx
        .send(outbound.0)
        .map_err(ErrorInternalServerError)?;

    Ok(HttpResponse::NoContent().finish())
}

pub async fn handle_shared_state_transport_sse<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    app: web::Data<ActixApp<T, R>>,
) -> Result<HttpResponse, actix_web::Error> {
    let Some(shared_state_transport) = app.shared_state_transport.clone() else {
        return Ok(HttpResponse::ServiceUnavailable().finish());
    };

    let stream = (shared_state_transport.inbound_receiver_factory)()
        .into_stream()
        .map(|inbound| {
            serde_json::to_string(&inbound)
                .map(|payload| Bytes::from(format!("data: {payload}\n\n")))
                .map_err(ErrorInternalServerError)
        });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(CacheControl(vec![CacheDirective::NoCache]))
        .streaming(stream))
}

#[allow(clippy::future_not_send, clippy::too_many_lines)]
pub async fn handle_shared_state_transport_ws<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    req: HttpRequest,
    body: web::Payload,
    app: web::Data<ActixApp<T, R>>,
) -> Result<HttpResponse, actix_web::Error> {
    let Some(shared_state_transport) = app.shared_state_transport.clone() else {
        return Ok(HttpResponse::ServiceUnavailable().finish());
    };

    let (response, mut session, message_stream) = actix_ws::handle(&req, body)?;
    let outbound_tx = shared_state_transport.outbound_tx;
    let inbound_stream = (shared_state_transport.inbound_receiver_factory)()
        .into_stream()
        .map(MessageOrInbound::InboundTransport);
    let client_stream = message_stream.map(MessageOrInbound::ClientMessage);

    let mut combined_stream = futures_util::stream::select(inbound_stream, client_stream);

    actix_web::rt::spawn(async move {
        while let Some(item) = combined_stream.next().await {
            match item {
                MessageOrInbound::InboundTransport(inbound) => {
                    match serde_json::to_string(&inbound) {
                        Ok(payload) => {
                            if let Err(error) = session.text(payload).await {
                                log::debug!(
                                    "Shared-state transport websocket send failed, closing: {error}"
                                );
                                break;
                            }
                        }
                        Err(error) => {
                            log::warn!(
                                "Failed to serialize shared-state transport inbound message: {error}"
                            );
                        }
                    }
                }
                MessageOrInbound::ClientMessage(Ok(message)) => {
                    if !handle_client_message(&mut session, &outbound_tx, message).await {
                        break;
                    }
                }
                MessageOrInbound::ClientMessage(Err(error)) => {
                    log::debug!(
                        "Shared-state transport websocket receive failed, closing: {error}"
                    );
                    break;
                }
            }
        }
    });

    Ok(response)
}

enum MessageOrInbound {
    InboundTransport(TransportInbound),
    ClientMessage(Result<actix_ws::Message, actix_ws::ProtocolError>),
}

async fn handle_client_message(
    session: &mut actix_ws::Session,
    outbound_tx: &flume::Sender<TransportOutbound>,
    message: actix_ws::Message,
) -> bool {
    match message {
        actix_ws::Message::Text(text) => {
            match serde_json::from_str::<TransportOutbound>(text.as_ref()) {
                Ok(outbound) => {
                    if outbound_tx.send(outbound).is_err() {
                        log::debug!(
                            "Shared-state transport outbound channel closed, closing websocket"
                        );
                        return false;
                    }
                }
                Err(error) => {
                    log::warn!("Failed to parse shared-state websocket text payload: {error}");
                }
            }
        }
        actix_ws::Message::Binary(binary) => {
            match serde_json::from_slice::<TransportOutbound>(&binary) {
                Ok(outbound) => {
                    if outbound_tx.send(outbound).is_err() {
                        log::debug!(
                            "Shared-state transport outbound channel closed, closing websocket"
                        );
                        return false;
                    }
                }
                Err(error) => {
                    log::warn!("Failed to parse shared-state websocket binary payload: {error}");
                }
            }
        }
        actix_ws::Message::Ping(payload) => {
            if let Err(error) = session.pong(&payload).await {
                log::debug!("Failed to send websocket pong: {error}");
                return false;
            }
        }
        actix_ws::Message::Close(reason) => {
            if let Err(error) = session.clone().close(reason).await {
                log::debug!("Failed to close websocket session: {error}");
            }
            return false;
        }
        actix_ws::Message::Continuation(_)
        | actix_ws::Message::Pong(_)
        | actix_ws::Message::Nop => {}
    }

    true
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use actix_web::{HttpRequest, HttpResponse, body::to_bytes, http::StatusCode, web};
    use async_trait::async_trait;
    use bytes::Bytes;
    use hyperchad_renderer::RendererEvent;
    use hyperchad_shared_state_models::{TransportInbound, TransportOutbound, TransportPing};

    use super::{handle_shared_state_transport_post, handle_shared_state_transport_sse};
    use crate::{ActixApp, ActixResponseProcessor};

    #[derive(Clone)]
    struct TestProcessor;

    #[async_trait]
    impl ActixResponseProcessor<()> for TestProcessor {
        fn prepare_request(
            &self,
            _req: HttpRequest,
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

    #[actix_web::test]
    async fn handle_shared_state_transport_post_sends_outbound_message() {
        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let (outbound_tx, outbound_rx) = flume::unbounded::<TransportOutbound>();

        let app = ActixApp::new(TestProcessor, renderer_event_rx).with_shared_state_transport(
            outbound_tx,
            || {
                let (_tx, rx) = flume::unbounded::<TransportInbound>();
                rx
            },
        );

        let outbound = TransportOutbound::Ping(TransportPing { sent_at_ms: 42 });
        let response =
            handle_shared_state_transport_post(web::Data::new(app), web::Json(outbound.clone()))
                .await
                .expect("post handler should succeed");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            outbound_rx
                .try_recv()
                .expect("outbound transport message should be received"),
            outbound
        );
    }

    #[actix_web::test]
    async fn handle_shared_state_transport_sse_streams_inbound_messages() {
        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let (outbound_tx, _outbound_rx) = flume::unbounded::<TransportOutbound>();

        let inbound = TransportInbound::Pong(TransportPing { sent_at_ms: 77 });
        let app = ActixApp::new(TestProcessor, renderer_event_rx).with_shared_state_transport(
            outbound_tx,
            move || {
                let (inbound_tx, inbound_rx) = flume::unbounded::<TransportInbound>();
                inbound_tx
                    .send(inbound.clone())
                    .expect("should enqueue inbound message");
                drop(inbound_tx);
                inbound_rx
            },
        );

        let response = handle_shared_state_transport_sse(web::Data::new(app))
            .await
            .expect("sse handler should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|x| x.to_str().ok()),
            Some("text/event-stream")
        );

        let body = to_bytes(response.into_body())
            .await
            .expect("stream body should be readable");
        let payload =
            serde_json::to_string(&TransportInbound::Pong(TransportPing { sent_at_ms: 77 }))
                .expect("inbound payload should serialize");
        assert_eq!(body, Bytes::from(format!("data: {payload}\n\n")));
    }

    #[actix_web::test]
    async fn handlers_return_service_unavailable_without_transport_bridge() {
        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let app = ActixApp::new(TestProcessor, renderer_event_rx);

        let post_response = handle_shared_state_transport_post(
            web::Data::new(app.clone()),
            web::Json(TransportOutbound::Ping(TransportPing { sent_at_ms: 1 })),
        )
        .await
        .expect("post handler should return response");
        assert_eq!(post_response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let sse_response = handle_shared_state_transport_sse(web::Data::new(app))
            .await
            .expect("sse handler should return response");
        assert_eq!(sse_response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
