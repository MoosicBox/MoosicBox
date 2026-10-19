//! Native, in-process route orchestration (not an HTTP transport).
//!
//! B owns one `Routing` for the lifetime of its facade. Run the returned
//! `RouteWorker::run` future on the existing runtime, never in paint. Drain with
//! `try_take` after traversal, dispatch `terminal_events` through G/H, then call
//! `complete` to apply. Before dispatch uses `prepare` -> `before_event` -> `start`.
//! `submit`/`try_complete` are convenience paths without lifecycle delivery. No
//! HTTP status/header fields: `Router::navigate` returns content, not HTTP.
//!
//! App continues to own its initial route and `navigate_send` consumer. Submit only
//! renderer-local routes here, never both here and through App's navigation channel.
//! `Applied.response` must be handed back to App's response consumer exactly once:
//! App owns internal redirects and external opening. Its existing private helper
//! cannot consume metadata directly: B must install one response-effects consumer
//! accepting `ResponseMetadata` (not re-render the already-applied view). Cookies
//! require a host store; the existing App helper does not implement one.
//! Do not feed that navigation back into this coordinator as well.
//! Title/events have no per-response representation in View: retain B's configured
//! title; H/G own authored action events.
//! Cookies are rejected by default; explicitly selecting `HostManaged` obligates B
//! to supply a real cookie store and process every returned cookie before redirect.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::{Future, poll_fn},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
};

use hyperchad_renderer::{Content, ResponseNavigation, View};
use hyperchad_router::{RequestInfo, RouteRequest, Router};
use hyperchad_transformer::models::{Selector, SwapStrategy};
pub use switchy_http::models::Method;

use crate::{
    document::{
        Document, DocumentError, ElementKey, ElementStamp, MutationSummary, RequestTicket,
        ViewOrigin,
    },
    widgets::forms::{FormSubmission, FormValue},
};

/// Request causality for native lifecycle events. Initial navigation is App-owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Route,
    Load,
    /// Geometry-gated automatic route, rearmed on failure re-entry or activation.
    Revealed,
    Form,
    Navigation,
}

/// Cookie effects require a real host store, never a pretend browser cookie jar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookiePolicy {
    Reject,
    HostManaged,
}

/// External links are returned to B for opening outside paint, never executed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalPolicy {
    Reject,
    HttpAndHttps,
}

/// A validated link decision. Internal URLs are absolute application paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Local(String),
    External(String),
}

/// Complete native request; query pairs and body are retained without logging secrets.
///
/// Headers are case-normalized at admission. Repeated GET query keys cannot be
/// represented by Router's map API and are explicitly rejected, not overwritten.
#[derive(Clone)]
pub struct RequestContext {
    /// Headers/cookies and query/body data are not exposed in lifecycle payloads.
    /// `source` is the form stamp; `submitter` is the selected button captured by J.
    pub source: ElementStamp,
    pub submitter: Option<ElementStamp>,
    pub scope: String,
    pub trigger: Trigger,
    pub path: String,
    pub method: Method,
    pub query: Vec<(String, String)>,
    pub headers: BTreeMap<String, String>,
    pub cookies: BTreeMap<String, String>,
    pub info: RequestInfo,
    pub body: Option<Arc<[u8]>>,
    pub target: Selector,
    pub swap: SwapStrategy,
}

impl RequestContext {
    /// Build a local GET, splitting and decoding its ordered query pairs.
    /// # Errors
    /// * Rejects non-local URLs, malformed escapes or non-UTF8 query values.
    pub fn get(
        source: ElementStamp,
        url: &str,
        target: Selector,
        swap: SwapStrategy,
    ) -> Result<Self, RoutingError> {
        validate_local(url)?;
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        let query = query
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (name, value) = part.split_once('=').unwrap_or((part, ""));
                Ok((decode(name)?, decode(value)?))
            })
            .collect::<Result<_, RoutingError>>()?;
        Ok(Self {
            source,
            submitter: None,
            scope: "route".into(),
            trigger: Trigger::Route,
            path: path.into(),
            method: Method::Get,
            query,
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: RequestInfo::default(),
            body: None,
            target,
            swap,
        })
    }

    /// Adapt the actual transformer route model. B supplies the event name from G.
    /// Only matching triggers are admitted; unsupported trigger syntax is explicit.
    /// # Errors
    /// * Invalid URLs or unsupported authored trigger expressions.
    pub fn route(
        source: ElementStamp,
        route: &hyperchad_transformer::models::Route,
        event: &str,
    ) -> Result<Option<Self>, RoutingError> {
        use hyperchad_transformer::models::Route;
        let (url, trigger, target, strategy, method) = match route {
            Route::Get {
                route,
                trigger,
                target,
                strategy,
            } => (route, trigger, target, strategy, Method::Get),
            Route::Post {
                route,
                trigger,
                target,
                strategy,
            } => (route, trigger, target, strategy, Method::Post),
            Route::Put {
                route,
                trigger,
                target,
                strategy,
            } => (route, trigger, target, strategy, Method::Put),
            Route::Delete {
                route,
                trigger,
                target,
                strategy,
            } => (route, trigger, target, strategy, Method::Delete),
            Route::Patch {
                route,
                trigger,
                target,
                strategy,
            } => (route, trigger, target, strategy, Method::Patch),
        };
        let trigger = trigger.as_deref().unwrap_or("click");
        if !matches!(
            trigger,
            "click" | "change" | "input" | "submit" | "load" | "revealed"
        ) {
            return Err(RoutingError::UnsupportedTrigger);
        }
        if trigger != event {
            return Ok(None);
        }
        let mut request = Self::get(source, url, target.clone(), strategy.clone())?;
        request.method = method;
        if trigger == "load" {
            request.trigger = Trigger::Load;
        } else if trigger == "revealed" {
            request.trigger = Trigger::Revealed;
            request.scope = "revealed".into();
        }
        Ok(Some(request))
    }

    /// Apply J's captured successful controls, including only the selected submitter.
    /// `current_url` is used for an omitted/empty action (never silently `/`).
    /// For non-GET forms, action query pairs remain query and controls become body.
    /// # Errors
    /// * Invalid action/method, unsupported GET files, unsafe MIME metadata, or a
    ///   file exceeding `max_bytes` (including total encoded request size).
    pub fn form(
        submission: FormSubmission,
        current_url: &str,
        target: Selector,
        swap: SwapStrategy,
        max_bytes: usize,
    ) -> Result<Self, RoutingError> {
        let action = submission
            .action
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(current_url);
        let mut request = Self::get(submission.form, action, target, swap)?;
        request.scope = "form".into();
        request.trigger = Trigger::Form;
        request.submitter = submission.submitter;
        validate_form_size(&submission, max_bytes)?;
        request.method = submission
            .method
            .as_deref()
            .unwrap_or("get")
            .to_ascii_uppercase()
            .parse()
            .map_err(|_| RoutingError::UnsupportedMethod)?;
        validate_method(request.method)?;
        if request.method == Method::Get || request.method == Method::Head {
            // Browser GET submission replaces the action's query, preserving order.
            request.query = form_query(submission.entries)?;
        } else if submission.multipart {
            let boundary = multipart_boundary(&submission.entries);
            let mut body = Vec::new();
            for entry in submission.entries {
                let name = disposition(&entry.name)?;
                append(
                    &mut body,
                    format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"")
                        .as_bytes(),
                    max_bytes,
                )?;
                match entry.value {
                    FormValue::Text(text) => {
                        append(&mut body, b"\r\n\r\n", max_bytes)?;
                        append(&mut body, text.as_bytes(), max_bytes)?;
                    }
                    FormValue::File(file) => {
                        let name = disposition(&file.name)?;
                        let media_type = file
                            .media_type
                            .as_deref()
                            .unwrap_or("application/octet-stream");
                        if !valid_media_type(media_type) {
                            return Err(RoutingError::UnsafeHeader);
                        }
                        append(
                            &mut body,
                            format!("; filename=\"{name}\"\r\nContent-Type: {media_type}\r\n\r\n")
                                .as_bytes(),
                            max_bytes,
                        )?;
                        append(&mut body, &file.bytes, max_bytes)?;
                    }
                }
                append(&mut body, b"\r\n", max_bytes)?;
            }
            append(
                &mut body,
                format!("--{boundary}--\r\n").as_bytes(),
                max_bytes,
            )?;
            request.headers.insert(
                "content-type".into(),
                format!("multipart/form-data; boundary={boundary}"),
            );
            request.body = Some(body.into());
        } else {
            request.body = Some(urlencoded_body(submission.entries, max_bytes)?.into());
            request.headers.insert(
                "content-type".into(),
                "application/x-www-form-urlencoded".into(),
            );
        }
        if request_size(&request) > max_bytes {
            return Err(RoutingError::TooLarge);
        }
        Ok(request)
    }
}

/// Admission/terminal failures have no fabricated transport status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutingError {
    InvalidCapacity,
    QueueFull,
    WorkerStopped,
    IdentityExhausted,
    DuplicateSubmit,
    AlreadyLoaded,
    InvalidUrl,
    UnsupportedMethod,
    UnsupportedTrigger,
    RepeatedQueryKey,
    MalformedEncoding,
    GetFileUnsupported,
    UnsafeHeader,
    TooLarge,
    Cancelled,
    Stale,
    CookieStoreRequired,
    UnsupportedContent,
    Handler(String),
    Document(DocumentError),
}
impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RoutingError {}

/// Stable identity for B/G correlation. Monotonically allocated; never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequestId(pub u64);

struct Cancellation {
    cancelled: AtomicBool,
    signal: flume::Sender<()>,
}
impl Cancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.signal.try_send(());
    }
}
struct Job {
    id: RequestId,
    request: RouteRequest,
    cancellation: Arc<Cancellation>,
    signal: flume::Receiver<()>,
    start: flume::Receiver<()>,
}
struct ResultMessage {
    id: RequestId,
    result: Result<Option<Content>, RoutingError>,
}
struct Pending {
    context: RequestContext,
    ticket: RequestTicket,
    cancellation: Arc<Cancellation>,
    taken: bool,
    start: flume::Sender<()>,
}

/// A queued result held by B while it dispatches lifecycle events before swapping.
///
/// It retains its coordinator slot/ticket until `Routing::complete`.
/// Cancellation releases the ticket but the completion must still be consumed.
/// Do not discard it: call `complete` even when lifecycle handlers change the tree.
pub struct PreparedCompletion {
    message: ResultMessage,
    cancellation: Arc<Cancellation>,
}

