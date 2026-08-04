use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, RwLock},
};

use actix_web::{
    HttpRequest, HttpResponse,
    error::{ErrorBadRequest, ErrorInternalServerError},
    http::{
        StatusCode,
        header::{CacheControl, CacheDirective, HeaderName, HeaderValue},
    },
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
use hyperchad_shared_state_models::{
    ChannelId, EventEnvelope, TransportInbound, TransportOutbound,
};
use hyperchad_shared_state_transport::{
    AuthenticatedTransportContext, SharedStateTransportDispatcher,
};

use crate::{ActixApp, ActixResponseProcessor};

pub type SharedStateInboundReceiverFactory = dyn Fn() -> Receiver<TransportInbound> + Send + Sync;

/// Web-renderer security hook for authenticating and authorizing one HTTP transport request.
#[async_trait(?Send)]
pub trait WebSharedStateSecurity: Send + Sync {
    /// Resolves a renderer-neutral identity and applies web-specific protections such as CSRF.
    ///
    /// # Errors
    ///
    /// Returns an Actix error when authentication or web request validation fails.
    async fn authenticate_request(
        &self,
        request: &HttpRequest,
        is_state_changing: bool,
    ) -> Result<AuthenticatedTransportContext, actix_web::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum WebSessionIdentityError {
    #[error("web session is unauthenticated")]
    Unauthenticated,
    #[error("web session is forbidden: {0}")]
    Forbidden(String),
    #[error("web session identity resolution failed: {0}")]
    Operation(String),
}

/// Resolves an opaque web session credential into renderer-neutral identity.
#[async_trait]
pub trait WebSessionIdentityResolver: Send + Sync {
    /// # Errors
    ///
    /// Returns an error when the opaque session credential is invalid or cannot be resolved.
    async fn resolve_session(
        &self,
        opaque_session: &str,
    ) -> Result<AuthenticatedTransportContext, WebSessionIdentityError>;
}

/// Cookie/header names used by the HTML/Actix shared-state transport security adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieCsrfWebSecurityConfig {
    pub session_cookie_name: String,
    pub csrf_cookie_name: String,
    pub csrf_header_name: String,
}

impl CookieCsrfWebSecurityConfig {
    #[must_use]
    pub fn new(
        session_cookie_name: impl Into<String>,
        csrf_cookie_name: impl Into<String>,
        csrf_header_name: impl Into<String>,
    ) -> Self {
        Self {
            session_cookie_name: session_cookie_name.into(),
            csrf_cookie_name: csrf_cookie_name.into(),
            csrf_header_name: csrf_header_name.into(),
        }
    }
}

const REQUEST_ID_HEADER: &str = "x-request-id";
const TRANSPORT_DIAGNOSTIC_HEADER: &str = "x-hyperchad-transport-diagnostic";
const CSRF_SOURCE_HEADER: &str = "x-hyperchad-csrf-source";
const CSRF_COOKIE_COUNT_HEADER: &str = "x-hyperchad-csrf-cookie-count";
const CSRF_META_MATCH_HEADER: &str = "x-hyperchad-csrf-meta-match";

fn request_header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing")
}

fn request_id(request: &HttpRequest) -> &str {
    request_header(request, REQUEST_ID_HEADER)
}

fn named_cookie_count(request: &HttpRequest, cookie_name: &str) -> usize {
    request
        .headers()
        .get_all(actix_web::http::header::COOKIE)
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .filter(|(name, _)| *name == cookie_name)
        .count()
}

const fn transport_operation(outbound: &TransportOutbound) -> &'static str {
    match outbound {
        TransportOutbound::Command(_) => "command",
        TransportOutbound::Subscribe(_) => "subscribe",
        TransportOutbound::Unsubscribe(_) => "unsubscribe",
        TransportOutbound::Ping(_) => "ping",
    }
}

fn diagnostic_response(
    status: StatusCode,
    request: &HttpRequest,
    diagnostic: &'static str,
) -> HttpResponse {
    let mut response = HttpResponse::build(status);
    response.insert_header((TRANSPORT_DIAGNOSTIC_HEADER, diagnostic));
    if let Some(value) = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| HeaderValue::from_bytes(value.as_bytes()).ok())
    {
        response.insert_header((HeaderName::from_static(REQUEST_ID_HEADER), value));
    }
    response.body(diagnostic)
}

/// HTML/Actix authentication and CSRF adapter with application-configured names and identity.
#[derive(Clone)]
pub struct CookieCsrfWebSecurity {
    config: CookieCsrfWebSecurityConfig,
    identity_resolver: Arc<dyn WebSessionIdentityResolver>,
}

