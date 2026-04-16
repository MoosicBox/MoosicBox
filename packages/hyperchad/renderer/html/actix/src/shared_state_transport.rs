use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, RwLock},
};

use actix_web::{
    HttpRequest, HttpResponse,
    error::{ErrorBadRequest, ErrorInternalServerError},
    http::header::{CacheControl, CacheDirective},
    web,
};
use async_trait::async_trait;
use bytes::Bytes;
use flume::Receiver;
use futures_util::{
    FutureExt as _, StreamExt as _,
    future::{Either, select},
    pin_mut,
};
use hyperchad_shared_state::{
    runtime::{ApplyPreparedCommandResult, SharedStateEngine},
    traits::{CommandStore, EventDraft, EventStore, FanoutBus, SnapshotStore},
};
use hyperchad_shared_state_models::{
    ChannelId, EventEnvelope, TransportInbound, TransportOutbound,
};

use crate::{ActixApp, ActixResponseProcessor};

pub type SharedStateInboundReceiverFactory = dyn Fn() -> Receiver<TransportInbound> + Send + Sync;

pub type SharedStateTransportDispatchError = Box<dyn std::error::Error + Send + Sync>;
pub type SharedStateTransportDispatchResult<T> = Result<T, SharedStateTransportDispatchError>;

#[async_trait]
pub trait SharedStateTransportDispatcher: Send + Sync {
    async fn ingest_outbound(
        &self,
        outbound: TransportOutbound,
    ) -> SharedStateTransportDispatchResult<Vec<TransportInbound>>;

    async fn subscribe_channel(
        &self,
        channel_id: &ChannelId,
    ) -> SharedStateTransportDispatchResult<Receiver<EventEnvelope>>;
}

#[derive(Clone)]
pub struct RuntimeFanoutTransportDispatcher<C, E, S, F>
where
    C: CommandStore,
    E: EventStore,
    S: SnapshotStore,
    F: FanoutBus,
{
    engine: Arc<SharedStateEngine<C, E, S, F>>,
    fanout_bus: Arc<F>,
    replay_limit: u32,
}

impl<C, E, S, F> RuntimeFanoutTransportDispatcher<C, E, S, F>
where
    C: CommandStore,
    E: EventStore,
    S: SnapshotStore,
    F: FanoutBus,
{
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(engine: Arc<SharedStateEngine<C, E, S, F>>, fanout_bus: Arc<F>) -> Self {
        Self {
            engine,
            fanout_bus,
            replay_limit: 100,
        }
    }

    #[must_use]
    pub const fn with_replay_limit(mut self, replay_limit: u32) -> Self {
        self.replay_limit = replay_limit;
        self
    }
}

#[async_trait]
impl<C, E, S, F> SharedStateTransportDispatcher for RuntimeFanoutTransportDispatcher<C, E, S, F>
where
    C: CommandStore + Send + Sync,
    E: EventStore + Send + Sync,
    S: SnapshotStore + Send + Sync,
    F: FanoutBus + Send + Sync,
{
    async fn ingest_outbound(
        &self,
        outbound: TransportOutbound,
    ) -> SharedStateTransportDispatchResult<Vec<TransportInbound>> {
        match outbound {
            TransportOutbound::Command(command) => {
                let drafts = vec![EventDraft::new(
                    command.command_name.clone(),
                    command.payload.clone(),
                    command.metadata.clone(),
                )];

                let result = self.engine.apply_prepared(&command, &drafts, None).await?;

                let inbound = match result {
                    ApplyPreparedCommandResult::Applied {
                        resulting_revision,
                        emitted_event_count: _,
                    } => TransportInbound::CommandAccepted {
                        command_id: command.command_id,
                        resulting_revision,
                    },
                    ApplyPreparedCommandResult::DuplicateApplied {
                        command_id,
                        resulting_revision,
                    } => TransportInbound::CommandAccepted {
                        command_id,
                        resulting_revision,
                    },
                    ApplyPreparedCommandResult::DuplicateRejected { command_id, reason } => {
                        TransportInbound::CommandRejected { command_id, reason }
                    }
                    ApplyPreparedCommandResult::Conflict { actual_revision } => {
                        TransportInbound::CommandRejected {
                            command_id: command.command_id,
                            reason: format!(
                                "Expected revision {} but actual revision is {}",
                                command.expected_revision, actual_revision
                            ),
                        }
                    }
                };

                Ok(vec![inbound])
            }
            TransportOutbound::Subscribe(subscribe) => {
                let replay = self
                    .engine
                    .replay_since(
                        &subscribe.channel_id,
                        subscribe.last_seen_revision,
                        self.replay_limit,
                    )
                    .await?;

                let mut inbound = Vec::new();
                if let Some(snapshot) = replay.snapshot {
                    inbound.push(TransportInbound::Snapshot(snapshot));
                }
                inbound.extend(replay.events.into_iter().map(TransportInbound::Event));

                Ok(inbound)
            }
            TransportOutbound::Unsubscribe(_unsubscribe) => Ok(Vec::new()),
            TransportOutbound::Ping(ping) => Ok(vec![TransportInbound::Pong(ping)]),
        }
    }

    async fn subscribe_channel(
        &self,
        channel_id: &ChannelId,
    ) -> SharedStateTransportDispatchResult<Receiver<EventEnvelope>> {
        Ok(self.fanout_bus.subscribe(channel_id).await?)
    }
}

