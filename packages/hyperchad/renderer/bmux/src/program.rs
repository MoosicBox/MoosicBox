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
    /// Correlated router result; checked again on the sole state owner.
    NavigationComplete {
        id: u64,
        url: String,
        view: Option<View>,
    },
    Canvas(CanvasUpdate),
    Emit(String, Option<String>),
    Responsive(String, ResponsiveTrigger),
    Background(Option<Color>),
    Shutdown,
    /// Immediate user quit, independent of network work.
    Quit,
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
    Resource {
        key: String,
        source: String,
    },
    File {
        key: String,
    },
    Request(crate::actions::RequestEffect),
    /// Read-only load effects use a separate bounded lane from user mutations.
    Read(crate::actions::RequestEffect),
    Other,
}

struct ReadInFlight {
    key: String,
    epoch: u64,
    request_id: Option<String>,
    cancel: tokio::sync::watch::Sender<bool>,
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
// Scheduler lane occupancy, lifecycle and terminal capability are independent flags.
#[allow(clippy::struct_excessive_bools)]
pub struct BmuxProgram {
    shared: Arc<Shared>,
    pub state: ViewState,
    pub controls: Controls,
    pub graphics: GraphicsStore,
    actions: ActionEngine,
    options: LayoutOptions,
    candidate: Option<LayoutTree>,
    effective: Option<Container>,
    prepared_source: Option<Container>,
    scroll_reuse: bool,
    committed: Option<(u64, LayoutTree)>,
    pending: VecDeque<Work>,
    active: bool,
    resource_active: usize,
    quitting: bool,
    stopping: bool,
    active_local_mutation: Option<(String, u64)>,
    read_active: usize,
    read_aborts: Vec<ReadInFlight>,
    active_abort: Option<(Option<String>, tokio::sync::watch::Sender<bool>)>,
    resources: BTreeMap<String, String>,
    resource_epochs: BTreeMap<String, u64>,
    // Successful admissions are distinct from failures: only successes may reload
    // after eviction, once per reveal (never continuously under cache pressure).
    resource_loaded: BTreeSet<String>,
    resource_visible: BTreeSet<String>,
    // Only resident decoded entries are indexed; the graphics store owns the budget.
    resource_sources: BTreeMap<String, String>,
    resource_serial: u64,
    resource_context: Option<u64>,
    resource_scan_counts: Option<(usize, usize)>,
    resource_observer: Option<bmux_tui_runtime::SubscriptionKey>,
    node_epochs: BTreeMap<String, u64>,
    key_chord: Vec<hyperchad_actions::Key>,
    http_headers: Option<BTreeMap<String, String>>,
    responsive: BTreeMap<String, ResponsiveTrigger>,
    background: Option<Color>,
    styles_changed: bool,
    loaded_routes: BTreeMap<String, hyperchad_transformer_models::Route>,
    resource_paint_only: bool,
    fixed_image_geometry: bool,
    geometry_dirty: bool,
    presentation_started: Option<Instant>,
    clock: Instant,
    location: String,
    navigation_serial: u64,
    navigation_pending: Option<(u64, String, Option<u64>)>,
    history_back: crate::history::DocumentHistory,
    history_forward: crate::history::DocumentHistory,
    restoring_history: bool,
    history_document_valid: bool,
    restored_immediate: BTreeSet<String>,
    completed_loads: BTreeSet<String>,
    resumed_loads: BTreeSet<(String, u64)>,
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
            prepared_source: None,
            scroll_reuse: false,
            committed: None,
            pending: VecDeque::new(),
            active: false,
            resource_active: 0,
            quitting: false,
            stopping: false,
            active_local_mutation: None,
            read_active: 0,
            read_aborts: Vec::new(),
            active_abort: None,
            resources: BTreeMap::new(),
            resource_epochs: BTreeMap::new(),
            resource_loaded: BTreeSet::new(),
            resource_visible: BTreeSet::new(),
            resource_sources: BTreeMap::new(),
            resource_serial: 0,
            resource_context: None,
            resource_scan_counts: None,
            resource_observer: None,
            node_epochs: BTreeMap::new(),
            key_chord: Vec::new(),
            http_headers: None,
            responsive: BTreeMap::new(),
            background: None,
            styles_changed: false,
            loaded_routes: BTreeMap::new(),
            resource_paint_only: false,
            fixed_image_geometry: false,
            geometry_dirty: true,
            presentation_started: None,
            clock: Instant::now(),
            location: String::new(),
            navigation_serial: 0,
            navigation_pending: None,
            history_back: crate::history::DocumentHistory::default(),
            history_forward: crate::history::DocumentHistory::default(),
            restoring_history: false,
            history_document_valid: true,
            restored_immediate: BTreeSet::new(),
            completed_loads: BTreeSet::new(),
            resumed_loads: BTreeSet::new(),
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
        self.geometry_dirty = true;
        self.scroll_reuse = false;
        self.options.viewport = size;
    }

    /// Prepare authoritative geometry; no output, actions or committed input changes occur.
    ///
    /// # Errors
    /// * Returns layout limit, invalid geometry or canvas resource errors.
    pub fn prepare(&mut self) -> Result<(), Error> {
        let started = Instant::now();
        self.revalidate_resource_context()?;
        let scroll_reuse = self.log_scroll_admission();
        if scroll_reuse
            && self.fixed_image_geometry
            && self.prepared_source.as_ref() == Some(self.state.root())
            && self.options.open_details == self.controls.details_open()
            && let Some((generation, tree)) = &self.committed
            && *generation == self.state.generation()
            && tree.viewport == self.options.viewport
            && let Some(root) = &self.effective
            && let Some(tree) = layout::reproject_scroll(root, tree, &self.options.scroll_offsets)
        {
            self.controls.prepare(&tree);
            self.candidate = Some(tree);
            self.geometry_dirty = false;
            self.schedule_resources()?;
            let elapsed_us = started.elapsed().as_micros();
            log::debug!("bmux prepare scroll-reuse elapsed_us={elapsed_us} full_layouts=0");
            return Ok(());
        }
        // A bounded image completion cannot alter explicitly sized image boxes.
        // Reuse only committed geometry of this exact document; input/view/context
        // changes reset this permission before any presentation is prepared.
        if std::mem::take(&mut self.resource_paint_only)
            && self.fixed_image_geometry
            && !self.geometry_dirty
            && let Some((generation, tree)) = &self.committed
            && *generation == self.state.generation()
            && tree.viewport == self.options.viewport
        {
            self.candidate = Some(tree.clone());
            self.schedule_resources()?;
            log::debug!(
                "bmux prepare resource-only elapsed_us={}",
                started.elapsed().as_micros()
            );
            return Ok(());
        }
        self.options.open_details = self.controls.details_open();
        let mut effective = self.state.root().clone();
        resolve_responsive(&mut effective, &self.responsive, self.options.viewport);
        let root = &effective;
        let controls = self.controls.measurer(root);
        let mut keys = BTreeMap::new();
        let mut fixed_image_geometry = true;
        visit(root, &mut Vec::new(), &mut |node, path| {
            if matches!(node.element, Element::Image { .. }) {
                fixed_image_geometry &=
                    matches!(node.width, Some(hyperchad_transformer::Number::Integer(_)))
                        && matches!(node.height, Some(hyperchad_transformer::Number::Integer(_)));
            }
            keys.insert(
                std::ptr::from_ref(node) as usize,
                layout::node_key(node, path),
            );
        });
        let measure = |node: &Container, constraints: Constraints| {
            self.measure_image(keys.get(&(std::ptr::from_ref(node) as usize)))
                .unwrap_or_else(|| controls.measure(node, constraints))
        };
        let layout_started = Instant::now();
        let tree = layout::layout(root, &self.options, &measure)?;
        let layout_us = layout_started.elapsed().as_micros();
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
        self.fixed_image_geometry = fixed_image_geometry;
        self.prepared_source = Some(self.state.root().clone());
        self.geometry_dirty = false;
        self.effective = Some(effective);
        self.candidate = Some(tree);
        let resources_started = Instant::now();
        self.schedule_resources()?;
        log::debug!(
            "bmux prepare elapsed_us={} layout_us={layout_us} resources_us={}",
            started.elapsed().as_micros(),
            resources_started.elapsed().as_micros()
        );
        Ok(())
    }

