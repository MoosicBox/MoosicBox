//! Replacement frame core: UI-owned document, bounded mailbox and one geometry projection.
//!
//! Intentionally not exported until the native feedback checkpoint is accepted.

use std::collections::BTreeMap;

use eframe::egui::{self, Response};
use hyperchad_transformer::Container;

use crate::{
    document::{Document, ElementKey, ElementStamp, MutationSummary},
    geometry::{GeometrySnapshot, GeometryViewport, NodeGeometry},
    layout::{EguiCalc, calculate_layout},
    runtime::{DocumentHandle, DocumentInbox, document_channel},
};

/// Real widget response and optional changed value for persistent event dispatch.
pub struct NodeOutput {
    pub response: Response,
    pub value: Option<String>,
}

/// Widget/resource boundary. Document mutation is deferred until after traversal.
pub trait FramePainter {
    /// Retire removed generations and reconcile retained resource/widget state.
    fn reconcile(
        &mut self,
        document: &Document,
        mutations: &[MutationSummary],
    ) -> Result<(), String>;
    /// Prepare inherited node state even when this parent is fully clipped.
    fn prepare_node(
        &mut self,
        _stamp: ElementStamp,
        _authored: &Container,
        _calculated: &Container,
        _geometry: &NodeGeometry<ElementKey>,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Paint one projected node. Must not lay out its HyperChad children.
    fn paint_node(
        &mut self,
        ui: &mut egui::Ui,
        stamp: ElementStamp,
        authored: &Container,
        calculated: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<Option<NodeOutput>, String>;
    /// Current-generation resource jobs still outstanding.
    fn pending_resources(&self) -> usize;
    /// Project retained widget state into fresh calculated roots before layout.
    fn prepare_layout(
        &mut self,
        _document: &Document,
        _roots: &mut [Container],
    ) -> Result<(), String> {
        Ok(())
    }
    /// Participation policy shared by paint, events and accessibility.
    fn allows_node(&self, _key: ElementKey) -> bool {
        true
    }
    /// Apply canvas updates at their exact position in the document mailbox.
    fn apply_canvas(
        &mut self,
        _document: &Document,
        _command: crate::canvas::CanvasCommand,
    ) -> Result<ElementStamp, String> {
        Err("canvas adapter not installed".to_owned())
    }
    /// Lossless native changes, especially select arrays and file lists.
    fn take_changes(&mut self) -> Vec<(ElementStamp, crate::events::EventValue)> {
        Vec::new()
    }
}

/// Interaction evidence retained until the event lane consumes this frame.
pub struct NodeInteraction {
    pub stamp: ElementStamp,
    pub output: NodeOutput,
}

/// Cloneable renderer facade. The UI bootstrap can be taken exactly once across
/// all clones, while pre-init views remain queued in the shared document mailbox.
type HostFactory<C> =
    dyn Fn(BackendFrame<C>, &egui::Context) -> Result<Box<dyn eframe::App>, String> + Send + Sync;

#[derive(Clone, Default)]
struct WindowConfiguration {
    size: Option<(f32, f32)>,
    position: Option<(i32, i32)>,
    background: Option<hyperchad_renderer::Color>,
    title: Option<String>,
}

pub struct EguiRenderer<C> {
    host_factory: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<HostFactory<C>>>>>,
    window: std::sync::Arc<std::sync::Mutex<WindowConfiguration>>,
    input_sender: flume::Sender<crate::events::InputEvent>,
    responsive: std::sync::Arc<
        std::sync::Mutex<BTreeMap<String, hyperchad_transformer::ResponsiveTrigger>>,
    >,
    context: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
    handle: DocumentHandle,
    bootstrap: std::sync::Arc<std::sync::Mutex<Option<BackendFrame<C>>>>,
    navigation: flume::Receiver<String>,
    pub(crate) navigation_sender: flume::Sender<String>,
    pub(crate) request_action: flume::Sender<(String, Option<hyperchad_actions::logic::Value>)>,
    pub(crate) on_resize: flume::Sender<(f32, f32)>,
    pub(crate) router: hyperchad_router::Router,
    pub(crate) client_info: std::sync::Arc<hyperchad_router::ClientInfo>,
}

impl<C> Clone for EguiRenderer<C> {
    fn clone(&self) -> Self {
        Self {
            host_factory: self.host_factory.clone(),
            window: self.window.clone(),
            input_sender: self.input_sender.clone(),
            responsive: self.responsive.clone(),
            context: self.context.clone(),
            handle: self.handle.clone(),
            bootstrap: self.bootstrap.clone(),
            navigation: self.navigation.clone(),
            navigation_sender: self.navigation_sender.clone(),
            request_action: self.request_action.clone(),
            on_resize: self.on_resize.clone(),
            router: self.router.clone(),
            client_info: self.client_info.clone(),
        }
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> EguiRenderer<C> {
    #[must_use]
    pub fn new(
        router: hyperchad_router::Router,
        request_action: flume::Sender<(String, Option<hyperchad_actions::logic::Value>)>,
        on_resize: flume::Sender<(f32, f32)>,
        client_info: std::sync::Arc<hyperchad_router::ClientInfo>,
        calculator: C,
    ) -> Self {
        let (handle, mut frame) = BackendFrame::new(calculator);
        let (input_sender, input_receiver) = flume::bounded(256);
        frame.input_receiver = Some(input_receiver);
        let (navigation_sender, navigation) = flume::unbounded();
        let navigation_output = navigation_sender.clone();
        let custom_output = request_action.clone();
        let resize_output = on_resize.clone();
        let factory: std::sync::Arc<HostFactory<C>> = std::sync::Arc::new(move |frame, _ctx| {
            Ok(Box::new(NativeHost {
                app: crate::feedback::FeedbackApp::from_frame(frame),
                navigation: navigation_output.clone(),
                custom: custom_output.clone(),
                resize: resize_output.clone(),
                size: None,
            }))
        });
        Self {
            host_factory: std::sync::Arc::new(std::sync::Mutex::new(Some(factory))),
            window: std::sync::Arc::default(),
            input_sender,
            responsive: std::sync::Arc::default(),
            context: std::sync::Arc::default(),
            handle,
            bootstrap: std::sync::Arc::new(std::sync::Mutex::new(Some(frame))),
            navigation,
            navigation_sender,
            request_action,
            on_resize,
            router,
            client_info,
        }
    }

    /// Install the real WidgetPainter/event/action/resource native host before taking the runner.
    /// No default host is guessed while adapter integration is in progress.
    pub fn install_host(
        &self,
        factory: impl Fn(BackendFrame<C>, &egui::Context) -> Result<Box<dyn eframe::App>, String>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), String> {
        *self
            .host_factory
            .lock()
            .map_err(|error| error.to_string())? = Some(std::sync::Arc::new(factory));
        Ok(())
    }

    /// Enqueue complete views including fragments/deletes/response metadata.
    ///
    /// # Errors
    /// * Returns the owned command if the UI mailbox has been dropped.
    pub fn submit(
        &self,
        view: hyperchad_renderer::View,
        origin: crate::document::ViewOrigin,
    ) -> Result<(), flume::TrySendError<crate::runtime::DocumentCommand>> {
        self.handle
            .submit(crate::runtime::DocumentCommand::Apply { view, origin })
    }

    /// Transfer exclusive frame ownership to native or feedback host, once only.
    ///
    /// # Errors
    /// * Returns an error if another clone already took the bootstrap or lock poisoned.
    pub fn take_frame(&self, ctx: &egui::Context) -> Result<BackendFrame<C>, String> {
        let frame = self
            .bootstrap
            .lock()
            .map_err(|error| error.to_string())?
            .take()
            .ok_or_else(|| "renderer UI bootstrap already taken".to_owned())?;
        *self.context.lock().map_err(|error| error.to_string())? = Some(ctx.clone());
        let ctx = ctx.clone();
        self.handle.wake().attach(move || ctx.request_repaint());
        Ok(frame)
    }

    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.navigation.recv_async().await.ok()
    }
}

struct NativeHost<C: EguiCalc + Clone + Send + Sync> {
    app: crate::feedback::FeedbackApp<C>,
    navigation: flume::Sender<String>,
    custom: flume::Sender<(String, Option<hyperchad_actions::logic::Value>)>,
    resize: flume::Sender<(f32, f32)>,
    size: Option<egui::Vec2>,
}
impl<C: EguiCalc + Clone + Send + Sync + 'static> eframe::App for NativeHost<C> {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.app.frame(ctx);
        for url in self.app.drain_navigation() {
            if let Err(error) = self.navigation.send(url) {
                log::error!("navigation output: {error}");
            }
        }
        for action in self.app.drain_actions() {
            if let Err(error) = self.custom.send(action) {
                log::error!("custom action output: {error}");
            }
        }
        let size = ctx.content_rect().size();
        if self.size != Some(size) {
            self.size = Some(size);
            let _ = self.resize.send((size.x, size.y));
        }
    }
}

pub struct EguiRenderRunner<C> {
    renderer: EguiRenderer<C>,
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> hyperchad_renderer::ToRenderRunner
    for EguiRenderer<C>
{
    fn to_runner(
        self,
        _: hyperchad_renderer::Handle,
    ) -> Result<Box<dyn hyperchad_renderer::RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(EguiRenderRunner { renderer: self }))
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> hyperchad_renderer::RenderRunner
    for EguiRenderRunner<C>
{
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        let error = |message: String| -> Box<dyn std::error::Error + Send> {
            Box::new(std::io::Error::other(message))
        };
        let config = self
            .renderer
            .window
            .lock()
            .map_err(|e| error(e.to_string()))?
            .clone();
        let size = config.size.ok_or_else(|| {
            error("renderer must be initialized before native startup".to_owned())
        })?;
        let factory = self
            .renderer
            .host_factory
            .lock()
            .map_err(|e| error(e.to_string()))?
            .clone()
            .ok_or_else(|| {
                error(
                    "replacement native WidgetPainter/event/resource host not installed".to_owned(),
                )
            })?;
        let mut viewport = egui::ViewportBuilder::default().with_inner_size([size.0, size.1]);
        if let Some((x, y)) = config.position {
            #[allow(clippy::cast_precision_loss)]
            {
                viewport = viewport.with_position([x as f32, y as f32]);
            }
        }
        let renderer = &self.renderer;
        eframe::run_native(
            config.title.as_deref().unwrap_or("HyperChad"),
            eframe::NativeOptions {
                viewport,
                ..Default::default()
            },
            Box::new(move |cc| {
                let frame = renderer
                    .take_frame(&cc.egui_ctx)
                    .map_err(std::io::Error::other)?;
                factory(frame, &cc.egui_ctx).map_err(|message| {
                    Box::new(std::io::Error::other(message))
                        as Box<dyn std::error::Error + Send + Sync>
                })
            }),
        )
        .map_err(|e| error(e.to_string()))
    }
}

#[async_trait::async_trait]
impl<C: EguiCalc + Clone + Send + Sync + 'static> hyperchad_renderer::Renderer for EguiRenderer<C> {
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<hyperchad_renderer::Color>,
        title: Option<&str>,
        _description: Option<&str>,
        _viewport: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(Box::new(std::io::Error::other(
                "invalid native viewport dimensions",
            )));
        }
        let mut config = self.window.lock().map_err(|e| {
            Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send>
        })?;
        config.size = Some((width, height));
        config.position = x.zip(y);
        config.title = title.map(str::to_owned);
        config.background = background;
        Ok(())
    }
    fn add_responsive_trigger(
        &mut self,
        name: String,
        trigger: hyperchad_transformer::ResponsiveTrigger,
    ) {
        self.responsive.lock().unwrap().insert(name, trigger);
    }
    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.input_sender
            .try_send(crate::events::InputEvent::Named {
                name: event_name,
                value: event_value.map_or(
                    crate::events::EventValue::Null,
                    crate::events::EventValue::String,
                ),
            })
            .map_err(|e| {
                Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send>
            })?;
        if let Some(ctx) = &*self.context.lock().unwrap() {
            ctx.request_repaint();
        }
        Ok(())
    }
    async fn render(
        &self,
        view: hyperchad_renderer::View,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.submit(view, crate::document::ViewOrigin::Navigation)
            .map_err(|e| {
                Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send>
            })
    }
    async fn render_canvas(
        &self,
        _update: hyperchad_renderer::canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.handle
            .submit(crate::runtime::DocumentCommand::Canvas(
                crate::canvas::CanvasCommand::Update(_update),
            ))
            .map_err(|error| {
                Box::new(std::io::Error::other(error.to_string()))
                    as Box<dyn std::error::Error + Send>
            })
    }
}