impl CookieCsrfWebSecurity {
    #[must_use]
    pub fn new(
        config: CookieCsrfWebSecurityConfig,
        identity_resolver: Arc<dyn WebSessionIdentityResolver>,
    ) -> Self {
        Self {
            config,
            identity_resolver,
        }
    }
}

#[async_trait(?Send)]
impl WebSharedStateSecurity for CookieCsrfWebSecurity {
    async fn authenticate_request(
        &self,
        request: &HttpRequest,
        is_state_changing: bool,
    ) -> Result<AuthenticatedTransportContext, actix_web::Error> {
        let opaque_session = request
            .cookie(&self.config.session_cookie_name)
            .ok_or_else(|| {
                log::warn!(
                    target: "hyperchad::shared_state_security",
                    "shared_state_auth_rejected request_id={} reason=missing_session_cookie state_changing={is_state_changing}",
                    request_id(request)
                );
                actix_web::error::ErrorUnauthorized("missing web session")
            })?;
        let csrf_cookie = request
            .cookie(&self.config.csrf_cookie_name)
            .ok_or_else(|| {
                log::warn!(
                    target: "hyperchad::shared_state_security",
                    "shared_state_auth_rejected request_id={} reason=missing_csrf_cookie state_changing={is_state_changing} header_present={} server_cookie_count={} client_source={} client_cookie_count={} client_meta_match={}",
                    request_id(request),
                    request.headers().contains_key(&self.config.csrf_header_name),
                    named_cookie_count(request, &self.config.csrf_cookie_name),
                    request_header(request, CSRF_SOURCE_HEADER),
                    request_header(request, CSRF_COOKIE_COUNT_HEADER),
                    request_header(request, CSRF_META_MATCH_HEADER)
                );
                actix_web::error::ErrorForbidden("missing CSRF cookie")
            })?;

        if is_state_changing {
            let csrf_header = request
                .headers()
                .get(&self.config.csrf_header_name)
                .and_then(|value| value.to_str().ok());
            if csrf_header != Some(csrf_cookie.value()) {
                let reason = if csrf_header.is_some() {
                    "csrf_mismatch"
                } else {
                    "missing_csrf_header"
                };
                log::warn!(
                    target: "hyperchad::shared_state_security",
                    "shared_state_auth_rejected request_id={} reason={reason} state_changing=true header_present={} server_cookie_count={} client_source={} client_cookie_count={} client_meta_match={}",
                    request_id(request),
                    csrf_header.is_some(),
                    named_cookie_count(request, &self.config.csrf_cookie_name),
                    request_header(request, CSRF_SOURCE_HEADER),
                    request_header(request, CSRF_COOKIE_COUNT_HEADER),
                    request_header(request, CSRF_META_MATCH_HEADER)
                );
                return Err(actix_web::error::ErrorForbidden("CSRF validation failed"));
            }
        }

        self.identity_resolver
            .resolve_session(opaque_session.value())
            .await
            .map_err(|error| {
                let reason = match &error {
                    WebSessionIdentityError::Unauthenticated => "invalid_session",
                    WebSessionIdentityError::Forbidden(_) => "forbidden_session",
                    WebSessionIdentityError::Operation(_) => "session_resolution_failed",
                };
                log::warn!(
                    target: "hyperchad::shared_state_security",
                    "shared_state_auth_rejected request_id={} reason={reason} state_changing={is_state_changing}",
                    request_id(request)
                );
                match error {
                    WebSessionIdentityError::Unauthenticated => {
                        actix_web::error::ErrorUnauthorized(error.to_string())
                    }
                    WebSessionIdentityError::Forbidden(_) => {
                        actix_web::error::ErrorForbidden(error.to_string())
                    }
                    WebSessionIdentityError::Operation(_) => {
                        ErrorInternalServerError(error.to_string())
                    }
                }
            })
    }
}

/// Fail-closed resolver for runtimes that have not connected durable web sessions yet.
#[derive(Debug, Default)]
pub struct RejectWebSessionIdentityResolver;

#[async_trait]
impl WebSessionIdentityResolver for RejectWebSessionIdentityResolver {
    async fn resolve_session(
        &self,
        _opaque_session: &str,
    ) -> Result<AuthenticatedTransportContext, WebSessionIdentityError> {
        Err(WebSessionIdentityError::Unauthenticated)
    }
}

struct SharedStateSseSession {
    context: AuthenticatedTransportContext,
    client_tx: flume::Sender<TransportInbound>,
    subscriptions: BTreeMap<ChannelId, flume::Sender<()>>,
}