    /// Paint only a previously prepared layout into a local, host-clipped context.
    pub fn paint(&mut self, cx: &mut PaintCx<'_, '_>) {
        self.presentation_started = Some(Instant::now());
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
                Ok(GraphicsPlacement::Image(mut image)) => {
                    // Terminal Rect clips negative coordinates. Keep the original
                    // allocation for fitting; only destination/clip may be cropped.
                    let Some(allocation) = image_allocation(node.content_bounds) else {
                        continue;
                    };
                    image.allocation = Some(allocation);
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
        if let Some(started) = self.presentation_started.take() {
            log::debug!(
                "bmux presentation paint-through-flush elapsed_us={}",
                started.elapsed().as_micros()
            );
        }
        if let Some(tree) = self.candidate.take() {
            self.committed = Some((self.state.generation(), tree));
        }
    }

    /// Discard failed preparation and restore committed control event geometry.
    pub fn discard_prepared(&mut self) {
        self.presentation_started = None;
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
        self.resource_loaded.remove(&key);
        self.resource_sources.remove(&key);
        self.graphics.remove(&key);
        self.resources.insert(key.clone(), source);
        self.resource_epochs.insert(key, serial);
        Ok(())
    }

    fn image_sources(&self) -> Vec<(String, String, bool)> {
        let mut images = Vec::new();
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            if let Element::Image {
                source: Some(source),
                loading,
                ..
            } = &node.element
            {
                images.push((
                    layout::node_key(node, path),
                    source.clone(),
                    *loading == Some(hyperchad_transformer_models::ImageLoading::Lazy),
                ));
            }
        });
        images
    }

    fn schedule_resources(&mut self) -> Result<(), Error> {
        let images = self.image_sources();
        let counts = (
            images.len(),
            images
                .iter()
                .map(|(_, source, _)| source)
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
        let present: BTreeSet<_> = images.iter().map(|(key, _, _)| key.clone()).collect();
        self.resources.retain(|key, _| present.contains(key));
        self.resource_epochs.retain(|key, _| present.contains(key));
        self.resource_loaded.retain(|key| present.contains(key));
        self.resource_sources
            .retain(|key, _| self.graphics.rgba(key).is_some());
        let visible: BTreeSet<_> = images
            .iter()
            .filter(|(key, _, _)| {
                // Eager means load as soon as displayed, not load hidden menus
                // and every offscreen placeholder into the resident graphics LRU.
                self.candidate
                    .as_ref()
                    .or_else(|| {
                        self.committed
                            .as_ref()
                            .filter(|(generation, _)| *generation == self.state.generation())
                            .map(|(_, tree)| tree)
                    })
                    .is_some_and(|tree| {
                        tree.get(key).is_some_and(|node| {
                            node.visible
                                && node.bounds.intersection(node.clip).width > 0
                                && node.bounds.intersection(node.clip).height > 0
                        })
                    })
            })
            .map(|(key, _, _)| key.clone())
            .collect();
        self.prune_resource_queue(&visible);
        for (key, source, _) in images {
            if !visible.contains(&key) {
                continue;
            }
            let admitted = self.resources.get(&key) == Some(&source);
            // Explicit host registration is already-authorized resident artwork.
            // Bind it on first visible admission rather than scheduling a fetch
            // that removes the registered pixels before the initial paint.
            if !admitted
                && !self.resources.contains_key(&key)
                && !self.resource_sources.contains_key(&key)
                && self.graphics.rgba(&key).is_some()
            {
                self.resource_loaded.insert(key.clone());
                self.resource_sources.insert(key.clone(), source.clone());
                self.resources.insert(key, source);
                continue;
            }
            let evicted = admitted
                && self.resource_loaded.contains(&key)
                && self.graphics.rgba(&key).is_none();
            // Retry eviction only on a genuine visibility transition. Retrying
            // resident eager aliases every completion can cycle the entire LRU.
            let evicted_reveal = evicted && !self.resource_visible.contains(&key);
            if !admitted || evicted_reveal {
                let cached = self
                    .resource_sources
                    .iter()
                    .chain(self.resources.iter())
                    .find_map(|(other, cached)| {
                        (cached == &source && self.graphics.rgba(other).is_some())
                            .then(|| other.clone())
                    });
                if let Some(cached) = cached {
                    self.resource_sources.remove(&key);
                    self.graphics.clone_resource(&cached, &key)?;
                    self.resource_loaded.insert(key.clone());
                    self.resource_sources.insert(key.clone(), source.clone());
                    self.resources.insert(key, source);
                } else if (!admitted || evicted_reveal)
                    && !self.resources.iter().any(|(other, scheduled)| {
                        scheduled == &source && !self.resource_loaded.contains(other)
                    })
                {
                    self.schedule_resource(key, source)?;
                }
            }
        }
        self.resource_visible = visible;
        self.resource_sources
            .retain(|key, _| self.graphics.rgba(key).is_some());
        Ok(())
    }

    // Queued lazy loads must not sit ahead of newly revealed artwork. Active
    // requests keep their identities and the same four-request concurrency cap.
    fn prune_resource_queue(&mut self, visible: &BTreeSet<String>) {
        let obsolete: BTreeSet<_> = self
            .pending
            .iter()
            .filter_map(|work| {
                if let Work::Resource { key, .. } = work {
                    (!visible.contains(key)).then(|| key.clone())
                } else {
                    None
                }
            })
            .collect();
        for key in obsolete {
            self.resources.remove(&key);
            self.resource_epochs.remove(&key);
        }
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
    }

    fn revalidate_resource_context(&mut self) -> Result<(), Error> {
        let context = self
            .shared
            .config
            .resource_context
            .as_ref()
            .map(|context| context());
        // History may have been populated independently of artwork. Purge before
        // any pending-navigation or document-validity checks, on every observation.
        self.history_back.set_context(context);
        self.history_forward.set_context(context);
        if context != self.resource_context {
            self.resource_paint_only = false;
            self.geometry_dirty = true;
            log::debug!(
                "bmux resource context changed cached_nodes={} queued_resources={}",
                self.resources.len(),
                self.pending
                    .iter()
                    .filter(|work| matches!(work, Work::Resource { .. }))
                    .count()
            );
            self.history_document_valid = false;
            self.history_back.set_context(context);
            self.history_forward.set_context(context);
            self.navigation_pending = None;
            let retained_mutation = self
                .active_local_mutation
                .as_ref()
                .filter(|(key, epoch)| self.node_epochs.get(key) == Some(epoch))
                .cloned();
            self.node_epochs.clear();
            if let Some((key, epoch)) = retained_mutation {
                self.node_epochs.insert(key, epoch);
            }
            self.pending
                .retain(|work| matches!(work, Work::Resource { .. }));
            if self.active_local_mutation.is_none()
                && let Some((_, cancel)) = &self.active_abort
            {
                let _ = cancel.send(true);
            }
            self.restored_immediate.clear();
            self.resource_scan_counts = None;
            // Purge successes and failures together; neither may cross authorization
            // contexts even when the source string is unchanged. In-flight work keeps
            // running, but its unique serial can no longer install a completion.
            for key in self.resources.keys().chain(self.resource_sources.keys()) {
                self.graphics.remove(key);
            }
            self.resources.clear();
            self.resource_epochs.clear();
            self.resource_loaded.clear();
            self.resource_visible.clear();
            self.resource_sources.clear();
            self.pending
                .retain(|work| !matches!(work, Work::Resource { .. }));
            self.resource_context = context;
        }
        self.schedule_resources()
    }

    #[cfg(test)]
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

    fn observe_resources(&mut self, update: &mut Update<Message>) {
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
    }

    fn abort_reads(&self, id: &str) {
        for read in &self.read_aborts {
            if read.request_id.as_deref() == Some(id) {
                let _ = read.cancel.send(true);
            }
        }
    }

    fn cancel_stale_reads(&mut self) {
        // Discard read-only requests whose source was replaced or invalidated.
        // Their cooperative abort frees bounded read capacity without cancelling
        // an ordered mutation or accepting stale response content.
        self.read_aborts.retain(|read| {
            if self.stopping || self.node_epochs.get(&read.key) != Some(&read.epoch) {
                let _ = read.cancel.send(true);
                false
            } else {
                read.cancel.receiver_count() > 0
            }
        });
    }

    fn drive(&mut self, mut update: Update<Message>) -> Update<Message> {
        self.cancel_stale_reads();
        if self.quitting {
            update.lifecycle = bmux_tui_runtime::Lifecycle::Exit;
            return update;
        }
        self.observe_resources(&mut update);
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
        if self.stopping
            && !self.active
            && self.read_active == 0
            && self.resource_active == 0
            && self.pending.is_empty()
        {
            update.lifecycle = bmux_tui_runtime::Lifecycle::Exit;
        }
        // Navigation hands intent to the application's independent router. Do not
        // serialize it behind in-flight HTTP requests or decorative asset loads.
        // Its completion must not release the ordered host-effect lane.
        let mut navigations = Vec::new();
        while let Some(index) = self
            .pending
            .iter()
            .position(|work| matches!(work, Work::Navigate(_)))
        {
            let Some(Work::Navigate(url)) = self.pending.remove(index) else {
                unreachable!()
            };
            let Some(id) = self.navigation_serial.checked_add(1) else {
                log::error!("navigation serial exhausted");
                continue;
            };
            self.navigation_serial = id;
            self.navigation_pending = Some((id, url.clone(), self.resource_context));
            navigations.push(crate::renderer::NavigationRequest { id, url });
        }
        if !navigations.is_empty() {
            let navigation = self.shared.navigation_tx.clone();
            update.commands.push(Command::concurrent(async move {
                for url in navigations {
                    if navigation.send_async(url).await.is_err() {
                        log::warn!("application navigation bridge closed");
                        break;
                    }
                }
                Some(Message::Advance)
            }));
        }
        while self.resource_active < 4
            && let Some(index) = self
                .pending
                .iter()
                .position(|work| matches!(work, Work::Resource { .. }))
            && let Some(work) = self.pending.remove(index)
        {
            update.commands.push(self.start_work(work));
        }
        if let Some(index) = self.pending.iter().position(|work| {
            matches!(
                work,
                Work::Host {
                    completion: Completion::Read(_),
                    ..
                }
            )
        }) && self.read_active < 4
            && let Some(work) = self.pending.remove(index)
        {
            update.commands.push(self.start_work(work));
        }
        if self.active {
            return update;
        }
        let Some(index) = self.pending.iter().position(|work| {
            !matches!(
                work,
                Work::Resource { .. }
                    | Work::Host {
                        completion: Completion::Read(_),
                        ..
                    }
            )
        }) else {
            return update;
        };
        let Some(work) = self.pending.remove(index) else {
            unreachable!()
        };
        update.commands.push(self.start_work(work));
        update
    }

    fn track_work(
        &mut self,
        work: &Work,
        completion: &Completion,
        generation: u64,
        resource: bool,
        abort_tx: tokio::sync::watch::Sender<bool>,
    ) {
        let read = matches!(
            work,
            Work::Host {
                completion: Completion::Read(_),
                ..
            }
        );
        if resource {
            self.resource_active += 1;
        } else if read {
            self.read_active += 1;
        } else {
            self.active = true;
        }
        let request_id = match completion {
            Completion::Request(request) => request.request_id.clone(),
            _ => None,
        };
        if let Completion::Read(request) = completion {
            self.read_aborts.push(ReadInFlight {
                key: request.source.key.clone(),
                epoch: generation,
                request_id: request.request_id.clone(),
                cancel: abort_tx,
            });
        } else if !resource {
            // Saving the connection advances its own resource epoch. Do not cancel
            // that local mutation halfway through persistence/activation.
            self.active_local_mutation = match work {
                Work::Host {
                    completion: Completion::Request(request),
                    generation,
                    ..
                } if request.method != "GET"
                    && request.url.starts_with('/')
                    && !request.url.starts_with("//") =>
                {
                    Some((request.source.key.clone(), *generation))
                }
                _ => None,
            };
            self.active_abort = Some((request_id, abort_tx));
        }
    }

    fn start_work(&mut self, work: Work) -> Command<Message> {
        let resource = matches!(&work, Work::Resource { .. });
        let host = self.shared.config.host.clone();
        let custom = self.shared.request_action.clone();
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
        self.track_work(
            &work,
            &timeout_completion,
            timeout_generation,
            resource,
            abort_tx,
        );
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
                    Work::Navigate(_) => {
                        unreachable!("navigation uses the independent router lane")
                    }
                };
                Message::Completed {
                    generation,
                    work: completion,
                    result,
                }
            };
            let failure = tokio::select! {
                result = tokio::time::timeout(timeout, operation) => match result { Ok(message) => return Some(message), Err(_) => std::io::Error::new(std::io::ErrorKind::TimedOut, "bmux effect deadline exceeded") },
                _ = abort_rx.changed(), if !resource => std::io::Error::new(std::io::ErrorKind::Interrupted, "bmux request aborted"),
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
        self.resource_paint_only = false;
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
        self.geometry_dirty = true;
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
            self.restored_immediate.remove(key);
            self.completed_loads.remove(key);
            self.loaded_routes.remove(key);
            // Document lifetimes reset event/request ownership, not immutable image
            // bytes. Authorization context changes are purged independently below.
            let unchanged_image = self.graphics.rgba(key).is_some()
                && self
                    .resources
                    .get(key)
                    .is_some_and(|source| image_sources.get(key) == Some(source));
            if !unchanged_image {
                // A reused identity now belonging to another element is not a
                // detached image cache entry (it may become a mutable canvas).
                if !image_sources.contains_key(key) && self.state.find_node(key).is_some() {
                    self.resource_sources.remove(key);
                }
                // Decoded sources may outlive their DOM placement, but remain in
                // the same bounded graphics LRU and authorization context.
                if !self.resource_sources.contains_key(key) {
                    self.graphics.remove(key);
                }
                self.resources.remove(key);
                self.resource_epochs.remove(key);
                self.resource_loaded.remove(key);
                self.resource_visible.remove(key);
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
        if !self.restoring_history {
            self.broadcast(&EventKind::Immediate, None)?;
        }
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
            if accepts_event(node, kind)
                && !(matches!(kind, EventKind::Immediate)
                    && self
                        .restored_immediate
                        .contains(&layout::node_key(node, path)))
                && !(matches!(kind, EventKind::Immediate)
                    && !node.actions.iter().any(|action| {
                        matches!(action.trigger, hyperchad_actions::ActionTrigger::Immediate)
                    })
                    && node.route.as_ref().is_some_and(|route| {
                        self.loaded_routes.get(&layout::node_key(node, path)) == Some(route)
                    }))
            {
                keys.push(layout::node_key(node, path));
            }
        });
        // Lifecycle actions run before presentation, but still require current
        // document geometry. This layout is not published for pointer hit testing.
        if keys.is_empty() {
            return Ok(());
        }
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

    fn measure_image(&self, key: Option<&String>) -> Option<LogicalSize> {
        #[cfg(test)]
        LEAF_MEASUREMENTS.with(|count| count.set(count.get() + 1));
        if !self.graphics_available || !self.shared.config.graphics_enabled {
            return None;
        }
        let (width, height, _) = key.and_then(|key| self.graphics.rgba(key))?;
        Some(LogicalSize::new(
            u64::from(width.div_ceil(8)),
            u64::from(height.div_ceil(16)),
        ))
    }

    fn complete_paint_event(
        &mut self,
        event: RuntimeEvent<Message>,
        pending: bool,
    ) -> Result<(), Error> {
        // Restore only current bounded completions after context revalidation and
        // successful decode. Normal prepare/reprojection guards remain authoritative.
        let retain = pending && self.pending_scroll_resource(&event);
        let key = match &event {
            RuntimeEvent::Message(Message::Completed {
                work: Completion::Resource { key, .. },
                ..
            }) => Some(key.clone()),
            _ => None,
        };
        if let RuntimeEvent::Message(message) = event {
            self.message(message)?;
        }
        self.scroll_reuse = retain
            && self.resource_paint_only
            && key.is_some_and(|key| {
                self.resource_loaded.contains(&key) && self.graphics.rgba(&key).is_some()
            });
        Ok(())
    }

    fn pending_scroll_resource(&self, event: &RuntimeEvent<Message>) -> bool {
        matches!(event, RuntimeEvent::Message(Message::Completed {
            generation,
            work: Completion::Resource { key, source },
            result: Ok(HostResponse::Bytes(_)),
        }) if self.resources.get(key) == Some(source)
            && self.resource_epochs.get(key) == Some(generation))
            && !self.stopping
            && self.fixed_image_geometry
            && self.prepared_source.as_ref() == Some(self.state.root())
            && self.options.open_details == self.controls.details_open()
            && self.effective.is_some()
            && self.committed.as_ref().is_some_and(|(generation, tree)| {
                *generation == self.state.generation() && tree.viewport == self.options.viewport
            })
    }

    fn log_scroll_admission(&mut self) -> bool {
        let requested = std::mem::take(&mut self.scroll_reuse);
        if !requested {
            return false;
        }
        let committed = self.committed.as_ref();
        log::debug!(
            "bmux scroll admission fixed_images={} source_equal={} details_equal={} committed={} generation_equal={} viewport_equal={} effective={}",
            self.fixed_image_geometry,
            self.prepared_source.as_ref() == Some(self.state.root()),
            self.options.open_details == self.controls.details_open(),
            committed.is_some(),
            committed.is_some_and(|(generation, _)| *generation == self.state.generation()),
            committed.is_some_and(|(_, tree)| tree.viewport == self.options.viewport),
            self.effective.is_some()
        );
        true
    }

    // Return dispatch/index-pass counts for focused regression instrumentation.
    fn emit_named(&mut self, name: &str, value: Option<&str>) -> Result<(usize, usize), Error> {
        let mut keys = Vec::new();
        let mut paths = BTreeMap::new();
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            let key = layout::node_key(node, path);
            keys.push(key.clone());
            paths.insert(key, path.to_vec());
        });
        let mut generation = self.state.generation();
        let mut index_passes = 1;
        let mut dispatched = 0;
        for key in keys {
            if generation != self.state.generation() {
                paths.clear();
                visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
                    paths.insert(layout::node_key(node, path), path.to_vec());
                });
                generation = self.state.generation();
                index_passes += 1;
            }
            let Some(node) = paths
                .get(&key)
                .and_then(|path| at_path(self.state.root(), path))
            else {
                continue;
            };
            // The first original dispatch retains transaction validation (payload
            // limits, tree indexing and clock), even for an entirely inert tree.
            if dispatched != 0
                && !self
                    .actions
                    .named_maintenance_due(self.state.root(), self.now_ms())
                && !self.actions.accepts_named(&key, node, name)
            {
                continue;
            }
            self.dispatch(
                &key,
                EventKind::Named(name.to_owned()),
                value.map(str::to_owned),
            )?;
            dispatched += 1;
        }
        log::debug!("bmux named admission dispatched={dispatched} index_passes={index_passes}");
        Ok((dispatched, index_passes))
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
        // Presented paths are valid only for the exact document generation.
        // Lifecycle geometry is freshly built from the current document.
        let indexed = geometry
            .or_else(|| {
                self.committed.as_ref().and_then(|(generation, tree)| {
                    (*generation == self.state.generation()).then_some(tree)
                })
            })
            .and_then(|tree| tree.get(key))
            .map(|node| node.path.clone());
        let path = if let Some(path) = indexed {
            path
        } else {
            let mut found = None;
            visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
                if layout::node_key(node, path) == key {
                    found = Some(path.to_vec());
                }
            });
            let Some(path) = found else {
                return Ok(());
            };
            path
        };
        let Some(path) = event_target_path(self.state.root(), &path, &kind) else {
            return Ok(());
        };
        let Some(node) = at_path(self.state.root(), &path) else {
            return Ok(());
        };
        // Reject no-op events before building full geometry/form/selector maps.
        if !accepts_event(node, &kind) {
            return Ok(());
        }
        let automatic_route = matches!(kind, EventKind::Immediate)
            .then(|| node.route.clone())
            .flatten();
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
        self.effects(effects)?;
        if let Some(route) = automatic_route {
            self.loaded_routes.insert(key.to_owned(), route);
        }
        Ok(())
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
                        completion: if request.method == "GET" {
                            Completion::Read(request.clone())
                        } else {
                            Completion::Request(request.clone())
                        },
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
                Effect::StylesChanged { .. } => {
                    self.styles_changed = true;
                    self.resource_paint_only = false;
                    self.geometry_dirty = true;
                }
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
        if let Completion::Request(request) | Completion::Read(request) = work {
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

    fn complete_request_view(
        &mut self,
        request: &crate::actions::RequestEffect,
        mut view: View,
        resumed: bool,
    ) -> Result<(), Error> {
        if resumed {
            view.response = hyperchad_renderer::ResponseMetadata::default();
        }
        let completed_route = self.completed_load_route(request);
        self.apply_request_view(request, &view)?;
        if let Some(route) = completed_route
            && self
                .state
                .find_node(&request.source.key)
                .is_some_and(|node| node.route.as_ref() == Some(&route))
        {
            self.completed_loads.insert(request.source.key.clone());
        }
        Ok(())
    }

    fn complete(
        &mut self,
        generation: u64,
        work: Completion,
        mut result: Result<HostResponse, Error>,
    ) -> Result<(), Error> {
        if matches!(&work, Completion::Resource { .. }) {
            self.resource_active = self.resource_active.saturating_sub(1);
        } else if matches!(&work, Completion::Read(_)) {
            self.read_active = self.read_active.saturating_sub(1);
        } else {
            self.active = false;
            self.active_local_mutation = None;
            self.active_abort = None;
        }
        self.revalidate_resource_context()?;
        let current = match &work {
            Completion::Resource { key, source } => {
                self.resources.get(key) == Some(source)
                    && self.resource_epochs.get(key) == Some(&generation)
            }
            Completion::File { key } => self.node_epochs.get(key) == Some(&generation),
            Completion::Request(request) | Completion::Read(request) => {
                self.node_epochs.get(&request.source.key) == Some(&generation)
            }
            Completion::Other => generation == self.state.generation(),
        };
        let class = match &work {
            Completion::Resource { .. } => "resource",
            Completion::File { .. } => "file",
            Completion::Request(_) | Completion::Read(_) => "http",
            Completion::Other => "other",
        };
        log::debug!(
            "bmux work complete class={class} generation={generation} current={current} success={} pending={}",
            result.is_ok(),
            self.pending.len()
        );
        if current {
            let resumed = if let Completion::Request(request) | Completion::Read(request) = &work {
                self.resumed_loads
                    .remove(&(request.source.key.clone(), request.source.generation))
            } else {
                false
            };
            if !resumed {
                self.complete_http_lifecycle(&work, &result)?;
            }
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
                    self.complete_resource(key, source, &bytes);
                }
                (Completion::File { key }, Ok(HostResponse::Files(files))) => {
                    let outcome = self.controls.set_files(&key, files);
                    self.control_events(outcome.events)?;
                }
                (
                    Completion::Request(request) | Completion::Read(request),
                    Ok(HostResponse::View(view)),
                ) => {
                    self.complete_request_view(&request, view, resumed)?;
                }
                (
                    Completion::Request(request) | Completion::Read(request),
                    Ok(HostResponse::Complete),
                ) => {
                    if self.completed_load_route(&request).is_some() {
                        self.completed_loads.insert(request.source.key);
                    }
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

    // Decode a current resource at its display size, then publish only resident
    // entries to the source-reuse index. Failure retains negative admission.
    fn complete_resource(&mut self, key: String, source: String, bytes: &[u8]) {
        let display_size = self
            .candidate
            .as_ref()
            .or_else(|| self.committed.as_ref().map(|(_, tree)| tree))
            .and_then(|tree| tree.get(&key))
            .filter(|node| node.content_bounds.width > 0 && node.content_bounds.height > 0)
            .map(|node| {
                (
                    u32::try_from(node.content_bounds.width.saturating_mul(8)).unwrap_or(u32::MAX),
                    u32::try_from(node.content_bounds.height.saturating_mul(16))
                        .unwrap_or(u32::MAX),
                )
            });
        // A completion can precede geometry after navigation. Do not rasterize
        // a small UI icon at an 800/1707px intrinsic size in that window.
        let display_size = display_size.or(Some((64, 64)));
        if let Err(error) = self.graphics.decode_for_display(
            &key,
            &crate::graphics::GraphicsSource::Memory,
            &crate::graphics::SourcePolicy {
                allow_memory: true,
                ..Default::default()
            },
            bytes,
            display_size,
        ) {
            // Preserve negative admission until explicit retry or source change.
            // A rejected decode is not a successful paint-only completion.
            self.resource_paint_only = false;
            log::warn!("image decode rejected: {error}");
        } else {
            if let Some((width, height, _)) = self.graphics.rgba(&key) {
                log::debug!(
                    "bmux resource installed key={key:?} width={width} height={height} encoded_bytes={} resident_bytes={}",
                    bytes.len(),
                    self.graphics.resident_bytes()
                );
            }
            self.resource_loaded.insert(key.clone());
            self.resource_sources.insert(key, source);
            self.resource_sources
                .retain(|key, _| self.graphics.rgba(key).is_some());
        }
    }

    fn completed_load_route(
        &self,
        request: &crate::actions::RequestEffect,
    ) -> Option<hyperchad_transformer_models::Route> {
        let route = self.state.find_node(&request.source.key)?.route.as_ref()?;
        if request.method == "GET"
            && matches!(route, hyperchad_transformer_models::Route::Get { route, trigger, .. } if route == &request.url && trigger.as_deref() == Some("load"))
        {
            Some(route.clone())
        } else {
            None
        }
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
            .and_then(|node| {
                if mouse.kind == MouseEventKind::Move {
                    // Hover belongs to the actionable ancestor, not whichever
                    // artwork/text descendant happens to be under the pointer.
                    // Enter and release must use the same action owner.
                    (0..=node.path.len()).rev().find_map(|len| {
                        let path = &node.path[..len];
                        let source = at_path(self.state.root(), path)?;
                        accepts_event(source, &EventKind::Hover)
                            .then(|| layout::node_key(source, path))
                    })
                } else {
                    Some(node.key.clone())
                }
            });
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

    fn trim_history(&mut self, prefer_back: bool) {
        while self.history_back.len() + self.history_forward.len() > 8
            || self.history_back.charged_bytes() + self.history_forward.charged_bytes()
                > 32 * 1024 * 1024
        {
            if prefer_back && !self.history_forward.is_empty() || self.history_back.is_empty() {
                self.history_forward.discard_oldest();
            } else {
                self.history_back.discard_oldest();
            }
        }
    }

    fn cache_current(&mut self, forward: bool) {
        if self.location.is_empty() || !self.history_document_valid {
            log::debug!("bmux history snapshot skipped reason=missing-location-or-stale-context");
            return;
        }
        let cache = if forward {
            &mut self.history_forward
        } else {
            &mut self.history_back
        };
        cache.push_with_completed(
            self.resource_context,
            &self.location,
            self.state.root(),
            &self.options.scroll_offsets,
            &self.options.open_details,
            &self.completed_loads,
        );
        self.trim_history(!forward);
    }

    fn invalidate_document_work(&mut self) {
        self.navigation_pending = None;
        self.node_epochs.clear();
        self.pending
            .retain(|work| matches!(work, Work::Resource { .. }));
        if let Some((_, cancel)) = &self.active_abort {
            let _ = cancel.send(true);
        }
        self.restored_immediate.clear();
        self.completed_loads.clear();
        self.resumed_loads.clear();
        self.loaded_routes.clear();
        self.controls = Controls::new();
        self.actions = ActionEngine::new(self.state.generation(), ActionLimits::default());
        self.hover = None;
        self.pressed = None;
        self.key_chord.clear();
        self.candidate = None;
        self.committed = None;
        self.geometry_dirty = true;
    }

    fn restore_history(&mut self, forward: bool) -> Result<(), Error> {
        fn restore_details(
            node: &mut Container,
            path: &mut Vec<usize>,
            details: &BTreeMap<String, bool>,
        ) {
            let key = layout::node_key(node, path);
            if let Element::Details { open } = &mut node.element
                && let Some(value) = details.get(&key)
            {
                *open = Some(*value);
            }
            for (index, child) in node.children.iter_mut().enumerate() {
                path.push(index);
                restore_details(child, path, details);
                path.pop();
            }
        }
        self.revalidate_resource_context()?;
        let snapshot = if forward {
            self.history_forward.pop(self.resource_context)
        } else {
            self.history_back.pop(self.resource_context)
        };
        let Some(snapshot) = snapshot else {
            log::debug!("bmux history restore miss forward={forward}");
            return Ok(());
        };
        self.cache_current(!forward);
        let crate::history::DocumentSnapshotParts {
            url,
            mut root,
            scroll,
            details,
            completed_loads,
        } = snapshot.into_parts();
        restore_details(&mut root, &mut Vec::new(), &details);
        self.invalidate_document_work();
        // Clear first so even repeated explicit IDs acquire fresh generations.
        // Neither intermediate nor restored trees replay load/immediate effects.
        self.restoring_history = true;
        let result = (|| {
            self.apply_view(View::from(Container::default()), None)?;
            self.options.scroll_offsets = scroll;
            self.options.open_details = details;
            self.apply_view(View::from(root), None)
        })();
        self.restoring_history = false;
        result?;
        self.location = url;
        self.history_document_valid = true;
        self.completed_loads = completed_loads;
        self.resume_history_loads()
    }

    fn resume_history_loads(&mut self) -> Result<(), Error> {
        let mut unfinished = Vec::new();
        visit(self.state.root(), &mut Vec::new(), &mut |node, path| {
            let key = layout::node_key(node, path);
            if !self.completed_loads.contains(&key)
                && matches!(&node.route, Some(hyperchad_transformer_models::Route::Get { trigger, .. }) if trigger.as_deref() == Some("load"))
            {
                unfinished.push(key.clone());
            }
            self.restored_immediate.insert(key);
        });
        for key in unfinished {
            // Dispatch only the unfinished GET route. Immediate actions and HTTP
            // lifecycle actions are not replayed as part of history restoration.
            self.resumed_loads
                .insert((key.clone(), self.state.generation()));
            let actions = self
                .state
                .find_node_mut(&key)
                .map(|node| std::mem::take(&mut node.actions));
            let result = self.dispatch(&key, EventKind::Immediate, None);
            if let (Some(node), Some(actions)) = (self.state.find_node_mut(&key), actions) {
                node.actions = actions;
            }
            result?;
        }
        Ok(())
    }

    fn complete_navigation(
        &mut self,
        id: u64,
        url: String,
        view: Option<View>,
    ) -> Result<(), Error> {
        self.revalidate_resource_context()?;
        if !self
            .navigation_pending
            .as_ref()
            .is_some_and(|(current, expected, epoch)| {
                *current == id && *expected == url && *epoch == self.resource_context
            })
        {
            return Ok(());
        }
        self.navigation_pending = None;
        let Some(view) = view else {
            return Ok(());
        };
        // Validate a candidate before caching or replacing the current document.
        validate_view(&view, &self.shared.config)?;
        let mut candidate = self.state.clone();
        candidate.apply_view(view.clone(), None)?;
        self.cache_current(false);
        self.history_forward.clear();
        self.invalidate_document_work();
        self.options.scroll_offsets.clear();
        self.options.open_details.clear();
        self.restoring_history = true;
        let cleared = self.apply_view(View::from(Container::default()), None);
        self.restoring_history = false;
        cleared?;
        self.location = url;
        self.apply_view(view, None)?;
        self.history_document_valid = true;
        Ok(())
    }

    fn apply_unsolicited(&mut self, view: View, target: Option<&str>) -> Result<(), Error> {
        self.apply_view(view, target)?;
        if target.is_none() {
            self.history_document_valid = true;
        }
        Ok(())
    }

    fn update_canvas(&mut self, update: &CanvasUpdate) -> Result<(), Error> {
        // Canvas messages are asynchronous. A connection update can remove the
        // old canvas (or evict its pixels) before a queued visualization arrives.
        // Only the current document may authorize recreating its backing resource.
        if !self
            .state
            .find_node(&update.target)
            .is_some_and(|node| matches!(node.element, Element::Canvas))
        {
            return Ok(());
        }
        if self.graphics.rgba(&update.target).is_none() {
            self.prepare()?;
            let size = self
                .candidate
                .as_ref()
                .and_then(|tree| tree.get(&update.target))
                .map(|node| (node.content_bounds.width, node.content_bounds.height));
            self.discard_prepared();
            let Some((width, height)) = size.filter(|(width, height)| *width > 0 && *height > 0)
            else {
                // No drawable surface yet; the next resize/visualization refresh
                // will populate it. This is not a malformed graphics operation.
                return Ok(());
            };
            if self.graphics.rgba(&update.target).is_none() {
                self.graphics.create_canvas(
                    &update.target,
                    u32::try_from(width.saturating_mul(8))?,
                    u32::try_from(height.saturating_mul(16))?,
                )?;
            }
        }
        self.graphics.update_canvas(update)?;
        Ok(())
    }

    fn message(&mut self, message: Message) -> Result<(), Error> {
        if !matches!(
            &message,
            Message::Completed {
                work: Completion::Resource { .. },
                ..
            }
        ) {
            self.resource_paint_only = false;
            self.geometry_dirty = true;
        }
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
                self.abort_reads(&id);
                if let Some((active, cancel)) = &self.active_abort
                    && active.as_ref() == Some(&id)
                {
                    let _ = cancel.send(true);
                }
                let mut cancelled = Vec::new();
                self.pending.retain(|work| {
                    if let Work::Host {
                        completion: Completion::Request(request) | Completion::Read(request),
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
            Message::Quit => {
                self.stopping = true;
                self.quitting = true;
                self.pending.clear();
                if let Some((_, cancel)) = &self.active_abort {
                    let _ = cancel.send(true);
                }
            }
            Message::InputFailed(error) => return Err(std::io::Error::other(error).into()),
            Message::NavigationComplete { id, url, view } => {
                self.complete_navigation(id, url, view)?;
            }
            Message::View(view, target) => self.apply_unsolicited(view, target.as_deref())?,
            Message::Canvas(update) => self.update_canvas(&update)?,
            Message::Emit(name, value) => {
                self.emit_named(&name, value.as_deref())?;
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
        self.resource_paint_only = false;
        if !matches!(event, Event::Mouse(mouse) if mouse.kind == MouseEventKind::Move) {
            self.geometry_dirty = true;
        }
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
            if key.modifiers.alt
                && !key.modifiers.ctrl
                && !key.modifiers.shift
                && !key.modifiers.super_key
                && !key.modifiers.meta
                && matches!(
                    key.key,
                    bmux_keyboard::KeyCode::Left | bmux_keyboard::KeyCode::Right
                )
            {
                log::debug!(
                    "bmux history chord forward={}",
                    key.key == bmux_keyboard::KeyCode::Right
                );
                return self.restore_history(key.key == bmux_keyboard::KeyCode::Right);
            }
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
                            at_path(self.state.root(), &node.path).is_some_and(|source| {
                                accepts_event(source, &EventKind::ClickOutside)
                            }) && !node
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
        self.geometry_dirty |= result.redraw || result.relayout || !result.events.is_empty();
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

// Unrepresentable allocations must not be refitted from a truncated rectangle.
fn image_allocation(bounds: layout::CellRect) -> Option<bmux_tui::image::ImageBounds> {
    Some(bmux_tui::image::ImageBounds {
        x: i32::try_from(bounds.x).ok()?,
        y: i32::try_from(bounds.y).ok()?,
        width: u16::try_from(bounds.width).ok()?,
        height: u16::try_from(bounds.height).ok()?,
    })
}

#[cfg(test)]
thread_local! {
    static LEAF_MEASUREMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Program for BmuxProgram {
    type Message = Message;
    type Error = Error;

    fn update(&mut self, event: RuntimeEvent<Message>) -> Result<Update<Message>, Error> {
        let pending_scroll_resource = self.scroll_reuse && self.pending_scroll_resource(&event);
        self.scroll_reuse = !self.geometry_dirty
            && matches!(
                &event,
                RuntimeEvent::Terminal(Event::Mouse(mouse)) if matches!(mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown | MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight)
            );
        if let RuntimeEvent::Terminal(Event::Mouse(mouse)) = &event
            && matches!(
                mouse.kind,
                MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            )
        {
            log::debug!(
                "bmux scroll intent reuse={} geometry_dirty={}",
                self.scroll_reuse,
                self.geometry_dirty
            );
        }
        self.resource_paint_only = false;
        let context_wake = matches!(
            &event,
            RuntimeEvent::Message(Message::ResourceContextChanged)
        );
        if context_wake && self.stopping {
            return Ok(self.drive(Update::none()));
        }
        let previous_context = self.resource_context;
        self.revalidate_resource_context()?;
        if self.geometry_dirty {
            self.scroll_reuse = false;
        }
        if context_wake {
            // A normal update may have already observed the epoch before this
            // queued wake arrived. Do not repaint redundantly in that case.
            return Ok(self.drive(if previous_context == self.resource_context {
                Update::none()
            } else {
                Update::redraw()
            }));
        }
        if matches!(&event, RuntimeEvent::Message(Message::Quit)) {
            self.message(Message::Quit)?;
            return Ok(self.drive(Update::none()));
        }
        if matches!(
            &event,
            RuntimeEvent::Message(
                Message::Completed {
                    work: Completion::Resource { .. },
                    ..
                } | Message::Advance
            )
        ) {
            self.resource_paint_only = matches!(
                &event,
                RuntimeEvent::Message(Message::Completed {
                    work: Completion::Resource { .. },
                    ..
                })
            );
            self.complete_paint_event(
                event,
                pending_scroll_resource && previous_context == self.resource_context,
            )?;
            return Ok(self.drive(Update::redraw()));
        }
        let pointer_move = matches!(
            &event,
            RuntimeEvent::Terminal(Event::Mouse(mouse)) if mouse.kind == MouseEventKind::Move
        );
        // Merely crossing a hit region is not a document mutation. Hover actions
        // and native controls mark geometry dirty when they actually change it.
        self.geometry_dirty |= !pointer_move;
        self.styles_changed = false;
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
        if generation_before == self.state.generation() && self.styles_changed {
            self.state.mark_changed()?;
            self.actions.reconcile_removed(
                self.state.root(),
                self.state.generation(),
                &BTreeSet::new(),
            )?;
        }
        Ok(self.drive(if pointer_move && !self.geometry_dirty {
            Update::none()
        } else {
            Update::redraw()
        }))
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
            completion: Completion::Request(_) | Completion::Read(_),
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

// Keep the action engine authoritative for defaults, named registrations and
// HTTP phases. Only prune built-in events whose handler/default admission can
// be decided without constructing a transactional action context.
// Pointer hit-testing returns the deepest painted node, not necessarily an interactive
// element. Resolve clicks to their nearest owner, without letting a nested playback
// button also activate the enclosing album link.
fn event_target_path(root: &Container, path: &[usize], kind: &EventKind) -> Option<Vec<usize>> {
    if !matches!(kind, EventKind::Click) {
        return Some(path.to_vec());
    }
    (0..=path.len()).rev().find_map(|len| {
        let candidate = &path[..len];
        let node = at_path(root, candidate)?;
        (node.route.is_some()
            || node.actions.iter().any(|action| {
                !matches!(action.trigger, hyperchad_actions::ActionTrigger::Event(_))
                    && hyperchad_actions::handler::should_trigger_action(&action.trigger, "click")
            })
            || matches!(
                node.element,
                Element::Anchor { href: Some(_), .. }
                    | Element::Button { .. }
                    | Element::Input { .. }
                    | Element::Textarea { .. }
            ))
        .then(|| candidate.to_vec())
    })
}

fn accepts_event(node: &Container, kind: &EventKind) -> bool {
    use hyperchad_actions::{ActionTrigger, handler::should_trigger_action};
    let name = match kind {
        EventKind::ClickOutside => "click_outside",
        EventKind::Immediate => "immediate",
        EventKind::Resize => "resize",
        EventKind::Hover => "hover",
        EventKind::MouseDown => "mouse_down",
        EventKind::KeyDown => "key_down",
        _ => return true,
    };
    if node.actions.iter().any(|action| {
        !matches!(action.trigger, ActionTrigger::Event(_))
            && should_trigger_action(&action.trigger, name)
    }) {
        return true;
    }
    if matches!(kind, EventKind::Immediate) {
        return crate::actions::has_load_route(node);
    }
    if matches!(kind, EventKind::Resize) {
        return node.route.as_ref().is_some_and(|route| {
            use hyperchad_transformer_models::Route;
            let trigger = match route {
                Route::Get { trigger, .. }
                | Route::Post { trigger, .. }
                | Route::Put { trigger, .. }
                | Route::Delete { trigger, .. }
                | Route::Patch { trigger, .. } => trigger,
            };
            trigger.as_deref() == Some("resize")
        });
    }
    false
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

    #[test]
    fn nested_album_cover_click_navigates_but_playback_button_does_not() {
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    element: Element::Anchor {
                        href: Some("/albums?albumId=42".into()),
                        target: None,
                    },
                    children: vec![Container {
                        children: vec![
                            Container {
                                str_id: Some("cover".into()),
                                element: Element::Image {
                                    source: Some("/cover.png".into()),
                                    alt: None,
                                    fit: None,
                                    source_set: None,
                                    sizes: None,
                                    loading: None,
                                },
                                ..Default::default()
                            },
                            Container {
                                element: Element::Button {
                                    r#type: Some("button".into()),
                                },
                                children: vec![Container {
                                    str_id: Some("play-icon".into()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program
            .dispatch("play-icon", EventKind::Click, None)
            .unwrap();
        assert!(
            !program
                .pending
                .iter()
                .any(|work| matches!(work, Work::Navigate(_)))
        );
        program.dispatch("cover", EventKind::Click, None).unwrap();
        assert!(
            program
                .pending
                .iter()
                .any(|work| { matches!(work, Work::Navigate(url) if url == "/albums?albumId=42") })
        );
    }

    #[test]
    fn late_visualization_after_removal_or_zero_size_does_not_crash() {
        use hyperchad_renderer::canvas::CanvasAction;
        let mut program = program();
        let update = || {
            Message::Canvas(CanvasUpdate {
                target: "visualization".into(),
                canvas_actions: vec![CanvasAction::Clear],
            })
        };
        // A live connection can deliver a clear before the corresponding view.
        program.message(update()).unwrap();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("visualization".into()),
                    element: Element::Canvas,
                    width: Some(hyperchad_transformer::Number::Integer(0)),
                    height: Some(hyperchad_transformer::Number::Integer(0)),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.message(update()).unwrap();
        program
            .apply_view(View::from(Container::default()), None)
            .unwrap();
        program.message(update()).unwrap();
    }

    #[test]
    fn evicted_live_canvas_is_recreated_before_update() {
        use hyperchad_renderer::canvas::CanvasAction;
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("visualization".into()),
                    element: Element::Canvas,
                    width: Some(hyperchad_transformer::Number::Integer(80)),
                    height: Some(hyperchad_transformer::Number::Integer(16)),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        program.discard_prepared();
        program.graphics.remove("visualization");
        program
            .message(Message::Canvas(CanvasUpdate {
                target: "visualization".into(),
                canvas_actions: vec![CanvasAction::Clear],
            }))
            .unwrap();
        assert!(program.graphics.rgba("visualization").is_some());
    }

    fn program() -> BmuxProgram {
        let (actions, _) = flume::bounded(8);
        let (resize, _) = flume::bounded(8);
        BmuxProgram::new(BmuxConfig::default(), actions, resize)
    }

    #[test]
    fn named_admission_large_stale_document_avoids_inert_dispatch_and_indexing() {
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("shell".into()),
                    children: (0..1508)
                        .map(|index| Container {
                            str_id: Some(format!("inert-{index}")),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        program.committed = Some((
            program.state.generation().wrapping_sub(1),
            program.candidate.take().unwrap(),
        ));
        crate::actions::NAMED_WORK_COUNTS.with(|counts| counts.set((0, 0)));
        assert_eq!(
            program.emit_named("play-track", Some("payload")).unwrap(),
            (1, 1)
        );
        crate::actions::NAMED_WORK_COUNTS.with(|counts| assert_eq!(counts.get(), (1, 1)));
        let oversized = "x".repeat(ActionLimits::default().max_value_bytes + 1);
        assert!(program.emit_named("play-track", Some(&oversized)).is_err());
    }

    #[test]
    fn targeted_playlist_fragments_preserve_shell_and_album_scroll() {
        let mut program = program();
        let node = |id: &str| Container {
            str_id: Some(id.into()),
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("shell".into()),
                    children: vec![
                        node("sidebar"),
                        node("main"),
                        node("footer"),
                        node("playback-sessions-content"),
                    ],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        let offset = layout::ScrollOffset { x: 0, y: 24 };
        program.options.scroll_offsets.insert("main".into(), offset);
        let containers = vec![Container {
            str_id: Some("playback-sessions-content".into()),
            children: vec![node("fixture-session")],
            ..Default::default()
        }];
        program
            .apply_view(View::builder().with_fragments(containers).build(), None)
            .unwrap();
        assert_eq!(program.state.root().str_id.as_deref(), Some("shell"));
        for id in ["sidebar", "main", "footer", "fixture-session"] {
            let mut found = false;
            visit(program.state.root(), &mut Vec::new(), &mut |node, _| {
                found |= node.str_id.as_deref() == Some(id);
            });
            assert!(found, "missing {id}");
        }
        assert_eq!(program.options.scroll_offsets["main"], offset);
    }

    #[test]
    fn named_fragment_updates_preserve_sibling_domains() {
        let ids = [
            "audio-zones-content",
            "albums",
            "search-results-local",
            "settings-download-settings-section",
            "settings-scan-settings-section",
        ];
        let node = |id: &str| Container {
            str_id: Some(id.into()),
            ..Default::default()
        };
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("shell".into()),
                    children: ids.iter().map(|id| node(id)).collect(),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        for (index, id) in ids.iter().enumerate() {
            let before = program.state.root().clone();
            let update = Container {
                children: vec![node(&format!("updated-{index}"))],
                ..node(id)
            };
            program
                .apply_view(
                    View::builder().with_fragments(vec![update.clone()]).build(),
                    None,
                )
                .unwrap();
            let mut expected = before;
            expected.children[index] = update;
            assert_eq!(*program.state.root(), expected, "target={id}");
        }
    }

    #[test]
    fn explicit_fragment_envelope_retains_self_target_replacement_semantics() {
        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("shell".into()),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        let view = View::builder()
            .with_fragment(vec![Container {
                str_id: Some("replacement".into()),
                ..Default::default()
            }])
            .build();
        assert!(matches!(
            view.fragments[0].selector,
            hyperchad_transformer_models::Selector::SelfTarget
        ));
        program.apply_view(view, None).unwrap();
        assert_eq!(program.state.root().str_id, None);
        assert_eq!(program.state.root().children.len(), 1);
        assert_eq!(
            program.state.root().children[0].str_id.as_deref(),
            Some("replacement")
        );
    }

    #[test]
    fn inert_pointer_motion_does_not_relayout_or_request_presentation() {
        use bmux_tui::{event::MouseEvent, geometry::Point};
        use bmux_tui_runtime::Invalidation;

        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("album".into()),
                    width: Some(hyperchad_transformer::Number::Integer(160)),
                    height: Some(hyperchad_transformer::Number::Integer(160)),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        program.commit_prepared();
        let generation = program.state.generation();
        let bounds = program
            .committed
            .as_ref()
            .unwrap()
            .1
            .get("album")
            .unwrap()
            .bounds;
        for position in [Point::new(0, 0), Point::new(1, 1), Point::new(50, 30)] {
            let update = program
                .update(RuntimeEvent::Terminal(Event::Mouse(MouseEvent::new(
                    MouseEventKind::Move,
                    position,
                ))))
                .unwrap();
            assert_eq!(update.invalidation, Invalidation::None);
            assert!(!program.geometry_dirty);
            assert_eq!(program.state.generation(), generation);
            assert_eq!(
                program
                    .committed
                    .as_ref()
                    .unwrap()
                    .1
                    .get("album")
                    .unwrap()
                    .bounds,
                bounds
            );
        }
    }

    #[test]
    fn hover_on_descendant_uses_and_releases_ancestor_style_owner() {
        use bmux_tui::{event::MouseEvent, geometry::Point};
        use hyperchad_actions::{Action, ActionTrigger, ActionType, ElementTarget, StyleAction};

        let mut program = program();
        program
            .apply_view(
                View::from(Container {
                    children: vec![Container {
                        str_id: Some("album".into()),
                        width: Some(hyperchad_transformer::Number::Integer(160)),
                        height: Some(hyperchad_transformer::Number::Integer(160)),
                        actions: vec![Action {
                            trigger: ActionTrigger::Hover,
                            effect: ActionType::Style {
                                target: ElementTarget::SelfTarget,
                                action: StyleAction::SetBackground(Some("#abcdef".into())),
                            }
                            .into(),
                        }],
                        children: vec![Container {
                            str_id: Some("cover".into()),
                            width: Some(hyperchad_transformer::Number::Integer(80)),
                            height: Some(hyperchad_transformer::Number::Integer(80)),
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
        program.commit_prepared();
        program
            .update(RuntimeEvent::Terminal(Event::Mouse(MouseEvent::new(
                MouseEventKind::Move,
                Point::new(0, 0),
            ))))
            .unwrap();
        assert_eq!(program.hover.as_deref(), Some("album"));
        assert!(program.state.root().children[0].background.is_some());
        assert!(program.geometry_dirty);
        program.prepare().unwrap();
        program.commit_prepared();
        program
            .update(RuntimeEvent::Terminal(Event::Mouse(MouseEvent::new(
                MouseEventKind::Move,
                Point::new(50, 30),
            ))))
            .unwrap();
        assert_eq!(program.hover, None);
        assert_eq!(program.state.root().children[0].background, None);
    }

    #[test]
    fn large_actionable_library_skips_unmatched_broadcast_and_outside_transactions() {
        use hyperchad_actions::{Action, ActionTrigger, ActionType};
        let mut program = program();
        let children = (0..193)
            .map(|index| Container {
                str_id: Some(format!("album-{index}")),
                actions: vec![Action {
                    trigger: ActionTrigger::Click,
                    effect: ActionType::Custom {
                        action: "open".into(),
                    }
                    .into(),
                }],
                ..Default::default()
            })
            .collect();
        program
            .apply_view(
                View::from(Container {
                    children,
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        // Any accidental transaction indexes all 194 nodes and fails this limit.
        // This is structural regression coverage, not a wall-clock assertion.
        program.actions = ActionEngine::new(
            program.state.generation(),
            ActionLimits {
                max_nodes: 1,
                ..Default::default()
            },
        );
        program.broadcast(&EventKind::Immediate, None).unwrap();
        for index in 0..193 {
            program
                .dispatch(&format!("album-{index}"), EventKind::ClickOutside, None)
                .unwrap();
        }
        assert!(program.pending.is_empty());
        assert!(!program.styles_changed);
    }

    #[test]
    fn event_admission_preserves_explicit_handlers_and_named_namespace() {
        use hyperchad_actions::{Action, ActionTrigger, ActionType};
        let handler = |trigger| Container {
            actions: vec![Action {
                trigger,
                effect: ActionType::Custom {
                    action: "handled".into(),
                }
                .into(),
            }],
            ..Default::default()
        };
        assert!(accepts_event(
            &handler(ActionTrigger::ClickOutside),
            &EventKind::ClickOutside
        ));
        assert!(accepts_event(
            &handler(ActionTrigger::Immediate),
            &EventKind::Immediate
        ));
        assert!(accepts_event(
            &handler(ActionTrigger::Resize),
            &EventKind::Resize
        ));
        assert!(!accepts_event(
            &handler(ActionTrigger::Event("click_outside".into())),
            &EventKind::ClickOutside
        ));
        assert!(accepts_event(
            &Container::default(),
            &EventKind::Named("registered".into())
        ));
        assert!(accepts_event(&Container::default(), &EventKind::Click));
    }

    #[test]
    fn completed_resource_reuses_fixed_geometry_but_not_pending_resize() {
        let mut program = program();
        program
            .apply_view(View::from(artwork_image("art", "/cover.webp", true)), None)
            .unwrap();
        program.prepare().unwrap();
        program.commit_prepared();
        // A real layout would reject this test-only bound. A resource-only
        // presentation must instead retain already committed fixed geometry.
        program.options.max_nodes = 0;
        program.resource_paint_only = true;
        program.prepare().unwrap();
        program.commit_prepared();
        program.resize(Size::new(100, 30));
        program.resource_paint_only = true;
        assert!(program.prepare().is_err());
    }

    #[test]
    fn retained_load_routes_skip_transactions_but_replacement_restarts_them() {
        use hyperchad_transformer_models::{Route, Selector, SwapStrategy};
        let mut program = program();
        let section = || Container {
            str_id: Some("load".into()),
            route: Some(Route::Get {
                route: "/load".into(),
                trigger: Some("load".into()),
                target: Selector::SelfTarget,
                strategy: SwapStrategy::This,
            }),
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    children: vec![section()],
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        program.pending.clear();
        program.actions = ActionEngine::new(
            program.state.generation(),
            ActionLimits {
                max_nodes: 1,
                ..Default::default()
            },
        );
        program.broadcast(&EventKind::Immediate, None).unwrap();
        assert!(program.pending.is_empty());
        program.actions = ActionEngine::new(program.state.generation(), ActionLimits::default());
        program
            .apply_view(View::from(section()), Some("load"))
            .unwrap();
        assert!(program.pending.iter().any(|work| matches!(
            work,
            Work::Host {
                completion: Completion::Request(_) | Completion::Read(_),
                ..
            }
        )));
    }

    fn artwork_image(key: &str, source: &str, lazy: bool) -> Container {
        Container {
            str_id: Some(key.into()),
            width: Some(hyperchad_transformer::Number::Integer(200)),
            height: Some(hyperchad_transformer::Number::Integer(200)),
            element: Element::Image {
                source: Some(source.into()),
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: lazy.then_some(hyperchad_transformer_models::ImageLoading::Lazy),
            },
            ..Default::default()
        }
    }

    fn complete_artwork(program: &mut BmuxProgram, key: &str, source: &str) {
        program.prepare().unwrap();
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            200,
            200,
            image::Rgba([91, 113, 157, 255]),
        ))
        .write_to(&mut encoded, image::ImageFormat::WebP)
        .unwrap();
        let generation = program.resource_epochs[key];
        program
            .pending
            .retain(|work| !matches!(work, Work::Resource { key: queued, .. } if queued == key));
        program
            .complete(
                generation,
                Completion::Resource {
                    key: key.into(),
                    source: source.into(),
                },
                Ok(HostResponse::Bytes(encoded.into_inner())),
            )
            .unwrap();
    }

    fn pending_wheel_program() -> BmuxProgram {
        use hyperchad_transformer::Number;
        use hyperchad_transformer_models::{LayoutDirection, LayoutOverflow};
        let mut program = program();
        program.resize(Size::new(80, 24));
        program.set_graphics_available(true);
        let root = Container {
            direction: LayoutDirection::Column,
            children: vec![
                Container {
                    str_id: Some("scroll".into()),
                    width: Some(Number::Integer(640)),
                    height: Some(Number::Integer(320)),
                    overflow_y: LayoutOverflow::Scroll,
                    children: (0..100)
                        .map(|index| Container {
                            width: Some(Number::Integer(640)),
                            height: Some(Number::Integer(32)),
                            children: vec![Container {
                                element: Element::Text {
                                    value: format!("Album {index}"),
                                },
                                ..Default::default()
                            }],
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(640)),
                    height: Some(Number::Integer(48)),
                    children: vec![Container {
                        width: Some(Number::Integer(32)),
                        height: Some(Number::Integer(32)),
                        ..artwork_image("footer-cover", "/footer.webp", false)
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        program.apply_view(View::from(root), None).unwrap();
        program.prepare().unwrap();
        program.commit_prepared();
        program.pending.clear();
        program
            .update(RuntimeEvent::Terminal(Event::Mouse(
                bmux_tui::event::MouseEvent::new(
                    MouseEventKind::ScrollDown,
                    bmux_tui::geometry::Point::new(10, 5),
                ),
            )))
            .unwrap();
        assert!(program.scroll_reuse);
        program
    }

    fn footer_bytes() -> Vec<u8> {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([91, 113, 157, 255]),
        ))
        .write_to(&mut encoded, image::ImageFormat::WebP)
        .unwrap();
        encoded.into_inner()
    }

    fn complete_pending_footer(program: &mut BmuxProgram, epoch: u64, bytes: Vec<u8>) {
        program
            .update(RuntimeEvent::Message(Message::Completed {
                generation: epoch,
                work: Completion::Resource {
                    key: "footer-cover".into(),
                    source: "/footer.webp".into(),
                },
                result: Ok(HostResponse::Bytes(bytes)),
            }))
            .unwrap();
    }

    #[test]
    fn pending_wheel_resource_matches_full_layout_without_measurement() {
        let mut program = pending_wheel_program();
        let epoch = program.resource_epochs["footer-cover"];
        complete_pending_footer(&mut program, epoch, footer_bytes());
        assert!(
            program.scroll_reuse,
            "current bounded completion must retain wheel intent"
        );
        LEAF_MEASUREMENTS.with(|count| count.set(0));
        program.prepare().unwrap();
        assert_eq!(LEAF_MEASUREMENTS.with(std::cell::Cell::get), 0);
        let projected_tree = program.candidate.as_ref().unwrap().clone();
        let projected = format!("{:?}", program.candidate);
        let mut buffer =
            bmux_tui::buffer::Buffer::empty(bmux_tui::geometry::Rect::new(0, 0, 80, 24));
        let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
        program.paint(&mut PaintCx::new(&mut frame));
        assert_eq!(frame.images().len(), 1);
        let images = format!("{:?}", frame.images());
        let cells = format!("{buffer:?}");
        program.commit_prepared();
        // Force the ordinary full baseline, not another reprojection.
        program.geometry_dirty = true;
        program.scroll_reuse = false;
        program.resource_paint_only = false;
        LEAF_MEASUREMENTS.with(|count| count.set(0));
        program.prepare().unwrap();
        assert!(LEAF_MEASUREMENTS.with(std::cell::Cell::get) > 0);
        assert_eq!(format!("{:?}", program.candidate), projected);
        for y in 0..24 {
            for x in 0..80 {
                assert_eq!(
                    projected_tree.hit_test(x, y).map(|node| &node.key),
                    program
                        .candidate
                        .as_ref()
                        .unwrap()
                        .hit_test(x, y)
                        .map(|node| &node.key)
                );
            }
        }
        let mut baseline =
            bmux_tui::buffer::Buffer::empty(bmux_tui::geometry::Rect::new(0, 0, 80, 24));
        let mut frame = bmux_tui::frame::Frame::new(&mut baseline);
        program.paint(&mut PaintCx::new(&mut frame));
        assert_eq!(format!("{:?}", frame.images()), images);
        assert_eq!(format!("{baseline:?}"), cells);
        assert_eq!(program.resource_epochs["footer-cover"], epoch);
        assert!(program.graphics.rgba("footer-cover").is_some());
    }

    #[test]
    fn pending_wheel_resource_rejects_invalidating_changes() {
        for case in 0..12 {
            let mut program = pending_wheel_program();
            let mut epoch = program.resource_epochs["footer-cover"];
            match case {
                0 => program.resize(Size::new(120, 40)),
                1 => {
                    program
                        .apply_view(View::from(Container::default()), None)
                        .unwrap();
                }
                2 => {
                    program
                        .update(RuntimeEvent::Terminal(Event::Mouse(
                            bmux_tui::event::MouseEvent::new(
                                MouseEventKind::Move,
                                bmux_tui::geometry::Point::new(1, 1),
                            ),
                        )))
                        .unwrap();
                }
                3 => epoch = epoch.wrapping_add(1),
                4 => {
                    program.state.mark_changed().unwrap();
                }
                5 => program.fixed_image_geometry = false,
                6 => program.resource_context = Some(123),
                8 => {
                    program.options.open_details.insert("changed".into(), true);
                }
                9 => {
                    program
                        .resources
                        .insert("footer-cover".into(), "/changed.webp".into());
                }
                10 => {
                    program
                        .update(RuntimeEvent::Terminal(Event::Mouse(
                            bmux_tui::event::MouseEvent::new(
                                MouseEventKind::Down(bmux_tui::event::MouseButton::Left),
                                bmux_tui::geometry::Point::new(1, 1),
                            ),
                        )))
                        .unwrap();
                }
                11 => program.stopping = true,
                _ => {}
            }
            complete_pending_footer(
                &mut program,
                epoch,
                if case == 7 { vec![0] } else { footer_bytes() },
            );
            assert!(
                !program.scroll_reuse,
                "case {case} must invalidate pending intent"
            );
        }
        let mut program = pending_wheel_program();
        program
            .update(RuntimeEvent::Message(Message::Completed {
                generation: program.resource_epochs["footer-cover"],
                work: Completion::Resource {
                    key: "footer-cover".into(),
                    source: "/footer.webp".into(),
                },
                result: Err(std::io::Error::other("failed").into()),
            }))
            .unwrap();
        assert!(!program.scroll_reuse);
    }

    fn assert_visible_source_pixels(image: &bmux_tui::image::ImagePlacement, x: i64, y: i64) {
        let bmux_tui::image::ImagePayload::Pixels {
            bytes,
            width,
            height,
            ..
        } = &image.payload
        else {
            panic!("expected patterned source pixels");
        };
        let fit = image.pixel_rect(11.0, 23.0);
        // Unclipped reference: 128-square source fitted to 12x6 host cells.
        // Source crop coordinates must agree for every displayed pixel, not
        // merely the destination's bounding rectangle.
        for py in u32::from(image.destination.y) * 23..u32::from(image.destination.bottom()) * 23 {
            for px in u32::from(image.destination.x) * 11..u32::from(image.destination.right()) * 11
            {
                let actual = (
                    (f64::from(px) - fit.x) * f64::from(*width) / fit.width,
                    (f64::from(py) - fit.y) * f64::from(*height) / fit.height,
                );
                let expected = (
                    f64::from(i32::try_from(x).unwrap()).mul_add(-11.0, f64::from(px)) * 128.0
                        / 132.0,
                    (f64::from(py) - f64::from(i32::try_from(y).unwrap()).mul_add(23.0, 3.0))
                        * 128.0
                        / 132.0,
                );
                assert_eq!(actual, expected);
                if (0.0..128.0).contains(&actual.0) && (0.0..128.0).contains(&actual.1) {
                    let sample = |point: (f64, f64)| {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let index = (point.1.floor() as usize * 128 + point.0.floor() as usize) * 4;
                        &bytes[index..index + 4]
                    };
                    assert_eq!(sample(actual), sample(expected));
                }
            }
        }
    }

    #[test]
    fn signed_image_allocation_survives_terminal_and_header_clipping() {
        let mut program = program();
        program.set_graphics_available(true);
        program
            .apply_view(
                View::from(artwork_image("cover", "/cover.webp", false)),
                None,
            )
            .unwrap();
        complete_artwork(&mut program, "cover", "/cover.webp");
        let pixels: Vec<u8> = (0_u8..128)
            .flat_map(|row| (0_u8..128).flat_map(move |col| [row, col, row ^ col, 255]))
            .collect();
        program
            .graphics
            .register_rgba("cover", 128, 128, pixels)
            .unwrap();
        program.prepare().unwrap();
        for (x, y, clip_y) in [(20, 2, 0), (20, -1, 0), (-2, 2, 0), (-2, -1, 3), (20, 0, 3)] {
            let tree = program.candidate.as_mut().unwrap();
            let node = tree
                .nodes
                .iter_mut()
                .find(|node| node.key == "cover")
                .unwrap();
            node.content_bounds = layout::CellRect::new(x, y, 12, 6);
            node.clip = layout::CellRect::new(0, clip_y, 80, 24);
            let before = format!("{tree:?}");
            let hits: Vec<_> = (0..24)
                .flat_map(|cy| (0..80).map(move |cx| (cx, cy)))
                .map(|(cx, cy)| tree.hit_test(cx, cy).map(|node| node.key.clone()))
                .collect();
            let mut buffer =
                bmux_tui::buffer::Buffer::empty(bmux_tui::geometry::Rect::new(0, 0, 80, 24));
            let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
            program.paint(&mut PaintCx::new(&mut frame));
            let ImageContribution::Present(image) = &frame.images()[0] else {
                panic!("expected image");
            };
            let allocation = image.allocation.unwrap();
            assert_eq!(
                (
                    i64::from(allocation.x),
                    i64::from(allocation.y),
                    allocation.width,
                    allocation.height
                ),
                (x, y, 12, 6)
            );
            let fit = image.pixel_rect(11.0, 23.0);
            assert_eq!(
                (fit.width, fit.height),
                (132.0, 132.0),
                "clipping must never refit the source"
            );
            assert!(allocation.covers(image.destination));
            assert_visible_source_pixels(image, x, y);
            let tree = program.candidate.as_ref().unwrap();
            assert_eq!(
                format!("{tree:?}"),
                before,
                "paint must not move text or geometry"
            );
            let after: Vec<_> = (0..24)
                .flat_map(|cy| (0..80).map(move |cx| (cx, cy)))
                .map(|(cx, cy)| tree.hit_test(cx, cy).map(|node| node.key.clone()))
                .collect();
            assert_eq!(after, hits);
        }
    }

    #[test]
    fn image_allocation_rejects_overflow_and_offscreen_is_not_painted() {
        for bounds in [
            layout::CellRect::new(i64::MIN, 0, 12, 6),
            layout::CellRect::new(i64::MAX, 0, 12, 6),
            layout::CellRect::new(0, i64::MIN, 12, 6),
            layout::CellRect::new(0, i64::MAX, 12, 6),
            layout::CellRect::new(0, 0, u64::MAX, 6),
            layout::CellRect::new(0, 0, 12, u64::MAX),
        ] {
            assert!(image_allocation(bounds).is_none());
        }
        assert_eq!(
            image_allocation(layout::CellRect::new(0, 0, 12, 6))
                .unwrap()
                .x,
            0
        );
        let mut program = program();
        program.set_graphics_available(true);
        program
            .apply_view(
                View::from(artwork_image("cover", "/cover.webp", false)),
                None,
            )
            .unwrap();
        complete_artwork(&mut program, "cover", "/cover.webp");
        for bounds in [
            layout::CellRect::new(-12, 0, 12, 6),
            layout::CellRect::new(0, -6, 12, 6),
            layout::CellRect::new(80, 0, 12, 6),
            layout::CellRect::new(0, 24, 12, 6),
            layout::CellRect::new(0, 0, 0, 6),
            layout::CellRect::new(0, 0, 12, 0),
        ] {
            program.prepare().unwrap();
            let node = program
                .candidate
                .as_mut()
                .unwrap()
                .nodes
                .iter_mut()
                .find(|node| node.key == "cover")
                .unwrap();
            node.content_bounds = bounds;
            node.clip = layout::CellRect::new(0, 0, 80, 24);
            let mut buffer =
                bmux_tui::buffer::Buffer::empty(bmux_tui::geometry::Rect::new(0, 0, 80, 24));
            let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
            program.paint(&mut PaintCx::new(&mut frame));
            assert!(frame.images().is_empty(), "offscreen/empty {bounds:?}");
        }
    }

    #[test]
    fn footer_completion_is_present_on_next_paint_and_retained_with_same_key() {
        use hyperchad_transformer::Number;
        use hyperchad_transformer_models::Position;

        for (width, height) in [(80, 24), (120, 40), (140, 60)] {
            let mut program = program();
            program.set_graphics_available(true);
            program.options.viewport = Size::new(width, height);
            let mut cover = artwork_image("footer-cover", "/footer.webp", false);
            cover.width = Some(Number::Integer(32));
            cover.height = Some(Number::Integer(32));
            program
                .apply_view(
                    View::from(Container {
                        children: vec![Container {
                            str_id: Some("footer".into()),
                            position: Some(Position::Fixed),
                            bottom: Some(Number::Integer(0)),
                            width: Some(Number::Integer(640)),
                            height: Some(Number::Integer(48)),
                            children: vec![cover],
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                    None,
                )
                .unwrap();
            program.prepare().unwrap();
            assert!(program.graphics.rgba("footer-cover").is_none());
            assert!(
                program.pending.iter().any(
                    |work| matches!(work, Work::Resource { key, .. } if key == "footer-cover")
                )
            );
            let mut buffer =
                bmux_tui::buffer::Buffer::empty(bmux_tui::geometry::Rect::new(0, 0, width, height));
            let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
            program.paint(&mut PaintCx::new(&mut frame));
            assert!(
                frame.images().is_empty(),
                "pending is not a ready placement"
            );
            program.commit_prepared();
            let geometry = format!("{:?}", program.committed);
            let epoch = program.resource_epochs["footer-cover"];
            let mut encoded = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                2,
                2,
                image::Rgba([91, 113, 157, 255]),
            ))
            .write_to(&mut encoded, image::ImageFormat::WebP)
            .unwrap();
            program.pending.clear();
            program
                .update(RuntimeEvent::Message(Message::Completed {
                    generation: epoch,
                    work: Completion::Resource {
                        key: "footer-cover".into(),
                        source: "/footer.webp".into(),
                    },
                    result: Ok(HostResponse::Bytes(encoded.into_inner())),
                }))
                .unwrap();
            let pixels = program.graphics.rgba("footer-cover").unwrap().2.to_vec();
            let mut previous = None;
            for _ in 0..2 {
                program.prepare().unwrap();
                let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
                program.paint(&mut PaintCx::new(&mut frame));
                assert_eq!(
                    frame.images().len(),
                    1,
                    "ready footer must paint immediately"
                );
                let placement = format!("{:?}", frame.images());
                if let Some(previous) = &previous {
                    assert_eq!(
                        &placement, previous,
                        "same-key next paint must retain placement"
                    );
                }
                previous = Some(placement);
                program.commit_prepared();
                assert_eq!(format!("{:?}", program.committed), geometry);
                assert_eq!(program.resource_epochs["footer-cover"], epoch);
                assert_eq!(program.graphics.rgba("footer-cover").unwrap().2, pixels);
                assert!(
                    program.pending.is_empty(),
                    "ready resource must not refetch"
                );
            }
        }
    }

    #[test]
    fn evicted_artwork_reloads_once_on_reveal_without_pressure_retry_loop() {
        let mut program = program();
        program.graphics = GraphicsStore::new(crate::graphics::GraphicsLimits {
            max_entries: 1,
            ..Default::default()
        });
        program
            .apply_view(View::from(artwork_image("art", "/cover.webp", true)), None)
            .unwrap();
        program.prepare().unwrap();
        complete_artwork(&mut program, "art", "/cover.webp");
        let serial = program.resource_epochs["art"];
        program
            .graphics
            .register_rgba("pressure", 200, 200, vec![255; 200 * 200 * 4])
            .unwrap();
        assert!(program.graphics.rgba("art").is_none());
        for _ in 0..3 {
            program.prepare().unwrap();
            assert!(
                program.pending.is_empty(),
                "cache pressure must not spin fetches"
            );
        }
        // Fully clipped viewport, then reveal the same node/source again.
        program.options.viewport = bmux_tui::geometry::Size::new(0, 0);
        program.prepare().unwrap();
        program.options.viewport = bmux_tui::geometry::Size::new(80, 24);
        program.prepare().unwrap();
        assert!(program.resource_epochs["art"] > serial);
        assert_eq!(program.pending.len(), 1);
        program.prepare().unwrap();
        assert_eq!(program.pending.len(), 1, "one reload admission only");
        complete_artwork(&mut program, "art", "/cover.webp");
        assert!(program.graphics.rgba("art").is_some());
    }

    #[test]
    fn recreated_artwork_nodes_reuse_bounded_context_scoped_source_cache() {
        let mut program = program();
        program
            .apply_view(View::from(artwork_image("old", "/cover.webp", false)), None)
            .unwrap();
        complete_artwork(&mut program, "old", "/cover.webp");
        let serial = program.resource_serial;
        program
            .apply_view(View::from(Container::default()), None)
            .unwrap();
        assert!(program.resources.is_empty());
        assert!(program.graphics.rgba("old").is_some());
        program
            .apply_view(View::from(artwork_image("new", "/cover.webp", false)), None)
            .unwrap();
        program.prepare().unwrap();
        assert!(program.graphics.rgba("new").is_some());
        assert_eq!(
            program.resource_serial, serial,
            "no network admission on cached return"
        );
        assert!(program.pending.is_empty());
        // A context change must purge cached resources even without live DOM nodes.
        program
            .apply_view(View::from(Container::default()), None)
            .unwrap();
        program.resource_context = Some(123);
        program.revalidate_resource_context().unwrap();
        assert!(program.graphics.rgba("old").is_none());
        assert!(program.graphics.rgba("new").is_none());
        assert!(program.resource_sources.is_empty());
        program
            .apply_view(View::from(artwork_image("new", "/cover.webp", false)), None)
            .unwrap();
        program.prepare().unwrap();
        assert!(program.resource_serial > serial);
    }

    #[test]
    fn offscreen_queued_lazy_artwork_releases_admission_for_next_reveal() {
        let mut program = program();
        program
            .apply_view(View::from(artwork_image("art", "/cover.webp", true)), None)
            .unwrap();
        program.prepare().unwrap();
        assert_eq!(program.pending.len(), 1);
        let serial = program.resource_epochs["art"];
        program.options.viewport = bmux_tui::geometry::Size::new(0, 0);
        program.prepare().unwrap();
        assert!(program.pending.is_empty());
        assert!(!program.resources.contains_key("art"));
        program.options.viewport = bmux_tui::geometry::Size::new(80, 24);
        program.prepare().unwrap();
        assert_eq!(program.pending.len(), 1);
        assert!(program.resource_epochs["art"] > serial);
    }

    #[test]
    fn failed_artwork_is_not_retried_on_reveal_or_unrelated_cache_eviction() {
        for response in [
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into()),
            Ok(HostResponse::Bytes(b"invalid raster".to_vec())),
        ] {
            let mut program = program();
            program
                .apply_view(View::from(artwork_image("art", "/cover.webp", true)), None)
                .unwrap();
            program.prepare().unwrap();
            let serial = program.resource_epochs["art"];
            program.pending.clear();
            program
                .complete(
                    serial,
                    Completion::Resource {
                        key: "art".into(),
                        source: "/cover.webp".into(),
                    },
                    response,
                )
                .unwrap();
            program.options.viewport = bmux_tui::geometry::Size::new(0, 0);
            program.prepare().unwrap();
            program.options.viewport = bmux_tui::geometry::Size::new(80, 24);
            program.prepare().unwrap();
            assert_eq!(program.resource_epochs["art"], serial);
            assert!(program.pending.is_empty());
            assert!(!program.resource_loaded.contains("art"));
        }
    }

    #[test]
    fn active_lazy_artwork_keeps_identity_when_scrolled_offscreen() {
        let mut program = program();
        program
            .apply_view(View::from(artwork_image("art", "/cover.webp", true)), None)
            .unwrap();
        program.prepare().unwrap();
        let serial = program.resource_epochs["art"];
        // Removing from the pending queue models dispatch into an active lane.
        program.pending.clear();
        program.resource_active = 1;
        program.options.viewport = bmux_tui::geometry::Size::new(0, 0);
        program.prepare().unwrap();
        assert_eq!(program.resource_active, 1);
        assert_eq!(program.resource_epochs["art"], serial);
        complete_artwork(&mut program, "art", "/cover.webp");
        assert_eq!(program.resource_active, 0);
        assert!(program.graphics.rgba("art").is_some());
        program.options.viewport = bmux_tui::geometry::Size::new(80, 24);
        program.prepare().unwrap();
        assert!(program.pending.is_empty());
    }

    #[test]
    fn detached_source_index_stays_within_graphics_lru_budget() {
        let mut program = program();
        program.graphics = GraphicsStore::new(crate::graphics::GraphicsLimits {
            max_entries: 2,
            max_total_bytes: 2 * 200 * 200 * 4,
            ..Default::default()
        });
        for index in 0..6 {
            let key = format!("art-{index}");
            let source = format!("/cover-{index}.webp");
            program
                .apply_view(View::from(artwork_image(&key, &source, false)), None)
                .unwrap();
            complete_artwork(&mut program, &key, &source);
            program
                .apply_view(View::from(Container::default()), None)
                .unwrap();
            assert!(program.resource_sources.len() <= 2);
            assert!(
                program
                    .resource_sources
                    .keys()
                    .all(|key| program.graphics.rgba(key).is_some())
            );
        }
        assert!(program.graphics.rgba("art-0").is_none());
        assert!(program.graphics.rgba("art-5").is_some());
        assert!(program.resources.is_empty());
        assert!(program.resource_loaded.is_empty());
    }

    #[test]
    fn failed_eviction_reload_becomes_negative_admission_not_reveal_retry() {
        let mut program = program();
        program
            .apply_view(View::from(artwork_image("art", "/cover.webp", true)), None)
            .unwrap();
        program.prepare().unwrap();
        complete_artwork(&mut program, "art", "/cover.webp");
        program.graphics.remove("art");
        program.options.viewport = bmux_tui::geometry::Size::new(0, 0);
        program.prepare().unwrap();
        program.options.viewport = bmux_tui::geometry::Size::new(80, 24);
        program.prepare().unwrap();
        let serial = program.resource_epochs["art"];
        program.pending.clear();
        program
            .complete(
                serial,
                Completion::Resource {
                    key: "art".into(),
                    source: "/cover.webp".into(),
                },
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into()),
            )
            .unwrap();
        for _ in 0..3 {
            program.options.viewport = bmux_tui::geometry::Size::new(0, 0);
            program.prepare().unwrap();
            program.options.viewport = bmux_tui::geometry::Size::new(80, 24);
            program.prepare().unwrap();
            assert!(program.pending.is_empty());
            assert_eq!(program.resource_epochs["art"], serial);
        }
    }

    #[test]
    fn correlated_navigation_and_history_restore_exact_document_without_load_replay() {
        let mut program = program();
        program.location = "/albums?sort=name".into();
        program
            .apply_view(
                View::from(artwork_image("cover", "/cover.webp", false)),
                None,
            )
            .unwrap();
        program
            .options
            .scroll_offsets
            .insert("main".into(), layout::ScrollOffset { x: 0, y: 42 });
        let albums = program.state.root().clone();
        program.navigation_pending =
            Some((7, "/album?albumId=2250".into(), program.resource_context));
        program
            .complete_navigation(
                6,
                "/album?albumId=2250".into(),
                Some(View::from(Container::default())),
            )
            .unwrap();
        assert_eq!(program.state.root(), &albums);
        program
            .complete_navigation(7, "/wrong".into(), Some(View::from(Container::default())))
            .unwrap();
        assert_eq!(program.state.root(), &albums);
        program
            .complete_navigation(
                7,
                "/album?albumId=2250".into(),
                Some(View::from(Container {
                    str_id: Some("detail".into()),
                    ..Default::default()
                })),
            )
            .unwrap();
        let generation = program.state.generation();
        assert_eq!(program.history_back.len(), 1);
        program.restore_history(false).unwrap();
        assert_eq!(program.location, "/albums?sort=name");
        assert_eq!(program.state.root(), &albums);
        assert_eq!(program.options.scroll_offsets["main"].y, 42);
        assert!(program.state.generation() > generation);
        assert!(program.navigation_pending.is_none());
        assert!(
            !program
                .pending
                .iter()
                .any(|work| matches!(work, Work::Navigate(_)))
        );
        program.restore_history(true).unwrap();
        assert_eq!(program.location, "/album?albumId=2250");
        assert_eq!(program.state.root().str_id.as_deref(), Some("detail"));
    }

    #[test]
    fn navigation_failure_and_context_change_never_commit_or_restore_stale_document() {
        let epoch = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let observed = Arc::clone(&epoch);
        let (actions, _) = flume::bounded(8);
        let (resize, _) = flume::bounded(8);
        let mut program = BmuxProgram::new(
            BmuxConfig {
                resource_context: Some(Arc::new(move || {
                    observed.load(std::sync::atomic::Ordering::SeqCst)
                })),
                ..Default::default()
            },
            actions,
            resize,
        );
        program.revalidate_resource_context().unwrap();
        program.location = "/albums".into();
        program.history_document_valid = true;
        program.cache_current(false);
        assert_eq!(program.history_back.len(), 1);
        program.navigation_pending = Some((1, "/detail".into(), program.resource_context));
        program
            .complete_navigation(1, "/detail".into(), None)
            .unwrap();
        assert_eq!(program.location, "/albums");
        assert_eq!(
            program.history_back.len(),
            1,
            "failed navigation preserves legitimate history"
        );
        program.navigation_pending = Some((2, "/detail".into(), program.resource_context));
        epoch.store(2, std::sync::atomic::Ordering::SeqCst);
        // A completion is itself an observation boundary: no explicit wake needed.
        program
            .complete_navigation(2, "/detail".into(), Some(View::from(Container::default())))
            .unwrap();
        assert!(program.history_back.is_empty());
        assert!(program.history_forward.is_empty());
        assert!(program.navigation_pending.is_none());
        assert_eq!(program.location, "/albums");
        program.restore_history(false).unwrap();
        assert_eq!(program.location, "/albums");
    }

    #[test]
    fn history_restoration_does_not_replay_immediate_routes_on_later_partial() {
        let mut program = program();
        program.location = "/albums".into();
        let root = Container {
            str_id: Some("albums".into()),
            route: Some(hyperchad_transformer_models::Route::Get {
                route: "/populate".into(),
                trigger: Some("load".into()),
                target: hyperchad_transformer_models::Selector::SelfTarget,
                strategy: hyperchad_transformer_models::SwapStrategy::Children,
            }),
            ..Default::default()
        };
        program.apply_view(View::from(root), None).unwrap();
        program.pending.clear();
        program.completed_loads.insert("albums".into());
        program.cache_current(false);
        program.location = "/detail".into();
        program
            .apply_view(View::from(Container::default()), None)
            .unwrap();
        program.restore_history(false).unwrap();
        assert!(
            !program
                .pending
                .iter()
                .any(|work| matches!(work, Work::Host { .. }))
        );
        program.broadcast(&EventKind::Immediate, None).unwrap();
        assert!(
            !program
                .pending
                .iter()
                .any(|work| matches!(work, Work::Host { .. }))
        );
    }

    #[test]
    fn history_restores_unfinished_load_get_once_under_fresh_ownership() {
        let mut program = program();
        program.location = "/albums".into();
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("pending-albums".into()),
                    route: Some(hyperchad_transformer_models::Route::Get {
                        route: "/populate".into(),
                        trigger: Some("load".into()),
                        target: hyperchad_transformer_models::Selector::SelfTarget,
                        strategy: hyperchad_transformer_models::SwapStrategy::Children,
                    }),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        let old = program
            .pending
            .iter()
            .find_map(|work| {
                if let Work::Host {
                    completion: Completion::Request(request) | Completion::Read(request),
                    ..
                } = work
                {
                    Some(request.clone())
                } else {
                    None
                }
            })
            .unwrap();
        program.cache_current(false);
        program.invalidate_document_work();
        program.location = "/detail".into();
        program
            .apply_view(View::from(Container::default()), None)
            .unwrap();
        program.restore_history(false).unwrap();
        let requests: Vec<_> = program
            .pending
            .iter()
            .filter_map(|work| {
                if let Work::Host {
                    completion: Completion::Request(request) | Completion::Read(request),
                    ..
                } = work
                {
                    Some(request.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(requests.len(), 1);
        let request = requests.into_iter().next().unwrap();
        assert_ne!(request.source.generation, old.source.generation);
        assert_eq!(request.method, "GET");
        program.pending.clear();
        program
            .complete(
                request.source.generation,
                Completion::Request(request),
                Ok(HostResponse::View(View::from(Container {
                    str_id: Some("loaded".into()),
                    ..Default::default()
                }))),
            )
            .unwrap();
        assert!(program.state.find_node("loaded").is_some());
        assert!(program.completed_loads.contains("pending-albums"));
        program.broadcast(&EventKind::Immediate, None).unwrap();
        assert!(
            !program
                .pending
                .iter()
                .any(|work| matches!(work, Work::Host { .. }))
        );
    }

    #[test]
    fn explicitly_registered_artwork_survives_first_visible_admission() {
        let mut program = program();
        program
            .apply_view(
                View::from(artwork_image("registered", "/cover.webp", false)),
                None,
            )
            .unwrap();
        program
            .graphics
            .register_rgba("registered", 1, 1, vec![91, 113, 157, 255])
            .unwrap();
        program.prepare().unwrap();
        assert_eq!(
            program.graphics.rgba("registered").unwrap().2,
            &[91, 113, 157, 255]
        );
        assert!(
            program.pending.is_empty(),
            "registered pixels must not schedule a destructive fetch"
        );
        assert!(program.resource_loaded.contains("registered"));
        program
            .apply_view(
                View::from(artwork_image("registered", "/different.webp", false)),
                None,
            )
            .unwrap();
        program.prepare().unwrap();
        assert!(
            program.graphics.rgba("registered").is_none(),
            "changed source must not inherit registered pixels"
        );
        assert!(program.pending.iter().any(
            |work| matches!(work, Work::Resource { source, .. } if source == "/different.webp")
        ));
    }

    #[test]
    fn eager_offscreen_aliases_do_not_fill_or_cycle_the_graphics_cache() {
        let mut program = program();
        let image = |index| Container {
            str_id: Some(format!("eager-{index}")),
            element: Element::Image {
                source: Some("/public/img/no-image-available.svg".into()),
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None,
            },
            width: Some(hyperchad_transformer::Number::Integer(200)),
            height: Some(hyperchad_transformer::Number::Integer(200)),
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    children: (0..100).map(image).collect(),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        assert!(
            program.resources.is_empty(),
            "all images wait for actual geometry"
        );
        program.prepare().unwrap();
        let visible = program.resource_visible.len();
        assert!(visible > 0 && visible < 100);
        let key = program.resources.keys().next().unwrap().clone();
        let source = program.resources[&key].clone();
        program.complete_resource(
            key,
            source,
            include_bytes!("../../../../app/native/public/img/no-image-available.svg"),
        );
        program.pending.clear();
        program.schedule_resources().unwrap();
        let bytes = program.graphics.resident_bytes();
        assert!(
            bytes < 4 * 1024 * 1024,
            "visible aliases must use display-sized rasters"
        );
        for _ in 0..10 {
            program.schedule_resources().unwrap();
        }
        assert_eq!(program.graphics.resident_bytes(), bytes);
        assert!(
            program.pending.is_empty(),
            "idle scans must not refetch or churn aliases"
        );
        assert!(program.graphics.len() <= visible);
    }

    #[test]
    fn navigation_dispatches_while_ordered_host_work_is_busy() {
        let mut program = program();
        program.active = true;
        program.pending.push_back(Work::Navigate("/albums".into()));
        program
            .pending
            .push_back(Work::Navigate("/settings".into()));
        let update = program.drive(Update::none());
        assert_eq!(update.commands.len(), 1);
        assert!(
            program.active,
            "navigation must not release the ordered effect lane"
        );
        assert!(program.pending.is_empty());
    }

    #[test]
    fn lazy_placeholder_grid_only_loads_visible_unique_sources() {
        let mut program = program();
        let image = |index| Container {
            str_id: Some(format!("placeholder-{index}")),
            width: Some(hyperchad_transformer::Number::Integer(32)),
            height: Some(hyperchad_transformer::Number::Integer(32)),
            element: Element::Image {
                source: Some("/album.svg".into()),
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: Some(hyperchad_transformer_models::ImageLoading::Lazy),
            },
            ..Default::default()
        };
        program
            .apply_view(
                View::from(Container {
                    children: (0..100).map(image).collect(),
                    ..Default::default()
                }),
                None,
            )
            .unwrap();
        assert!(
            program.resources.is_empty(),
            "lazy images wait for viewport geometry"
        );
        program.prepare().unwrap();
        assert_eq!(
            program
                .pending
                .iter()
                .filter(|work| matches!(work, Work::Resource { .. }))
                .count(),
            1
        );
        let key = program.resources.keys().next().unwrap().clone();
        program
            .graphics
            .register_rgba(&key, 1, 1, vec![255; 4])
            .unwrap();
        program.pending.clear();
        program.schedule_resources().unwrap();
        assert!(
            program.pending.is_empty(),
            "same-source placeholders reuse decoded bytes"
        );
        assert!(
            program.resources.len() < 100,
            "offscreen placeholders are not loaded"
        );
    }

    #[test]
    fn image_requests_have_bounded_parallelism() {
        let mut program = program();
        for index in 0..10 {
            program.pending.push_back(Work::Resource {
                key: index.to_string(),
                source: format!("/{index}.png"),
                generation: index,
            });
        }
        let update = program.drive(Update::none());
        assert_eq!(update.commands.len(), 4);
        assert_eq!(program.resource_active, 4);
        assert_eq!(program.pending.len(), 6);
        assert!(program.drive(Update::none()).commands.is_empty());
    }

    #[test]
    fn user_quit_exits_without_draining_stalled_work() {
        let mut program = program();
        program.active = true;
        program.resource_active = 1;
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        program.active_abort = Some((None, cancel));
        program
            .pending
            .push_back(Work::Navigate("/downloads".into()));
        let update = program
            .update(RuntimeEvent::Message(Message::Quit))
            .unwrap();
        assert_eq!(update.lifecycle, bmux_tui_runtime::Lifecycle::Exit);
        assert!(*receiver.borrow());
        assert!(program.pending.is_empty());
        assert!(update.commands.is_empty());
    }

    #[test]
    fn image_loads_do_not_wait_for_stalled_http_or_release_its_lane() {
        let mut program = program();
        program.active = true;
        program.pending.push_back(Work::Resource {
            key: "logo".into(),
            source: "/logo".into(),
            generation: 1,
        });
        let update = program.drive(Update::none());
        assert_eq!(update.commands.len(), 1);
        assert_eq!(program.resource_active, 1);
        assert!(program.active);
        program
            .complete(
                1,
                Completion::Resource {
                    key: "logo".into(),
                    source: "/logo".into(),
                },
                Err(std::io::Error::other("denied").into()),
            )
            .unwrap();
        assert_eq!(program.resource_active, 0);
        assert!(program.active);
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
            program.prepare().unwrap();
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
                program.prepare().unwrap();
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
            program.prepare().unwrap();
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
        program.prepare().unwrap();
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
            matches!(program.pending.front(), Some(Work::Host { completion: Completion::Request(request) | Completion::Read(request), .. })
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
            matches!(program.pending.front(), Some(Work::Host { completion: Completion::Request(request) | Completion::Read(request), .. })
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
            completion: Completion::Request(request) | Completion::Read(request),
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
                    width: Some(hyperchad_transformer::Number::Integer(24)),
                    height: Some(hyperchad_transformer::Number::Integer(24)),
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
        program.prepare().unwrap();
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
            program.prepare().unwrap();
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
        assert!(
            buffer
                .cells()
                .iter()
                .any(|cell| { cell.style.bg == Some(bmux_tui::style::Color::Rgb(40, 40, 40)) }),
            "native inputs must contrast with the application background"
        );
        for cell in buffer.cells() {
            assert!(
                matches!(
                    cell.style.bg,
                    Some(
                        bmux_tui::style::Color::Rgb(24, 26, 27)
                            | bmux_tui::style::Color::Rgb(40, 40, 40)
                    )
                ),
                "unexpected transparent or unrelated background for {}",
                cell.symbol
            );
        }
    }

    #[test]
    fn anonymous_image_sibling_partials_preserve_rgba_and_single_queued_resource() {
        let mut program = program();
        let image = Container {
            width: Some(hyperchad_transformer::Number::Integer(24)),
            height: Some(hyperchad_transformer::Number::Integer(24)),
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
        program.prepare().unwrap();
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
                completion: Completion::Request(_) | Completion::Read(_),
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
                    width: Some(hyperchad_transformer::Number::Integer(24)),
                    height: Some(hyperchad_transformer::Number::Integer(24)),
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
        program.prepare().unwrap();
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
                    width: Some(hyperchad_transformer::Number::Integer(24)),
                    height: Some(hyperchad_transformer::Number::Integer(24)),
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
        program.prepare().unwrap();
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
