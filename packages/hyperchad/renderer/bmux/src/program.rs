//! Serialized document, action, control and resource state driven by bmux scheduling.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Instant,
};

use bmux_tui::{
    component::{Constraints, LogicalSize},
    event::{Event, MouseEventKind},
    geometry::Size,
    image::ImageContribution,
    paint::PaintCx,
};
use bmux_tui_runtime::{Command, Program, RuntimeEvent, Update};
use hyperchad_renderer::{Color, View, canvas::CanvasUpdate};
use hyperchad_transformer::{Container, Element, ResponsiveTrigger};

use crate::{
    actions::{ActionEngine, ActionLimits, Effect, EventContext, EventKind, Source},
    controls::{ControlEvent, Controls},
    graphics::{GraphicsFit, GraphicsPlacement, GraphicsStore},
    layout::{self, LayoutOptions, LayoutTree, LeafMeasure, TerminalMetrics},
    renderer::{BmuxConfig, Error, HostRequest, HostResponse, Shared},
    state::ViewState,
};

/// Ordered renderer messages. Producers must use the bounded facade admission path.
// Keep the public producer contract stable; admission bounds message count and payload bytes.
#[allow(clippy::large_enum_variant)]
pub enum Message {
    View(View, Option<String>),
    Canvas(CanvasUpdate),
    Emit(String, Option<String>),
    Responsive(String, ResponsiveTrigger),
    Background(Option<Color>),
    Shutdown,
    /// Cancel a correlated active or queued HTTP request.
    AbortRequest(String),
    /// Retry an image after an authorization, fetch, or decode failure.
    RetryResource(String),
    InputFailed(String),
    /// One serialized asynchronous effect completed; generations reject stale responses.
    Completed {
        generation: u64,
        work: Completion,
        result: Result<HostResponse, Error>,
    },
    /// Wake resource invalidation after an opaque authorization epoch changes.
    ResourceContextChanged,
    /// Advance debounced/delayed action state using the program's monotonic clock.
    Advance,
}

/// Correlation metadata; contains no resource bytes or file authorization grants.
#[derive(Debug, Clone)]
pub enum Completion {
    Resource { key: String, source: String },
    File { key: String },
    Request(crate::actions::RequestEffect),
    Other,
}

enum Work {
    Resource {
        key: String,
        source: String,
        generation: u64,
    },
    Host {
        request: Box<HostRequest>,
        completion: Completion,
        generation: u64,
    },
    Custom(String, Option<hyperchad_actions::logic::Value>),
    Navigate(String),
}

/// Sole owner of mutable renderer state. Presentation candidates are not input authority.
pub struct BmuxProgram {
    shared: Arc<Shared>,
    pub state: ViewState,
    pub controls: Controls,
    pub graphics: GraphicsStore,
    actions: ActionEngine,
    options: LayoutOptions,
    candidate: Option<LayoutTree>,
    effective: Option<Container>,
    committed: Option<(u64, LayoutTree)>,
    pending: VecDeque<Work>,
    active: bool,
    stopping: bool,
    active_abort: Option<(Option<String>, tokio::sync::watch::Sender<bool>)>,
    resources: BTreeMap<String, String>,
    resource_epochs: BTreeMap<String, u64>,
    resource_serial: u64,
    resource_context: Option<u64>,
    resource_scan_counts: Option<(usize, usize)>,
    resource_observer: Option<bmux_tui_runtime::SubscriptionKey>,
    node_epochs: BTreeMap<String, u64>,
    key_chord: Vec<hyperchad_actions::Key>,
    http_headers: Option<BTreeMap<String, String>>,
    responsive: BTreeMap<String, ResponsiveTrigger>,
    background: Option<Color>,
    clock: Instant,
    location: String,
    hover: Option<String>,
    pressed: Option<String>,
    namespace: String,
    graphics_available: bool,
}

impl BmuxProgram {
    /// Construct a directly driven program without terminal ownership.
    #[must_use]
    pub fn new(
        config: BmuxConfig,
        request_action: flume::Sender<(String, Option<hyperchad_actions::logic::Value>)>,
        on_resize: flume::Sender<(f32, f32)>,
    ) -> Self {
        let renderer = crate::renderer::BmuxRenderer::new(config, request_action, on_resize);
        Self::from_shared(Arc::clone(&renderer.shared))
    }

    pub(crate) fn from_shared(shared: Arc<Shared>) -> Self {
        let options = LayoutOptions {
            viewport: Size::new(shared.config.width, shared.config.height),
            metrics: TerminalMetrics::default(),
            max_nodes: shared.config.max_nodes,
            max_depth: shared.config.max_depth,
            ..Default::default()
        };
        let graphics = GraphicsStore::new(shared.config.graphics);
        Self {
            shared,
            state: ViewState::new(),
            controls: Controls::new(),
            graphics,
            actions: ActionEngine::new(0, ActionLimits::default()),
            options,
            candidate: None,
            effective: None,
            committed: None,
            pending: VecDeque::new(),
            active: false,
            stopping: false,
            active_abort: None,
            resources: BTreeMap::new(),
            resource_epochs: BTreeMap::new(),
            resource_serial: 0,
            resource_context: None,
            resource_scan_counts: None,
            resource_observer: None,
            node_epochs: BTreeMap::new(),
            key_chord: Vec::new(),
            http_headers: None,
            responsive: BTreeMap::new(),
            background: None,
            clock: Instant::now(),
            location: String::new(),
            hover: None,
            pressed: None,
            graphics_available: cfg!(feature = "images"),
            namespace: format!(
                "hyperchad-{}",
                NEXT_SURFACE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ),
        }
    }

    #[must_use]
    pub fn config(&self) -> &BmuxConfig {
        &self.shared.config
    }

    /// Host/presenter-negotiated graphics availability, independent of decoded resources.
    pub const fn set_graphics_available(&mut self, available: bool) {
        self.graphics_available = available;
    }

    /// Set host-provided cell geometry without assuming ownership of its terminal.
    pub const fn resize(&mut self, size: Size) {
        self.options.viewport = size;
    }

    /// Prepare authoritative geometry; no output, actions or committed input changes occur.
    ///
    /// # Errors
    /// * Returns layout limit, invalid geometry or canvas resource errors.
    pub fn prepare(&mut self) -> Result<(), Error> {
        self.revalidate_resource_context()?;
        self.options.open_details = self.controls.details_open();
        let mut effective = self.state.root().clone();
        resolve_responsive(&mut effective, &self.responsive, self.options.viewport);
        let root = &effective;
        let controls = self.controls.measurer(root);
        let mut keys = BTreeMap::new();
        visit(root, &mut Vec::new(), &mut |node, path| {
            keys.insert(
                std::ptr::from_ref(node) as usize,
                layout::node_key(node, path),
            );
        });
        let measure = |node: &Container, constraints: Constraints| {
            let key = keys.get(&(std::ptr::from_ref(node) as usize));
            if self.graphics_available
                && self.shared.config.graphics_enabled
                && let Some((width, height, _)) = key.and_then(|key| self.graphics.rgba(key))
            {
                return LogicalSize::new(
                    u64::from(width.div_ceil(8)),
                    u64::from(height.div_ceil(16)),
                );
            }
            controls.measure(node, constraints)
        };
        let tree = layout::layout(root, &self.options, &measure)?;
        drop(controls);
        self.controls.prepare(&tree);
        for node in &tree.nodes {
            if matches!(
                at_path(self.state.root(), &node.path).map(|node| &node.element),
                Some(Element::Canvas)
            ) && self.graphics.rgba(&node.key).is_none()
            {
                let width = u32::try_from(node.content_bounds.width.saturating_mul(8))
                    .map_err(|_| std::io::Error::other("canvas width overflow"))?;
                let height = u32::try_from(node.content_bounds.height.saturating_mul(16))
                    .map_err(|_| std::io::Error::other("canvas height overflow"))?;
                if width > 0 && height > 0 {
                    self.graphics.create_canvas(&node.key, width, height)?;
                }
            }
        }
        self.effective = Some(effective);
        self.candidate = Some(tree);
        Ok(())
    }

    /// Paint only a previously prepared layout into a local, host-clipped context.
    pub fn paint(&mut self, cx: &mut PaintCx<'_, '_>) {
        let base_style = self
            .background
            .map_or_else(bmux_tui::style::Style::new, |color| {
                bmux_tui::style::Style::new()
                    .bg(bmux_tui::style::Color::Rgb(color.r, color.g, color.b))
            });
        // A fill alone is insufficient: subsequent glyph/control writes otherwise
        // replace cells with a default (terminal-transparent) background.
        cx.with_style(base_style, |cx| self.paint_document(cx));
    }

    fn paint_document(&mut self, cx: &mut PaintCx<'_, '_>) {
        let Some(tree) = &self.candidate else {
            return;
        };
        if let Some(color) = self.background {
            cx.fill(
                bmux_tui::paint::LocalRect::new(0, 0, tree.viewport.width, tree.viewport.height),
                " ",
                bmux_tui::style::Style::new()
                    .bg(bmux_tui::style::Color::Rgb(color.r, color.g, color.b)),
            );
        }
        crate::presentation::paint_text(tree, &self.controls, cx);
        for node in &tree.nodes {
            if !node.visible {
                continue;
            }
            cx.with_style(node.style.style, |cx| {
                self.controls.paint(&node.key, cx);
            });
            let Some(source) = at_path(
                self.effective.as_ref().unwrap_or_else(|| self.state.root()),
                &node.path,
            ) else {
                continue;
            };
            let alt = match &source.element {
                Element::Image { alt, .. } => alt.as_deref().unwrap_or("[image]"),
                Element::Canvas => "[canvas]",
                _ => continue,
            };
            let placement_key = format!("{}:{}", self.namespace, node.key);
            match self.graphics.placement(
                &node.key,
                &placement_key,
                node.content_bounds.terminal(),
                node.clip.terminal(),
                GraphicsFit::Contain,
                (8.0, 16.0),
                self.shared.config.graphics_enabled && self.graphics_available,
                alt,
            ) {
                Ok(GraphicsPlacement::Image(image)) => {
                    cx.push_image(ImageContribution::Present(image));
                }
                Ok(GraphicsPlacement::Fallback { rect, text }) => {
                    cx.with_child(
                        0,
                        0,
                        bmux_tui::paint::LocalRect::new(
                            i32::from(rect.x),
                            i64::from(rect.y),
                            rect.width,
                            rect.height,
                        ),
                        |cx| {
                            cx.write_line(
                                bmux_tui::paint::LocalRect::new(
                                    i32::from(rect.x),
                                    i64::from(rect.y),
                                    rect.width,
                                    1,
                                ),
                                &bmux_tui::text::Line::from(text),
                            );
                        },
                    );
                }
                Ok(GraphicsPlacement::Hidden) => {}
                Err(error) => log::warn!("graphics placement rejected for {}: {error}", node.key),
            }
        }
    }

    /// Publish geometry only after the enclosing host output flush succeeds.
    pub fn commit_prepared(&mut self) {
        if let Some(tree) = self.candidate.take() {
            self.committed = Some((self.state.generation(), tree));
        }
    }