struct SharedStateSseSession {
    client_tx: flume::Sender<TransportInbound>,
    subscriptions: BTreeMap<ChannelId, flume::Sender<()>>,
}

impl SharedStateSseSession {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    fn new(client_tx: flume::Sender<TransportInbound>) -> Self {
        Self {
            client_tx,
            subscriptions: BTreeMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct SharedStateTransportBridge {
    pub outbound_tx: flume::Sender<TransportOutbound>,
    pub inbound_receiver_factory: Arc<SharedStateInboundReceiverFactory>,
    dispatcher: Option<Arc<dyn SharedStateTransportDispatcher>>,
    sse_sessions: Arc<RwLock<BTreeMap<String, Arc<Mutex<SharedStateSseSession>>>>>,
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
            dispatcher: None,
            sse_sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    #[must_use]
    pub fn new_with_dispatcher(dispatcher: Arc<dyn SharedStateTransportDispatcher>) -> Self {
        let (outbound_tx, _outbound_rx) = flume::unbounded();
        let (_inbound_tx, inbound_rx) = flume::unbounded();

        Self {
            outbound_tx,
            inbound_receiver_factory: Arc::new(move || inbound_rx.clone()),
            dispatcher: Some(dispatcher),
            sse_sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

fn lock_poison_error(context: &str) -> actix_web::Error {
    ErrorInternalServerError(format!("{context}: lock poisoned"))
}

fn session_id_from_request(req: &HttpRequest) -> Option<String> {
    let query = qstring::QString::from(req.query_string());

    query
        .get("session_id")
        .or_else(|| query.get("session"))
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            req.cookie("v-shared-state-session-id")
                .map(|cookie| cookie.value().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            req.cookie("v-sse-stream-id")
                .map(|cookie| cookie.value().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn sse_session_sender(
    session: &Arc<Mutex<SharedStateSseSession>>,
) -> Result<flume::Sender<TransportInbound>, actix_web::Error> {
    session
        .lock()
        .map_err(|_| lock_poison_error("sse session lock"))
        .map(|session| session.client_tx.clone())
}

fn upsert_sse_session_stream(
    bridge: &SharedStateTransportBridge,
    session_id: &str,
) -> Result<Receiver<TransportInbound>, actix_web::Error> {
    let (client_tx, client_rx) = flume::unbounded();

    let mut sessions = bridge
        .sse_sessions
        .write()
        .map_err(|_| lock_poison_error("sse sessions write"))?;

    if let Some(session) = sessions.get(session_id) {
        let mut session = session
            .lock()
            .map_err(|_| lock_poison_error("sse session lock"))?;
        session.client_tx = client_tx;
    } else {
        sessions.insert(
            session_id.to_string(),
            Arc::new(Mutex::new(SharedStateSseSession::new(client_tx))),
        );
    }

    drop(sessions);

    Ok(client_rx)
}

fn lookup_sse_session(
    bridge: &SharedStateTransportBridge,
    session_id: &str,
) -> Result<Option<Arc<Mutex<SharedStateSseSession>>>, actix_web::Error> {
    bridge
        .sse_sessions
        .read()
        .map_err(|_| lock_poison_error("sse sessions read"))
        .map(|sessions| sessions.get(session_id).cloned())
}

fn remove_session_subscription(
    session: &Arc<Mutex<SharedStateSseSession>>,
    channel_id: &ChannelId,
) -> Result<(), actix_web::Error> {
    let stop_tx = session
        .lock()
        .map_err(|_| lock_poison_error("sse session lock"))?
        .subscriptions
        .remove(channel_id);

    if let Some(stop_tx) = stop_tx {
        let _ = stop_tx.send(());
    }

    Ok(())
}

fn spawn_sse_subscription_forwarder(
    session: Arc<Mutex<SharedStateSseSession>>,
    channel_id: ChannelId,
    event_rx: Receiver<EventEnvelope>,
    stop_rx: Receiver<()>,
) {
    actix_web::rt::spawn(async move {
        loop {
            let stop = stop_rx.recv_async().fuse();
            let event = event_rx.recv_async().fuse();
            pin_mut!(stop, event);

            match select(stop, event).await {
                Either::Left((_stop, _pending_event)) => {
                    break;
                }
                Either::Right((event, _pending_stop)) => {
                    let Ok(event) = event else {
                        break;
                    };

                    let sender = match session.lock() {
                        Ok(session) => session.client_tx.clone(),
                        Err(_error) => break,
                    };

                    if sender.send(TransportInbound::Event(event)).is_err() {
                        break;
                    }
                }
            }
        }

        if let Ok(mut session) = session.lock() {
            session.subscriptions.remove(&channel_id);
        }
    });
}

async fn ensure_sse_session_subscription(
    session: Arc<Mutex<SharedStateSseSession>>,
    dispatcher: Arc<dyn SharedStateTransportDispatcher>,
    channel_id: ChannelId,
) -> Result<(), actix_web::Error> {
    let already_subscribed = session
        .lock()
        .map_err(|_| lock_poison_error("sse session lock"))?
        .subscriptions
        .contains_key(&channel_id);

    if already_subscribed {
        return Ok(());
    }

    let event_rx = dispatcher
        .subscribe_channel(&channel_id)
        .await
        .map_err(ErrorInternalServerError)?;
    let (stop_tx, stop_rx) = flume::bounded(1);

    session
        .lock()
        .map_err(|_| lock_poison_error("sse session lock"))?
        .subscriptions
        .insert(channel_id.clone(), stop_tx);

    spawn_sse_subscription_forwarder(session, channel_id, event_rx, stop_rx);

    Ok(())
}

fn spawn_ws_subscription_forwarder(
    outbound_tx: flume::Sender<TransportInbound>,
    event_rx: Receiver<EventEnvelope>,
    stop_rx: Receiver<()>,
) {
    actix_web::rt::spawn(async move {
        loop {
            let stop = stop_rx.recv_async().fuse();
            let event = event_rx.recv_async().fuse();
            pin_mut!(stop, event);

            match select(stop, event).await {
                Either::Left((_stop, _pending_event)) => {
                    break;
                }
                Either::Right((event, _pending_stop)) => {
                    let Ok(event) = event else {
                        break;
                    };

                    if outbound_tx.send(TransportInbound::Event(event)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

async fn ensure_ws_subscription(
    subscriptions: &mut BTreeMap<ChannelId, flume::Sender<()>>,
    dispatcher: Arc<dyn SharedStateTransportDispatcher>,
    outbound_tx: flume::Sender<TransportInbound>,
    channel_id: ChannelId,
) -> Result<(), actix_web::Error> {
    if subscriptions.contains_key(&channel_id) {
        return Ok(());
    }

    let event_rx = dispatcher
        .subscribe_channel(&channel_id)
        .await
        .map_err(ErrorInternalServerError)?;
    let (stop_tx, stop_rx) = flume::bounded(1);
    subscriptions.insert(channel_id, stop_tx);

    spawn_ws_subscription_forwarder(outbound_tx, event_rx, stop_rx);

    Ok(())
}

fn remove_ws_subscription(
    subscriptions: &mut BTreeMap<ChannelId, flume::Sender<()>>,
    channel_id: &ChannelId,
) {
    if let Some(stop_tx) = subscriptions.remove(channel_id) {
        let _ = stop_tx.send(());
    }
}

async fn process_ws_dispatcher_outbound(
    outbound: TransportOutbound,
    dispatcher: Arc<dyn SharedStateTransportDispatcher>,
    subscriptions: &mut BTreeMap<ChannelId, flume::Sender<()>>,
    ws_outbound_tx: flume::Sender<TransportInbound>,
) -> Result<(), actix_web::Error> {
    let responses = dispatcher
        .ingest_outbound(outbound.clone())
        .await
        .map_err(ErrorInternalServerError)?;

    match outbound {
        TransportOutbound::Subscribe(subscribe) => {
            ensure_ws_subscription(
                subscriptions,
                dispatcher,
                ws_outbound_tx.clone(),
                subscribe.channel_id,
            )
            .await?;
        }
        TransportOutbound::Unsubscribe(unsubscribe) => {
            remove_ws_subscription(subscriptions, &unsubscribe.channel_id);
        }
        TransportOutbound::Command(_) | TransportOutbound::Ping(_) => {}
    }

    for response in responses {
        if ws_outbound_tx.send(response).is_err() {
            break;
        }
    }

    Ok(())
}

fn parse_ws_transport_outbound(message: &actix_ws::Message) -> Option<TransportOutbound> {
    match message {
        actix_ws::Message::Text(text) => {
            serde_json::from_str::<TransportOutbound>(text.as_ref()).ok()
        }
        actix_ws::Message::Binary(binary) => {
            serde_json::from_slice::<TransportOutbound>(binary).ok()
        }
        _ => None,
    }
}

#[allow(clippy::future_not_send)]
pub async fn handle_shared_state_transport_post<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    req: HttpRequest,
    app: web::Data<ActixApp<T, R>>,
    outbound: web::Json<TransportOutbound>,
) -> Result<HttpResponse, actix_web::Error> {
    let Some(shared_state_transport) = &app.shared_state_transport else {
        return Ok(HttpResponse::ServiceUnavailable().finish());
    };

    if let Some(dispatcher) = shared_state_transport.dispatcher.clone() {
        let session_id = session_id_from_request(&req).ok_or_else(|| {
            ErrorBadRequest("Missing shared-state session id (query 'session_id' or cookie)")
        })?;
        let Some(session) = lookup_sse_session(shared_state_transport, &session_id)? else {
            return Ok(HttpResponse::Conflict().finish());
        };

        let outbound = outbound.0;
        let responses = dispatcher
            .ingest_outbound(outbound.clone())
            .await
            .map_err(ErrorInternalServerError)?;

        match outbound {
            TransportOutbound::Subscribe(subscribe) => {
                ensure_sse_session_subscription(
                    session.clone(),
                    dispatcher.clone(),
                    subscribe.channel_id,
                )
                .await?;
            }
            TransportOutbound::Unsubscribe(unsubscribe) => {
                remove_session_subscription(&session, &unsubscribe.channel_id)?;
            }
            TransportOutbound::Command(_) | TransportOutbound::Ping(_) => {}
        }

        let sender = sse_session_sender(&session)?;
        for response in responses {
            if sender.send(response).is_err() {
                return Ok(HttpResponse::Conflict().finish());
            }
        }

        return Ok(HttpResponse::NoContent().finish());
    }

    shared_state_transport
        .outbound_tx
        .send(outbound.0)
        .map_err(ErrorInternalServerError)?;

    Ok(HttpResponse::NoContent().finish())
}

#[allow(clippy::future_not_send)]
pub async fn handle_shared_state_transport_sse<
    T: Send + Sync + Clone + 'static,
    R: ActixResponseProcessor<T> + Send + Sync + Clone + 'static,
>(
    req: HttpRequest,
    app: web::Data<ActixApp<T, R>>,
) -> Result<HttpResponse, actix_web::Error> {
    let Some(shared_state_transport) = app.shared_state_transport.clone() else {
        return Ok(HttpResponse::ServiceUnavailable().finish());
    };

    let inbound_rx = if shared_state_transport.dispatcher.is_some() {
        let session_id = session_id_from_request(&req).ok_or_else(|| {
            ErrorBadRequest("Missing shared-state session id (query 'session_id' or cookie)")
        })?;
        upsert_sse_session_stream(&shared_state_transport, &session_id)?
    } else {
        (shared_state_transport.inbound_receiver_factory)()
    };

    let stream = inbound_rx.into_stream().map(|inbound| {
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

    if let Some(dispatcher) = shared_state_transport.dispatcher.clone() {
        let (response, mut session, message_stream) = actix_ws::handle(&req, body)?;
        let (ws_outbound_tx, ws_outbound_rx) = flume::unbounded::<TransportInbound>();

        actix_web::rt::spawn(async move {
            let mut subscriptions: BTreeMap<ChannelId, flume::Sender<()>> = BTreeMap::new();

            let ws_outbound_stream = ws_outbound_rx
                .into_stream()
                .map(WsDispatcherLoopItem::OutboundMessage);
            let ws_inbound_stream = message_stream.map(WsDispatcherLoopItem::ClientMessage);
            let mut combined = futures_util::stream::select(ws_outbound_stream, ws_inbound_stream);

            while let Some(item) = combined.next().await {
                match item {
                    WsDispatcherLoopItem::OutboundMessage(inbound) => {
                        let payload = match serde_json::to_string(&inbound) {
                            Ok(payload) => payload,
                            Err(error) => {
                                log::warn!(
                                    "Failed to serialize shared-state websocket outbound message: {error}"
                                );
                                continue;
                            }
                        };

                        if let Err(error) = session.text(payload).await {
                            log::debug!(
                                "Shared-state transport websocket send failed, closing: {error}"
                            );
                            break;
                        }
                    }
                    WsDispatcherLoopItem::ClientMessage(Ok(message)) => match message {
                        actix_ws::Message::Ping(payload) => {
                            if let Err(error) = session.pong(&payload).await {
                                log::debug!("Failed to send websocket pong: {error}");
                                break;
                            }
                        }
                        actix_ws::Message::Close(reason) => {
                            if let Err(error) = session.clone().close(reason).await {
                                log::debug!("Failed to close websocket session: {error}");
                            }
                            break;
                        }
                        actix_ws::Message::Text(_)
                        | actix_ws::Message::Binary(_)
                        | actix_ws::Message::Continuation(_)
                        | actix_ws::Message::Pong(_)
                        | actix_ws::Message::Nop => {
                            if let Some(outbound) = parse_ws_transport_outbound(&message)
                                && let Err(error) = process_ws_dispatcher_outbound(
                                    outbound,
                                    dispatcher.clone(),
                                    &mut subscriptions,
                                    ws_outbound_tx.clone(),
                                )
                                .await
                            {
                                log::debug!(
                                    "Shared-state transport websocket outbound processing failed: {error}"
                                );
                                break;
                            }
                        }
                    },
                    WsDispatcherLoopItem::ClientMessage(Err(error)) => {
                        log::debug!(
                            "Shared-state transport websocket receive failed, closing: {error}"
                        );
                        break;
                    }
                }
            }

            for stop_tx in subscriptions.into_values() {
                let _ = stop_tx.send(());
            }
        });

        return Ok(response);
    }

    let (response, mut session, message_stream) = actix_ws::handle(&req, body)?;
    let outbound_tx = shared_state_transport.outbound_tx;
    let inbound_stream = (shared_state_transport.inbound_receiver_factory)()
        .into_stream()
        .map(WsChannelBridgeLoopItem::InboundTransport);
    let client_stream = message_stream.map(WsChannelBridgeLoopItem::ClientMessage);

    let mut combined_stream = futures_util::stream::select(inbound_stream, client_stream);

    actix_web::rt::spawn(async move {
        while let Some(item) = combined_stream.next().await {
            match item {
                WsChannelBridgeLoopItem::InboundTransport(inbound) => {
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
                WsChannelBridgeLoopItem::ClientMessage(Ok(message)) => {
                    if !handle_client_message_for_channel_bridge(
                        &mut session,
                        &outbound_tx,
                        message,
                    )
                    .await
                    {
                        break;
                    }
                }
                WsChannelBridgeLoopItem::ClientMessage(Err(error)) => {
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

enum WsDispatcherLoopItem {
    OutboundMessage(TransportInbound),
    ClientMessage(Result<actix_ws::Message, actix_ws::ProtocolError>),
}

enum WsChannelBridgeLoopItem {
    InboundTransport(TransportInbound),
    ClientMessage(Result<actix_ws::Message, actix_ws::ProtocolError>),
}

async fn handle_client_message_for_channel_bridge(
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
    #![allow(
        clippy::map_unwrap_or,
        clippy::significant_drop_in_scrutinee,
        clippy::significant_drop_tightening
    )]

    use std::{collections::BTreeMap, sync::Arc};

    use actix_web::{HttpRequest, HttpResponse, body::to_bytes, http::StatusCode, test, web};
    use async_trait::async_trait;
    use bytes::Bytes;
    use hyperchad_renderer::RendererEvent;
    use hyperchad_shared_state::{
        fanout::InProcessFanoutBus,
        runtime::SharedStateEngine,
        traits::{
            AppendEventsResult, BeginCommandResult, CommandStore, EventDraft, EventStore,
            SnapshotStore,
        },
    };
    use hyperchad_shared_state_models::{
        ChannelId, CommandEnvelope, CommandId, EventEnvelope, EventId, IdempotencyKey,
        ParticipantId, PayloadBlob, Revision, SnapshotEnvelope, TransportInbound,
        TransportOutbound, TransportPing, TransportSubscribe,
    };

    use super::{
        RuntimeFanoutTransportDispatcher, SharedStateTransportDispatcher,
        handle_shared_state_transport_post, handle_shared_state_transport_sse,
    };
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

    #[derive(Default)]
    struct MemoryStore {
        commands: std::sync::Mutex<BTreeMap<(String, String), CommandEnvelope>>,
        command_results: std::sync::Mutex<BTreeMap<String, Result<Revision, String>>>,
        channel_revisions: std::sync::Mutex<BTreeMap<String, Revision>>,
        events: std::sync::Mutex<BTreeMap<String, Vec<EventEnvelope>>>,
        snapshots: std::sync::Mutex<BTreeMap<String, SnapshotEnvelope>>,
    }

    impl MemoryStore {
        fn lock_poison_error(context: &str) -> hyperchad_shared_state::SharedStateError {
            hyperchad_shared_state::SharedStateError::Conversion(format!(
                "{context}: lock poisoned"
            ))
        }
    }

    #[async_trait]
    impl CommandStore for MemoryStore {
        async fn begin_command(
            &self,
            command: &CommandEnvelope,
        ) -> Result<BeginCommandResult, hyperchad_shared_state::SharedStateError> {
            let key = (
                command.channel_id.to_string(),
                command.idempotency_key.to_string(),
            );

            if let Some(existing) = self
                .commands
                .lock()
                .map_err(|_| Self::lock_poison_error("commands lock"))?
                .get(&key)
                .cloned()
            {
                if let Some(result) = self
                    .command_results
                    .lock()
                    .map_err(|_| Self::lock_poison_error("command_results lock"))?
                    .get(existing.command_id.as_str())
                    .cloned()
                {
                    return match result {
                        Ok(revision) => Ok(BeginCommandResult::DuplicateApplied {
                            command_id: existing.command_id,
                            resulting_revision: revision,
                        }),
                        Err(reason) => Ok(BeginCommandResult::DuplicateRejected {
                            command_id: existing.command_id,
                            reason,
                        }),
                    };
                }

                return Ok(BeginCommandResult::DuplicateRejected {
                    command_id: existing.command_id,
                    reason: "Command with idempotency key already pending".to_string(),
                });
            }

            self.commands
                .lock()
                .map_err(|_| Self::lock_poison_error("commands lock"))?
                .insert(key, command.clone());

            Ok(BeginCommandResult::New)
        }

        async fn mark_applied(
            &self,
            command_id: &CommandId,
            resulting_revision: Revision,
        ) -> Result<(), hyperchad_shared_state::SharedStateError> {
            self.command_results
                .lock()
                .map_err(|_| Self::lock_poison_error("command_results lock"))?
                .insert(command_id.to_string(), Ok(resulting_revision));
            Ok(())
        }

        async fn mark_rejected(
            &self,
            command_id: &CommandId,
            reason: &str,
        ) -> Result<(), hyperchad_shared_state::SharedStateError> {
            self.command_results
                .lock()
                .map_err(|_| Self::lock_poison_error("command_results lock"))?
                .insert(command_id.to_string(), Err(reason.to_string()));
            Ok(())
        }

        async fn load_by_idempotency_key(
            &self,
            channel_id: &ChannelId,
            idempotency_key: &IdempotencyKey,
        ) -> Result<Option<CommandEnvelope>, hyperchad_shared_state::SharedStateError> {
            Ok(self
                .commands
                .lock()
                .map_err(|_| Self::lock_poison_error("commands lock"))?
                .get(&(channel_id.to_string(), idempotency_key.to_string()))
                .cloned())
        }
    }

    #[async_trait]
    impl EventStore for MemoryStore {
        async fn append_events(
            &self,
            command: &CommandEnvelope,
            drafts: &[EventDraft],
        ) -> Result<AppendEventsResult, hyperchad_shared_state::SharedStateError> {
            let mut revisions = self
                .channel_revisions
                .lock()
                .map_err(|_| Self::lock_poison_error("channel_revisions lock"))?;

            let actual_revision = revisions
                .get(command.channel_id.as_str())
                .copied()
                .unwrap_or_default();

            if actual_revision != command.expected_revision {
                return Ok(AppendEventsResult::Conflict { actual_revision });
            }

            if drafts.is_empty() {
                return Ok(AppendEventsResult::Appended {
                    from_revision: command.expected_revision,
                    to_revision: command.expected_revision,
                    events: Vec::new(),
                });
            }

            let mut events = Vec::with_capacity(drafts.len());
            let mut channel_events = self
                .events
                .lock()
                .map_err(|_| Self::lock_poison_error("events lock"))?;
            let entries = channel_events
                .entry(command.channel_id.to_string())
                .or_default();

            for (index, draft) in drafts.iter().enumerate() {
                let revision = command.expected_revision.incremented_by(
                    u64::try_from(index).map_err(|error| {
                        hyperchad_shared_state::SharedStateError::Conversion(format!(
                            "index conversion failed: {error}"
                        ))
                    })? + 1,
                );

                let event = EventEnvelope {
                    event_id: EventId::new(format!("{}:{revision}", command.command_id)),
                    channel_id: command.channel_id.clone(),
                    revision,
                    command_id: Some(command.command_id.clone()),
                    event_name: draft.event_name.clone(),
                    payload: draft.payload.clone(),
                    metadata: draft.metadata.clone(),
                    created_at_ms: command.created_at_ms,
                };

                entries.push(event.clone());
                events.push(event);
            }

            let to_revision = events
                .last()
                .map(|event| event.revision)
                .unwrap_or(command.expected_revision);
            revisions.insert(command.channel_id.to_string(), to_revision);

            Ok(AppendEventsResult::Appended {
                from_revision: command.expected_revision,
                to_revision,
                events,
            })
        }

        async fn read_events(
            &self,
            channel_id: &ChannelId,
            from_exclusive_revision: Option<Revision>,
            limit: u32,
        ) -> Result<Vec<EventEnvelope>, hyperchad_shared_state::SharedStateError> {
            let events = self
                .events
                .lock()
                .map_err(|_| Self::lock_poison_error("events lock"))?
                .get(channel_id.as_str())
                .cloned()
                .unwrap_or_default();

            let filtered = events
                .into_iter()
                .filter(|event| from_exclusive_revision.is_none_or(|from| event.revision > from))
                .take(usize::try_from(limit).map_err(|error| {
                    hyperchad_shared_state::SharedStateError::Conversion(format!(
                        "limit conversion failed: {error}"
                    ))
                })?)
                .collect();

            Ok(filtered)
        }

        async fn latest_revision(
            &self,
            channel_id: &ChannelId,
        ) -> Result<Option<Revision>, hyperchad_shared_state::SharedStateError> {
            Ok(self
                .channel_revisions
                .lock()
                .map_err(|_| Self::lock_poison_error("channel_revisions lock"))?
                .get(channel_id.as_str())
                .copied())
        }
    }

    #[async_trait]
    impl SnapshotStore for MemoryStore {
        async fn load_latest_snapshot(
            &self,
            channel_id: &ChannelId,
        ) -> Result<Option<SnapshotEnvelope>, hyperchad_shared_state::SharedStateError> {
            Ok(self
                .snapshots
                .lock()
                .map_err(|_| Self::lock_poison_error("snapshots lock"))?
                .get(channel_id.as_str())
                .cloned())
        }

        async fn put_snapshot(
            &self,
            snapshot: &SnapshotEnvelope,
        ) -> Result<(), hyperchad_shared_state::SharedStateError> {
            self.snapshots
                .lock()
                .map_err(|_| Self::lock_poison_error("snapshots lock"))?
                .insert(snapshot.channel_id.to_string(), snapshot.clone());
            Ok(())
        }
    }

    #[actix_web::test]
    async fn runtime_dispatcher_translates_transport_messages() {
        let store = Arc::new(MemoryStore::default());
        let fanout = Arc::new(InProcessFanoutBus::new());
        let engine = Arc::new(SharedStateEngine::new(
            store.clone(),
            store.clone(),
            store.clone(),
            fanout.clone(),
        ));
        let dispatcher = RuntimeFanoutTransportDispatcher::new(engine, fanout.clone());

        let command = CommandEnvelope {
            command_id: CommandId::new("command-1"),
            channel_id: ChannelId::new("channel-a"),
            participant_id: ParticipantId::new("participant-1"),
            idempotency_key: IdempotencyKey::new("idem-1"),
            expected_revision: Revision::new(0),
            command_name: "SET_COUNTER".to_string(),
            payload: PayloadBlob::from_serializable(&1_u32).expect("payload should serialize"),
            metadata: BTreeMap::new(),
            created_at_ms: 1,
        };

        let command_result = dispatcher
            .ingest_outbound(TransportOutbound::Command(command.clone()))
            .await
            .expect("command dispatch should succeed");
        assert_eq!(
            command_result,
            vec![TransportInbound::CommandAccepted {
                command_id: CommandId::new("command-1"),
                resulting_revision: Revision::new(1),
            }]
        );

        let replay_result = dispatcher
            .ingest_outbound(TransportOutbound::Subscribe(TransportSubscribe {
                channel_id: ChannelId::new("channel-a"),
                last_seen_revision: None,
            }))
            .await
            .expect("subscribe replay should succeed");
        assert_eq!(replay_result.len(), 1);
        assert!(matches!(replay_result[0], TransportInbound::Event(_)));

        let receiver = dispatcher
            .subscribe_channel(&ChannelId::new("channel-a"))
            .await
            .expect("fanout subscription should succeed");

        let channel_b_command = CommandEnvelope {
            command_id: CommandId::new("command-2"),
            channel_id: ChannelId::new("channel-b"),
            participant_id: ParticipantId::new("participant-1"),
            idempotency_key: IdempotencyKey::new("idem-2"),
            expected_revision: Revision::new(0),
            command_name: "SET_COUNTER".to_string(),
            payload: PayloadBlob::from_serializable(&2_u32).expect("payload should serialize"),
            metadata: BTreeMap::new(),
            created_at_ms: 2,
        };

        dispatcher
            .ingest_outbound(TransportOutbound::Command(channel_b_command))
            .await
            .expect("channel-b command should succeed");
        assert!(receiver.is_empty());

        let channel_a_command = CommandEnvelope {
            command_id: CommandId::new("command-3"),
            channel_id: ChannelId::new("channel-a"),
            participant_id: ParticipantId::new("participant-1"),
            idempotency_key: IdempotencyKey::new("idem-3"),
            expected_revision: Revision::new(1),
            command_name: "SET_COUNTER".to_string(),
            payload: PayloadBlob::from_serializable(&3_u32).expect("payload should serialize"),
            metadata: BTreeMap::new(),
            created_at_ms: 3,
        };

        dispatcher
            .ingest_outbound(TransportOutbound::Command(channel_a_command))
            .await
            .expect("channel-a command should succeed");

        let forwarded = receiver
            .recv_async()
            .await
            .expect("channel-a subscriber should receive event");
        assert_eq!(forwarded.channel_id, ChannelId::new("channel-a"));
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
        let request = test::TestRequest::post().to_http_request();
        let response = handle_shared_state_transport_post(
            request,
            web::Data::new(app),
            web::Json(outbound.clone()),
        )
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

        let response = handle_shared_state_transport_sse(
            test::TestRequest::get().to_http_request(),
            web::Data::new(app),
        )
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
    async fn dispatcher_post_requires_session_id() {
        #[derive(Debug)]
        struct TestDispatcher;

        #[async_trait]
        impl SharedStateTransportDispatcher for TestDispatcher {
            async fn ingest_outbound(
                &self,
                _outbound: TransportOutbound,
            ) -> super::SharedStateTransportDispatchResult<Vec<TransportInbound>> {
                Ok(Vec::new())
            }

            async fn subscribe_channel(
                &self,
                _channel_id: &ChannelId,
            ) -> super::SharedStateTransportDispatchResult<flume::Receiver<EventEnvelope>>
            {
                let (_tx, rx) = flume::unbounded();
                Ok(rx)
            }
        }

        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let app = ActixApp::new(TestProcessor, renderer_event_rx)
            .with_shared_state_transport_dispatcher(Arc::new(TestDispatcher));

        let response = handle_shared_state_transport_post(
            test::TestRequest::post().to_http_request(),
            web::Data::new(app),
            web::Json(TransportOutbound::Ping(TransportPing { sent_at_ms: 1 })),
        )
        .await;

        assert!(response.is_err());
    }

    #[actix_web::test]
    async fn dispatcher_post_accepts_session_cookie() {
        #[derive(Debug)]
        struct TestDispatcher;

        #[async_trait]
        impl SharedStateTransportDispatcher for TestDispatcher {
            async fn ingest_outbound(
                &self,
                _outbound: TransportOutbound,
            ) -> super::SharedStateTransportDispatchResult<Vec<TransportInbound>> {
                Ok(Vec::new())
            }

            async fn subscribe_channel(
                &self,
                _channel_id: &ChannelId,
            ) -> super::SharedStateTransportDispatchResult<flume::Receiver<EventEnvelope>>
            {
                let (_tx, rx) = flume::unbounded();
                Ok(rx)
            }
        }

        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let app = ActixApp::new(TestProcessor, renderer_event_rx)
            .with_shared_state_transport_dispatcher(Arc::new(TestDispatcher));

        let session_cookie =
            actix_web::cookie::Cookie::new("v-shared-state-session-id", "session-cookie-1");

        let sse_request = test::TestRequest::get()
            .cookie(session_cookie.clone())
            .to_http_request();
        let sse_response =
            handle_shared_state_transport_sse(sse_request, web::Data::new(app.clone()))
                .await
                .expect("sse handler should succeed with cookie session");
        assert_eq!(sse_response.status(), StatusCode::OK);

        let post_request = test::TestRequest::post()
            .cookie(session_cookie)
            .to_http_request();
        let post_response = handle_shared_state_transport_post(
            post_request,
            web::Data::new(app),
            web::Json(TransportOutbound::Ping(TransportPing { sent_at_ms: 1 })),
        )
        .await
        .expect("post handler should succeed with cookie session");

        assert_eq!(post_response.status(), StatusCode::NO_CONTENT);
    }

    #[actix_web::test]
    async fn handlers_return_service_unavailable_without_transport_bridge() {
        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let app = ActixApp::new(TestProcessor, renderer_event_rx);

        let post_response = handle_shared_state_transport_post(
            test::TestRequest::post().to_http_request(),
            web::Data::new(app.clone()),
            web::Json(TransportOutbound::Ping(TransportPing { sent_at_ms: 1 })),
        )
        .await
        .expect("post handler should return response");
        assert_eq!(post_response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let sse_response = handle_shared_state_transport_sse(
            test::TestRequest::get().to_http_request(),
            web::Data::new(app),
        )
        .await
        .expect("sse handler should return response");
        assert_eq!(sse_response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