impl SharedStateSseSession {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    fn new(
        context: AuthenticatedTransportContext,
        client_tx: flume::Sender<TransportInbound>,
    ) -> Self {
        Self {
            context,
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
    web_security: Option<Arc<dyn WebSharedStateSecurity>>,
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
            web_security: None,
            sse_sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    #[must_use]
    pub fn new_with_dispatcher(
        dispatcher: Arc<dyn SharedStateTransportDispatcher>,
        web_security: Arc<dyn WebSharedStateSecurity>,
    ) -> Self {
        let (outbound_tx, _outbound_rx) = flume::unbounded();
        let (_inbound_tx, inbound_rx) = flume::unbounded();

        Self {
            outbound_tx,
            inbound_receiver_factory: Arc::new(move || inbound_rx.clone()),
            dispatcher: Some(dispatcher),
            web_security: Some(web_security),
            sse_sessions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

fn lock_poison_error(context: &str) -> actix_web::Error {
    ErrorInternalServerError(format!("{context}: lock poisoned"))
}

#[allow(clippy::future_not_send)]
async fn resolve_authenticated_context(
    bridge: &SharedStateTransportBridge,
    req: &HttpRequest,
    is_state_changing: bool,
) -> Result<AuthenticatedTransportContext, actix_web::Error> {
    bridge
        .web_security
        .as_ref()
        .ok_or_else(|| ErrorInternalServerError("Missing shared-state web security"))?
        .authenticate_request(req, is_state_changing)
        .await
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
    context: AuthenticatedTransportContext,
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
        if session.context != context {
            return Err(ErrorBadRequest(
                "shared-state session identity does not match authenticated request",
            ));
        }
        session.client_tx = client_tx;
    } else {
        sessions.insert(
            session_id.to_string(),
            Arc::new(Mutex::new(SharedStateSseSession::new(context, client_tx))),
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
    dispatcher: Arc<dyn SharedStateTransportDispatcher>,
    context: AuthenticatedTransportContext,
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
                    let Some(event) = dispatcher.project_event(&context, &event) else {
                        continue;
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

    let context = session
        .lock()
        .map_err(|_| lock_poison_error("sse session lock"))?
        .context
        .clone();
    let event_rx = dispatcher
        .subscribe_channel(&context, &channel_id)
        .await
        .map_err(ErrorInternalServerError)?;
    let (stop_tx, stop_rx) = flume::bounded(1);

    session
        .lock()
        .map_err(|_| lock_poison_error("sse session lock"))?
        .subscriptions
        .insert(channel_id.clone(), stop_tx);

    spawn_sse_subscription_forwarder(session, dispatcher, context, channel_id, event_rx, stop_rx);

    Ok(())
}

fn spawn_ws_subscription_forwarder(
    dispatcher: Arc<dyn SharedStateTransportDispatcher>,
    context: AuthenticatedTransportContext,
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
                    let Some(event) = dispatcher.project_event(&context, &event) else {
                        continue;
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
    context: AuthenticatedTransportContext,
    outbound_tx: flume::Sender<TransportInbound>,
    channel_id: ChannelId,
) -> Result<(), actix_web::Error> {
    if subscriptions.contains_key(&channel_id) {
        return Ok(());
    }

    let event_rx = dispatcher
        .subscribe_channel(&context, &channel_id)
        .await
        .map_err(ErrorInternalServerError)?;
    let (stop_tx, stop_rx) = flume::bounded(1);
    subscriptions.insert(channel_id, stop_tx);

    spawn_ws_subscription_forwarder(dispatcher, context, outbound_tx, event_rx, stop_rx);

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
    context: &AuthenticatedTransportContext,
    outbound: TransportOutbound,
    dispatcher: Arc<dyn SharedStateTransportDispatcher>,
    subscriptions: &mut BTreeMap<ChannelId, flume::Sender<()>>,
    ws_outbound_tx: flume::Sender<TransportInbound>,
) -> Result<(), actix_web::Error> {
    let responses = dispatcher
        .ingest_outbound(context, outbound.clone())
        .await
        .map_err(ErrorInternalServerError)?;

    match outbound {
        TransportOutbound::Subscribe(subscribe) => {
            ensure_ws_subscription(
                subscriptions,
                dispatcher,
                context.clone(),
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

#[allow(clippy::future_not_send, clippy::too_many_lines)]
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
        let operation = transport_operation(&outbound);
        let Some(session_id) = session_id_from_request(&req) else {
            log::warn!(
                target: "hyperchad::shared_state_transport",
                "shared_state_post_rejected request_id={} reason=missing_transport_session operation={operation}",
                request_id(&req)
            );
            return Ok(diagnostic_response(
                StatusCode::BAD_REQUEST,
                &req,
                "missing_transport_session",
            ));
        };
        let Some(session) = lookup_sse_session(shared_state_transport, &session_id)? else {
            log::warn!(
                target: "hyperchad::shared_state_transport",
                "shared_state_post_rejected request_id={} reason=unknown_transport_session operation={operation}",
                request_id(&req)
            );
            return Ok(diagnostic_response(
                StatusCode::CONFLICT,
                &req,
                "unknown_transport_session",
            ));
        };

        let request_context = match resolve_authenticated_context(
            shared_state_transport,
            &req,
            true,
        )
        .await
        {
            Ok(context) => context,
            Err(error) => {
                let status = error.as_response_error().status_code();
                let diagnostic = match status {
                    StatusCode::UNAUTHORIZED => "authentication_rejected",
                    StatusCode::FORBIDDEN => "csrf_rejected",
                    _ => "security_adapter_failed",
                };
                log::warn!(
                    target: "hyperchad::shared_state_transport",
                    "shared_state_post_rejected request_id={} reason={diagnostic} operation={operation} status={}",
                    request_id(&req),
                    status.as_u16()
                );
                return Ok(diagnostic_response(status, &req, diagnostic));
            }
        };
        let context = session
            .lock()
            .map_err(|_| lock_poison_error("sse session lock"))?
            .context
            .clone();
        if context != request_context {
            log::warn!(
                target: "hyperchad::shared_state_transport",
                "shared_state_post_rejected request_id={} reason=transport_identity_mismatch operation={operation}",
                request_id(&req)
            );
            return Ok(diagnostic_response(
                StatusCode::BAD_REQUEST,
                &req,
                "transport_identity_mismatch",
            ));
        }
        let outbound = outbound.0;
        let responses = dispatcher
            .ingest_outbound(&context, outbound.clone())
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
        let context = resolve_authenticated_context(&shared_state_transport, &req, false).await?;
        match upsert_sse_session_stream(&shared_state_transport, &session_id, context) {
            Ok(receiver) => receiver,
            Err(error) if error.as_response_error().status_code() == StatusCode::BAD_REQUEST => {
                log::warn!(
                    target: "hyperchad::shared_state_transport",
                    "shared_state_sse_rejected request_id={} reason=transport_identity_mismatch",
                    request_id(&req)
                );
                return Ok(diagnostic_response(
                    StatusCode::BAD_REQUEST,
                    &req,
                    "transport_identity_mismatch",
                ));
            }
            Err(error) => return Err(error),
        }
    } else {
        (shared_state_transport.inbound_receiver_factory)()
    };

    let stream = inbound_rx.into_stream().map(|inbound| {
        serde_json::to_string(&inbound)
            .map(|payload| Bytes::from(format!("data: {payload}\n\n")))
            .map_err(ErrorInternalServerError)
    });
    let stream = Box::pin(stream);
    let stream = futures_util::stream::unfold(stream, |mut stream| async move {
        let next = stream.next().fuse();
        let heartbeat = actix_web::rt::time::sleep(std::time::Duration::from_secs(20)).fuse();
        pin_mut!(next, heartbeat);
        match select(next, heartbeat).await {
            Either::Left((Some(item), _)) => Some((item, stream)),
            Either::Left((None, _)) => None,
            Either::Right(((), _)) => Some((
                Ok::<_, actix_web::Error>(Bytes::from_static(b": keepalive\n\n")),
                stream,
            )),
        }
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header((actix_web::http::header::CONTENT_ENCODING, "identity"))
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
        let context = resolve_authenticated_context(&shared_state_transport, &req, false).await?;
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
                                    &context,
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
        runtime::{RuntimeFanoutTransportDispatcher, SharedStateEngine},
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
        AuthenticatedTransportContext, CSRF_COOKIE_COUNT_HEADER, CSRF_META_MATCH_HEADER,
        CSRF_SOURCE_HEADER, REQUEST_ID_HEADER, SharedStateTransportDispatcher,
        TRANSPORT_DIAGNOSTIC_HEADER, WebSharedStateSecurity, handle_shared_state_transport_post,
        handle_shared_state_transport_sse,
    };
    use crate::{ActixApp, ActixResponseProcessor};

    #[derive(Debug, Clone)]
    struct TestProcessor;

    #[derive(Debug)]
    struct TestWebSecurity;

    #[async_trait(?Send)]
    impl WebSharedStateSecurity for TestWebSecurity {
        async fn authenticate_request(
            &self,
            request: &HttpRequest,
            is_state_changing: bool,
        ) -> Result<AuthenticatedTransportContext, actix_web::Error> {
            let csrf_cookie = request.cookie("test-csrf");
            let csrf_header = request.headers().get("x-test-csrf");
            if csrf_cookie.as_ref().map(actix_web::cookie::Cookie::value) != Some("csrf-a")
                || (is_state_changing
                    && csrf_header.and_then(|value| value.to_str().ok()) != Some("csrf-a"))
            {
                return Err(actix_web::error::ErrorForbidden("CSRF validation failed"));
            }

            Ok(AuthenticatedTransportContext {
                participant_id: ParticipantId::new("participant-a"),
                identity_binding: "identity-a".to_string(),
            })
        }
    }

    #[derive(Debug)]
    struct TestSessionIdentityResolver;

    #[async_trait]
    impl super::WebSessionIdentityResolver for TestSessionIdentityResolver {
        async fn resolve_session(
            &self,
            opaque_session: &str,
        ) -> Result<AuthenticatedTransportContext, super::WebSessionIdentityError> {
            if opaque_session != "opaque-session" {
                return Err(super::WebSessionIdentityError::Unauthenticated);
            }
            Ok(AuthenticatedTransportContext {
                participant_id: ParticipantId::new("participant-web"),
                identity_binding: "identity-web".to_string(),
            })
        }
    }

    fn test_context() -> AuthenticatedTransportContext {
        AuthenticatedTransportContext {
            participant_id: ParticipantId::new("participant-1"),
            identity_binding: "identity-a".to_string(),
        }
    }

    #[derive(Debug)]
    struct PrivateProjectionPolicy;

    #[async_trait]
    impl hyperchad_shared_state_transport::SharedStateTransportPolicy for PrivateProjectionPolicy {
        async fn authorize_channel(
            &self,
            context: &AuthenticatedTransportContext,
            channel_id: &ChannelId,
            _access: hyperchad_shared_state_transport::ChannelAccess,
        ) -> Result<(), hyperchad_shared_state_transport::TransportAuthorizationError> {
            let expected = format!("private:{}", context.participant_id);
            if channel_id.as_str() == "public-game" || channel_id.as_str() == expected {
                Ok(())
            } else {
                Err(
                    hyperchad_shared_state_transport::TransportAuthorizationError::Denied(
                        "participant cannot access this private channel".to_string(),
                    ),
                )
            }
        }

        fn project_event(
            &self,
            context: &AuthenticatedTransportContext,
            event: &EventEnvelope,
        ) -> Option<EventEnvelope> {
            let expected = format!("private:{}", context.participant_id);
            (event.channel_id.as_str() == "public-game" || event.channel_id.as_str() == expected)
                .then(|| event.clone())
        }

        fn project_snapshot(
            &self,
            context: &AuthenticatedTransportContext,
            snapshot: &SnapshotEnvelope,
        ) -> Option<SnapshotEnvelope> {
            let expected = format!("private:{}", context.participant_id);
            (snapshot.channel_id.as_str() == "public-game"
                || snapshot.channel_id.as_str() == expected)
                .then(|| snapshot.clone())
        }
    }

    fn participant_context(participant: &str) -> AuthenticatedTransportContext {
        AuthenticatedTransportContext {
            participant_id: ParticipantId::new(participant),
            identity_binding: format!("identity-{participant}"),
        }
    }

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

        async fn to_fragment_body(
            &self,
            _fragment: &hyperchad_renderer::ReplaceContainer,
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
    async fn counts_duplicate_named_cookies_without_logging_values() {
        let request = test::TestRequest::get()
            .insert_header((
                actix_web::http::header::COOKIE,
                "custom-csrf=first-secret; other=value; custom-csrf=second-secret",
            ))
            .to_http_request();

        assert_eq!(super::named_cookie_count(&request, "custom-csrf"), 2);
        assert_eq!(super::named_cookie_count(&request, "other"), 1);
    }

    #[actix_web::test]
    async fn cookie_csrf_rejection_does_not_expose_credentials() {
        let security = super::CookieCsrfWebSecurity::new(
            super::CookieCsrfWebSecurityConfig::new(
                "custom-session",
                "custom-csrf",
                "x-custom-csrf",
            ),
            Arc::new(TestSessionIdentityResolver),
        );
        let request = test::TestRequest::post()
            .insert_header((REQUEST_ID_HEADER, "request-42"))
            .insert_header((CSRF_SOURCE_HEADER, "cookie"))
            .insert_header((CSRF_COOKIE_COUNT_HEADER, "2"))
            .insert_header((CSRF_META_MATCH_HEADER, "false"))
            .insert_header(("x-custom-csrf", "header-secret"))
            .cookie(actix_web::cookie::Cookie::new(
                "custom-session",
                "session-secret",
            ))
            .cookie(actix_web::cookie::Cookie::new(
                "custom-csrf",
                "cookie-secret",
            ))
            .to_http_request();

        let error = security
            .authenticate_request(&request, true)
            .await
            .expect_err("mismatched CSRF credentials should fail");
        let response = error.error_response();
        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("error body should read");
        let body = std::str::from_utf8(&body).expect("error body should be UTF-8");

        assert_eq!(
            error.as_response_error().status_code(),
            StatusCode::FORBIDDEN
        );
        for secret in ["header-secret", "cookie-secret", "session-secret"] {
            assert!(!body.contains(secret));
        }
    }

    #[actix_web::test]
    async fn cookie_csrf_web_security_uses_configured_web_names() {
        let security = super::CookieCsrfWebSecurity::new(
            super::CookieCsrfWebSecurityConfig::new(
                "custom-session",
                "custom-csrf",
                "x-custom-csrf",
            ),
            Arc::new(TestSessionIdentityResolver),
        );
        let request = test::TestRequest::post()
            .cookie(actix_web::cookie::Cookie::new(
                "custom-session",
                "opaque-session",
            ))
            .cookie(actix_web::cookie::Cookie::new("custom-csrf", "csrf-value"))
            .insert_header(("x-custom-csrf", "csrf-value"))
            .to_http_request();

        let context = security
            .authenticate_request(&request, true)
            .await
            .expect("configured web authentication should succeed");
        assert_eq!(
            context.participant_id,
            ParticipantId::new("participant-web")
        );

        let invalid = test::TestRequest::post()
            .cookie(actix_web::cookie::Cookie::new(
                "custom-session",
                "opaque-session",
            ))
            .cookie(actix_web::cookie::Cookie::new("custom-csrf", "csrf-value"))
            .insert_header(("x-custom-csrf", "wrong-value"))
            .to_http_request();
        assert!(security.authenticate_request(&invalid, true).await.is_err());
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
        let dispatcher = RuntimeFanoutTransportDispatcher::new(
            engine,
            fanout.clone(),
            Arc::new(hyperchad_shared_state_transport::AllowAllSharedStateTransportPolicy),
        );

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
            .ingest_outbound(&test_context(), TransportOutbound::Command(command.clone()))
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
            .ingest_outbound(
                &test_context(),
                TransportOutbound::Subscribe(TransportSubscribe {
                    channel_id: ChannelId::new("channel-a"),
                    last_seen_revision: None,
                }),
            )
            .await
            .expect("subscribe replay should succeed");
        assert_eq!(replay_result.len(), 1);
        assert!(matches!(replay_result[0], TransportInbound::Event(_)));

        let receiver = dispatcher
            .subscribe_channel(&test_context(), &ChannelId::new("channel-a"))
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
            .ingest_outbound(
                &test_context(),
                TransportOutbound::Command(channel_b_command),
            )
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
            .ingest_outbound(
                &test_context(),
                TransportOutbound::Command(channel_a_command),
            )
            .await
            .expect("channel-a command should succeed");

        let forwarded = receiver
            .recv_async()
            .await
            .expect("channel-a subscriber should receive event");
        assert_eq!(forwarded.channel_id, ChannelId::new("channel-a"));
    }

    #[actix_web::test]
    async fn authenticated_clients_receive_public_and_only_their_private_replay() {
        let store = Arc::new(MemoryStore::default());
        let fanout = Arc::new(InProcessFanoutBus::new());
        let engine = Arc::new(SharedStateEngine::new(
            store.clone(),
            store.clone(),
            store.clone(),
            fanout.clone(),
        ));
        let dispatcher = RuntimeFanoutTransportDispatcher::new(
            engine,
            fanout,
            Arc::new(PrivateProjectionPolicy),
        );
        let alice = participant_context("alice");
        let bob = participant_context("bob");

        for (channel, participant, command_id) in [
            ("public-game", "alice", "public-command"),
            ("private:alice", "alice", "alice-command"),
            ("private:bob", "bob", "bob-command"),
        ] {
            let command = CommandEnvelope {
                command_id: CommandId::new(command_id),
                channel_id: ChannelId::new(channel),
                participant_id: ParticipantId::new(participant),
                idempotency_key: IdempotencyKey::new(format!("idem-{command_id}")),
                expected_revision: Revision::new(0),
                command_name: "VALUE_CHANGED".to_string(),
                payload: PayloadBlob::from_serializable(&channel)
                    .expect("payload should serialize"),
                metadata: BTreeMap::new(),
                created_at_ms: 1,
            };
            let context = participant_context(participant);
            dispatcher
                .ingest_outbound(&context, TransportOutbound::Command(command))
                .await
                .expect("authorized command should apply");
        }

        for context in [&alice, &bob] {
            let public = dispatcher
                .ingest_outbound(
                    context,
                    TransportOutbound::Subscribe(TransportSubscribe {
                        channel_id: ChannelId::new("public-game"),
                        last_seen_revision: Some(Revision::new(0)),
                    }),
                )
                .await
                .expect("public replay should be authorized");
            assert_eq!(public.len(), 1);
        }

        let alice_private = dispatcher
            .ingest_outbound(
                &alice,
                TransportOutbound::Subscribe(TransportSubscribe {
                    channel_id: ChannelId::new("private:alice"),
                    last_seen_revision: Some(Revision::new(0)),
                }),
            )
            .await
            .expect("Alice private replay should be authorized");
        assert_eq!(alice_private.len(), 1);

        let bob_private = dispatcher
            .ingest_outbound(
                &bob,
                TransportOutbound::Subscribe(TransportSubscribe {
                    channel_id: ChannelId::new("private:bob"),
                    last_seen_revision: Some(Revision::new(0)),
                }),
            )
            .await
            .expect("Bob private replay should be authorized");
        assert_eq!(bob_private.len(), 1);

        assert!(
            dispatcher
                .ingest_outbound(
                    &bob,
                    TransportOutbound::Subscribe(TransportSubscribe {
                        channel_id: ChannelId::new("private:alice"),
                        last_seen_revision: Some(Revision::new(0)),
                    }),
                )
                .await
                .is_err(),
            "Bob must not replay Alice's private channel"
        );
        assert!(
            dispatcher
                .ingest_outbound(
                    &alice,
                    TransportOutbound::Subscribe(TransportSubscribe {
                        channel_id: ChannelId::new("private:bob"),
                        last_seen_revision: Some(Revision::new(0)),
                    }),
                )
                .await
                .is_err(),
            "Alice must not replay Bob's private channel"
        );
    }

    #[actix_web::test]
    async fn reconnect_and_multiple_tabs_replay_without_cross_participant_leakage() {
        let store = Arc::new(MemoryStore::default());
        let fanout = Arc::new(InProcessFanoutBus::new());
        let engine = Arc::new(SharedStateEngine::new(
            store.clone(),
            store.clone(),
            store.clone(),
            fanout.clone(),
        ));
        let dispatcher = RuntimeFanoutTransportDispatcher::new(
            engine,
            fanout,
            Arc::new(PrivateProjectionPolicy),
        );
        let alice = participant_context("alice");
        let bob = participant_context("bob");

        let alice_tab_one = dispatcher
            .subscribe_channel(&alice, &ChannelId::new("private:alice"))
            .await
            .expect("first Alice tab should subscribe");
        let alice_tab_two = dispatcher
            .subscribe_channel(&alice, &ChannelId::new("private:alice"))
            .await
            .expect("second Alice tab should subscribe");
        assert!(
            dispatcher
                .subscribe_channel(&bob, &ChannelId::new("private:alice"))
                .await
                .is_err(),
            "Bob must not subscribe to Alice's private channel"
        );

        let command = CommandEnvelope {
            command_id: CommandId::new("alice-private-command"),
            channel_id: ChannelId::new("private:alice"),
            participant_id: ParticipantId::new("alice"),
            idempotency_key: IdempotencyKey::new("alice-private-idem"),
            expected_revision: Revision::new(0),
            command_name: "PRIVATE_VALUE_CHANGED".to_string(),
            payload: PayloadBlob::from_serializable(&"alice-secret")
                .expect("payload should serialize"),
            metadata: BTreeMap::new(),
            created_at_ms: 1,
        };
        dispatcher
            .ingest_outbound(&alice, TransportOutbound::Command(command))
            .await
            .expect("authorized private command should apply");

        for receiver in [&alice_tab_one, &alice_tab_two] {
            let event = receiver
                .recv_async()
                .await
                .expect("each Alice tab should receive private fanout");
            assert_eq!(event.channel_id, ChannelId::new("private:alice"));
        }
        drop(alice_tab_one);

        let replay = dispatcher
            .ingest_outbound(
                &alice,
                TransportOutbound::Subscribe(TransportSubscribe {
                    channel_id: ChannelId::new("private:alice"),
                    last_seen_revision: Some(Revision::new(0)),
                }),
            )
            .await
            .expect("reconnected Alice tab should replay missed state");
        assert_eq!(replay.len(), 1);
        assert!(matches!(replay[0], TransportInbound::Event(_)));

        let duplicate = CommandEnvelope {
            command_id: CommandId::new("alice-private-command-duplicate"),
            channel_id: ChannelId::new("private:alice"),
            participant_id: ParticipantId::new("alice"),
            idempotency_key: IdempotencyKey::new("alice-private-idem"),
            expected_revision: Revision::new(1),
            command_name: "PRIVATE_VALUE_CHANGED".to_string(),
            payload: PayloadBlob::from_serializable(&"different")
                .expect("payload should serialize"),
            metadata: BTreeMap::new(),
            created_at_ms: 2,
        };
        let duplicate_result = dispatcher
            .ingest_outbound(&alice, TransportOutbound::Command(duplicate))
            .await
            .expect("duplicate idempotency result should be transportable");
        assert!(matches!(
            duplicate_result.as_slice(),
            [TransportInbound::CommandAccepted {
                resulting_revision,
                ..
            }] if *resulting_revision == Revision::new(1)
        ));

        let stale = CommandEnvelope {
            command_id: CommandId::new("alice-stale-command"),
            channel_id: ChannelId::new("private:alice"),
            participant_id: ParticipantId::new("alice"),
            idempotency_key: IdempotencyKey::new("alice-stale-idem"),
            expected_revision: Revision::new(0),
            command_name: "PRIVATE_VALUE_CHANGED".to_string(),
            payload: PayloadBlob::from_serializable(&"stale").expect("payload should serialize"),
            metadata: BTreeMap::new(),
            created_at_ms: 3,
        };
        let conflict = dispatcher
            .ingest_outbound(&alice, TransportOutbound::Command(stale))
            .await
            .expect("revision conflict should be transportable");
        assert!(matches!(
            conflict.as_slice(),
            [TransportInbound::CommandRejected { reason, .. }]
                if reason.contains("Expected revision 0") && reason.contains("actual revision is 1")
        ));
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

        assert_eq!(
            response
                .headers()
                .get("content-encoding")
                .and_then(|x| x.to_str().ok()),
            Some("identity")
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
                _context: &AuthenticatedTransportContext,
                _outbound: TransportOutbound,
            ) -> hyperchad_shared_state_transport::SharedStateTransportDispatchResult<
                Vec<TransportInbound>,
            > {
                Ok(Vec::new())
            }

            async fn subscribe_channel(
                &self,
                _context: &AuthenticatedTransportContext,
                _channel_id: &ChannelId,
            ) -> hyperchad_shared_state_transport::SharedStateTransportDispatchResult<
                flume::Receiver<EventEnvelope>,
            > {
                let (_tx, rx) = flume::unbounded();
                Ok(rx)
            }

            fn project_event(
                &self,
                _context: &AuthenticatedTransportContext,
                event: &EventEnvelope,
            ) -> Option<EventEnvelope> {
                Some(event.clone())
            }
        }

        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let app = ActixApp::new(TestProcessor, renderer_event_rx)
            .with_shared_state_transport_dispatcher(
                Arc::new(TestDispatcher),
                Arc::new(TestWebSecurity),
            );

        let response = handle_shared_state_transport_post(
            test::TestRequest::post()
                .insert_header((REQUEST_ID_HEADER, "request-42"))
                .to_http_request(),
            web::Data::new(app),
            web::Json(TransportOutbound::Ping(TransportPing { sent_at_ms: 1 })),
        )
        .await
        .expect("missing transport session should return a diagnostic response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(TRANSPORT_DIAGNOSTIC_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("missing_transport_session")
        );
        assert_eq!(
            response
                .headers()
                .get(REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("request-42")
        );
    }

    #[actix_web::test]
    async fn dispatcher_post_accepts_session_cookie() {
        #[derive(Debug)]
        struct TestDispatcher;

        #[async_trait]
        impl SharedStateTransportDispatcher for TestDispatcher {
            async fn ingest_outbound(
                &self,
                _context: &AuthenticatedTransportContext,
                _outbound: TransportOutbound,
            ) -> hyperchad_shared_state_transport::SharedStateTransportDispatchResult<
                Vec<TransportInbound>,
            > {
                Ok(Vec::new())
            }

            async fn subscribe_channel(
                &self,
                _context: &AuthenticatedTransportContext,
                _channel_id: &ChannelId,
            ) -> hyperchad_shared_state_transport::SharedStateTransportDispatchResult<
                flume::Receiver<EventEnvelope>,
            > {
                let (_tx, rx) = flume::unbounded();
                Ok(rx)
            }

            fn project_event(
                &self,
                _context: &AuthenticatedTransportContext,
                event: &EventEnvelope,
            ) -> Option<EventEnvelope> {
                Some(event.clone())
            }
        }

        let (_renderer_event_tx, renderer_event_rx) = flume::unbounded::<RendererEvent>();
        let app = ActixApp::new(TestProcessor, renderer_event_rx)
            .with_shared_state_transport_dispatcher(
                Arc::new(TestDispatcher),
                Arc::new(TestWebSecurity),
            );

        let session_cookie =
            actix_web::cookie::Cookie::new("v-shared-state-session-id", "session-cookie-1");
        let csrf_cookie = actix_web::cookie::Cookie::new("test-csrf", "csrf-a");

        let sse_request = test::TestRequest::get()
            .cookie(session_cookie.clone())
            .cookie(csrf_cookie.clone())
            .to_http_request();
        let sse_response =
            handle_shared_state_transport_sse(sse_request, web::Data::new(app.clone()))
                .await
                .expect("sse handler should succeed with cookie session");
        assert_eq!(sse_response.status(), StatusCode::OK);

        let post_request = test::TestRequest::post()
            .cookie(session_cookie)
            .cookie(csrf_cookie)
            .insert_header(("x-test-csrf", "csrf-a"))
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