/// UI-thread-only state; no document or calculated tree is shared with producers.
pub struct BackendFrame<C> {
    document: Document,
    inbox: DocumentInbox,
    calculator: C,
    initialized: bool,
    revision: u64,
    roots: Vec<Container>,
    geometry: Option<GeometrySnapshot<ElementKey>>,
    scroll: BTreeMap<ElementKey, egui::Vec2>,
    viewport: Option<GeometryViewport>,
    backlog: bool,
    input_receiver: Option<flume::Receiver<crate::events::InputEvent>>,
    actions: crate::action_runtime::ActionRuntime,
    pub action_effects: Vec<crate::action_runtime::ActionEffects>,
    action_dirty: bool,
    pointer_gesture: u64,
    active_gestures: BTreeMap<u8, u64>,
    events: crate::events::EventRouter,
    pub routed_events: Vec<crate::events::UiEvent>,
    /// Already-normalized inputs from the platform adapter, in native order.
    pub pending_inputs: Vec<crate::events::InputEvent>,
    pub diagnostics: Vec<String>,
    pub interactions: Vec<NodeInteraction>,
    /// Response effects are retained for the host; unsupported effects also diagnose.
    pub response_effects: Vec<hyperchad_renderer::ResponseMetadata>,
}

impl<C: EguiCalc + Clone> BackendFrame<C> {
    /// Create a single UI owner and its cloneable pre-init command producer.
    #[must_use]
    pub fn new(calculator: C) -> (DocumentHandle, Self) {
        let (handle, inbox) = document_channel();
        (
            handle,
            Self {
                document: Document::new(),
                inbox,
                calculator,
                initialized: false,
                revision: 0,
                roots: Vec::new(),
                geometry: None,
                scroll: BTreeMap::new(),
                viewport: None,
                backlog: false,
                input_receiver: None,
                actions: crate::action_runtime::ActionRuntime::new(),
                action_effects: Vec::new(),
                action_dirty: false,
                pointer_gesture: 0,
                active_gestures: BTreeMap::new(),
                events: crate::events::EventRouter::new(),
                routed_events: Vec::new(),
                pending_inputs: Vec::new(),
                diagnostics: Vec::new(),
                interactions: Vec::new(),
                response_effects: Vec::new(),
            },
        )
    }