impl PreparedCompletion {
    /// Whether the successful native response contains an authored primary view.
    ///
    /// This is response shape, not permission to commit navigation. Capture before
    /// `complete`; update the current URL only when that completion returns `Applied`
    /// and its context trigger is `Navigation`. Stale/cancelled/failed responses,
    /// redirects and HEAD metadata must never commit a URL based on this flag alone.
    #[must_use]
    pub fn has_primary(&self) -> bool {
        matches!(&self.message.result, Ok(Some(Content::View(view))) if view.primary.is_some())
    }
}

impl std::fmt::Debug for PreparedCompletion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedCompletion")
            .field("id", &self.message.id)
            .finish_non_exhaustive()
    }
}

/// Terminal lifecycle: emit native after/success/error/abort, not HTTP events with
/// invented status. `Applied.response` effects are host-owned, exactly once.
pub struct Completion {
    pub id: RequestId,
    pub context: RequestContext,
    pub outcome: Result<NativeOutcome, RoutingError>,
}
pub enum NativeOutcome {
    NoContent,
    Applied(MutationSummary),
    Redirected(hyperchad_renderer::ResponseMetadata),
    Metadata(hyperchad_renderer::ResponseMetadata),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AutomaticPhase {
    Ready,
    Pending,
    Failed,
    Done,
}

struct AutomaticRoute {
    source: ElementStamp,
    route: hyperchad_transformer::models::Route,
    near: bool,
    phase: AutomaticPhase,
    chain: u64,
}

#[derive(Default)]
struct AutomaticChain {
    admissions: u8,
    motion: Option<(eframe::egui::Rect, Vec<(ElementKey, eframe::egui::Vec2)>)>,
}

/// UI-owned admission and stale-result gate. Every queue is bounded by capacity;
/// cancelled jobs retain a slot until drained, so rapid replacement cannot grow it.
pub struct Routing {
    requests: flume::Sender<Job>,
    results: flume::Receiver<ResultMessage>,
    pending: BTreeMap<RequestId, Pending>,
    loaded: BTreeMap<(ElementKey, String), u64>,
    automatic: BTreeMap<ElementKey, AutomaticRoute>,
    chains: BTreeMap<u64, AutomaticChain>,
    next_chain: u64,
    next: u64,
    capacity: usize,
    max_bytes: usize,
    pub cookie_policy: CookiePolicy,
    pub external_policy: ExternalPolicy,
}

impl Routing {
    /// Collect automatic routes after layout, never during widget traversal.
    /// Hidden load nodes remain eligible; revealed nodes require resolved geometry.
    pub(crate) fn automatic_requests(
        &mut self,
        document: &Document,
        geometry: &crate::geometry::GeometrySnapshot<ElementKey>,
        viewport: eframe::egui::Rect,
    ) -> Vec<RequestContext> {
        self.automatic.retain(|_, entry| {
            document.is_current(entry.source)
                && document
                    .get(entry.source.key)
                    .and_then(|node| node.route.as_ref())
                    == Some(&entry.route)
        });
        let live: BTreeSet<_> = self.automatic.values().map(|entry| entry.chain).collect();
        self.chains.retain(|id, _| live.contains(id));
        let mut requests = Vec::new();
        for key in document.keys() {
            let Some(route) = document.get(key).and_then(|node| node.route.as_ref()) else {
                continue;
            };
            let trigger = match route {
                hyperchad_transformer::models::Route::Get { trigger, .. }
                | hyperchad_transformer::models::Route::Post { trigger, .. }
                | hyperchad_transformer::models::Route::Put { trigger, .. }
                | hyperchad_transformer::models::Route::Delete { trigger, .. }
                | hyperchad_transformer::models::Route::Patch { trigger, .. } => trigger.as_deref(),
            };
            let Some(event @ ("load" | "revealed")) = trigger else {
                continue;
            };
            let Some(source) = document.stamp(key) else {
                continue;
            };
            if !self.automatic.contains_key(&key) {
                let Some(chain) = self.next_chain.checked_add(1) else {
                    break;
                };
                self.next_chain = chain;
                self.automatic.insert(
                    key,
                    AutomaticRoute {
                        source,
                        route: route.clone(),
                        near: false,
                        phase: AutomaticPhase::Ready,
                        chain,
                    },
                );
            }
            let entry = self.automatic.get_mut(&key).unwrap();
            let chain = self.chains.entry(entry.chain).or_default();
            let near = if event == "load" {
                true
            } else if let Some(node) = geometry.node(key) {
                let mut offsets = Vec::new();
                let mut visible = node.visible;
                let mut parent = node.parent;
                while let Some(ancestor) = parent.and_then(|key| geometry.node(key)) {
                    visible &= ancestor.visible;
                    if ancestor.scroll_offset != eframe::egui::Vec2::ZERO {
                        offsets.push((ancestor.key, ancestor.scroll_offset));
                    }
                    parent = ancestor.parent;
                }
                let motion = (viewport, offsets);
                if chain.motion.as_ref() != Some(&motion) {
                    chain.admissions = 0;
                    chain.motion = Some(motion);
                }
                let clip = node.paint_clip;
                let margin = (clip.height() / 2.0).min(128.0);
                visible
                    && clip.is_positive()
                    && node.border_box.is_positive()
                    && node
                        .border_box
                        .intersects(clip.expand2(eframe::egui::vec2(0.0, margin)))
            } else {
                false
            };
            let entered = near && !entry.near;
            entry.near = near;
            if near
                && chain.admissions < 4
                && (entry.phase == AutomaticPhase::Ready
                    || (event == "revealed" && entered && entry.phase == AutomaticPhase::Failed))
                && let Ok(Some(request)) = RequestContext::route(source, route, event)
            {
                requests.push(request);
            }
        }
        requests
    }

    pub(crate) fn automatic_admission_failed(&mut self, source: ElementStamp) {
        if let Some(entry) = self.automatic.get_mut(&source.key)
            && entry.source == source
            && entry.phase == AutomaticPhase::Ready
        {
            entry.phase = AutomaticPhase::Failed;
        }
    }

    pub(crate) fn retry_revealed(&mut self, source: ElementStamp) {
        if let Some(entry) = self.automatic.get_mut(&source.key)
            && entry.source == source
            && matches!(entry.phase, AutomaticPhase::Failed | AutomaticPhase::Ready)
        {
            entry.phase = AutomaticPhase::Ready;
            if let Some(chain) = self.chains.get_mut(&entry.chain) {
                chain.admissions = 0;
            }
        }
    }

    fn finish_automatic(
        &mut self,
        document: &Document,
        context: &RequestContext,
        outcome: &Result<NativeOutcome, RoutingError>,
    ) {
        if !matches!(context.trigger, Trigger::Load | Trigger::Revealed) {
            return;
        }
        let Some(entry) = self
            .automatic
            .get_mut(&context.source.key)
            .filter(|entry| entry.source == context.source)
        else {
            return;
        };
        entry.phase = if outcome.is_ok() {
            AutomaticPhase::Done
        } else {
            AutomaticPhase::Failed
        };
        let chain = entry.chain;
        if outcome.is_err() && context.trigger == Trigger::Load {
            self.loaded
                .remove(&(context.source.key, context.scope.clone()));
        }
        if let Ok(NativeOutcome::Applied(summary)) = outcome {
            // Only response-inserted automatic nodes inherit continuation allowance.
            // Unrelated existing sources retain independent budgets.
            for stamp in summary.inserted.iter().chain(&summary.changed) {
                let Some(route) = document.get(stamp.key).and_then(|node| node.route.as_ref())
                else {
                    continue;
                };
                if RequestContext::route(*stamp, route, "revealed")
                    .ok()
                    .flatten()
                    .is_some()
                    || RequestContext::route(*stamp, route, "load")
                        .ok()
                        .flatten()
                        .is_some()
                {
                    self.automatic.insert(
                        stamp.key,
                        AutomaticRoute {
                            source: *stamp,
                            route: route.clone(),
                            near: false,
                            phase: AutomaticPhase::Ready,
                            chain,
                        },
                    );
                }
            }
        }
    }