    /// Discard failed preparation and restore committed control event geometry.
    pub fn discard_prepared(&mut self) {
        self.candidate = None;
        if let Some((_, tree)) = &self.committed {
            self.controls.prepare(tree);
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn enqueue(&mut self, work: Work) -> Result<(), Error> {
        if self.pending.len() >= self.shared.config.runtime.max_queued_commands.max(1) {
            return Err(std::io::Error::other("bmux pending effect limit exceeded").into());
        }
        self.pending.push_back(work);
        Ok(())
    }

    // Resource identity is independent of document generations: context changes and
    // explicit retries can occur without any document mutation.
    fn schedule_resource(&mut self, key: String, source: String) -> Result<(), Error> {
        let serial = self
            .resource_serial
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("resource serial exhausted"))?;
        self.pending.retain(|work| {
            !matches!(work,
            Work::Resource { key: queued, .. } if queued == &key)
        });
        self.enqueue(Work::Resource {
            key: key.clone(),
            source: source.clone(),
            generation: serial,
        })?;
        self.resource_serial = serial;
        self.graphics.remove(&key);
        self.resources.insert(key.clone(), source);
        self.resource_epochs.insert(key, serial);
        Ok(())
    }

    fn schedule_resources(&mut self) -> Result<(), Error> {
        let mut images = Vec::new();
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            if let Element::Image {
                source: Some(source),
                ..
            } = &node.element
            {
                images.push((layout::node_key(node, path), source.clone()));
            }
        });
        let counts = (
            images.len(),
            images
                .iter()
                .map(|(_, source)| source)
                .collect::<BTreeSet<_>>()
                .len(),
        );
        if self.resource_scan_counts != Some(counts) {
            log::debug!(
                "bmux resource scan images={} unique_sources={}",
                counts.0,
                counts.1
            );
            self.resource_scan_counts = Some(counts);
        }
        let present: BTreeSet<_> = images.iter().map(|(key, _)| key.clone()).collect();
        self.resources.retain(|key, _| present.contains(key));
        self.resource_epochs.retain(|key, _| present.contains(key));
        self.pending.retain(|work| match work {
            Work::Resource {
                key,
                source,
                generation,
            } => {
                self.resources.get(key) == Some(source)
                    && self.resource_epochs.get(key) == Some(generation)
            }
            _ => true,
        });
        for (key, source) in images {
            if self.resources.get(&key) != Some(&source) {
                self.schedule_resource(key, source)?;
            }
        }
        Ok(())
    }

    fn revalidate_resource_context(&mut self) -> Result<(), Error> {
        let context = self
            .shared
            .config
            .resource_context
            .as_ref()
            .map(|context| context());
        if context != self.resource_context {
            log::debug!(
                "bmux resource context changed cached_nodes={} queued_resources={}",
                self.resources.len(),
                self.pending
                    .iter()
                    .filter(|work| matches!(work, Work::Resource { .. }))
                    .count()
            );
            self.resource_scan_counts = None;
            // Purge successes and failures together; neither may cross authorization
            // contexts even when the source string is unchanged. In-flight work keeps
            // running, but its unique serial can no longer install a completion.
            for key in self.resources.keys() {
                self.graphics.remove(key);
            }
            self.resources.clear();
            self.resource_epochs.clear();
            self.pending
                .retain(|work| !matches!(work, Work::Resource { .. }));
            self.resource_context = context;
        }
        self.schedule_resources()
    }

    fn next_work(&mut self) -> Option<Work> {
        // Ordered semantic effects must not wait behind decorative asset loads.
        // Preserve FIFO within both classes and never preempt in-flight work.
        let next = self
            .pending
            .iter()
            .position(|work| !matches!(work, Work::Resource { .. }))
            .unwrap_or(0);
        self.pending.remove(next)
    }

    fn drive(&mut self, mut update: Update<Message>) -> Update<Message> {
        let resource_key = bmux_tui_runtime::SubscriptionKey::new("hyperchad-resource-context");
        if self.stopping || self.shared.config.resource_context.is_none() {
            if let Some(registered) = self.resource_observer.take() {
                update.cancelled_subscriptions.push(registered);
            } else {
                update.cancelled_subscriptions.push(resource_key);
            }
        } else if self.resource_observer.is_none()
            && let Some(context) = self.shared.config.resource_context.clone()
        {
            let observed = self.resource_context.unwrap_or_else(|| context());
            update.subscriptions.push(bmux_tui_runtime::Subscription::new(
                resource_key.clone(),
                move |sender| observe_resource_context(
                    context,
                    observed,
                    || tokio::time::sleep(std::time::Duration::from_millis(250)),
                    move || {
                        let sender = sender.clone();
                        async move { sender.send(Message::ResourceContextChanged).await.is_ok() }
                    },
                ),
            ));
            // Runtime replaces, rather than deduplicates, repeated keys. Register
            // once so unrelated redraws cannot keep restarting the poll interval.
            self.resource_observer = Some(resource_key);
        }
        let timer_key = bmux_tui_runtime::SubscriptionKey::new("hyperchad-actions");
        if let Some(deadline) = self.actions.next_deadline() {
            let delay = std::time::Duration::from_millis(deadline.saturating_sub(self.now_ms()));
            update
                .subscriptions
                .push(bmux_tui_runtime::Subscription::new(
                    timer_key,
                    move |sender| async move {
                        tokio::time::sleep(delay).await;
                        let _ = sender.send(Message::Advance).await;
                    },
                ));
        } else {
            update.cancelled_subscriptions.push(timer_key);
        }
        if self.stopping && !self.active && self.pending.is_empty() {
            update.lifecycle = bmux_tui_runtime::Lifecycle::Exit;
        }
        if self.active {
            return update;
        }
        let Some(work) = self.next_work() else {
            return update;
        };
        update.commands.push(self.start_work(work));
        update
    }

    fn start_work(&mut self, work: Work) -> Command<Message> {
        self.active = true;
        let host = self.shared.config.host.clone();
        let custom = self.shared.request_action.clone();
        let navigation = self.shared.navigation_tx.clone();
        let max_bytes = self.shared.config.graphics.max_encoded_bytes;
        let current_generation = self.state.generation();
        let timeout = self.shared.config.effect_timeout;
        let (timeout_generation, timeout_completion) = work_correlation(&work, current_generation);
        let class = work_class(&work);
        log::debug!(
            "bmux work start class={class} generation={timeout_generation} pending={}",
            self.pending.len()
        );
        let (abort_tx, mut abort_rx) = tokio::sync::watch::channel(false);
        let request_id = match &timeout_completion {
            Completion::Request(request) => request.request_id.clone(),
            _ => None,
        };
        self.active_abort = Some((request_id, abort_tx));
        Command::concurrent(async move {
            let operation = async move {
                let (generation, completion, result) = match work {
                    Work::Resource {
                        key,
                        source,
                        generation,
                    } => {
                        let result = match host.request {
                            Some(host) => {
                                host(HostRequest::Resource {
                                    source: source.clone(),
                                    max_bytes,
                                })
                                .await
                            }
                            None => Err(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                "resource loading requires a host callback",
                            )
                            .into()),
                        };
                        (generation, Completion::Resource { key, source }, result)
                    }
                    Work::Host {
                        request,
                        completion,
                        generation,
                    } => {
                        let result = match host.request {
                            Some(host) => host(*request).await,
                            None => Err(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                "host operation not authorized",
                            )
                            .into()),
                        };
                        (generation, completion, result)
                    }
                    Work::Custom(action, value) => (
                        current_generation,
                        Completion::Other,
                        custom
                            .send_async((action, value))
                            .await
                            .map(|()| HostResponse::Complete)
                            .map_err(|_| -> Error {
                                std::io::Error::other("application action bridge closed").into()
                            }),
                    ),
                    Work::Navigate(url) => (
                        current_generation,
                        Completion::Other,
                        navigation
                            .send_async(url)
                            .await
                            .map(|()| HostResponse::Complete)
                            .map_err(|_| -> Error {
                                std::io::Error::other("application navigation bridge closed").into()
                            }),
                    ),
                };
                Message::Completed {
                    generation,
                    work: completion,
                    result,
                }
            };
            let failure = tokio::select! {
                result = tokio::time::timeout(timeout, operation) => match result { Ok(message) => return Some(message), Err(_) => std::io::Error::new(std::io::ErrorKind::TimedOut, "bmux effect deadline exceeded") },
                _ = abort_rx.changed() => std::io::Error::new(std::io::ErrorKind::Interrupted, "bmux request aborted"),
            };
            Some(Message::Completed {
                generation: timeout_generation,
                work: timeout_completion,
                result: Err(failure.into()),
            })
        })
    }

    /// Apply a bounded document transaction.
    ///
    /// # Errors
    /// * Rejects invalid selectors, identities, resource budgets or closed effect admission.
    pub fn apply_view(&mut self, view: View, target: Option<&str>) -> Result<(), Error> {
        self.apply_response(
            view,
            target,
            hyperchad_transformer_models::SwapStrategy::This,
        )
    }

    /// Apply a complete authorized response using its exact swap strategy.
    ///
    /// # Errors
    /// * Rejects invalid targets, duplicate identities, resource limits or effect admission.
    // Matches Renderer ownership: callers transfer a complete response transaction.
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply_response(
        &mut self,
        view: View,
        target: Option<&str>,
        strategy: hyperchad_transformer_models::SwapStrategy,
    ) -> Result<(), Error> {
        self.apply_targets(&view, vec![target.map(str::to_owned)], &strategy)
    }

    fn apply_targets(
        &mut self,
        view: &View,
        targets: Vec<Option<String>>,
        strategy: &hyperchad_transformer_models::SwapStrategy,
    ) -> Result<(), Error> {
        validate_view(view, &self.shared.config)?;
        let mut next = self.state.clone();
        let mut report = None;
        for target in targets {
            let current = next.apply_response(view.clone(), target.as_deref(), strategy.clone())?;
            if let Some(report) = &mut report {
                let report: &mut crate::state::ApplyReport = report;
                report.removed.extend(current.removed);
                report.lifecycle_removed.extend(current.lifecycle_removed);
                report.inserted.extend(current.inserted);
                report.retained.extend(current.retained);
                report.controls = current.controls;
                report.generation = current.generation;
            } else {
                report = Some(current);
            }
        }
        let Some(mut report) = report else {
            return Ok(());
        };
        crate::admission::validate_tree(next.root(), &self.shared.config)?;
        // Canonical keyed no-op transactions preserve their state generation.
        // A renderer replacement still starts new source lifetimes; give those
        // sources a fresh epoch so prior request completions cannot target them.
        if report.generation == self.state.generation() && !report.lifecycle_removed.is_empty() {
            report.generation = next.mark_changed()?;
        }
        self.state = next;
        let mut image_sources = BTreeMap::new();
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            if let Element::Image {
                source: Some(source),
                ..
            } = &node.element
            {
                image_sources.insert(layout::node_key(node, path), source.clone());
            }
        });
        for key in &report.lifecycle_removed {
            // Document lifetimes reset event/request ownership, not immutable image
            // bytes. Authorization context changes are purged independently below.
            let unchanged_image = self.graphics.rgba(key).is_some()
                && self
                    .resources
                    .get(key)
                    .is_some_and(|source| image_sources.get(key) == Some(source));
            if !unchanged_image {
                self.graphics.remove(key);
                self.resources.remove(key);
                self.resource_epochs.remove(key);
            }
        }
        self.controls.reconcile_report(self.state.root(), &report);
        self.actions.reconcile_removed(
            self.state.root(),
            report.generation,
            &report.lifecycle_removed,
        )?;
        for key in &report.lifecycle_removed {
            self.node_epochs.remove(key);
        }
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            self.node_epochs
                .entry(layout::node_key(node, path))
                .or_insert(report.generation);
        });
        self.revalidate_resource_context()?;
        if !report.response.cookies.is_empty() {
            self.enqueue(Work::Host {
                request: Box::new(HostRequest::Cookies(report.response.cookies.clone())),
                completion: Completion::Other,
                generation: self.state.generation(),
            })?;
        }
        if let Some(navigation) = report.response.navigation {
            match navigation {
                hyperchad_renderer::ResponseNavigation::Internal(url) => {
                    hyperchad_renderer::ResponseNavigation::internal(url.clone())?;
                    self.enqueue(Work::Navigate(url))?;
                }
                hyperchad_renderer::ResponseNavigation::External(url) => {
                    hyperchad_renderer::ResponseNavigation::external(url.clone())?;
                    self.enqueue(Work::Host {
                        request: Box::new(HostRequest::ExternalLink { url }),
                        completion: Completion::Other,
                        generation: self.state.generation(),
                    })?;
                }
            }
        }
        self.broadcast(&EventKind::Immediate, None)?;
        Ok(())
    }

    fn broadcast(&mut self, kind: &EventKind, value: Option<&str>) -> Result<(), Error> {
        let mut keys = Vec::new();
        let mut has_immediate = false;
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            has_immediate |= crate::actions::has_load_route(node)
                || node.actions.iter().any(|action| {
                    matches!(action.trigger, hyperchad_actions::ActionTrigger::Immediate)
                });
            keys.push(layout::node_key(node, path));
        });
        // Lifecycle actions run before presentation, but still require current
        // document geometry. This layout is not published for pointer hit testing.
        let lifecycle_tree = if (has_immediate && matches!(kind, EventKind::Immediate))
            || matches!(kind, EventKind::Resize)
        {
            let mut options = self.options.clone();
            options.open_details = self.controls.details_open();
            let mut effective = self.state.root().clone();
            resolve_responsive(&mut effective, &self.responsive, self.options.viewport);
            Some(layout::layout(
                &effective,
                &options,
                &self.controls.measurer(&effective),
            )?)
        } else {
            None
        };
        for key in keys {
            self.dispatch_with_geometry(
                &key,
                kind.clone(),
                value.map(str::to_owned),
                lifecycle_tree.as_ref(),
            )?;
        }
        Ok(())
    }

    fn dispatch(&mut self, key: &str, kind: EventKind, value: Option<String>) -> Result<(), Error> {
        self.dispatch_with_geometry(key, kind, value, None)
    }

    fn dispatch_with_geometry(
        &mut self,
        key: &str,
        kind: EventKind,
        value: Option<String>,
        geometry: Option<&LayoutTree>,
    ) -> Result<(), Error> {
        // Do not format event payloads: named events may contain application data.
        let event_kind = match &kind {
            EventKind::Click => "Click",
            EventKind::ClickOutside => "ClickOutside",
            EventKind::MouseDown => "MouseDown",
            EventKind::KeyDown => "KeyDown",
            EventKind::Hover => "Hover",
            EventKind::Change => "Change",
            EventKind::Resize => "Resize",
            EventKind::Immediate => "Immediate",
            EventKind::Named(_) => "Named",
            EventKind::Http(_) => "Http",
            EventKind::Submit => "Submit",
            EventKind::Reset => "Reset",
        };
        log::debug!("bmux action dispatch key={key:?} kind={event_kind}");
        let mut found = None;
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            if layout::node_key(node, path) == key {
                found = Some(path.to_vec());
            }
        });
        let Some(path) = found else {
            return Ok(());
        };
        let source = Source::at(self.state.root(), path, self.state.generation())?;
        let mut context = EventContext::new(source, kind, self.now_ms());
        context.value = value;
        context.keys.clone_from(&self.key_chord);
        context.location.clone_from(&self.location);
        if let Some(tree) = geometry.or_else(|| self.committed.as_ref().map(|(_, tree)| tree)) {
            context.geometry = tree
                .nodes
                .iter()
                .map(|node| {
                    (
                        node.key.clone(),
                        crate::actions::Geometry {
                            x: f32::from(i16::try_from(node.bounds.x).unwrap_or(
                                if node.bounds.x < 0 {
                                    i16::MIN
                                } else {
                                    i16::MAX
                                },
                            )) * 8.0,
                            y: f32::from(i16::try_from(node.bounds.y).unwrap_or(
                                if node.bounds.y < 0 {
                                    i16::MIN
                                } else {
                                    i16::MAX
                                },
                            )) * 16.0,
                            width: f32::from(u16::try_from(node.bounds.width).unwrap_or(u16::MAX))
                                * 8.0,
                            height: f32::from(
                                u16::try_from(node.bounds.height).unwrap_or(u16::MAX),
                            ) * 16.0,
                        },
                    )
                })
                .collect();
        }
        let effects = self.actions.dispatch(
            self.state.root_mut(),
            &context,
            &self.controls.action_values(),
        )?;
        self.effects(effects)
    }

    fn http_event(
        &mut self,
        request: &crate::actions::RequestEffect,
        phase: crate::actions::HttpPhase,
        status: Option<u16>,
    ) -> Result<(), Error> {
        let mut source = request.source.clone();
        let Some(node) = self.state.find_node(&source.key) else {
            return Ok(());
        };
        let mut path = None;
        visit(
            self.state.root(),
            &mut Vec::new(),
            &mut |candidate, candidate_path| {
                if std::ptr::eq(candidate, node) {
                    path = Some(candidate_path.to_vec());
                }
            },
        );
        source.path = path.unwrap_or_default();
        source.generation = self.state.generation();
        let mut context = EventContext::new(source, EventKind::Http(phase), self.now_ms());
        context.request_id.clone_from(&request.request_id);
        context.http = Some(hyperchad_actions::HttpEventContext {
            url: request.url.clone(),
            method: request.method.clone(),
            status,
            headers: self.http_headers.clone(),
            duration_ms: None,
            error: None,
        });
        let effects = self.actions.dispatch(
            self.state.root_mut(),
            &context,
            &self.controls.action_values(),
        )?;
        self.effects(effects)
    }

    fn effects(&mut self, effects: Vec<Effect>) -> Result<(), Error> {
        for effect in effects {
            match effect {
                Effect::Custom { action, value, .. } => {
                    self.enqueue(Work::Custom(action, value))?;
                }
                Effect::Navigate { url, target, .. } => {
                    if url.starts_with('/')
                        && !url.starts_with("//")
                        && !matches!(
                            target,
                            Some(hyperchad_transformer_models::LinkTarget::Blank)
                        )
                    {
                        self.location.clone_from(&url);
                        self.enqueue(Work::Navigate(url))?;
                    } else {
                        self.enqueue(Work::Host {
                            request: Box::new(HostRequest::ExternalLink { url }),
                            completion: Completion::Other,
                            generation: self.state.generation(),
                        })?;
                    }
                }
                Effect::Request(request) => {
                    log::debug!(
                        "bmux request effect method={} source={:?}",
                        request.method,
                        request.source.key
                    );
                    self.http_event(&request, crate::actions::HttpPhase::Before, None)?;
                    self.enqueue(Work::Host {
                        generation: self
                            .node_epochs
                            .get(&request.source.key)
                            .copied()
                            .unwrap_or(request.source.generation),
                        completion: Completion::Request(request.clone()),
                        request: Box::new(HostRequest::Http(Box::new(request))),
                    })?;
                }
                Effect::Log { level, message } => log::info!("HyperChad {level:?}: {message}"),
                Effect::Focus { source, focused } => {
                    self.controls
                        .focus(if focused { Some(&source.key) } else { None });
                }
                Effect::Select { source } => {
                    self.controls.select_all(&source.key);
                }
                Effect::Reset { source, .. } => {
                    self.controls.reset(&source.key);
                }
                Effect::StylesChanged { .. } => {}
            }
        }
        Ok(())
    }

    fn control_events(&mut self, events: Vec<ControlEvent>) -> Result<(), Error> {
        let activated: BTreeSet<_> = events
            .iter()
            .filter_map(|event| match event {
                ControlEvent::Activate { key } => Some(key.clone()),
                _ => None,
            })
            .collect();
        for event in events {
            match event {
                ControlEvent::Activate { key } => self.dispatch(&key, EventKind::Click, None)?,
                ControlEvent::Navigate { key, .. } if !activated.contains(&key) => {
                    self.dispatch(&key, EventKind::Click, None)?;
                }
                ControlEvent::Change { key, value } => self.dispatch(
                    &key,
                    EventKind::Change,
                    Some(match value {
                        crate::controls::ControlValue::Text(text) => text,
                        crate::controls::ControlValue::Checked(value) => value.to_string(),
                        other => format!("{other:?}"),
                    }),
                )?,
                ControlEvent::Submit { form, submitter } => {
                    let key = submitter.as_deref().unwrap_or(&form);
                    if !activated.contains(key) {
                        self.dispatch(key, EventKind::Submit, None)?;
                    }
                }
                ControlEvent::FileRequest { key, .. } => self.enqueue(Work::Host {
                    generation: self
                        .node_epochs
                        .get(&key)
                        .copied()
                        .unwrap_or_else(|| self.state.generation()),
                    completion: Completion::File { key: key.clone() },
                    request: Box::new(HostRequest::ChooseFile { node: key }),
                })?,
                ControlEvent::Scroll { key, edge } => {
                    let scroll = self.options.scroll_offsets.entry(key).or_default();
                    scroll.y = scroll.y.saturating_add_signed(i64::from(edge));
                }
                ControlEvent::Reveal { key } => self.reveal(&key),
                ControlEvent::Navigate { .. }
                | ControlEvent::Toggle { .. }
                | ControlEvent::Reset { .. }
                | ControlEvent::Focus { .. }
                | ControlEvent::Popup { .. } => {}
            }
        }
        Ok(())
    }

    fn reveal(&mut self, key: &str) {
        let Some((_, tree)) = &self.committed else {
            return;
        };
        let Some(target) = tree.get(key) else {
            return;
        };
        let mut bounds = target.bounds;
        let mut parent = target.parent_key.clone();
        while let Some(key) = parent.as_ref() {
            let Some(node) = tree.get(key) else {
                break;
            };
            let Some(source) = at_path(self.state.root(), &node.path) else {
                break;
            };
            let scrollable = |overflow| {
                matches!(
                    overflow,
                    hyperchad_transformer_models::LayoutOverflow::Auto
                        | hyperchad_transformer_models::LayoutOverflow::Scroll
                )
            };
            let viewport = node.content_bounds;
            let reveal_axis = |(start, end): (i64, i64),
                               (visible_start, visible_end): (i64, i64),
                               current: u64,
                               (extent, visible): (u64, u64),
                               enabled: bool| {
                if !enabled {
                    return (0, 0);
                }
                let delta = if start < visible_start {
                    start - visible_start
                } else {
                    (end - visible_end).max(0)
                };
                let next = current
                    .saturating_add_signed(delta)
                    .min(extent.saturating_sub(visible));
                let applied = i64::try_from(next).unwrap_or(i64::MAX)
                    - i64::try_from(current).unwrap_or(i64::MAX);
                (next, applied)
            };
            let (x, dx) = reveal_axis(
                (bounds.x, bounds.right()),
                (viewport.x, viewport.right()),
                node.scroll_offset.x,
                (node.content_extents.width, viewport.width),
                scrollable(source.overflow_x),
            );
            let (y, dy) = reveal_axis(
                (bounds.y, bounds.bottom()),
                (viewport.y, viewport.bottom()),
                node.scroll_offset.y,
                (node.content_extents.height, viewport.height),
                scrollable(source.overflow_y),
            );
            if scrollable(source.overflow_x) || scrollable(source.overflow_y) {
                self.options
                    .scroll_offsets
                    .insert(key.clone(), layout::ScrollOffset { x, y });
            }
            bounds = bounds.translated(-dx, -dy);
            parent.clone_from(&node.parent_key);
        }
    }

    fn complete_http_lifecycle(
        &mut self,
        work: &Completion,
        result: &Result<HostResponse, Error>,
    ) -> Result<(), Error> {
        self.http_headers = match result {
            Ok(HostResponse::Route { headers, .. }) => Some(headers.clone()),
            _ => None,
        };
        if let Completion::Request(request) = work {
            let (phase, status) = match result {
                Ok(HostResponse::Http { status, .. } | HostResponse::Route { status, .. }) => (
                    if *status < 400 {
                        crate::actions::HttpPhase::Success
                    } else {
                        crate::actions::HttpPhase::Error
                    },
                    Some(*status),
                ),
                Ok(_) => (crate::actions::HttpPhase::Success, None),
                Err(error) => (
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::TimedOut)
                    {
                        crate::actions::HttpPhase::Timeout
                    } else if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::Interrupted)
                    {
                        crate::actions::HttpPhase::Abort
                    } else {
                        crate::actions::HttpPhase::Error
                    },
                    None,
                ),
            };
            self.http_event(request, phase, status)?;
            self.http_event(request, crate::actions::HttpPhase::After, status)?;
        }
        self.http_headers = None;

        Ok(())
    }

    fn apply_request_view(
        &mut self,
        request: &crate::actions::RequestEffect,
        view: &View,
    ) -> Result<(), Error> {
        let mut targets = Vec::new();
        if request.navigation {
            targets.push(None);
        } else {
            let mut source_path = request.source.path.clone();
            visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
                if layout::node_key(node, path) == request.source.key {
                    source_path = path.to_vec();
                }
            });
            visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
                let key = layout::node_key(node, path);
                let matches = match &request.target {
                    hyperchad_transformer_models::Selector::Id(id) => {
                        node.str_id.as_ref() == Some(id) || key == *id
                    }
                    hyperchad_transformer_models::Selector::SelfTarget => key == request.source.key,
                    hyperchad_transformer_models::Selector::Class(class) => {
                        node.classes.contains(class)
                    }
                    hyperchad_transformer_models::Selector::ChildClass(class) => {
                        node.classes.contains(class)
                            && path.len() == source_path.len() + 1
                            && path.starts_with(&source_path)
                    }
                };
                if matches {
                    targets.push(Some(key));
                }
            });
        }
        self.apply_targets(view, targets, &request.strategy)
    }

    fn complete(
        &mut self,
        generation: u64,
        work: Completion,
        mut result: Result<HostResponse, Error>,
    ) -> Result<(), Error> {
        self.active = false;
        self.active_abort = None;
        self.revalidate_resource_context()?;
        let current = match &work {
            Completion::Resource { key, source } => {
                self.resources.get(key) == Some(source)
                    && self.resource_epochs.get(key) == Some(&generation)
            }
            Completion::File { key } => self.node_epochs.get(key) == Some(&generation),
            Completion::Request(request) => {
                self.node_epochs.get(&request.source.key) == Some(&generation)
            }
            Completion::Other => generation == self.state.generation(),
        };
        let class = match &work {
            Completion::Resource { .. } => "resource",
            Completion::File { .. } => "file",
            Completion::Request(_) => "http",
            Completion::Other => "other",
        };
        log::debug!(
            "bmux work complete class={class} generation={generation} current={current} success={} pending={}",
            result.is_ok(),
            self.pending.len()
        );
        if current {
            self.complete_http_lifecycle(&work, &result)?;
            result = match result {
                Ok(HostResponse::Route {
                    view: Some(view),
                    body,
                    ..
                }) if body.len() <= self.shared.config.max_text_bytes => {
                    Ok(HostResponse::View(view))
                }
                Ok(HostResponse::Route {
                    view: None, body, ..
                }) if body.len() <= self.shared.config.max_text_bytes => Ok(HostResponse::Complete),
                Ok(HostResponse::Route { .. }) => {
                    Err(std::io::Error::other("host response exceeds byte limit").into())
                }
                other => other,
            };
            match (work, result) {
                (Completion::Resource { key, source }, Ok(HostResponse::Bytes(bytes)))
                    if self.resources.get(&key) == Some(&source) =>
                {
                    if let Err(error) = self.graphics.decode(
                        &key,
                        &crate::graphics::GraphicsSource::Memory,
                        &crate::graphics::SourcePolicy {
                            allow_memory: true,
                            ..Default::default()
                        },
                        &bytes,
                    ) {
                        // Preserve negative admission until explicit retry or source change.
                        log::warn!("image decode rejected: {error}");
                    }
                }
                (Completion::File { key }, Ok(HostResponse::Files(files))) => {
                    let outcome = self.controls.set_files(&key, files);
                    self.control_events(outcome.events)?;
                }
                (Completion::Request(request), Ok(HostResponse::View(view))) => {
                    self.apply_request_view(&request, &view)?;
                }
                (Completion::Resource { .. }, Err(error)) => {
                    // View refreshes must not retry denied resources automatically.
                    log::warn!("image resource load failed: {error}");
                }
                (_, Err(error)) => {
                    log::warn!("authorized bmux host operation failed: {error}");
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn scroll_pointer(&mut self, mouse: bmux_tui::event::MouseEvent) {
        if let Some((_, tree)) = &self.committed {
            let mut node = tree.hit_test(i64::from(mouse.position.x), i64::from(mouse.position.y));
            while let Some(current) = node {
                let vertical = matches!(
                    mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                );
                let extent = if vertical {
                    current.content_extents.height
                } else {
                    current.content_extents.width
                };
                let visible = if vertical {
                    current.content_bounds.height
                } else {
                    current.content_bounds.width
                };
                if extent > visible {
                    let offset = self
                        .options
                        .scroll_offsets
                        .entry(current.key.clone())
                        .or_insert(current.scroll_offset);
                    let value = if vertical {
                        &mut offset.y
                    } else {
                        &mut offset.x
                    };
                    let delta = if matches!(
                        mouse.kind,
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollLeft
                    ) {
                        -3
                    } else {
                        3
                    };
                    *value = value.saturating_add_signed(delta).min(extent - visible);
                    break;
                }
                node = current.parent_key.as_ref().and_then(|key| tree.get(key));
            }
        }
    }

    fn pointer(&mut self, mouse: bmux_tui::event::MouseEvent) -> Result<(), Error> {
        if matches!(
            mouse.kind,
            MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight
        ) {
            self.scroll_pointer(mouse);
            return Ok(());
        }
        let hit = self
            .committed
            .as_ref()
            .and_then(|(_, tree)| {
                tree.hit_test(i64::from(mouse.position.x), i64::from(mouse.position.y))
            })
            .map(|node| node.key.clone());
        match mouse.kind {
            MouseEventKind::Down(_) => {
                self.pressed.clone_from(&hit);
                if let Some(key) = hit {
                    self.dispatch(&key, EventKind::MouseDown, None)?;
                }
            }
            MouseEventKind::Up(_) if hit == self.pressed.take() => {
                if let Some(key) = hit {
                    self.dispatch(&key, EventKind::Click, None)?;
                }
            }
            MouseEventKind::Move if hit != self.hover => {
                if let Some(previous) = self.hover.take()
                    && let Some(node) = self
                        .committed
                        .as_ref()
                        .and_then(|(_, tree)| tree.get(&previous))
                {
                    let source = Source::at(
                        self.state.root(),
                        node.path.clone(),
                        self.state.generation(),
                    )?;
                    let context = EventContext::new(source, EventKind::Hover, self.now_ms());
                    let effects = self.actions.release(self.state.root_mut(), &context)?;
                    self.effects(effects)?;
                }
                self.hover.clone_from(&hit);
                if let Some(key) = hit {
                    self.dispatch(&key, EventKind::Hover, None)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn message(&mut self, message: Message) -> Result<(), Error> {
        match message {
            Message::RetryResource(key) => {
                if let Some(Container {
                    element:
                        Element::Image {
                            source: Some(source),
                            ..
                        },
                    ..
                }) = self.state.find_node(&key)
                {
                    self.schedule_resource(key, source.clone())?;
                }
            }
            Message::AbortRequest(id) => {
                if let Some((active, cancel)) = &self.active_abort
                    && active.as_ref() == Some(&id)
                {
                    let _ = cancel.send(true);
                }
                let mut cancelled = Vec::new();
                self.pending.retain(|work| {
                    if let Work::Host {
                        completion: Completion::Request(request),
                        ..
                    } = work
                        && request.request_id.as_ref() == Some(&id)
                    {
                        cancelled.push(request.clone());
                        return false;
                    }
                    true
                });
                for request in cancelled {
                    self.http_event(&request, crate::actions::HttpPhase::Abort, None)?;
                    self.http_event(&request, crate::actions::HttpPhase::After, None)?;
                }
            }
            Message::ResourceContextChanged => {}
            Message::Shutdown => self.stopping = true,
            Message::InputFailed(error) => return Err(std::io::Error::other(error).into()),
            Message::View(view, target) => self.apply_view(view, target.as_deref())?,
            Message::Canvas(update) => {
                if self.graphics.rgba(&update.target).is_none() {
                    self.prepare()?;
                    self.discard_prepared();
                }
                self.graphics.update_canvas(&update)?;
            }
            Message::Emit(name, value) => {
                let mut keys = Vec::new();
                visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
                    keys.push(layout::node_key(node, path));
                });
                for key in keys {
                    self.dispatch(&key, EventKind::Named(name.clone()), value.clone())?;
                }
            }
            Message::Responsive(name, trigger) => {
                self.responsive.insert(name, trigger);
            }
            Message::Background(color) => self.background = color,
            Message::Advance => {
                let now = self.now_ms();
                let effects = self.actions.advance(self.state.root_mut(), now)?;
                self.effects(effects)?;
            }
            Message::Completed {
                generation,
                work,
                result,
            } => {
                self.complete(generation, work, result)?;
            }
        }
        Ok(())
    }

    fn terminal(&mut self, event: &Event) -> Result<(), Error> {
        if !matches!(event, Event::Key(_)) {
            self.key_chord.clear();
        }
        if let Event::Resize(size) = event {
            self.resize(*size);
            self.broadcast(&EventKind::Resize, None)?;
            // Resize is latest-only state: a full app callback queue must not block input.
            let _ = self
                .shared
                .on_resize
                .try_send((f32::from(size.width) * 8.0, f32::from(size.height) * 16.0));
        }
        if self
            .committed
            .as_ref()
            .is_some_and(|(generation, _)| *generation != self.state.generation())
        {
            return Ok(());
        }
        if let Some((_, tree)) = &self.committed {
            self.controls.prepare(tree);
        }
        if let Event::Key(key) = event {
            self.key_chord.clone_from(&key_chord(key));
            if let Some(focused) = self.controls.focused().map(str::to_owned) {
                self.dispatch(&focused, EventKind::KeyDown, Some(format!("{:?}", key.key)))?;
            }
        }
        if let Event::Mouse(mouse) = event
            && matches!(mouse.kind, MouseEventKind::Down(_))
        {
            let outside: Vec<_> = self
                .committed
                .as_ref()
                .map(|(_, tree)| {
                    tree.nodes
                        .iter()
                        .filter(|node| {
                            !node
                                .bounds
                                .contains(i64::from(mouse.position.x), i64::from(mouse.position.y))
                        })
                        .map(|node| node.key.clone())
                        .collect()
                })
                .unwrap_or_default();
            for key in outside {
                self.dispatch(&key, EventKind::ClickOutside, None)?;
            }
        }
        let result = self.controls.event(event);
        self.control_events(result.events)?;
        if result.consumed {
            return Ok(());
        }
        if let Event::Mouse(mouse) = event {
            self.pointer(*mouse)?;
        }
        Ok(())
    }
}

impl Program for BmuxProgram {
    type Message = Message;
    type Error = Error;

    fn update(&mut self, event: RuntimeEvent<Message>) -> Result<Update<Message>, Error> {
        let context_wake = matches!(
            &event,
            RuntimeEvent::Message(Message::ResourceContextChanged)
        );
        if context_wake && self.stopping {
            return Ok(self.drive(Update::none()));
        }
        let previous_context = self.resource_context;
        self.revalidate_resource_context()?;
        if context_wake {
            // A normal update may have already observed the epoch before this
            // queued wake arrived. Do not repaint redundantly in that case.
            return Ok(self.drive(if previous_context == self.resource_context {
                Update::none()
            } else {
                Update::redraw()
            }));
        }
        let before = self.state.root().clone();
        let generation_before = self.state.generation();
        match event {
            RuntimeEvent::Terminal(event) => self.terminal(&event)?,
            RuntimeEvent::Timer(_) => {
                let now = self.now_ms();
                let effects = self.actions.advance(self.state.root_mut(), now)?;
                self.effects(effects)?;
            }
            RuntimeEvent::Message(message) => self.message(message)?,
        }
        if generation_before == self.state.generation() && before != *self.state.root() {
            self.state.mark_changed()?;
            self.actions.reconcile_removed(
                self.state.root(),
                self.state.generation(),
                &BTreeSet::new(),
            )?;
        }
        Ok(self.drive(Update::redraw()))
    }

    fn presentation_committed(
        &mut self,
        _report: bmux_tui_runtime::PresentReport,
    ) -> Update<Message> {
        self.drive(Update::none())
    }
}

// Generic clock/notification boundaries make the observer deterministic in tests
// without adding a timer-testing feature or exposing credentials to the worker.
async fn observe_resource_context<Wait, Tick, Notify, Sent>(
    context: Arc<dyn Fn() -> u64 + Send + Sync>,
    mut observed: u64,
    mut wait: Wait,
    mut notify: Notify,
) where
    Wait: FnMut() -> Tick,
    Tick: std::future::Future<Output = ()>,
    Notify: FnMut() -> Sent,
    Sent: std::future::Future<Output = bool>,
{
    loop {
        wait().await;
        let current = context();
        if current != observed {
            if !notify().await {
                return;
            }
            observed = current;
        }
    }
}

const fn work_class(work: &Work) -> &'static str {
    match work {
        Work::Resource { .. } => "resource",
        Work::Host {
            completion: Completion::Request(_),
            ..
        } => "http",
        Work::Host {
            completion: Completion::File { .. },
            ..
        } => "file",
        Work::Host { .. } => "host-other",
        Work::Custom(..) => "custom",
        Work::Navigate(_) => "navigation",
    }
}

fn work_correlation(work: &Work, current_generation: u64) -> (u64, Completion) {
    match work {
        Work::Resource {
            key,
            source,
            generation,
        } => (
            *generation,
            Completion::Resource {
                key: key.clone(),
                source: source.clone(),
            },
        ),
        Work::Host {
            completion,
            generation,
            ..
        } => (*generation, completion.clone()),
        _ => (current_generation, Completion::Other),
    }
}

fn character_key(character: char) -> Option<hyperchad_actions::Key> {
    use hyperchad_actions::Key as K;

    let letter = character.to_ascii_uppercase();
    let names = [
        K::A,
        K::B,
        K::C,
        K::D,
        K::E,
        K::F,
        K::G,
        K::H,
        K::I,
        K::J,
        K::K,
        K::L,
        K::M,
        K::N,
        K::O,
        K::P,
        K::Q,
        K::R,
        K::S,
        K::T,
        K::U,
        K::V,
        K::W,
        K::X,
        K::Y,
        K::Z,
    ];
    if letter.is_ascii_uppercase() {
        names.get((letter as u8 - b'A') as usize).cloned()
    } else if letter.is_ascii_digit() {
        [
            K::Key0,
            K::Key1,
            K::Key2,
            K::Key3,
            K::Key4,
            K::Key5,
            K::Key6,
            K::Key7,
            K::Key8,
            K::Key9,
        ]
        .get((letter as u8 - b'0') as usize)
        .cloned()
    } else {
        None
    }
}

fn key_chord(stroke: &bmux_keyboard::KeyStroke) -> Vec<hyperchad_actions::Key> {
    use bmux_keyboard::KeyCode as C;
    use hyperchad_actions::Key as K;
    let mut keys = Vec::new();
    if stroke.modifiers.ctrl {
        keys.push(K::Control);
    }
    if stroke.modifiers.alt {
        keys.push(K::Alt);
    }
    if stroke.modifiers.shift {
        keys.push(K::Shift);
    }
    if stroke.modifiers.super_key || stroke.modifiers.meta {
        keys.push(K::Meta);
    }
    let key = match stroke.key {
        C::Escape => Some(K::Escape),
        C::Enter => Some(K::Enter),
        C::Tab => Some(K::Tab),
        C::Backspace => Some(K::Backspace),
        C::Delete => Some(K::Delete),
        C::Insert => Some(K::Insert),
        C::Up => Some(K::ArrowUp),
        C::Down => Some(K::ArrowDown),
        C::Left => Some(K::ArrowLeft),
        C::Right => Some(K::ArrowRight),
        C::Home => Some(K::Home),
        C::End => Some(K::End),
        C::PageUp => Some(K::PageUp),
        C::PageDown => Some(K::PageDown),
        C::Char(character) => character_key(character),
        C::F(number) => [
            K::F1,
            K::F2,
            K::F3,
            K::F4,
            K::F5,
            K::F6,
            K::F7,
            K::F8,
            K::F9,
            K::F10,
            K::F11,
            K::F12,
        ]
        .get(usize::from(number.saturating_sub(1)))
        .cloned(),
        C::Space => None,
    };
    if let Some(key) = key {
        keys.push(key);
    }
    keys
}

fn resolve_responsive(
    root: &mut Container,
    triggers: &BTreeMap<String, ResponsiveTrigger>,
    viewport: Size,
) {
    use hyperchad_transformer::{OverrideCondition, OverrideItem};
    let width = f32::from(viewport.width) * 8.0;
    let height = f32::from(viewport.height) * 16.0;
    let overrides = root.overrides.clone();
    for config in overrides {
        let OverrideCondition::ResponsiveTarget { name } = &config.condition;
        let enabled = match triggers.get(name) {
            Some(ResponsiveTrigger::MaxWidth(number)) => width <= number.calc(width, width, height),
            Some(ResponsiveTrigger::MaxHeight(number)) => {
                height <= number.calc(height, width, height)
            }
            None => false,
        };
        for item in if enabled {
            config.overrides
        } else {
            config.default.into_iter().collect()
        } {
            macro_rules! set { ($($variant:ident => $field:ident),* $(,)?) => {
                match item {
                    OverrideItem::StrId(_) | OverrideItem::Classes(_) => {}, // Presentation cannot rewrite canonical identities.
                    OverrideItem::Direction(value) => root.direction = value,
                    OverrideItem::OverflowX(value) => root.overflow_x = value,
                    OverrideItem::OverflowY(value) => root.overflow_y = value,
                    $(OverrideItem::$variant(value) => root.$field = Some(value),)*
                }
            }; }
            set! {
                GridCellSize => grid_cell_size, JustifyContent => justify_content, AlignItems => align_items,
                TextAlign => text_align, WhiteSpace => white_space, TextDecoration => text_decoration,
                FontFamily => font_family, FontWeight => font_weight, Width => width, MinWidth => min_width,
                MaxWidth => max_width, Height => height, MinHeight => min_height, MaxHeight => max_height,
                Flex => flex, ColumnGap => column_gap, RowGap => row_gap, Opacity => opacity,
                Left => left, Right => right, Top => top, Bottom => bottom, TranslateX => translate_x,
                TranslateY => translate_y, Cursor => cursor, UserSelect => user_select,
                OverflowWrap => overflow_wrap, TextOverflow => text_overflow, Position => position,
                Background => background, BorderTop => border_top, BorderRight => border_right,
                BorderBottom => border_bottom, BorderLeft => border_left,
                BorderTopLeftRadius => border_top_left_radius, BorderTopRightRadius => border_top_right_radius,
                BorderBottomLeftRadius => border_bottom_left_radius, BorderBottomRightRadius => border_bottom_right_radius,
                MarginLeft => margin_left, MarginRight => margin_right, MarginTop => margin_top, MarginBottom => margin_bottom,
                PaddingLeft => padding_left, PaddingRight => padding_right, PaddingTop => padding_top, PaddingBottom => padding_bottom,
                FontSize => font_size, Color => color, Hidden => hidden, Visibility => visibility,
            }
        }
    }
    for child in &mut root.children {
        resolve_responsive(child, triggers, viewport);
    }
}

static NEXT_SURFACE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn at_path<'a>(mut root: &'a Container, path: &[usize]) -> Option<&'a Container> {
    for &index in path {
        root = root.children.get(index)?;
    }
    Some(root)
}

fn visit(root: &Container, path: &mut Vec<usize>, callback: &mut impl FnMut(&Container, &[usize])) {
    callback(root, path);
    for (index, child) in root.children.iter().enumerate() {
        path.push(index);
        visit(child, path, callback);
        path.pop();
    }
}

/// Validate document admission before queueing a potentially large transaction.
///
/// # Errors
/// * Returns an error for excessive node count, nesting or text/source bytes.
pub fn validate_view(view: &View, config: &BmuxConfig) -> Result<(), Error> {
    crate::admission::validate_view(view, config)
}

#[cfg(test)]
mod runtime_regressions {
    use super::*;

    fn program() -> BmuxProgram {
        let (actions, _) = flume::bounded(8);
        let (resize, _) = flume::bounded(8);
        BmuxProgram::new(BmuxConfig::default(), actions, resize)
    }

    #[test]
    fn shutdown_waits_for_prior_cookie_effect_completion() {
        let mut program = program();
        program.pending.push_back(Work::Host {
            request: Box::new(HostRequest::Cookies(Vec::new())),
            completion: Completion::Other,
            generation: program.state.generation(),
        });
        let update = program
            .update(RuntimeEvent::Message(Message::Shutdown))
            .unwrap();
        assert_eq!(update.lifecycle, bmux_tui_runtime::Lifecycle::Continue);
        assert_eq!(update.commands.len(), 1);
        let update = program
            .update(RuntimeEvent::Message(Message::Completed {
                generation: program.state.generation(),
                work: Completion::Other,
                result: Ok(HostResponse::Complete),
            }))
            .unwrap();
        assert_eq!(update.lifecycle, bmux_tui_runtime::Lifecycle::Exit);
    }

    #[test]
    fn correlated_timeout_dispatches_timeout_and_after() {
        let mut program = program();
        let action = |trigger, name: &str| hyperchad_actions::Action {
            trigger,
            effect: hyperchad_actions::ActionEffect {
                action: hyperchad_actions::ActionType::Custom {
                    action: name.into(),
                },
                ..Default::default()
            },
        };
        let root = Container {
            str_id: Some("request".into()),
            actions: vec![
                action(
                    hyperchad_actions::ActionTrigger::HttpRequestTimeout,
                    "timeout",
                ),
                action(hyperchad_actions::ActionTrigger::HttpAfterRequest, "after"),
            ],
            ..Default::default()
        };
        program.apply_view(View::from(root), None).unwrap();
        let request = crate::actions::RequestEffect {
            source: Source::at(program.state.root(), vec![], program.state.generation()).unwrap(),
            url: "/test".into(),
            base_url: "/".into(),
            method: "GET".into(),
            body: crate::actions::RequestBody::Empty,
            headers: BTreeMap::new(),
            target: hyperchad_transformer_models::Selector::SelfTarget,
            strategy: hyperchad_transformer_models::SwapStrategy::None,
            request_id: Some("one".into()),
            navigation: false,
        };
        let epoch = program.node_epochs["request"];
        let update = program
            .update(RuntimeEvent::Message(Message::Completed {
                generation: epoch,
                work: Completion::Request(request),
                result: Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "deadline").into()),
            }))
            .unwrap();
        assert_eq!(update.commands.len(), 1);
        assert!(matches!(program.pending.front(), Some(Work::Custom(name, _)) if name == "after"));
    }

    #[test]
    fn decoded_image_without_protocol_has_visible_alt_text() {
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("art".into()),
                    element: Element::Image {
                        source: None,
                        alt: Some("visible cover".into()),
                        fit: None,
                        source_set: None,
                        sizes: None,
                        loading: None,
                    },
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program
            .graphics
            .register_rgba("art", 1, 1, vec![255, 0, 0, 255])
            .unwrap();
        program.set_graphics_available(false);
        program.prepare().unwrap();
        let mut terminal = bmux_tui::terminal::Terminal::new(
            Vec::new(),
            bmux_tui::geometry::Rect::new(0, 0, 80, 24),
        );
        terminal.draw(|cx| program.paint(cx)).unwrap();
        let output = String::from_utf8_lossy(terminal.writer());
        assert!(output.contains("visible cover"));
    }

    #[test]
    fn response_cookies_are_forwarded_to_explicit_host_operation() {
        let mut program = program();
        let mut view = View::from(Container::default());
        view.response
            .cookies
            .push(hyperchad_renderer::ResponseCookie::secure(
                "session", "opaque",
            ));
        program.apply_view(view, None).unwrap();
        assert!(
            matches!(program.pending.front(), Some(Work::Host { request, .. }) if matches!(request.as_ref(), HostRequest::Cookies(cookies) if cookies.len() == 1))
        );
    }

    #[test]
    fn chord_metadata_contains_modifiers_and_character() {
        let chord = key_chord(&bmux_keyboard::KeyStroke::with_modifiers(
            bmux_keyboard::KeyCode::Char('a'),
            bmux_keyboard::Modifiers {
                ctrl: true,
                shift: true,
                ..bmux_keyboard::Modifiers::NONE
            },
        ));
        assert!(chord.contains(&hyperchad_actions::Key::Control));
        assert!(chord.contains(&hyperchad_actions::Key::Shift));
        assert!(chord.contains(&hyperchad_actions::Key::A));
    }

    #[test]
    fn unrelated_response_preserves_file_incarnation() {
        let mut program = program();
        program.node_epochs.insert("file".into(), 4);
        program.state.mark_changed().unwrap();
        assert_eq!(program.node_epochs.get("file"), Some(&4));
    }

    #[test]
    fn resource_failure_requires_explicit_retry_or_source_change() {
        for result in [
            Err(std::io::Error::other("host denied resource").into()),
            Ok(HostResponse::Bytes(vec![0, 1, 2])),
        ] {
            let mut program = program();
            let image = |source: &str| {
                View::from(Container {
                    str_id: Some("image".into()),
                    element: Element::Image {
                        source: Some(source.into()),
                        alt: Some("fallback".into()),
                        fit: None,
                        source_set: None,
                        sizes: None,
                        loading: None,
                    },
                    ..Default::default()
                })
            };
            let source = "https://example.test/a";
            program
                .apply_view(
                    View::from(Container {
                        children: vec![
                            image(source).primary.unwrap(),
                            Container {
                                str_id: Some("sibling".into()),
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    }),
                    None,
                )
                .unwrap();
            program.pending.clear();
            let generation = program.resource_epochs["image"];
            program
                .update(RuntimeEvent::Message(Message::Completed {
                    generation,
                    work: Completion::Resource {
                        key: "image".into(),
                        source: source.into(),
                    },
                    result,
                }))
                .unwrap();
            assert_eq!(
                program.resources.get("image").map(String::as_str),
                Some(source)
            );
            assert!(program.graphics.rgba("image").is_none());
            for index in 0..10 {
                program
                    .apply_response(
                        View::from(Container {
                            children: vec![Container {
                                str_id: Some(format!("refresh-{index}")),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }),
                        Some("sibling"),
                        hyperchad_transformer_models::SwapStrategy::Children,
                    )
                    .unwrap();
                assert!(program.pending.is_empty());
                assert_eq!(program.resource_epochs["image"], generation);
            }
            assert_failed_resource_replacement(&mut program, image(source), source, generation);
            let update = program
                .update(RuntimeEvent::Message(Message::RetryResource(
                    "image".into(),
                )))
                .unwrap();
            assert_eq!(update.commands.len(), 1);
            program
                .apply_view(image("https://example.test/b"), None)
                .unwrap();
            assert!(
                matches!(program.pending.back(), Some(Work::Resource { source, .. }) if source.ends_with("/b"))
            );
        }
    }

    fn assert_failed_resource_replacement(
        program: &mut BmuxProgram,
        image: View,
        source: &str,
        previous_generation: u64,
    ) {
        // Replacement creates a new incarnation even with an identical source.
        program.apply_view(image, Some("image")).unwrap();
        let generation = program.resource_epochs["image"];
        assert_ne!(generation, previous_generation);
        assert_eq!(program.pending.len(), 1);
        assert!(
            matches!(program.pending.pop_front(), Some(Work::Resource { source: queued, generation: epoch, .. }) if queued == source && epoch == generation)
        );
        program
            .complete(
                generation,
                Completion::Resource {
                    key: "image".into(),
                    source: source.into(),
                },
                Err(std::io::Error::other("replacement denied").into()),
            )
            .unwrap();
        program.schedule_resources().unwrap();
        assert!(program.pending.is_empty());
    }

    #[test]
    fn multiple_targets_apply_atomically_with_cloned_strategy() {
        let mut program = program();
        let child = |id: &str| Container {
            str_id: Some(id.into()),
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    children: vec![child("a"), child("b")],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program
            .apply_targets(
                &View::from(Container {
                    element: Element::Text {
                        value: "inserted".into(),
                    },
                    ..Default::default()
                }),
                vec![Some("a".into()), Some("b".into())],
                &hyperchad_transformer_models::SwapStrategy::Children,
            )
            .unwrap();
        assert_eq!(program.state.find_node("a").unwrap().children.len(), 1);
        assert_eq!(program.state.find_node("b").unwrap().children.len(), 1);
    }

    #[test]
    fn focus_reveal_scrolls_main_not_sidebar_or_non_scrollable_wrappers() {
        use hyperchad_transformer::Number;
        use hyperchad_transformer_models::{LayoutDirection, LayoutOverflow};
        for width in [80, 140] {
            let mut program = program();
            program.resize(Size::new(width, 60));
            let sized = |key: &str, width: i64, height: i64| Container {
                str_id: Some(key.into()),
                width: Some(Number::Integer(width * 8)),
                height: Some(Number::Integer(height * 16)),
                ..Default::default()
            };
            let mut sidebar = sized("sidebar", 30, 55);
            sidebar.children.push(Container {
                str_id: Some("home".into()),
                element: Element::Anchor {
                    href: Some("/".into()),
                    target: None,
                },
                children: vec![Container {
                    element: Element::Text {
                        value: "Home".into(),
                    },
                    ..Default::default()
                }],
                ..Default::default()
            });
            let mut form = sized("form", 40, 90);
            form.children = vec![
                sized("spacer", 1, 80),
                Container {
                    element: Element::Input {
                        name: Some("url".into()),
                        input: hyperchad_transformer::Input::Text {
                            value: None,
                            placeholder: None,
                        },
                        autofocus: None,
                    },
                    ..sized("url", 40, 2)
                },
            ];
            let mut main = sized("main", i64::from(width) - 30, 55);
            main.overflow_y = LayoutOverflow::Auto;
            main.children.push(form);
            let mut row = sized("row", i64::from(width), 55);
            row.direction = LayoutDirection::Row;
            row.children = vec![sidebar, main];
            let mut root = sized("root", i64::from(width), 60);
            root.children.push(row);
            program.apply_view(View::from(root), None).unwrap();
            program.prepare().unwrap();
            program.commit_prepared();
            let home = program
                .committed
                .as_ref()
                .unwrap()
                .1
                .get("home")
                .unwrap()
                .bounds;
            program.reveal("url");
            assert!(program.options.scroll_offsets["main"].y > 0);
            for key in ["root", "row", "sidebar", "form"] {
                assert!(
                    !program.options.scroll_offsets.contains_key(key),
                    "non-scrollable {key}"
                );
            }
            program.prepare().unwrap();
            program.commit_prepared();
            let tree = &program.committed.as_ref().unwrap().1;
            assert_eq!(tree.get("home").unwrap().bounds, home);
            let input = tree.get("url").unwrap();
            assert!(input.bounds.y >= tree.get("main").unwrap().content_bounds.y);
            assert!(input.bounds.bottom() <= tree.get("main").unwrap().content_bounds.bottom());
            program.reveal("home");
            program.prepare().unwrap();
            program.commit_prepared();
            assert_eq!(
                program
                    .committed
                    .as_ref()
                    .unwrap()
                    .1
                    .get("home")
                    .unwrap()
                    .bounds,
                home
            );
        }
    }

    #[test]
    fn sidebar_list_anchor_glyphs_survive_inherited_background_painting() {
        let mut program = program();
        let link = |label: &str| Container {
            element: Element::ListItem,
            children: vec![Container {
                str_id: Some(label.into()),
                element: Element::Anchor {
                    href: Some(format!("/{label}")),
                    target: None,
                },
                children: vec![Container {
                    element: Element::Text {
                        value: label.into(),
                    },
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    background: Some(Color::try_from_hex("#111111").unwrap()),
                    children: vec![Container {
                        element: Element::Aside,
                        children: vec![Container {
                            element: Element::UnorderedList,
                            children: vec![link("Home"), link("Albums")],
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        for label in ["Home", "Albums"] {
            let node = program.candidate.as_ref().unwrap().get(label).unwrap();
            assert_eq!(
                node.text
                    .iter()
                    .map(|fragment| fragment.text.as_str())
                    .collect::<String>(),
                label
            );
            assert_ne!(node.hit_regions, [] as [layout::CellRect; 0]);
            assert!(node.style.style.bg.is_none());
        }
        let mut terminal = bmux_tui::terminal::Terminal::new(
            Vec::new(),
            bmux_tui::geometry::Rect::new(0, 0, 80, 24),
        );
        terminal.draw(|cx| program.paint(cx)).unwrap();
        let output = String::from_utf8_lossy(terminal.writer());
        assert!(
            output.contains("Home"),
            "sidebar Home glyphs must be painted"
        );
        assert!(
            output.contains("Albums"),
            "sidebar Albums glyphs must be painted"
        );
    }

    #[test]
    fn image_only_anchor_keeps_image_alt_and_interaction_geometry() {
        let mut program = program();
        program.set_graphics_available(false);
        program
            .apply_view(
                View::from(Container {
                    background: Some(Color::try_from_hex("#111111").unwrap()),
                    children: vec![Container {
                        str_id: Some("image-link".into()),
                        element: Element::Anchor {
                            href: Some("/settings".into()),
                            target: None,
                        },
                        children: vec![Container {
                            str_id: Some("icon".into()),
                            element: Element::Image {
                                source: None,
                                alt: Some("Settings icon".into()),
                                fit: None,
                                source_set: None,
                                sizes: None,
                                loading: None,
                            },
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        let tree = program.candidate.as_ref().unwrap();
        assert!(tree.get("image-link").unwrap().bounds.width > 0);
        assert!(tree.get("icon").unwrap().bounds.width > 0);
        let mut terminal = bmux_tui::terminal::Terminal::new(
            Vec::new(),
            bmux_tui::geometry::Rect::new(0, 0, 80, 24),
        );
        terminal.draw(|cx| program.paint(cx)).unwrap();
        assert!(String::from_utf8_lossy(terminal.writer()).contains("Settings icon"));
    }

    #[test]
    fn resize_actions_use_new_geometry_without_publishing_hit_test_layout() {
        use hyperchad_actions::{
            Action, ActionTrigger, ActionType,
            logic::{CalcValue, Value},
        };
        let mut program = program();
        program.resize(Size::new(40, 20));
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("visualization".into()),
                    width: Some(hyperchad_transformer::Number::IntegerPercent(100)),
                    actions: vec![Action {
                        trigger: ActionTrigger::Resize,
                        effect: ActionType::Parameterized {
                            action: Box::new(ActionType::Custom {
                                action: "width".into(),
                            }),
                            value: Value::Calc(CalcValue::WidthPx {
                                target: hyperchad_transformer_models::ElementTarget::SelfTarget,
                            }),
                        }
                        .into(),
                    }],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        program.commit_prepared();
        let committed_width = program.committed.as_ref().unwrap().1.nodes[0].bounds.width;
        assert_eq!(committed_width, 40);
        program.terminal(&Event::Resize(Size::new(80, 20))).unwrap();
        let Some(Work::Custom(name, Some(Value::Real(width)))) = program.pending.front() else {
            panic!("resize must enqueue the width action");
        };
        assert_eq!(name, "width");
        assert!(
            (*width - 640.0).abs() < f32::EPSILON,
            "resize width: {width}"
        );
        assert_eq!(
            program.committed.as_ref().unwrap().1.nodes[0].bounds.width,
            committed_width
        );
    }

    #[test]
    fn settings_load_routes_survive_repeated_sibling_responses_without_refetch() {
        use hyperchad_transformer_models::Route;
        let mut program = program();
        let section = |id: &str, url: &str| Container {
            str_id: Some(id.into()),
            hidden: Some(true),
            route: Some(Route::Get {
                route: url.into(),
                trigger: Some("load".into()),
                target: hyperchad_transformer_models::Selector::SelfTarget,
                strategy: hyperchad_transformer_models::SwapStrategy::This,
            }),
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    children: vec![
                        section("scan", "/settings/scan-settings"),
                        section("music", "https://example.test/settings/music-api-settings"),
                        Container {
                            str_id: Some("sibling".into()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        assert_eq!(program.pending.len(), 2);
        program.pending.clear();
        for _ in 0..3 {
            program
                .apply_response(
                    View::from(Container::default()),
                    Some("sibling"),
                    hyperchad_transformer_models::SwapStrategy::Children,
                )
                .unwrap();
            assert!(program.pending.is_empty());
        }
        program
            .apply_view(
                View::from(section("scan", "/settings/scan-settings")),
                Some("scan"),
            )
            .unwrap();
        assert_eq!(program.pending.len(), 1);
        assert!(
            matches!(program.pending.front(), Some(Work::Host { completion: Completion::Request(request), .. })
            if request.url == "/settings/scan-settings")
        );
        program.pending.clear();
        program
            .apply_response(
                View::from(section("new", "/settings/new")),
                Some("sibling"),
                hyperchad_transformer_models::SwapStrategy::Children,
            )
            .unwrap();
        assert_eq!(program.pending.len(), 1);
        assert!(
            matches!(program.pending.front(), Some(Work::Host { completion: Completion::Request(request), .. })
            if request.url == "/settings/new")
        );
        program.pending.clear();
        program
            .apply_response(
                View::from(Container::default()),
                Some("sibling"),
                hyperchad_transformer_models::SwapStrategy::BeforeEnd,
            )
            .unwrap();
        assert!(program.pending.is_empty());
    }

    fn anonymous_settings_program() -> BmuxProgram {
        use hyperchad_transformer_models::{Route, Selector, SwapStrategy};
        let load = |name: &str| Container {
            route: Some(Route::Get {
                route: format!("/settings/{name}"),
                trigger: Some("load".into()),
                target: Selector::SelfTarget,
                strategy: SwapStrategy::This,
            }),
            ..Default::default()
        };
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    children: vec![
                        Container {
                            str_id: Some("settings-connections".into()),
                            children: vec![Container {
                                element: Element::Button { r#type: None },
                                route: Some(Route::Post {
                                    route: "/settings/new-connection".into(),
                                    trigger: None,
                                    target: Selector::Id("settings-connections".into()),
                                    strategy: SwapStrategy::This,
                                }),
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                        load("download-settings"),
                        load("scan-settings"),
                        load("music-api-settings"),
                    ],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        assert_eq!(program.pending.len(), 3);
        program
    }

    #[test]
    fn anonymous_settings_loads_and_queued_post_survive_sibling_responses() {
        let mut program = anonymous_settings_program();
        let music_source = program.state.find_node("@3").unwrap().clone();
        program.dispatch("@0/0", EventKind::Click, None).unwrap();
        assert_eq!(program.pending.len(), 4);
        let mut requests = Vec::new();
        while let Some(Work::Host {
            generation,
            completion: Completion::Request(request),
            ..
        }) = program.pending.pop_front()
        {
            requests.push((generation, request));
        }
        assert_eq!(requests.len(), 4);
        let source_epochs = program.node_epochs.clone();
        for (index, (generation, request)) in requests.iter().enumerate() {
            assert_eq!(
                program.node_epochs[&request.source.key], *generation,
                "queued request must remain current after preceding sibling responses"
            );
            let response = if index < 3 {
                Container {
                    children: vec![Container {
                        element: Element::Text {
                            value: format!("loaded-{index}"),
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                }
            } else {
                Container {
                    str_id: Some("settings-connections".into()),
                    children: vec![Container {
                        str_id: Some("created-form".into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }
            };
            program
                .complete(
                    *generation,
                    Completion::Request(request.clone()),
                    Ok(HostResponse::View(View::from(response))),
                )
                .unwrap();
            assert!(
                program.pending.is_empty(),
                "completed sibling must not restart other anonymous loads"
            );
            for (_, queued) in &requests[index + 1..] {
                assert_eq!(
                    program.node_epochs[&queued.source.key],
                    source_epochs[&queued.source.key]
                );
            }
        }
        assert!(program.state.find_node("created-form").is_some());
        // A genuine replacement restarts this source, but the previous request
        // incarnation can no longer replace the new source's content.
        let (old_epoch, old_request) = &requests[2];
        program
            .apply_view(View::from(music_source), Some("@3"))
            .unwrap();
        assert_eq!(program.pending.len(), 1);
        assert_ne!(program.node_epochs["@3"], *old_epoch);
        program.pending.clear();
        program
            .complete(
                *old_epoch,
                Completion::Request(old_request.clone()),
                Ok(HostResponse::View(View::from(Container {
                    str_id: Some("stale-response".into()),
                    ..Default::default()
                }))),
            )
            .unwrap();
        assert!(program.state.find_node("stale-response").is_none());
        assert!(program.pending.is_empty());
    }

    #[test]
    fn navigation_preserves_unchanged_image_bytes_and_epoch() {
        let mut program = program();
        let page = |label: &str, source: &str| Container {
            str_id: Some("page".into()),
            children: vec![
                Container {
                    str_id: Some("logo".into()),
                    element: Element::Image {
                        source: Some(source.into()),
                        alt: None,
                        fit: None,
                        source_set: None,
                        sizes: None,
                        loading: None,
                    },
                    ..Default::default()
                },
                Container {
                    element: Element::Raw {
                        value: label.into(),
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        program
            .apply_view(View::from(page("Home", "/logo.svg")), None)
            .unwrap();
        let epoch = program.resource_epochs["logo"];
        program
            .graphics
            .register_rgba("logo", 1, 1, vec![255; 4])
            .unwrap();
        program.pending.clear();
        for label in ["Settings", "Home", "Settings"] {
            program
                .apply_view(View::from(page(label, "/logo.svg")), None)
                .unwrap();
            assert_eq!(program.resource_epochs["logo"], epoch);
            assert!(program.graphics.rgba("logo").is_some());
            assert!(
                !program
                    .pending
                    .iter()
                    .any(|work| matches!(work, Work::Resource { .. }))
            );
        }
        program
            .apply_view(View::from(page("Home", "/different.svg")), None)
            .unwrap();
        assert_ne!(program.resource_epochs["logo"], epoch);
        assert!(program.graphics.rgba("logo").is_none());
    }

    #[test]
    fn text_controls_and_fallbacks_keep_application_background() {
        let mut program = program();
        program.background = Some(hyperchad_color::Color::from_hex("#181a1b"));
        program.set_graphics_available(false);
        program
            .apply_view(
                View::from(Container {
                    children: vec![
                        Container {
                            element: Element::Raw {
                                value: "Settings".into(),
                            },
                            ..Default::default()
                        },
                        Container {
                            element: Element::Button { r#type: None },
                            children: vec![Container {
                                element: Element::Raw {
                                    value: "Save".into(),
                                },
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                        Container {
                            element: Element::Image {
                                source: Some("/missing.svg".into()),
                                alt: Some("Missing icon".into()),
                                fit: None,
                                source_set: None,
                                sizes: None,
                                loading: None,
                            },
                            ..Default::default()
                        },
                        Container {
                            element: Element::Input {
                                autofocus: None,
                                name: Some("name".into()),
                                input: hyperchad_transformer::Input::Text {
                                    value: Some("Desktop".into()),
                                    placeholder: None,
                                },
                            },
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        let mut buffer =
            bmux_tui::buffer::Buffer::empty(bmux_tui::geometry::Rect::new(0, 0, 80, 24));
        let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
        program.paint(&mut PaintCx::new(&mut frame));
        assert!(buffer.cells().iter().any(|cell| cell.symbol == "S"));
        for cell in buffer.cells() {
            assert_eq!(
                cell.style.bg,
                Some(bmux_tui::style::Color::Rgb(24, 26, 27)),
                "{}",
                cell.symbol
            );
        }
    }

    #[test]
    fn anonymous_image_sibling_partials_preserve_rgba_and_single_queued_resource() {
        let mut program = program();
        let image = Container {
            element: Element::Image {
                source: Some("/art".into()),
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None,
            },
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    children: vec![
                        image,
                        Container {
                            str_id: Some("sibling".into()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        let epoch = program.resource_epochs["@0"];
        program
            .graphics
            .register_rgba("@0", 1, 1, vec![1, 2, 3, 255])
            .unwrap();
        // Keep an actual HTTP request behind the image while siblings change.
        program
            .apply_view(
                View::from(Container {
                    route: Some(hyperchad_transformer_models::Route::Get {
                        route: "/load".into(),
                        trigger: Some("load".into()),
                        target: hyperchad_transformer_models::Selector::SelfTarget,
                        strategy: hyperchad_transformer_models::SwapStrategy::This,
                    }),
                    str_id: Some("sibling".into()),
                    ..Default::default()
                }),
                Some("sibling"),
            )
            .unwrap();
        for index in 0..5 {
            program
                .apply_response(
                    View::from(Container {
                        children: vec![Container {
                            str_id: Some(format!("fragment-{index}")),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    Some("sibling"),
                    hyperchad_transformer_models::SwapStrategy::Children,
                )
                .unwrap();
            assert_eq!(program.resource_epochs["@0"], epoch);
            assert_eq!(
                program.graphics.rgba("@0"),
                Some((1, 1, &[1, 2, 3, 255][..]))
            );
            assert_eq!(
                program
                    .pending
                    .iter()
                    .filter(|work| matches!(work, Work::Resource { .. }))
                    .count(),
                1
            );
        }
        assert!(program.pending.iter().any(|work| matches!(
            work,
            Work::Host {
                completion: Completion::Request(_),
                ..
            }
        )));
        program.pending.clear();
        program.schedule_resources().unwrap();
        assert!(program.pending.is_empty());
        assert_eq!(
            program.graphics.rgba("@0"),
            Some((1, 1, &[1, 2, 3, 255][..]))
        );
    }

    #[test]
    fn semantic_effects_bypass_image_backlog_preserving_each_fifo() {
        let mut program = program();
        program
            .schedule_resource("first".into(), "/one".into())
            .unwrap();
        program.pending.push_back(Work::Navigate("/first".into()));
        program
            .schedule_resource("second".into(), "/two".into())
            .unwrap();
        program.pending.push_back(Work::Navigate("/second".into()));
        assert!(matches!(program.next_work(), Some(Work::Navigate(url)) if url == "/first"));
        assert!(matches!(program.next_work(), Some(Work::Navigate(url)) if url == "/second"));
        assert!(matches!(program.next_work(), Some(Work::Resource { key, .. }) if key == "first"));
        assert!(matches!(program.next_work(), Some(Work::Resource { key, .. }) if key == "second"));
        assert!(program.next_work().is_none());
    }

    #[test]
    fn stale_queued_resources_are_coalesced_and_removed() {
        let mut program = program();
        program
            .schedule_resource("obsolete".into(), "/first".into())
            .unwrap();
        program
            .schedule_resource("obsolete".into(), "/second".into())
            .unwrap();
        assert_eq!(program.pending.len(), 1);
        program.schedule_resources().unwrap();
        assert!(program.pending.is_empty());
    }

    #[test]
    fn identical_keyed_immediate_replacement_starts_a_new_lifecycle() {
        use hyperchad_actions::{Action, ActionTrigger, ActionType};
        let source = || Container {
            str_id: Some("immediate".into()),
            actions: vec![Action {
                trigger: ActionTrigger::Immediate,
                effect: ActionType::Custom {
                    action: "mounted".into(),
                }
                .into(),
            }],
            ..Default::default()
        };
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    children: vec![
                        source(),
                        Container {
                            str_id: Some("sibling".into()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        assert!(
            matches!(program.pending.pop_front(), Some(Work::Custom(name, _)) if name == "mounted")
        );
        let previous_epoch = program.node_epochs["immediate"];
        program
            .apply_view(View::from(source()), Some("immediate"))
            .unwrap();
        assert!(program.node_epochs["immediate"] > previous_epoch);
        assert!(
            matches!(program.pending.pop_front(), Some(Work::Custom(name, _)) if name == "mounted")
        );
        program
            .apply_response(
                View::from(Container::default()),
                Some("sibling"),
                hyperchad_transformer_models::SwapStrategy::Children,
            )
            .unwrap();
        assert!(program.pending.is_empty());
    }

    #[test]
    fn unrelated_dispatch_does_not_invalidate_resource_epoch() {
        let mut program = program();
        program
            .apply_view(View::from(Container::default()), None)
            .unwrap();
        let generation = program.state.generation();
        program
            .update(RuntimeEvent::Message(Message::Emit(
                "unregistered".into(),
                None,
            )))
            .unwrap();
        program
            .update(RuntimeEvent::Message(Message::Advance))
            .unwrap();
        assert_eq!(program.state.generation(), generation);
    }

    #[test]
    fn canonical_limit_rejection_is_atomic() {
        let mut program = program();
        program.shared = Arc::new(Shared {
            stopping: std::sync::atomic::AtomicBool::new(false),
            config: BmuxConfig {
                max_nodes: 1,
                ..Default::default()
            },
            receiver: std::sync::Mutex::new(None),
            navigation_tx: program.shared.navigation_tx.clone(),
            request_action: program.shared.request_action.clone(),
            on_resize: program.shared.on_resize.clone(),
        });
        let before = program.state.root().clone();
        let generation = program.state.generation();
        let root = Container {
            children: vec![Container::default()],
            ..Default::default()
        };
        assert!(program.apply_view(View::from(root), None).is_err());
        assert_eq!(*program.state.root(), before);
        assert_eq!(program.state.generation(), generation);
    }

    #[tokio::test]
    async fn idle_resource_observer_wakes_only_on_epoch_change_and_cancels_on_shutdown() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let epoch = Arc::new(AtomicU64::new(1));
        let callback_epoch = Arc::clone(&epoch);
        let callback: Arc<dyn Fn() -> u64 + Send + Sync> =
            Arc::new(move || callback_epoch.load(Ordering::SeqCst));
        let (actions, _) = flume::bounded(8);
        let (resize, _) = flume::bounded(8);
        let mut program = BmuxProgram::new(
            BmuxConfig {
                resource_context: Some(Arc::clone(&callback)),
                ..BmuxConfig::default()
            },
            actions,
            resize,
        );
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("art".into()),
                    element: Element::Image {
                        source: Some("/same".into()),
                        alt: None,
                        fit: None,
                        source_set: None,
                        sizes: None,
                        loading: None,
                    },
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.pending.clear();
        program
            .graphics
            .register_rgba("art", 1, 1, vec![255, 0, 0, 255])
            .unwrap();
        let old_serial = program.resource_epochs["art"];
        let update = program.drive(Update::none());
        assert_eq!(update.subscriptions.len(), 1);
        assert_eq!(update.invalidation, bmux_tui_runtime::Invalidation::None);
        // A redraw/presentation must not replace the running observer task.
        assert!(program.drive(Update::none()).subscriptions.is_empty());
        let ticks = AtomicU64::new(0);
        let mut notifications = 0;
        observe_resource_context(
            callback,
            1,
            || {
                let tick = ticks.fetch_add(1, Ordering::SeqCst) + 1;
                assert!(
                    tick <= 3,
                    "observer must exit when notification admission closes"
                );
                if tick == 3 {
                    epoch.store(2, Ordering::SeqCst);
                }
                std::future::ready(())
            },
            || {
                notifications += 1;
                let update = program
                    .update(RuntimeEvent::Message(Message::ResourceContextChanged))
                    .unwrap();
                assert_eq!(update.invalidation, bmux_tui_runtime::Invalidation::Redraw);
                assert_eq!(update.commands.len(), 1, "idle wake must drive the reload");
                assert!(update.subscriptions.is_empty());
                assert!(program.graphics.rgba("art").is_none());
                assert!(program.resource_epochs["art"] > old_serial);
                // Simulate a duplicate queued wake: no redundant frame.
                let unchanged = program
                    .update(RuntimeEvent::Message(Message::ResourceContextChanged))
                    .unwrap();
                assert_eq!(unchanged.invalidation, bmux_tui_runtime::Invalidation::None);
                std::future::ready(false)
            },
        )
        .await;
        assert_eq!(ticks.load(Ordering::SeqCst), 3);
        assert_eq!(notifications, 1, "unchanged polls must not notify/redraw");
        let update = program
            .update(RuntimeEvent::Message(Message::Shutdown))
            .unwrap();
        let key = bmux_tui_runtime::SubscriptionKey::new("hyperchad-resource-context");
        assert!(update.cancelled_subscriptions.contains(&key));
        assert!(update.subscriptions.is_empty());
        assert!(program.resource_observer.is_none());
        epoch.store(3, Ordering::SeqCst);
        let serial = program.resource_epochs["art"];
        let late = program
            .update(RuntimeEvent::Message(Message::ResourceContextChanged))
            .unwrap();
        assert_eq!(late.invalidation, bmux_tui_runtime::Invalidation::None);
        assert_eq!(program.resource_epochs["art"], serial);
        let mut no_callback = super::runtime_regressions::program();
        let disabled = no_callback.drive(Update::none());
        assert!(disabled.subscriptions.is_empty());
        assert!(disabled.cancelled_subscriptions.contains(&key));
    }

    #[test]
    fn resource_context_change_invalidates_decoded_pending_and_inflight_identity() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let epoch = Arc::new(AtomicU64::new(1));
        let callback_epoch = Arc::clone(&epoch);
        let (actions, _) = flume::bounded(8);
        let (resize, _) = flume::bounded(8);
        let mut program = BmuxProgram::new(
            BmuxConfig {
                resource_context: Some(Arc::new(move || callback_epoch.load(Ordering::SeqCst))),
                ..BmuxConfig::default()
            },
            actions,
            resize,
        );
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("art".into()),
                    element: Element::Image {
                        source: Some("/same?moosicboxProfile=master".into()),
                        alt: None,
                        fit: None,
                        source_set: None,
                        sizes: None,
                        loading: None,
                    },
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        let old_serial = program.resource_epochs["art"];
        let document_generation = program.state.generation();
        program
            .graphics
            .register_rgba("art", 1, 1, vec![255, 0, 0, 255])
            .unwrap();
        epoch.store(2, Ordering::SeqCst);
        program.prepare().unwrap();
        assert!(program.graphics.rgba("art").is_none());
        assert_eq!(program.pending.len(), 1);
        let new_serial = program.resource_epochs["art"];
        assert!(new_serial > old_serial);
        assert_eq!(program.state.generation(), document_generation);
        program
            .complete(
                old_serial,
                Completion::Resource {
                    key: "art".into(),
                    source: "/same?moosicboxProfile=master".into(),
                },
                Ok(HostResponse::Bytes(vec![0])),
            )
            .unwrap();
        assert!(program.graphics.rgba("art").is_none());
        assert_eq!(program.resource_epochs["art"], new_serial);
        // Failed/successful attempts are stable within the context, but a fresh
        // context must retry even the identical source after an earlier failure.
        program.pending.clear();
        program
            .complete(
                new_serial,
                Completion::Resource {
                    key: "art".into(),
                    source: "/same?moosicboxProfile=master".into(),
                },
                Err(std::io::Error::other("denied").into()),
            )
            .unwrap();
        program.prepare().unwrap();
        assert!(program.pending.is_empty());
        epoch.store(3, Ordering::SeqCst);
        program
            .update(RuntimeEvent::Message(Message::Advance))
            .unwrap();
        assert!(program.resource_epochs["art"] > new_serial);
    }

    #[test]
    fn stale_resource_recreation_does_not_install_bytes() {
        let mut program = program();
        program.resources.insert("image".into(), "same".into());
        program.resource_epochs.insert("image".into(), 2);
        program
            .update(RuntimeEvent::Message(Message::Completed {
                generation: 1,
                work: Completion::Resource {
                    key: "image".into(),
                    source: "same".into(),
                },
                result: Ok(HostResponse::Bytes(vec![0])),
            }))
            .unwrap();
        assert!(program.graphics.rgba("image").is_none());
    }

    #[test]
    fn view_response_navigation_reaches_ordered_effect_queue() {
        let mut program = program();
        let mut view = View::from(Container::default());
        view.response.navigation =
            Some(hyperchad_renderer::ResponseNavigation::internal("/next").unwrap());
        program.apply_view(view, None).unwrap();
        assert!(matches!(program.pending.front(), Some(Work::Navigate(url)) if url == "/next"));
    }

    #[test]
    fn responsive_projection_does_not_mutate_canonical_root() {
        let mut root = Container::default();
        root.overrides.push(hyperchad_transformer::ConfigOverride {
            condition: "small".into(),
            overrides: vec![hyperchad_transformer::OverrideItem::Hidden(true)],
            default: None,
        });
        let mut triggers = BTreeMap::new();
        triggers.insert(
            "small".into(),
            ResponsiveTrigger::MaxWidth(hyperchad_transformer::Number::from(800)),
        );
        let original = root.clone();
        resolve_responsive(&mut root, &triggers, Size::new(80, 24));
        assert_eq!(root.hidden, Some(true));
        assert_eq!(original.hidden, None);
    }
}