    /// Clear accumulated diagnostics after the host has observed them.
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }

    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn geometry(&self) -> Option<&GeometrySnapshot<ElementKey>> {
        self.geometry.as_ref()
    }

    /// Apply queued mutations, calculate owned clones and paint projected geometry.
    /// No network operation, blocking receive, or callback-side document mutation.
    pub fn frame(&mut self, ctx: &egui::Context, painter: &mut impl FramePainter) {
        if let Some(receiver) = &self.input_receiver {
            self.pending_inputs.extend(receiver.try_iter().take(256));
            if !receiver.is_empty() {
                ctx.request_repaint();
            }
        }
        self.interactions.clear();
        self.routed_events.clear();
        if !self.initialized {
            egui_extras::install_image_loaders(ctx);
            self.calculator = self.calculator.clone().with_context(ctx.clone());
            self.initialized = true;
        }
        let batch = self
            .inbox
            .drain(&mut self.document, 256, |document, command| {
                painter.apply_canvas(document, command)
            });
        self.backlog = batch.has_more;
        if self.backlog {
            ctx.request_repaint();
        }
        let mut mutations = Vec::new();
        for result in batch.results {
            match result {
                crate::runtime::CommandOutcome::Document(Ok(mutation)) => {
                    for diagnostic in &mutation.diagnostics {
                        self.diagnostics.push(format!("document: {diagnostic:?}"));
                    }
                    if mutation.response != hyperchad_renderer::ResponseMetadata::default() {
                        self.diagnostics.push(format!(
                            "unhandled response metadata: {:?}",
                            mutation.response
                        ));
                        self.response_effects.push(mutation.response.clone());
                    }
                    mutations.push(mutation);
                }
                crate::runtime::CommandOutcome::Document(Err(error)) => {
                    self.diagnostics.push(error.to_string())
                }
                crate::runtime::CommandOutcome::Canvas(Ok(_)) => {}
                crate::runtime::CommandOutcome::Canvas(Err(error)) => self.diagnostics.push(error),
            }
        }
        match self.events.reconcile(&self.document) {
            Ok(events) => self.routed_events.extend(events),
            Err(error) => self.diagnostics.push(error.to_string()),
        }
        if let Err(error) = painter.reconcile(&self.document, &mutations) {
            self.diagnostics.push(error);
        }
        let viewport = GeometryViewport {
            rect: ctx.content_rect(),
            zoom: 1.0,
        };
        let dirty = self
            .viewport
            .as_ref()
            .is_none_or(|old| old.rect != viewport.rect || old.zoom != viewport.zoom)
            || self.action_dirty
            || mutations.iter().any(|mutation| mutation.layout_dirty);
        if dirty {
            self.geometry = None;
            self.roots = self.actions.projected_roots(&self.document);
            self.action_dirty = false;
            if let Err(error) = painter.prepare_layout(&self.document, &mut self.roots) {
                self.diagnostics.push(error);
            }
            let mut failed = false;
            for root in &mut self.roots {
                if let Err(error) = calculate_layout(&self.calculator, root, viewport) {
                    self.diagnostics.push(error.to_string());
                    failed = true;
                }
            }
            let paths = key_paths(&self.document);
            if !failed {
                self.revision = match self.revision.checked_add(1) {
                    Some(revision) => revision,
                    None => {
                        self.diagnostics.push("frame revision exhausted".to_owned());
                        return;
                    }
                };
                match GeometrySnapshot::build_roots(
                    &self.roots,
                    viewport,
                    self.revision,
                    &self.scroll,
                    |path, _| paths.get(path).copied(),
                ) {
                    Ok(snapshot) => self.geometry = Some(snapshot),
                    Err(error) => self.diagnostics.push(error.to_string()),
                }
            }
            self.viewport = Some(viewport);
        }
        let Some(snapshot) = &self.geometry else {
            return;
        };
        if let Err(error) = self.actions.refresh(&self.document, snapshot) {
            self.diagnostics.push(error);
        }
        match self.events.refresh_geometry(&self.document, snapshot) {
            Ok(events) => self.routed_events.extend(events),
            Err(error) => self.diagnostics.push(error.to_string()),
        }
        let paths = key_paths(&self.document);
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ctx, |ui| {
                for (path, key) in &paths {
                    let (Some(geometry), Some(stamp), Some(authored), Some(calculated)) = (
                        snapshot.node(*key),
                        self.document.stamp(*key),
                        self.document.get(*key),
                        at_path(&self.roots, path),
                    ) else {
                        continue;
                    };
                    if !painter.allows_node(*key) {
                        continue;
                    }
                    if let Err(error) = painter.prepare_node(stamp, authored, calculated, geometry)
                    {
                        self.diagnostics.push(error);
                    }
                    if geometry.visible_bounds().is_none() {
                        continue;
                    }
                    let mut builder = egui::UiBuilder::new()
                        .id_salt(*key)
                        .max_rect(geometry.border_box);
                    if let Some(parent) = self.document.parent(*key) {
                        builder = builder.accessibility_parent(egui::Id::new(parent));
                    }
                    ui.scope_builder(builder, |ui| {
                        ui.set_clip_rect(geometry.paint_clip);
                        match painter.paint_node(ui, stamp, authored, calculated, geometry) {
                            Ok(Some(output)) => {
                                self.interactions.push(NodeInteraction { stamp, output })
                            }
                            Ok(None) => {}
                            Err(error) => self.diagnostics.push(error),
                        }
                    });
                }
            });
        // Feed normalized platform facts and authoritative native widget facts to
        // G's persistent router. H consumes routed_events after this traversal.
        let mut released_gesture = None;
        let raw_events = ctx.input(|input| input.events.clone());
        for event in raw_events {
            use crate::events::{InputEvent, PointerEvent};
            match event {
                egui::Event::PointerMoved(position) => {
                    self.pending_inputs.push(InputEvent::PointerMoved(position))
                }
                egui::Event::PointerGone => self.pending_inputs.push(InputEvent::PointerCancelled),
                egui::Event::Key {
                    key,
                    physical_key: _,
                    pressed,
                    repeat,
                    modifiers,
                } => {
                    let logical = match key {
                        egui::Key::ArrowDown => "ArrowDown",
                        egui::Key::ArrowUp => "ArrowUp",
                        egui::Key::ArrowLeft => "ArrowLeft",
                        egui::Key::ArrowRight => "ArrowRight",
                        egui::Key::Escape => "Escape",
                        egui::Key::Enter => "Enter",
                        egui::Key::Tab => "Tab",
                        egui::Key::Backspace => "Backspace",
                        egui::Key::Delete => "Delete",
                        egui::Key::Space => " ",
                        key => key.name(),
                    }
                    .to_owned();
                    let editing_consumed = self.interactions.iter().any(|interaction| {
                        interaction.output.response.has_focus()
                            && self
                                .document
                                .get(interaction.stamp.key)
                                .is_some_and(|node| {
                                    matches!(
                                        node.element,
                                        hyperchad_transformer::Element::Input { .. }
                                            | hyperchad_transformer::Element::Textarea { .. }
                                    )
                                })
                    });
                    self.pending_inputs
                        .push(InputEvent::Key(crate::events::KeyboardEvent {
                            key: logical,
                            code: None,
                            pressed,
                            repeat,
                            modifiers,
                            composing: false,
                            editing_consumed,
                        }));
                }
                egui::Event::WindowFocused(focus) => {
                    self.pending_inputs.push(InputEvent::WindowFocus(focus))
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    let gesture = if pressed {
                        self.pointer_gesture = self
                            .pointer_gesture
                            .checked_add(1)
                            .expect("pointer gesture exhausted");
                        self.active_gestures
                            .insert(button as u8, self.pointer_gesture);
                        self.pointer_gesture
                    } else {
                        self.active_gestures
                            .remove(&(button as u8))
                            .unwrap_or(self.pointer_gesture)
                    };
                    let native_widget = self
                        .interactions
                        .iter()
                        .rev()
                        .find(|interaction| interaction.output.response.rect.contains(pos))
                        .map(|interaction| interaction.stamp.key);
                    if !pressed {
                        released_gesture = Some(gesture);
                    }
                    self.pending_inputs.push(InputEvent::Pointer(PointerEvent {
                        gesture,
                        position: pos,
                        button,
                        pressed,
                        modifiers,
                        native_widget,
                    }));
                }
                _ => {}
            }
        }
        for input in std::mem::take(&mut self.pending_inputs) {
            match self.events.route(&self.document, snapshot, input) {
                Ok(events) => self.routed_events.extend(events),
                Err(error) => self.diagnostics.push(error.to_string()),
            }
        }
        for interaction in &self.interactions {
            use crate::events::{EventValue, InputEvent, WidgetEvent, WidgetEventKind};
            let mut facts = vec![WidgetEventKind::Focus {
                focused: interaction.output.response.has_focus(),
                editing: matches!(
                    self.document
                        .get(interaction.stamp.key)
                        .map(|node| &node.element),
                    Some(
                        hyperchad_transformer::Element::Input { .. }
                            | hyperchad_transformer::Element::Textarea { .. }
                    )
                ),
            }];
            if interaction.output.response.clicked() {
                facts.push(WidgetEventKind::Activate {
                    gesture: released_gesture,
                });
            }
            let value = interaction.output.value.clone().map(EventValue::String);
            if let Some(value) = value {
                facts.push(WidgetEventKind::Change(value));
            }
            for kind in facts {
                match self.events.route(
                    &self.document,
                    snapshot,
                    InputEvent::Widget(WidgetEvent {
                        source: interaction.stamp,
                        kind,
                    }),
                ) {
                    Ok(events) => self.routed_events.extend(events),
                    Err(error) => self.diagnostics.push(error.to_string()),
                }
            }
        }
        for (source, value) in painter.take_changes() {
            use crate::events::{InputEvent, WidgetEvent, WidgetEventKind};
            match self.events.route(
                &self.document,
                snapshot,
                InputEvent::Widget(WidgetEvent {
                    source,
                    kind: WidgetEventKind::Change(value),
                }),
            ) {
                Ok(events) => self.routed_events.extend(events),
                Err(error) => self.diagnostics.push(error.to_string()),
            }
        }
        self.events.end_input_batch();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let now_ms = ctx.input(|input| (input.time.max(0.0) * 1000.0) as u64);
        for event in &self.routed_events {
            if let Err(error) = self.actions.dispatch(&self.document, event, now_ms) {
                self.diagnostics.push(error);
            }
        }
        if let Err(error) = self.actions.tick(now_ms) {
            self.diagnostics.push(error);
        }
        let effects = self.actions.take_effects();
        for (key, focused) in &effects.focus {
            let id = egui::Id::new(*key);
            ctx.memory_mut(|memory| {
                if *focused {
                    memory.request_focus(id);
                } else {
                    memory.surrender_focus(id);
                }
            });
        }
        for key in &effects.select_input {
            let id = egui::Id::new(*key);
            if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, id) {
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(usize::MAX),
                    )));
                state.store(ctx, id);
            } else {
                self.diagnostics
                    .push("select-input target has no mounted native editor".to_owned());
            }
        }
        self.action_dirty |= effects.rebuild;
        self.diagnostics.extend(effects.diagnostics.iter().cloned());
        self.action_effects.push(effects);
        if self.action_dirty {
            ctx.request_repaint();
        }
        if let Some(deadline) = self.actions.next_deadline() {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                deadline.saturating_sub(now_ms),
            ));
        }
    }

    #[must_use]
    pub fn ready(&self, painter: &impl FramePainter) -> bool {
        !self.backlog && self.geometry.is_some() && painter.pending_resources() == 0
    }
}