    /// Create queues; the worker must be driven on B's existing runtime.
    /// `wake` must only schedule a UI repaint, not execute application code.
    /// # Errors
    /// * Zero capacity or byte limit is invalid.
    pub fn new(
        capacity: usize,
        max_bytes: usize,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(Self, RouteWorker), RoutingError> {
        if capacity == 0 || max_bytes == 0 {
            return Err(RoutingError::InvalidCapacity);
        }
        let (requests, incoming) = flume::bounded(capacity);
        let (outgoing, results) = flume::bounded(capacity);
        Ok((
            Self {
                requests,
                results,
                pending: BTreeMap::new(),
                loaded: BTreeMap::new(),
                automatic: BTreeMap::new(),
                chains: BTreeMap::new(),
                next_chain: 0,
                next: 0,
                capacity,
                max_bytes,
                cookie_policy: CookiePolicy::Reject,
                external_policy: ExternalPolicy::Reject,
            },
            RouteWorker {
                incoming,
                outgoing,
                wake,
            },
        ))
    }

    /// Classify a link without running handlers, opening a browser or making I/O.
    /// # Errors
    /// * External URLs are opt-in; unsupported schemes/credentials/controls reject.
    pub fn link(&self, url: &str) -> Result<Link, RoutingError> {
        if url.starts_with('/') {
            validate_local(url)?;
            return Ok(Link::Local(url.into()));
        }
        if self.external_policy != ExternalPolicy::HttpAndHttps {
            return Err(RoutingError::InvalidUrl);
        }
        validate_external(url)?;
        Ok(Link::External(url.into()))
    }

    /// Admit a captured request after traversal. Forms reject duplicate pending
    /// submissions; other same-source/scope requests replace older work. A single
    /// FIFO worker preserves draft-change -> submit execution order across scopes.
    /// Load fires once per source generation/scope after successful admission.
    /// # Errors
    /// * Full/disconnected queues, invalid inputs, duplicate forms/loads, stale source
    ///   or exhausted identity. Rejection never cancels an existing request.
    pub fn submit(
        &mut self,
        document: &mut Document,
        context: RequestContext,
    ) -> Result<RequestId, RoutingError> {
        let id = self.prepare(document, context)?;
        let _ = self.start(id);
        Ok(id)
    }

    fn revealed_blocked(&self, context: &RequestContext) -> bool {
        matches!(context.trigger, Trigger::Load | Trigger::Revealed)
            && self
                .automatic
                .get(&context.source.key)
                .is_some_and(|entry| {
                    entry.source == context.source
                        && (matches!(entry.phase, AutomaticPhase::Pending | AutomaticPhase::Done)
                            || self
                                .chains
                                .get(&entry.chain)
                                .is_some_and(|chain| chain.admissions >= 4))
                })
            || (context.trigger == Trigger::Revealed
                && self.pending.values().any(|pending| {
                    pending.context.source == context.source
                        && pending.context.trigger == Trigger::Revealed
                }))
    }

    fn mark_automatic_pending(&mut self, context: &RequestContext) {
        if matches!(context.trigger, Trigger::Load | Trigger::Revealed)
            && let Some(entry) = self.automatic.get_mut(&context.source.key)
            && entry.source == context.source
        {
            entry.phase = AutomaticPhase::Pending;
            let chain = self.chains.entry(entry.chain).or_default();
            chain.admissions = chain.admissions.saturating_add(1);
        }
    }

    /// Reserve FIFO work without permitting handler execution. B dispatches
    /// `before_event(id)` through G/H, then calls `start(id)` (or `cancel`).
    /// This makes before-request ordering deterministic even on a multithread runtime.
    /// # Errors
    /// * Same validation/admission errors as `submit`.
    pub fn prepare(
        &mut self,
        document: &mut Document,
        context: RequestContext,
    ) -> Result<RequestId, RoutingError> {
        if context
            .submitter
            .is_some_and(|stamp| !document.is_current(stamp))
        {
            return Err(RoutingError::Stale);
        }
        if !document.is_current(context.source) {
            return Err(RoutingError::Stale);
        }
        if self.pending.len() >= self.capacity {
            return Err(RoutingError::QueueFull);
        }
        if self.revealed_blocked(&context) {
            return Err(RoutingError::AlreadyLoaded);
        }
        let request = route_request(&context, self.max_bytes)?;
        if self.cookie_policy == CookiePolicy::Reject && !context.cookies.is_empty() {
            return Err(RoutingError::CookieStoreRequired);
        }
        let same_form = self.pending.values().any(|p| {
            p.context.source.key == context.source.key && p.context.trigger == Trigger::Form
        });
        if context.trigger == Trigger::Form && same_form {
            return Err(RoutingError::DuplicateSubmit);
        }
        self.loaded.retain(|(key, _), generation| {
            document.is_current(ElementStamp {
                key: *key,
                generation: *generation,
            })
        });
        let load_key = (context.source.key, context.scope.clone());
        if context.trigger == Trigger::Load
            && self.loaded.get(&load_key) == Some(&context.source.generation)
        {
            return Err(RoutingError::AlreadyLoaded);
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or(RoutingError::IdentityExhausted)?;
        // Queue cannot be full: each queued/in-flight/completed job retains pending.
        // Do not mint the ticket until the worker-disconnection check passes.
        if self.requests.is_disconnected() {
            return Err(RoutingError::WorkerStopped);
        }
        let ticket = document
            .begin_request(
                context.source.key,
                context.scope.clone(),
                context.target.clone(),
                context.swap.clone(),
            )
            .map_err(RoutingError::Document)?;
        let (start_sender, start) = flume::bounded(1);
        let (signal_sender, signal) = flume::bounded(1);
        let cancellation = Arc::new(Cancellation {
            cancelled: AtomicBool::new(false),
            signal: signal_sender,
        });
        let id = RequestId(next);
        if self
            .requests
            .try_send(Job {
                id,
                request,
                cancellation: cancellation.clone(),
                signal,
                start,
            })
            .is_err()
        {
            document.cancel_request(&ticket);
            return Err(RoutingError::WorkerStopped);
        }
        for pending in self.pending.values() {
            if context.trigger == Trigger::Navigation
                || (pending.context.source.key == context.source.key
                    && pending.context.scope == context.scope)
            {
                pending.cancellation.cancel();
            }
        }
        if context.trigger == Trigger::Load {
            self.loaded.insert(load_key, context.source.generation);
        }
        self.mark_automatic_pending(&context);
        self.next = next;
        self.pending.insert(
            id,
            Pending {
                context,
                ticket,
                cancellation,
                taken: false,
                start: start_sender,
            },
        );
        Ok(id)
    }

    /// Release one prepared request after G/H before-event delivery. A prepared
    /// request must be started or cancelled; it intentionally holds FIFO order.
    #[must_use]
    pub fn start(&self, id: RequestId) -> bool {
        self.pending
            .get(&id)
            .is_some_and(|pending| pending.start.try_send(()).is_ok())
    }

    /// Explicit cancellation is terminal when drained; dropping handler futures
    /// cannot undo side effects already performed by a handler.
    pub fn cancel(&mut self, document: &mut Document, id: RequestId) -> bool {
        let Some(pending) = self.pending.get(&id) else {
            return false;
        };
        pending.cancellation.cancel();
        document.cancel_request(&pending.ticket);
        true
    }

    /// Cancel exactly the completion whose lifecycle delivery failed.
    ///
    /// Checks coordinator authority as well as ID; foreign or consumed completions
    /// return false without changing any request. The host must still consume the
    /// owned value with `complete`, which returns `Cancelled` and releases its slot
    /// without applying document or response effects. Repeated cancellation is safe.
    pub fn cancel_prepared(
        &mut self,
        document: &mut Document,
        prepared: &PreparedCompletion,
    ) -> bool {
        let Some(pending) = self.pending.get(&prepared.message.id) else {
            return false;
        };
        if !Arc::ptr_eq(&pending.cancellation, &prepared.cancellation) {
            return false;
        }
        pending.cancellation.cancel();
        document.cancel_request(&pending.ticket);
        true
    }

    /// Reconcile after every D mutation before admitting frame work. Source/target
    /// changes promptly cancel in-flight futures instead of waiting for a result.
    pub fn reconcile(&mut self, document: &mut Document) {
        for pending in self.pending.values() {
            if !document.request_is_current(&pending.ticket) {
                pending.cancellation.cancel();
                document.cancel_request(&pending.ticket);
            }
        }
        self.loaded.retain(|(key, _), generation| {
            document.is_current(ElementStamp {
                key: *key,
                generation: *generation,
            })
        });
    }

    /// Release document authority at facade teardown before dropping this owner.
    /// This is cooperative cancellation, not rollback of completed handler effects.
    pub fn shutdown(&mut self, document: &mut Document) {
        for pending in self.pending.values() {
            pending.cancellation.cancel();
            document.cancel_request(&pending.ticket);
        }
        self.loaded.clear();
    }

    /// Nonblocking convenience for hosts without G lifecycle dispatch.
    pub fn try_complete(&mut self, document: &mut Document) -> Option<Completion> {
        let prepared = self.try_take()?;
        self.complete(document, prepared)
    }

    /// Drain without applying. B calls `terminal_events`, routes those through G/H,
    /// then calls `complete` (which revalidates after event-induced mutations).
    /// The outstanding result still occupies a bounded admission slot.
    pub fn try_take(&mut self) -> Option<PreparedCompletion> {
        let message = match self.results.try_recv() {
            Ok(message) => message,
            Err(flume::TryRecvError::Disconnected) => {
                let id = self
                    .pending
                    .iter()
                    .find(|(_, pending)| !pending.taken)
                    .map(|(id, _)| *id)?;
                ResultMessage {
                    id,
                    result: Err(RoutingError::WorkerStopped),
                }
            }
            Err(flume::TryRecvError::Empty) => return None,
        };
        let pending = self.pending.get_mut(&message.id)?;
        pending.taken = true;
        Some(PreparedCompletion {
            message,
            cancellation: pending.cancellation.clone(),
        })
    }

    /// G's before envelope, only for successfully admitted requests. Never exposes
    /// bodies, form fields, cookies, credentials, or synthetic HTTP metadata.
    #[allow(clippy::must_use_candidate)] // Option is already must_use; repository convention.
    pub fn before_event(&self, id: RequestId) -> Option<crate::events::InputEvent> {
        let pending = self.pending.get(&id)?;
        Some(lifecycle_event(
            id,
            &pending.context,
            crate::events::RequestPhase::Before,
        ))
    }

    /// Native transport terminal envelopes, dispatched BEFORE document mutation.
    /// Detached sources receive no events. Invalid/superseded responses are aborts;
    /// policy/handler failures are errors. After is emitted exactly once by B.
    #[must_use]
    pub fn terminal_events(
        &self,
        document: &Document,
        prepared: &PreparedCompletion,
    ) -> Vec<crate::events::InputEvent> {
        use crate::events::RequestPhase;
        let Some(pending) = self.pending.get(&prepared.message.id) else {
            return Vec::new();
        };
        if !Arc::ptr_eq(&pending.cancellation, &prepared.cancellation) {
            return Vec::new();
        }
        if !document.is_current(pending.context.source) {
            return Vec::new();
        }
        let phase = if pending.cancellation.cancelled.load(Ordering::Acquire)
            || !document.request_is_current(&pending.ticket)
        {
            RequestPhase::Abort
        } else {
            match &prepared.message.result {
                Ok(Some(Content::View(view))) if self.validate_response(view).is_ok() => {
                    RequestPhase::Success
                }
                Ok(None) => RequestPhase::Success,
                Err(RoutingError::Cancelled | RoutingError::Stale) => RequestPhase::Abort,
                _ => RequestPhase::Error,
            }
        };
        vec![
            lifecycle_event(prepared.message.id, &pending.context, RequestPhase::After),
            lifecycle_event(prepared.message.id, &pending.context, phase),
        ]
    }

    /// Apply once after lifecycle delivery. All response effects are behind the
    /// document ticket gate. Metadata effects are returned, never executed.
    pub fn complete(
        &mut self,
        document: &mut Document,
        prepared: PreparedCompletion,
    ) -> Option<Completion> {
        if !Arc::ptr_eq(
            &self.pending.get(&prepared.message.id)?.cancellation,
            &prepared.cancellation,
        ) {
            return None;
        }
        let message = prepared.message;
        let pending = self.pending.remove(&message.id)?;
        let outcome = if pending.cancellation.cancelled.load(Ordering::Acquire) {
            Err(RoutingError::Cancelled)
        } else if !document.request_is_current(&pending.ticket) {
            Err(RoutingError::Stale)
        } else {
            message.result.and_then(|content| match content {
                None => Ok(NativeOutcome::NoContent),
                Some(Content::View(view)) if pending.context.method == Method::Head => {
                    self.validate_response(&view)?;
                    if view.response.navigation.is_some() {
                        Ok(NativeOutcome::Redirected(view.response))
                    } else if !view.response.cookies.is_empty() {
                        Ok(NativeOutcome::Metadata(view.response))
                    } else {
                        Ok(NativeOutcome::NoContent)
                    }
                }
                Some(Content::View(view)) => self.apply(document, &pending, *view),
                Some(_) => Err(RoutingError::UnsupportedContent),
            })
        };
        self.finish_automatic(document, &pending.context, &outcome);
        document.cancel_request(&pending.ticket);
        Some(Completion {
            id: message.id,
            context: pending.context,
            outcome,
        })
    }

    fn validate_response(&self, view: &View) -> Result<(), RoutingError> {
        if self.cookie_policy == CookiePolicy::Reject && !view.response.cookies.is_empty() {
            return Err(RoutingError::CookieStoreRequired);
        }
        match &view.response.navigation {
            Some(ResponseNavigation::Internal(path)) => validate_local(path)?,
            Some(ResponseNavigation::External(url)) => {
                if self.external_policy == ExternalPolicy::Reject {
                    return Err(RoutingError::InvalidUrl);
                }
                validate_external(url)?;
            }
            None => {}
        }
        Ok(())
    }

    fn apply(
        &self,
        document: &mut Document,
        pending: &Pending,
        view: View,
    ) -> Result<NativeOutcome, RoutingError> {
        self.validate_response(&view)?;
        // Match App: redirects do not also render their primary/fragments.
        if view.response.navigation.is_some() {
            return Ok(NativeOutcome::Redirected(view.response));
        }
        let summary = if pending.context.trigger == Trigger::Navigation {
            document.apply_view(view, ViewOrigin::Navigation)
        } else {
            document.complete_request(&pending.ticket, view)
        }
        .map_err(RoutingError::Document)?;
        Ok(NativeOutcome::Applied(summary))
    }
}
impl Drop for Routing {
    fn drop(&mut self) {
        for pending in self.pending.values() {
            pending.cancellation.cancel();
        }
    }
}

/// One production in-process Router adapter. No network/client/provider is used.
/// Sequential execution deliberately preserves draft/save/submit side-effect order.
pub struct RouteWorker {
    incoming: flume::Receiver<Job>,
    outgoing: flume::Sender<ResultMessage>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl RouteWorker {
    /// Drive outside paint. Cancellation drops the native handler future promptly.
    /// Stops when Routing is dropped; no thread/task is spawned by this component.
    pub async fn run(self, router: Router) {
        while let Ok(job) = self.incoming.recv_async().await {
            let mut future = Box::pin(router.navigate(job.request));
            let mut signal = Box::pin(job.signal.recv_async());
            let mut start = Box::pin(job.start.recv_async());
            let mut started = false;
            let result = poll_fn(|cx| {
                if signal.as_mut().poll(cx).is_ready()
                    || job.cancellation.cancelled.load(Ordering::Acquire)
                {
                    return Poll::Ready(Err(RoutingError::Cancelled));
                }
                if !started {
                    match start.as_mut().poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Ok(())) => started = true,
                        Poll::Ready(Err(_)) => return Poll::Ready(Err(RoutingError::Cancelled)),
                    }
                }
                future
                    .as_mut()
                    .poll(cx)
                    .map(|result| result.map_err(|error| RoutingError::Handler(error.to_string())))
            })
            .await;
            if self
                .outgoing
                .send_async(ResultMessage { id: job.id, result })
                .await
                .is_err()
            {
                break;
            }
            (self.wake)();
        }
    }
}

fn lifecycle_event(
    id: RequestId,
    context: &RequestContext,
    phase: crate::events::RequestPhase,
) -> crate::events::InputEvent {
    use crate::events::{EventValue, InputEvent};
    let fields = BTreeMap::from([
        (
            "transport".into(),
            EventValue::String("native-router".into()),
        ),
        ("requestId".into(), EventValue::String(id.0.to_string())),
        (
            "method".into(),
            EventValue::String(context.method.to_string()),
        ),
        // Path only: no query/body/headers that could expose form secrets.
        ("path".into(), EventValue::String(context.path.clone())),
    ]);
    InputEvent::Request {
        source: context.source,
        phase,
        value: EventValue::Object(fields),
    }
}

fn route_request(context: &RequestContext, max_bytes: usize) -> Result<RouteRequest, RoutingError> {
    validate_local(&context.path)?;
    if context.path.contains('?') {
        return Err(RoutingError::InvalidUrl);
    }
    validate_method(context.method)?;
    if request_size(context) > max_bytes {
        return Err(RoutingError::TooLarge);
    }
    let mut names = BTreeSet::new();
    for (key, _) in &context.query {
        if !names.insert(key) {
            return Err(RoutingError::RepeatedQueryKey);
        }
    }
    let mut headers = BTreeMap::new();
    for (name, value) in &context.headers {
        if name.is_empty()
            || !name.bytes().all(header_token)
            || value.bytes().any(|b| b < 32 || b == 127)
        {
            return Err(RoutingError::UnsafeHeader);
        }
        if headers
            .insert(name.to_ascii_lowercase(), value.clone())
            .is_some()
        {
            return Err(RoutingError::UnsafeHeader);
        }
    }
    // Native identity is RequestInfo, never spoofed Host/forwarded headers.
    if headers.keys().any(|name| {
        matches!(
            name.as_str(),
            "host"
                | "connection"
                | "transfer-encoding"
                | "content-length"
                | "cookie"
                | "set-cookie"
        ) || name.starts_with("x-forwarded-")
    }) {
        return Err(RoutingError::UnsafeHeader);
    }
    Ok(RouteRequest {
        path: context.path.clone(),
        method: context.method,
        query: context.query.iter().cloned().collect(),
        headers,
        cookies: context.cookies.clone(),
        info: context.info.clone(),
        body: context
            .body
            .as_ref()
            .map(|body| Arc::new(body.to_vec().into())),
    })
}
const fn validate_method(method: Method) -> Result<(), RoutingError> {
    match method {
        Method::Get
        | Method::Post
        | Method::Put
        | Method::Delete
        | Method::Patch
        | Method::Head
        | Method::Options => Ok(()),
        _ => Err(RoutingError::UnsupportedMethod),
    }
}
fn validate_local(url: &str) -> Result<(), RoutingError> {
    if !url.starts_with('/')
        || url.chars().any(char::is_control)
        || url.starts_with("//")
        || url.contains(['\\', '#'])
        || url.bytes().any(|b| b <= 32 || b == 127)
    {
        return Err(RoutingError::InvalidUrl);
    }
    let path = url.split('?').next().unwrap_or(url);
    let decoded = decode(path)?;
    if decoded.chars().any(char::is_control)
        || decoded.starts_with("//")
        || decoded.contains(['\\', '#', '?'])
        || decoded.bytes().any(|b| b < 32 || b == 127)
        || decoded.split('/').any(|s| matches!(s, "." | ".."))
    {
        return Err(RoutingError::InvalidUrl);
    }
    Ok(())
}
fn validate_external(url: &str) -> Result<(), RoutingError> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or(RoutingError::InvalidUrl)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty()
        || authority.contains(['@', '%', '\\'])
        || !authority.is_ascii()
        || authority
            .bytes()
            .any(|b| !b.is_ascii_alphanumeric() && !b".:-[]".contains(&b))
        || url.bytes().any(|b| b <= 32 || b == 127)
        || url.contains('\\')
    {
        return Err(RoutingError::InvalidUrl);
    }
    ResponseNavigation::external(url).map_err(|_| RoutingError::InvalidUrl)?;
    Ok(())
}
fn request_size(context: &RequestContext) -> usize {
    let mut size = context
        .path
        .len()
        .saturating_add(context.scope.len())
        .saturating_add(context.body.as_ref().map_or(0, |body| body.len()));
    for (key, value) in context
        .query
        .iter()
        .map(|(key, value)| (key, value))
        .chain(context.headers.iter())
        .chain(context.cookies.iter())
    {
        size = size
            .saturating_add(key.len())
            .saturating_add(value.len())
            .saturating_add(2);
    }
    size
}
fn multipart_boundary(entries: &[crate::widgets::forms::FormEntry]) -> String {
    // Deterministic boundary, collision-checked against all entry data.
    let mut boundary = "hyperchad-native-form".to_owned();
    while entries.iter().any(|entry| {
        entry.name.contains(&boundary)
            || match &entry.value {
                FormValue::Text(text) => text.contains(&boundary),
                FormValue::File(file) => {
                    file.name.contains(&boundary)
                        || file
                            .bytes
                            .windows(boundary.len())
                            .any(|s| s == boundary.as_bytes())
                }
            }
    }) {
        boundary.push('x');
    }
    boundary
}

fn form_query(
    entries: Vec<crate::widgets::forms::FormEntry>,
) -> Result<Vec<(String, String)>, RoutingError> {
    entries
        .into_iter()
        .map(|entry| match entry.value {
            FormValue::Text(value) => Ok((entry.name, value)),
            FormValue::File(_) => Err(RoutingError::GetFileUnsupported),
        })
        .collect()
}

fn validate_form_size(submission: &FormSubmission, max_bytes: usize) -> Result<(), RoutingError> {
    let raw_size = submission.entries.iter().fold(0usize, |size, entry| {
        let value = match &entry.value {
            FormValue::Text(value) => value.len(),
            FormValue::File(file) => file
                .bytes
                .len()
                .saturating_add(file.name.len())
                .saturating_add(file.media_type.as_ref().map_or(0, String::len)),
        };
        size.saturating_add(entry.name.len()).saturating_add(value)
    });
    if raw_size > max_bytes {
        return Err(RoutingError::TooLarge);
    }
    Ok(())
}

fn urlencoded_body(
    entries: Vec<crate::widgets::forms::FormEntry>,
    max_bytes: usize,
) -> Result<Vec<u8>, RoutingError> {
    let mut body = Vec::new();
    for (i, entry) in entries.into_iter().enumerate() {
        if i > 0 {
            append(&mut body, b"&", max_bytes)?;
        }
        let value = match entry.value {
            FormValue::Text(value) => value,
            FormValue::File(file) => file.name,
        };
        append(
            &mut body,
            format!("{}={}", encode(&entry.name), encode(&value)).as_bytes(),
            max_bytes,
        )?;
    }
    Ok(body)
}

fn append(body: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), RoutingError> {
    if body.len().saturating_add(bytes.len()) > limit {
        return Err(RoutingError::TooLarge);
    }
    body.extend_from_slice(bytes);
    Ok(())
}
fn disposition(value: &str) -> Result<String, RoutingError> {
    if value.bytes().any(|b| b < 32 || b == 127) {
        return Err(RoutingError::UnsafeHeader);
    }
    Ok(value.replace('\\', "\\\\").replace('"', "\\\""))
}
fn header_token(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}
fn valid_media_type(value: &str) -> bool {
    value.split_once('/').is_some_and(|(a, b)| {
        !a.is_empty() && !b.is_empty() && a.bytes().all(header_token) && b.bytes().all(header_token)
    })
}
fn encode(value: &str) -> String {
    let mut encoded = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"*-._".contains(&b) {
            encoded.push(char::from(b));
        } else if b == b' ' {
            encoded.push('+');
        } else {
            use std::fmt::Write as _;
            write!(encoded, "%{b:02X}").expect("String write");
        }
    }
    encoded
}
fn decode(value: &str) -> Result<String, RoutingError> {
    let mut bytes = value.bytes();
    let mut decoded = Vec::new();
    while let Some(b) = bytes.next() {
        decoded.push(match b {
            b'+' => b' ',
            b'%' => {
                let a = bytes
                    .next()
                    .and_then(|b| char::from(b).to_digit(16))
                    .ok_or(RoutingError::MalformedEncoding)?;
                let b = bytes
                    .next()
                    .and_then(|b| char::from(b).to_digit(16))
                    .ok_or(RoutingError::MalformedEncoding)?;
                u8::try_from(a * 16 + b).map_err(|_| RoutingError::MalformedEncoding)?
            }
            b => b,
        });
    }
    String::from_utf8(decoded).map_err(|_| RoutingError::MalformedEncoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::forms::{FormEntry, FormFile};
    use hyperchad_transformer::Container;
    use std::task::{Context, Waker};

    fn pump(future: std::pin::Pin<&mut impl Future<Output = ()>>) {
        assert!(
            future
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    fn fixture() -> (Document, ElementStamp) {
        let mut document = Document::default();
        document
            .apply_view(
                View::from(Container {
                    id: 1,
                    str_id: Some("root".into()),
                    ..Container::default()
                }),
                ViewOrigin::Navigation,
            )
            .unwrap();
        let key = document.select(&Selector::Id("root".into()), None)[0];
        let stamp = document.stamp(key).unwrap();
        (document, stamp)
    }
    fn request(source: ElementStamp) -> RequestContext {
        RequestContext::get(
            source,
            "/local?q=hello+world",
            Selector::SelfTarget,
            SwapStrategy::Children,
        )
        .unwrap()
    }
    fn coordinator() -> (Routing, RouteWorker) {
        Routing::new(8, 64 * 1024, Arc::new(|| {})).unwrap()
    }
    fn entry(name: &str, value: &str) -> FormEntry {
        FormEntry {
            name: name.into(),
            value: FormValue::Text(value.into()),
        }
    }
    fn form(source: ElementStamp, method: &str, multipart: bool) -> FormSubmission {
        FormSubmission {
            form: source,
            submitter: Some(source),
            action: Some("/local?action=yes".into()),
            method: Some(method.into()),
            multipart,
            entries: vec![
                entry("tag", "a b"),
                entry("tag", "c&d"),
                entry("selected", "save"),
            ],
        }
    }

    #[test]
    fn all_methods_reach_real_router_with_request_context_and_wake() {
        for method in [
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Delete,
            Method::Patch,
            Method::Head,
            Method::Options,
        ] {
            let (mut document, source) = fixture();
            let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counter = wakes.clone();
            let (mut routing, worker) = Routing::new(
                2,
                1024,
                Arc::new(move || {
                    counter.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap();
            routing.cookie_policy = CookiePolicy::HostManaged;
            let called = Arc::new(AtomicBool::new(false));
            let flag = called.clone();
            let router = Router::new().with_route("/local", move |req: RouteRequest| {
                let flag = flag.clone();
                async move {
                    assert_eq!(req.method, method);
                    assert_eq!(req.query.get("q").unwrap(), "hello world");
                    assert_eq!(req.headers.get("x-test").unwrap(), "value");
                    assert_eq!(req.cookies.get("session").unwrap(), "native");
                    assert_eq!(req.info.client.os.name, "fixture");
                    assert_eq!(req.body.as_ref().unwrap().as_ref().as_ref(), b"payload");
                    flag.store(true, Ordering::Release);
                    View::from(Container::from(vec![Container {
                        str_id: Some("result".into()),
                        ..Container::default()
                    }]))
                }
            });
            let mut work = Box::pin(worker.run(router.clone()));
            let mut context = request(source);
            context.method = method;
            context.headers.insert("X-Test".into(), "value".into());
            context.cookies.insert("session".into(), "native".into());
            context.info.client = Arc::new(hyperchad_router::ClientInfo {
                os: hyperchad_router::ClientOs {
                    name: "fixture".into(),
                },
            });
            context.body = Some(Arc::from(&b"payload"[..]));
            let id = routing.submit(&mut document, context).unwrap();
            pump(work.as_mut());
            assert!(called.load(Ordering::Acquire));
            assert_eq!(wakes.load(Ordering::Relaxed), 1);
            let completion = routing.try_complete(&mut document).unwrap();
            assert_eq!(completion.id, id);
            if method == Method::Head {
                assert!(matches!(completion.outcome, Ok(NativeOutcome::NoContent)));
                assert_eq!(document.select(&Selector::Id("result".into()), None), []);
            } else {
                assert!(matches!(completion.outcome, Ok(NativeOutcome::Applied(_))));
                assert_eq!(
                    document.select(&Selector::Id("result".into()), None).len(),
                    1
                );
            }
            // Local dispatch must NOT also publish into App's navigation consumer.
            let mut app_channel = Box::pin(router.wait_for_navigation());
            assert!(
                app_channel
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
    }

    #[test]
    fn form_urlencoding_and_selected_submitter_preserve_repeated_keys() {
        let (_, source) = fixture();
        let context = RequestContext::form(
            form(source, "pOsT", false),
            "/current",
            Selector::SelfTarget,
            SwapStrategy::Children,
            4096,
        )
        .unwrap();
        assert_eq!(context.method, Method::Post);
        assert_eq!(context.submitter, Some(source));
        assert_eq!(context.query, [("action".into(), "yes".into())]);
        assert_eq!(
            context.body.as_deref().unwrap(),
            b"tag=a+b&tag=c%26d&selected=save"
        );
        assert_eq!(
            context.headers["content-type"],
            "application/x-www-form-urlencoded"
        );
        let mut get = context;
        get.method = Method::Get;
        get.query = vec![("tag".into(), "a".into()), ("tag".into(), "b".into())];
        assert!(matches!(
            route_request(&get, 4096),
            Err(RoutingError::RepeatedQueryKey)
        ));
    }

    #[test]
    fn real_router_receives_multipart_file_and_repeated_entries() {
        let (mut document, source) = fixture();
        let mut submission = form(source, "post", true);
        submission.entries.push(FormEntry {
            name: "upload".into(),
            value: FormValue::File(FormFile {
                name: "test.bin".into(),
                media_type: Some("application/octet-stream".into()),
                bytes: Arc::from(&b"\0\xffhyperchad-native-form"[..]),
            }),
        });
        let context = RequestContext::form(
            submission,
            "/current",
            Selector::SelfTarget,
            SwapStrategy::None,
            4096,
        )
        .unwrap();
        let expected = context.body.clone().unwrap();
        let content_type = context.headers["content-type"].clone();
        assert!(content_type.ends_with("boundary=hyperchad-native-formx"));
        let called = Arc::new(AtomicBool::new(false));
        let flag = called.clone();
        let router = Router::new().with_route("/local", move |req: RouteRequest| {
            let expected = expected.clone();
            let content_type = content_type.clone();
            let flag = flag.clone();
            async move {
                assert_eq!(req.content_type(), Some(content_type.as_str()));
                assert_eq!(
                    req.body.as_ref().unwrap().as_ref().as_ref(),
                    expected.as_ref()
                );
                let body = String::from_utf8_lossy(&expected);
                assert_eq!(body.matches("name=\"tag\"").count(), 2);
                assert!(body.contains("filename=\"test.bin\""));
                assert!(body.contains("name=\"selected\"\r\n\r\nsave"));
                flag.store(true, Ordering::Release);
                View::default()
            }
        });
        let (mut routing, worker) = coordinator();
        routing.submit(&mut document, context).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        assert!(called.load(Ordering::Acquire));
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Ok(NativeOutcome::Applied(_))
        ));
    }

    #[test]
    fn queued_cancellation_skips_handler_and_does_not_cancel_new_ticket() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = calls.clone();
        let router = Router::new().with_route("/local", move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
            async { View::default() }
        });
        let first = routing.submit(&mut document, request(source)).unwrap();
        let second = routing.submit(&mut document, request(source)).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        let completion = routing.try_complete(&mut document).unwrap();
        assert_eq!(completion.id, first);
        assert!(matches!(completion.outcome, Err(RoutingError::Cancelled)));
        let completion = routing.try_complete(&mut document).unwrap();
        assert_eq!(completion.id, second);
        assert!(matches!(completion.outcome, Ok(NativeOutcome::Applied(_))));
        // Router::navigate selects/invokes the handler only when its future polls.
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn cancellation_drops_pending_handler_and_retains_bounded_slot_until_drain() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = Routing::new(1, 1024, Arc::new(|| {})).unwrap();
        let router = Router::new().with_route("/local", |_| async {
            std::future::pending::<()>().await;
            View::default()
        });
        let id = routing.submit(&mut document, request(source)).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        assert!(routing.cancel(&mut document, id));
        assert_eq!(
            routing.submit(&mut document, request(source)),
            Err(RoutingError::QueueFull)
        );
        pump(work.as_mut());
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Err(RoutingError::Cancelled)
        ));
        assert!(routing.submit(&mut document, request(source)).is_ok());
    }

    #[test]
    fn removed_source_discards_all_response_effects() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let router = Router::new().with_route("/local", |_| async {
            let mut view = View::from(Container {
                str_id: Some("must-not-appear".into()),
                ..Container::default()
            });
            view.response.navigation =
                Some(ResponseNavigation::Internal("/must-not-follow".into()));
            view
        });
        routing.submit(&mut document, request(source)).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        document
            .apply_view(View::from(Container::default()), ViewOrigin::Navigation)
            .unwrap();
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Err(RoutingError::Stale)
        ));
        assert_eq!(
            document.select(&Selector::Id("must-not-appear".into()), None),
            []
        );
    }

    #[test]
    fn draft_then_submit_fifo_and_duplicate_submit_rejection() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = seen.clone();
        let router = Router::new().with_route("/local", move |req: RouteRequest| {
            captured.lock().unwrap().push(req.method);
            async { View::default() }
        });
        let mut draft = request(source);
        draft.scope = "draft".into();
        draft.method = Method::Patch;
        routing.submit(&mut document, draft).unwrap();
        let form = RequestContext::form(
            form(source, "post", false),
            "/current",
            Selector::SelfTarget,
            SwapStrategy::None,
            4096,
        )
        .unwrap();
        routing.submit(&mut document, form.clone()).unwrap();
        assert_eq!(
            routing.submit(&mut document, form),
            Err(RoutingError::DuplicateSubmit)
        );
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        assert_eq!(*seen.lock().unwrap(), [Method::Patch, Method::Post]);
    }

    fn automatic_node(id: usize, y: f32, trigger: &str) -> Container {
        Container {
            id,
            calculated_x: Some(0.0),
            calculated_y: Some(y),
            calculated_width: Some(100.0),
            calculated_height: Some(20.0),
            route: Some(hyperchad_transformer::models::Route::Get {
                route: "/automatic".into(),
                trigger: Some(trigger.into()),
                target: Selector::SelfTarget,
                strategy: SwapStrategy::This,
            }),
            ..Container::default()
        }
    }

    fn automatic_view(node: Container) -> View {
        View::from(Container {
            id: 100,
            calculated_x: Some(0.0),
            calculated_y: Some(0.0),
            calculated_width: Some(400.0),
            calculated_height: Some(1200.0),
            children: vec![node],
            ..Container::default()
        })
    }

    fn automatic_geometry(document: &Document) -> crate::geometry::GeometrySnapshot<ElementKey> {
        crate::geometry::GeometrySnapshot::build_roots(
            document.roots(),
            crate::geometry::GeometryViewport {
                rect: eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(400.0, 300.0),
                ),
                zoom: 1.0,
            },
            0,
            &BTreeMap::new(),
            |_, node| {
                document
                    .keys()
                    .into_iter()
                    .find(|key| document.get(*key).unwrap().id == node.id)
            },
        )
        .unwrap()
    }

    fn automatic_frame(routing: &mut Routing, document: &Document) -> Vec<RequestContext> {
        routing.automatic_requests(
            document,
            &automatic_geometry(document),
            eframe::egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(400.0, 300.0),
            ),
        )
    }

    #[test]
    fn automatic_hidden_load_not_starved_and_offscreen_reveal_not_drained() {
        let mut hidden = automatic_node(2, 0.0, "load");
        hidden.hidden = Some(true);
        let mut offscreen = automatic_node(1, 800.0, "revealed");
        offscreen.children.push(hidden);
        let mut document = Document::default();
        document
            .apply_view(automatic_view(offscreen), ViewOrigin::Navigation)
            .unwrap();
        let (mut routing, _worker) = coordinator();
        let requests = automatic_frame(&mut routing, &document);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].trigger, Trigger::Load);
        routing
            .submit(&mut document, requests.into_iter().next().unwrap())
            .unwrap();
        assert!(automatic_frame(&mut routing, &document).is_empty());
    }

    #[test]
    fn automatic_geometry_failure_reentry_and_stale_replacement() {
        let mut document = Document::default();
        document
            .apply_view(
                automatic_view(automatic_node(1, 10.0, "revealed")),
                ViewOrigin::Navigation,
            )
            .unwrap();
        let (mut routing, worker) = coordinator();
        let router = Router::new().with_route_result("/automatic", |_| async {
            Err::<View, _>(std::io::Error::other("temporary"))
        });
        let mut work = Box::pin(worker.run(router));
        let request = automatic_frame(&mut routing, &document).pop().unwrap();
        routing.submit(&mut document, request).unwrap();
        assert!(automatic_frame(&mut routing, &document).is_empty());
        pump(work.as_mut());
        assert!(
            routing
                .try_complete(&mut document)
                .unwrap()
                .outcome
                .is_err()
        );
        assert!(
            automatic_frame(&mut routing, &document).is_empty(),
            "stationary failure must not loop"
        );
        let mut away = document.roots().to_vec();
        away[0].children[0].calculated_y = Some(1000.0);
        let viewport = eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::vec2(400.0, 300.0),
        );
        let geometry = crate::geometry::GeometrySnapshot::build_roots(
            &away,
            crate::geometry::GeometryViewport {
                rect: viewport,
                zoom: 1.0,
            },
            0,
            &BTreeMap::new(),
            |_, node| {
                document
                    .keys()
                    .into_iter()
                    .find(|key| document.get(*key).unwrap().id == node.id)
            },
        )
        .unwrap();
        assert!(
            routing
                .automatic_requests(&document, &geometry, viewport)
                .is_empty()
        );
        // Re-enter with the same source lifetime; only geometry has changed.
        let retry = automatic_frame(&mut routing, &document).pop().unwrap();
        routing.submit(&mut document, retry).unwrap();
        document
            .apply_view(
                automatic_view(automatic_node(1, 10.0, "revealed")),
                ViewOrigin::Navigation,
            )
            .unwrap();
        routing.reconcile(&mut document);
        pump(work.as_mut());
        assert!(
            routing
                .try_complete(&mut document)
                .unwrap()
                .outcome
                .is_err()
        );
        assert_eq!(
            automatic_frame(&mut routing, &document).len(),
            1,
            "replacement owns a fresh lifetime"
        );
    }

    #[test]
    fn automatic_replacement_chain_is_bounded_and_activation_rearms() {
        let mut document = Document::default();
        document
            .apply_view(
                automatic_view(automatic_node(1, 10.0, "revealed")),
                ViewOrigin::Navigation,
            )
            .unwrap();
        let (mut routing, worker) = coordinator();
        let router = Router::new().with_route("/automatic", |_| async {
            automatic_view(automatic_node(2, 10.0, "revealed"))
        });
        let mut work = Box::pin(worker.run(router));
        for _ in 0..4 {
            let request = automatic_frame(&mut routing, &document)
                .pop()
                .expect("allowed continuation");
            routing.submit(&mut document, request).unwrap();
            pump(work.as_mut());
            assert!(routing.try_complete(&mut document).unwrap().outcome.is_ok());
        }
        assert!(
            automatic_frame(&mut routing, &document).is_empty(),
            "no fifth stationary successor"
        );
        let source = document
            .keys()
            .into_iter()
            .find_map(|key| {
                document
                    .get(key)
                    .unwrap()
                    .route
                    .as_ref()
                    .map(|_| document.stamp(key).unwrap())
            })
            .unwrap();
        routing.retry_revealed(source);
        assert_eq!(automatic_frame(&mut routing, &document).len(), 1);
    }

    #[test]
    fn revealed_parser_pending_dedupe_and_explicit_retry() {
        use hyperchad_transformer::models::Route;
        let (mut document, source) = fixture();
        let (mut routing, _worker) = coordinator();
        let route = Route::Get {
            route: "/local".into(),
            trigger: Some("revealed".into()),
            target: Selector::SelfTarget,
            strategy: SwapStrategy::This,
        };
        assert!(
            RequestContext::route(source, &route, "load")
                .unwrap()
                .is_none()
        );
        let request = RequestContext::route(source, &route, "revealed")
            .unwrap()
            .unwrap();
        assert_eq!(request.trigger, Trigger::Revealed);
        routing.automatic.insert(
            source.key,
            super::AutomaticRoute {
                source,
                route,
                near: true,
                phase: super::AutomaticPhase::Ready,
                chain: 1,
            },
        );
        routing.submit(&mut document, request.clone()).unwrap();
        assert_eq!(
            routing.submit(&mut document, request),
            Err(RoutingError::AlreadyLoaded)
        );
        routing.retry_revealed(source);
        assert!(matches!(
            routing.automatic[&source.key].phase,
            super::AutomaticPhase::Pending
        ));
        routing.automatic.get_mut(&source.key).unwrap().phase = super::AutomaticPhase::Failed;
        routing.chains.get_mut(&1).unwrap().admissions = 4;
        routing.retry_revealed(source);
        assert!(matches!(
            routing.automatic[&source.key].phase,
            super::AutomaticPhase::Ready
        ));
        assert_eq!(routing.chains[&1].admissions, 0);
    }

    #[test]
    fn authored_load_trigger_is_once_per_generation_and_unsupported_is_not_noop() {
        use hyperchad_transformer::models::Route;
        let (mut document, source) = fixture();
        let (mut routing, _worker) = coordinator();
        let route = Route::Get {
            route: "/local".into(),
            trigger: Some("load".into()),
            target: Selector::SelfTarget,
            strategy: SwapStrategy::Children,
        };
        assert!(
            RequestContext::route(source, &route, "click")
                .unwrap()
                .is_none()
        );
        let context = RequestContext::route(source, &route, "load")
            .unwrap()
            .unwrap();
        routing.submit(&mut document, context.clone()).unwrap();
        assert_eq!(
            routing.submit(&mut document, context),
            Err(RoutingError::AlreadyLoaded)
        );
        let unsupported = Route::Get {
            route: "/local".into(),
            trigger: Some("every 1s".into()),
            target: Selector::SelfTarget,
            strategy: SwapStrategy::Children,
        };
        assert!(matches!(
            RequestContext::route(source, &unsupported, "load"),
            Err(RoutingError::UnsupportedTrigger)
        ));
    }

    #[test]
    fn native_redirect_and_cookie_policies_are_explicit_and_do_not_render() {
        for cookies in [false, true] {
            let (mut document, source) = fixture();
            let (mut routing, worker) = coordinator();
            let router = Router::new().with_route("/local", move |_| async move {
                let mut view = View::from(Container {
                    str_id: Some("ignored".into()),
                    ..Container::default()
                });
                view.response.navigation = Some(ResponseNavigation::Internal("/next".into()));
                if cookies {
                    view.response
                        .cookies
                        .push(hyperchad_renderer::ResponseCookie::secure(
                            "session", "secret",
                        ));
                }
                view
            });
            routing.submit(&mut document, request(source)).unwrap();
            let mut work = Box::pin(worker.run(router));
            pump(work.as_mut());
            let outcome = routing.try_complete(&mut document).unwrap().outcome;
            if cookies {
                assert!(matches!(outcome, Err(RoutingError::CookieStoreRequired)));
            } else {
                assert!(matches!(outcome, Ok(NativeOutcome::Redirected(_))));
            }
            assert_eq!(document.select(&Selector::Id("ignored".into()), None), []);
        }
    }

    #[test]
    fn native_no_content_missing_route_and_worker_shutdown_release_tickets() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let router = Router::new()
            .with_no_content_result("/local", |_| async { Ok::<(), std::io::Error>(()) });
        routing.submit(&mut document, request(source)).unwrap();
        let mut missing = request(source);
        missing.path = "/missing".into();
        missing.scope = "missing".into();
        routing.submit(&mut document, missing).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Ok(NativeOutcome::NoContent)
        ));
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Err(RoutingError::Handler(_))
        ));
        routing.submit(&mut document, request(source)).unwrap();
        drop(work);
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Err(RoutingError::WorkerStopped)
        ));
    }

    #[test]
    fn lifecycle_is_native_and_delivered_before_swap_then_revalidated() {
        use crate::events::{EventValue, InputEvent, RequestPhase};
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let router = Router::new().with_route("/local", |_| async {
            View::from(Container::from(vec![Container {
                str_id: Some("new".into()),
                ..Container::default()
            }]))
        });
        let id = routing.prepare(&mut document, request(source)).unwrap();
        let InputEvent::Request {
            phase,
            value: EventValue::Object(value),
            ..
        } = routing.before_event(id).unwrap()
        else {
            panic!("native before envelope");
        };
        assert_eq!(phase, RequestPhase::Before);
        assert_eq!(
            value["transport"],
            EventValue::String("native-router".into())
        );
        assert!(!value.contains_key("status"));
        assert!(!value.contains_key("headers"));
        assert!(!value.contains_key("body"));
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        assert!(routing.try_take().is_none()); // handler cannot run before G/H before.
        assert!(routing.start(id));
        pump(work.as_mut());
        let prepared = routing.try_take().unwrap();
        let events = routing.terminal_events(&document, &prepared);
        assert!(matches!(
            events[0],
            InputEvent::Request {
                phase: RequestPhase::After,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            InputEvent::Request {
                phase: RequestPhase::Success,
                ..
            }
        ));
        assert_eq!(document.select(&Selector::Id("new".into()), None), []);
        // A lifecycle action can replace the document. No response may resurrect it.
        document
            .apply_view(View::from(Container::default()), ViewOrigin::Navigation)
            .unwrap();
        assert!(matches!(
            routing.complete(&mut document, prepared).unwrap().outcome,
            Err(RoutingError::Stale)
        ));
    }

    fn flatten(nodes: &[Container], output: &mut Vec<String>) {
        for node in nodes {
            if let Some(id) = &node.str_id {
                output.push(id.clone());
            }
            flatten(&node.children, output);
        }
    }

    #[test]
    fn real_router_all_swaps_fragments_and_deletes_are_document_transactions() {
        for (strategy, expected) in [
            (SwapStrategy::This, vec!["payload"]),
            (SwapStrategy::Children, vec!["root", "payload"]),
            (SwapStrategy::BeforeBegin, vec!["payload", "root", "old"]),
            (SwapStrategy::AfterBegin, vec!["root", "payload", "old"]),
            (SwapStrategy::BeforeEnd, vec!["root", "old", "payload"]),
            (SwapStrategy::AfterEnd, vec!["root", "old", "payload"]),
            (SwapStrategy::Delete, vec![]),
            (SwapStrategy::None, vec!["root", "old"]),
        ] {
            let (mut document, source) = fixture();
            document
                .apply_view(
                    View::from(Container::from(vec![Container {
                        id: 2,
                        str_id: Some("old".into()),
                        ..Container::default()
                    }])),
                    ViewOrigin::Element {
                        source: source.key,
                        target: Selector::SelfTarget,
                        strategy: SwapStrategy::Children,
                    },
                )
                .unwrap();
            let source = document.stamp(source.key).unwrap();
            let (mut routing, worker) = coordinator();
            let router = Router::new().with_route("/local", |_| async {
                View::from(Container::from(vec![Container {
                    id: 3,
                    str_id: Some("payload".into()),
                    ..Container::default()
                }]))
            });
            let mut context = request(source);
            context.swap = strategy.clone();
            routing.submit(&mut document, context).unwrap();
            let mut work = Box::pin(worker.run(router));
            pump(work.as_mut());
            assert!(matches!(
                routing.try_complete(&mut document).unwrap().outcome,
                Ok(NativeOutcome::Applied(_))
            ));
            let mut actual = Vec::new();
            flatten(document.roots(), &mut actual);
            assert_eq!(actual, expected, "{strategy:?}");
            // Flattened preorder includes descendants: sibling insertion must keep
            // root -> old intact, not merely leave both IDs somewhere in the tree.
            // Browser idiomorph uses insertAdjacentHTML beforebegin/afterend;
            // Document likewise splices an empty sibling range, not the target.
            if matches!(strategy, SwapStrategy::BeforeBegin | SwapStrategy::AfterEnd) {
                let roots = document.roots();
                let root_index = usize::from(strategy == SwapStrategy::BeforeBegin);
                let root = &roots[root_index];
                assert_eq!(roots.len(), 2);
                assert_eq!(root.str_id.as_deref(), Some("root"));
                assert_eq!(document.stamp(source.key), Some(source));
                assert_eq!(root.children.len(), 1);
                assert_eq!(root.children[0].str_id.as_deref(), Some("old"));
                let payload = &roots[1 - root_index];
                assert_eq!(payload.str_id.as_deref(), Some("payload"));
                assert_eq!(payload.children, []);
            }
        }
        assert_fragment_then_delete_transaction();
    }

    fn assert_fragment_then_delete_transaction() {
        let (mut document, source) = fixture();
        // Keep the request source alive across both operations. Replacing the source
        // itself detaches its key; D intentionally refuses subsequent selector
        // resolution from that stale source (including otherwise-global selectors).
        document
            .apply_view(
                View::from(Container::from(vec![Container {
                    id: 2,
                    str_id: Some("victim".into()),
                    ..Container::default()
                }])),
                ViewOrigin::Element {
                    source: source.key,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::Children,
                },
            )
            .unwrap();
        let source = document.stamp(source.key).unwrap();
        let victim = document.select(&Selector::Id("victim".into()), None)[0];
        let (mut routing, worker) = coordinator();
        let router = Router::new().with_route("/local", |_| async {
            View {
                primary: None,
                fragments: vec![hyperchad_renderer::ReplaceContainer {
                    selector: Selector::Id("victim".into()),
                    container: Container {
                        id: 4,
                        str_id: Some("fragment".into()),
                        ..Container::default()
                    },
                }],
                delete_selectors: vec![Selector::Id("fragment".into())],
                ..View::default()
            }
        });
        let mut context = request(source);
        context.swap = SwapStrategy::None;
        routing.submit(&mut document, context).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        let Ok(NativeOutcome::Applied(summary)) =
            routing.try_complete(&mut document).unwrap().outcome
        else {
            panic!("fragment/delete response must apply atomically");
        };
        assert_eq!(document.roots().len(), 1);
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("root"));
        assert_eq!(document.roots()[0].children, []);
        assert!(document.stamp(source.key).is_some());
        assert!(document.stamp(victim).is_none());
        assert_eq!(document.select(&Selector::Id("fragment".into()), None), []);
        assert_eq!(summary.removed.len(), 1);
        assert_eq!(summary.inserted, []); // transient fragment never survives commit
        assert_eq!(summary.diagnostics, []);
    }

    #[test]
    fn local_navigation_keeps_primary_root_and_cancels_older_scopes() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let router = Router::new().with_route("/local", |_| async {
            View::from(Container {
                id: 9,
                str_id: Some("new-root".into()),
                ..Container::default()
            })
        });
        let old = routing.submit(&mut document, request(source)).unwrap();
        let mut navigation = request(source);
        navigation.scope = "navigation".into();
        navigation.trigger = Trigger::Navigation;
        routing.submit(&mut document, navigation).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        let completion = routing.try_complete(&mut document).unwrap();
        assert_eq!(completion.id, old);
        assert!(matches!(completion.outcome, Err(RoutingError::Cancelled)));
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Ok(NativeOutcome::Applied(_))
        ));
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("new-root"));
    }

    #[test]
    fn before_action_mutation_reconcile_cancels_without_polling_handler() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let called = Arc::new(AtomicBool::new(false));
        let captured = called.clone();
        let router = Router::new().with_route("/local", move |_| {
            captured.store(true, Ordering::Release);
            async { View::default() }
        });
        let id = routing.prepare(&mut document, request(source)).unwrap();
        let ticket = routing.pending[&id].ticket.clone();
        // Simulate Before handler replacing its own source via the document mailbox.
        document
            .apply_view(View::from(Container::default()), ViewOrigin::Navigation)
            .unwrap();
        routing.reconcile(&mut document);
        assert!(!document.request_is_current(&ticket));
        let _ = routing.start(id); // A late host release cannot revive cancelled authority.
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        assert!(!called.load(Ordering::Acquire));
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Err(RoutingError::Cancelled)
        ));
    }

    #[test]
    fn authentic_primary_shape_only_commits_successful_primary_navigation() {
        for case in [
            "primary",
            "fragment",
            "failed",
            "stale",
            "cancelled",
            "redirect",
            "local",
            "head",
        ] {
            let (mut document, source) = fixture();
            let (mut routing, worker) = coordinator();
            let router = Router::new().with_route_result("/local", move |_| async move {
                if case == "failed" {
                    return Err(std::io::Error::other("fixture handler failure"));
                }
                let mut view = if case == "fragment" {
                    View {
                        fragments: vec![hyperchad_renderer::ReplaceContainer {
                            selector: Selector::Id("root".into()),
                            container: Container {
                                str_id: Some("fragment-root".into()),
                                ..Container::default()
                            },
                        }],
                        ..View::default()
                    }
                } else {
                    View::from(Container {
                        str_id: Some("primary-root".into()),
                        ..Container::default()
                    })
                };
                if case == "redirect" {
                    view.response.navigation =
                        Some(ResponseNavigation::Internal("/redirect".into()));
                }
                Ok(view)
            });
            let mut context = request(source);
            context.trigger = if case == "local" {
                Trigger::Route
            } else {
                Trigger::Navigation
            };
            if case == "head" {
                context.method = Method::Head;
            }
            routing.submit(&mut document, context).unwrap();
            let mut work = Box::pin(worker.run(router));
            pump(work.as_mut());
            let prepared = routing.try_take().unwrap();
            let has_primary = prepared.has_primary();
            assert_eq!(
                has_primary,
                !matches!(case, "fragment" | "failed"),
                "{case}"
            );
            if case == "stale" {
                document
                    .apply_view(View::from(Container::default()), ViewOrigin::Navigation)
                    .unwrap();
            } else if case == "cancelled" {
                assert!(routing.cancel_prepared(&mut document, &prepared));
            }
            // This is B's exact URL commit predicate, not a root-change heuristic.
            let completion = routing.complete(&mut document, prepared).unwrap();
            let commits_url = has_primary
                && completion.context.trigger == Trigger::Navigation
                && matches!(&completion.outcome, Ok(NativeOutcome::Applied(_)));
            assert_eq!(commits_url, case == "primary", "{case}");
            match case {
                "primary" | "fragment" | "local" => {
                    assert!(matches!(completion.outcome, Ok(NativeOutcome::Applied(_))));
                }
                "failed" => assert!(matches!(completion.outcome, Err(RoutingError::Handler(_)))),
                "stale" => assert!(matches!(completion.outcome, Err(RoutingError::Stale))),
                "cancelled" => assert!(matches!(completion.outcome, Err(RoutingError::Cancelled))),
                "redirect" => assert!(matches!(
                    completion.outcome,
                    Ok(NativeOutcome::Redirected(_))
                )),
                "head" => assert!(matches!(completion.outcome, Ok(NativeOutcome::NoContent))),
                _ => unreachable!(),
            }
            assert_eq!(routing.pending.len(), 0);
        }
    }

    #[test]
    fn prepared_completion_debug_redacts_success_payload_and_handler_errors() {
        let cancellation = Arc::new(Cancellation {
            cancelled: AtomicBool::new(false),
            signal: flume::bounded(1).0,
        });
        let mut view = View::from(Container {
            str_id: Some("private-payload".into()),
            ..Container::default()
        });
        view.response
            .cookies
            .push(hyperchad_renderer::ResponseCookie::secure(
                "session",
                "private-cookie",
            ));
        for result in [
            Ok(Some(Content::View(Box::new(view)))),
            Err(RoutingError::Handler("private-handler-error".into())),
        ] {
            let prepared = PreparedCompletion {
                message: ResultMessage {
                    id: RequestId(7),
                    result,
                },
                cancellation: cancellation.clone(),
            };
            assert_eq!(
                format!("{prepared:?}"),
                "PreparedCompletion { id: RequestId(7), .. }"
            );
            assert_eq!(
                format!("{prepared:#?}"),
                "PreparedCompletion {\n    id: RequestId(\n        7,\n    ),\n    ..\n}"
            );
        }
    }

    #[test]
    fn failed_lifecycle_cancels_only_its_prepared_request_and_consumes_without_effects() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        let router = Router::new().with_route("/local", |_| async {
            let mut view = View::from(Container {
                str_id: Some("forbidden".into()),
                ..Container::default()
            });
            view.response.navigation = Some(ResponseNavigation::Internal("/forbidden".into()));
            view.response
                .cookies
                .push(hyperchad_renderer::ResponseCookie::secure(
                    "secret",
                    "never-apply",
                ));
            view
        });
        let failed_id = routing.submit(&mut document, request(source)).unwrap();
        let failed_ticket = routing.pending[&failed_id].ticket.clone();
        let mut other = request(source);
        other.scope = "independent".into();
        let other_id = routing.submit(&mut document, other).unwrap();
        let other_ticket = routing.pending[&other_id].ticket.clone();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        let prepared = routing.try_take().unwrap();
        assert!(routing.cancel_prepared(&mut document, &prepared));
        assert!(routing.cancel_prepared(&mut document, &prepared));
        assert!(!document.request_is_current(&failed_ticket));
        assert!(document.request_is_current(&other_ticket));
        assert!(
            !routing.pending[&other_id]
                .cancellation
                .cancelled
                .load(Ordering::Acquire)
        );
        let replay = PreparedCompletion {
            message: ResultMessage {
                id: failed_id,
                result: Ok(None),
            },
            cancellation: prepared.cancellation.clone(),
        };
        let completion = routing.complete(&mut document, prepared).unwrap();
        assert!(!routing.cancel_prepared(&mut document, &replay));
        assert!(routing.complete(&mut document, replay).is_none());
        assert_eq!(completion.id, failed_id);
        assert!(matches!(completion.outcome, Err(RoutingError::Cancelled)));
        assert!(!routing.pending.contains_key(&failed_id));
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("root"));
        assert_eq!(document.select(&Selector::Id("forbidden".into()), None), []);
        // The independent request still reaches policy validation, not cancellation.
        let completion = routing.try_complete(&mut document).unwrap();
        assert_eq!(completion.id, other_id);
        assert!(matches!(
            completion.outcome,
            Err(RoutingError::CookieStoreRequired)
        ));
        assert_eq!(routing.pending.len(), 0);
        assert!(!document.request_is_current(&other_ticket));
    }

    #[test]
    fn prepared_completion_retains_capacity_and_cannot_cross_coordinator_authority() {
        let (mut document, source) = fixture();
        let (mut other_document, other_source) = fixture();
        let (mut routing, worker) = Routing::new(1, 1024, Arc::new(|| {})).unwrap();
        let (mut other, other_worker) = Routing::new(1, 1024, Arc::new(|| {})).unwrap();
        let router = Router::new().with_route("/local", |_| async { View::default() });
        let id = routing.submit(&mut document, request(source)).unwrap();
        let other_id = other
            .submit(&mut other_document, request(other_source))
            .unwrap();
        assert_eq!(id, other_id); // IDs alone are deliberately not ticket authority.
        let mut work = Box::pin(worker.run(router.clone()));
        let mut other_work = Box::pin(other_worker.run(router));
        pump(work.as_mut());
        pump(other_work.as_mut());
        let prepared = routing.try_take().unwrap();
        assert_eq!(
            routing.submit(&mut document, request(source)),
            Err(RoutingError::QueueFull)
        );
        assert!(other.terminal_events(&other_document, &prepared).is_empty());
        assert!(!other.cancel_prepared(&mut other_document, &prepared));
        assert!(document.request_is_current(&routing.pending[&id].ticket));
        assert!(other_document.request_is_current(&other.pending[&other_id].ticket));
        // Moving a completion to the wrong owner must not consume that owner's job.
        assert!(other.complete(&mut other_document, prepared).is_none());
        assert!(matches!(
            other.try_complete(&mut other_document).unwrap().outcome,
            Ok(NativeOutcome::Applied(_))
        ));
        routing.shutdown(&mut document); // clean abandoned prepared authority on teardown
        assert!(!routing.cancel(&mut document, RequestId(u64::MAX)));
    }

    #[test]
    fn head_metadata_is_returned_without_rendering_and_shutdown_releases_load_ticket() {
        let (mut document, source) = fixture();
        let (mut routing, worker) = coordinator();
        routing.cookie_policy = CookiePolicy::HostManaged;
        let router = Router::new().with_route("/local", |_| async {
            let mut view = View::from(Container {
                str_id: Some("never-render-head".into()),
                ..Container::default()
            });
            view.response
                .cookies
                .push(hyperchad_renderer::ResponseCookie::secure(
                    "session", "native",
                ));
            view
        });
        let mut context = request(source);
        context.method = Method::Head;
        routing.submit(&mut document, context).unwrap();
        let mut work = Box::pin(worker.run(router));
        pump(work.as_mut());
        let Ok(NativeOutcome::Metadata(metadata)) =
            routing.try_complete(&mut document).unwrap().outcome
        else {
            panic!("HEAD must retain native response effects");
        };
        assert_eq!(metadata.cookies.len(), 1);
        assert_eq!(
            document.select(&Selector::Id("never-render-head".into()), None),
            []
        );
        let mut context = request(source);
        context.trigger = Trigger::Load;
        let id = routing.prepare(&mut document, context).unwrap();
        let ticket = routing.pending[&id].ticket.clone();
        assert!(document.request_is_current(&ticket));
        routing.shutdown(&mut document);
        assert!(!document.request_is_current(&ticket));
        pump(work.as_mut());
        assert!(matches!(
            routing.try_complete(&mut document).unwrap().outcome,
            Err(RoutingError::Cancelled)
        ));
        assert_eq!(routing.loaded, BTreeMap::new());
    }

    #[test]
    fn security_limits_encoding_and_external_link_policy() {
        let (_, source) = fixture();
        let (mut routing, _) = coordinator();
        for url in [
            "javascript:alert(1)",
            "file:///tmp/a",
            "//evil.test",
            "/\\evil",
            "/%2fhost",
            "/%2e%2e/private",
            "/bad%",
            "/x\n",
        ] {
            assert!(routing.link(url).is_err(), "{url}");
        }
        assert!(routing.link("https://example.test/path").is_err());
        routing.external_policy = ExternalPolicy::HttpAndHttps;
        assert!(matches!(
            routing.link("https://example.test/path"),
            Ok(Link::External(_))
        ));
        assert!(routing.link("https://user:password@example.test/").is_err());
        let mut context = request(source);
        context.method = Method::Trace;
        assert!(matches!(
            route_request(&context, 4096),
            Err(RoutingError::UnsupportedMethod)
        ));
        context.method = Method::Post;
        context
            .headers
            .insert("X-Test".into(), "bad\r\nHeader: value".into());
        assert!(matches!(
            route_request(&context, 4096),
            Err(RoutingError::UnsafeHeader)
        ));
        assert!(matches!(
            RequestContext::form(
                form(source, "post", true),
                "/",
                Selector::SelfTarget,
                SwapStrategy::None,
                3
            ),
            Err(RoutingError::TooLarge)
        ));
        for value in ["", "a b+c&d=é", "\0", "*_.-~"] {
            assert_eq!(decode(&encode(value)).unwrap(), value);
        }
    }
}