fn key_paths(document: &Document) -> BTreeMap<Vec<usize>, ElementKey> {
    fn collect(
        document: &Document,
        key: ElementKey,
        path: Vec<usize>,
        paths: &mut BTreeMap<Vec<usize>, ElementKey>,
    ) {
        paths.insert(path.clone(), key);
        for (index, child) in document.children(key).into_iter().enumerate() {
            let mut child_path = path.clone();
            child_path.push(index);
            collect(document, child, child_path, paths);
        }
    }
    let mut paths = BTreeMap::new();
    for (index, key) in document.root_keys().into_iter().enumerate() {
        collect(document, key, vec![index], &mut paths);
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ViewOrigin;
    use crate::runtime::DocumentCommand;

    #[derive(Clone)]
    struct Calc;
    impl hyperchad_transformer::layout::Calc for Calc {
        fn calc(&self, _: &mut Container) -> bool {
            false
        }
    }
    impl EguiCalc for Calc {
        fn with_context(self, _: egui::Context) -> Self {
            self
        }
    }
    struct Probe;
    impl FramePainter for Probe {
        fn reconcile(&mut self, _: &Document, _: &[MutationSummary]) -> Result<(), String> {
            Ok(())
        }
        fn pending_resources(&self) -> usize {
            0
        }
        fn paint_node(
            &mut self,
            _: &mut egui::Ui,
            _: ElementStamp,
            _: &Container,
            _: &Container,
            _: &NodeGeometry<ElementKey>,
        ) -> Result<Option<NodeOutput>, String> {
            Ok(None)
        }
    }

    #[test]
    fn preinit_mailbox_budget_and_backlog_are_observable() {
        let (handle, inbox) = crate::runtime::document_channel_with_capacity(
            std::num::NonZeroUsize::new(512).unwrap(),
        );
        let mut frame = BackendFrame::new(Calc).1;
        frame.inbox = inbox;
        for _ in 0..257 {
            handle
                .submit(DocumentCommand::Apply {
                    view: hyperchad_renderer::View::default(),
                    origin: ViewOrigin::Navigation,
                })
                .unwrap();
        }
        let ctx = egui::Context::default();
        let mut painter = Probe;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            frame.frame(ctx, &mut painter)
        });
        assert!(frame.backlog);
        assert!(!frame.ready(&painter));
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            frame.frame(ctx, &mut painter)
        });
        assert!(!frame.backlog);
    }

    #[test]
    fn duplicate_source_ids_keep_distinct_path_keys() {
        let (handle, mut frame) = BackendFrame::new(Calc);
        let root = Container {
            id: 7,
            children: vec![Container {
                id: 7,
                ..Container::default()
            }],
            ..Container::default()
        };
        handle
            .submit(DocumentCommand::Apply {
                view: hyperchad_renderer::View {
                    primary: Some(root),
                    ..Default::default()
                },
                origin: ViewOrigin::Navigation,
            })
            .unwrap();
        let _ = frame.inbox.drain(&mut frame.document, 256, |_, _| {
            Err("unexpected canvas".to_owned())
        });
        let paths = key_paths(&frame.document);
        assert_eq!(paths.len(), 2);
        assert_ne!(paths[&vec![0]], paths[&vec![0, 0]]);
    }
}

fn at_path<'a>(roots: &'a [Container], path: &[usize]) -> Option<&'a Container> {
    let (root, children) = path.split_first()?;
    let mut node = roots.get(*root)?;
    for index in children {
        node = node.children.get(*index)?;
    }
    Some(node)
}
