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

/// Select from this crate's renderer features, independent of dependency feature
/// unification. Wgpu wins when both renderers are enabled, matching eframe's default.
const fn native_renderer() -> eframe::Renderer {
    #[cfg(feature = "wgpu")]
    {
        eframe::Renderer::Wgpu
    }
    #[cfg(not(feature = "wgpu"))]
    {
        eframe::Renderer::Glow
    }
}

/// Persistent native facts only; widgets remain the sole text/IME editors.
#[derive(Default)]
struct NativeKeyboardState {
    composing: bool,
}

impl NativeKeyboardState {
    const fn observe(&mut self, event: &egui::Event) {
        match event {
            egui::Event::Ime(egui::ImeEvent::Enabled | egui::ImeEvent::Preedit(_)) => {
                self.composing = true;
            }
            egui::Event::Ime(egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled)
            | egui::Event::WindowFocused(false) => self.composing = false,
            _ => {}
        }
    }

    fn key(
        &self,
        event: &egui::Event,
        editing_consumed: bool,
    ) -> Option<crate::events::KeyboardEvent> {
        let egui::Event::Key {
            key,
            physical_key,
            pressed,
            repeat,
            modifiers,
        } = event
        else {
            return None;
        };
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
        Some(crate::events::KeyboardEvent {
            key: logical,
            code: physical_key.and_then(physical_code).map(str::to_owned),
            pressed: *pressed,
            repeat: *repeat,
            modifiers: *modifiers,
            composing: self.composing,
            editing_consumed,
        })
    }
}

/// egui collapses numpad/main-row digits, Enter and punctuation. Do not invent
/// DOM physical codes for those ambiguous variants or infer from the logical key.
const fn physical_code(key: egui::Key) -> Option<&'static str> {
    use egui::Key;
    Some(match key {
        Key::A => "KeyA",
        Key::B => "KeyB",
        Key::C => "KeyC",
        Key::D => "KeyD",
        Key::E => "KeyE",
        Key::F => "KeyF",
        Key::G => "KeyG",
        Key::H => "KeyH",
        Key::I => "KeyI",
        Key::J => "KeyJ",
        Key::K => "KeyK",
        Key::L => "KeyL",
        Key::M => "KeyM",
        Key::N => "KeyN",
        Key::O => "KeyO",
        Key::P => "KeyP",
        Key::Q => "KeyQ",
        Key::R => "KeyR",
        Key::S => "KeyS",
        Key::T => "KeyT",
        Key::U => "KeyU",
        Key::V => "KeyV",
        Key::W => "KeyW",
        Key::X => "KeyX",
        Key::Y => "KeyY",
        Key::Z => "KeyZ",
        Key::ArrowDown => "ArrowDown",
        Key::ArrowUp => "ArrowUp",
        Key::ArrowLeft => "ArrowLeft",
        Key::ArrowRight => "ArrowRight",
        Key::Escape => "Escape",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Space => "Space",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        _ => return None,
    })
}

/// Keep native widget hit testing identical to `HyperChad`'s exact geometry.
/// Call before the first egui frame so nearest-widget picking never expands bounds.
pub fn configure_exact_hit_testing(ctx: &egui::Context) {
    ctx.style_mut(|style| style.interaction.interact_radius = 0.0);
}

#[cfg(test)]
mod exact_hit_policy_tests {
    use eframe::egui;
    #[test]
    fn outside_press_release_does_not_activate_or_change_value() {
        let ctx = egui::Context::default();
        super::configure_exact_hit_testing(&ctx);
        let bounds = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(16.0, 16.0));
        let outside = bounds.max + egui::vec2(1.0, 1.0);
        assert!(!bounds.contains(outside));
        let mut checked = false;
        for pressed in [None, Some(true), Some(false)] {
            let mut events = vec![egui::Event::PointerMoved(outside)];
            if let Some(pressed) = pressed {
                events.push(egui::Event::PointerButton {
                    pos: outside,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                });
            }
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().interact_size = egui::Vec2::ZERO;
                        ui.spacing_mut().icon_width = 16.0;
                        let response = ui.put(bounds, egui::Checkbox::new(&mut checked, ""));
                        assert!(!response.clicked());
                        assert!(!response.hovered());
                        assert!(!response.changed());
                    });
                },
            );
            assert!(!checked);
        }
    }
}

/// Real widget response and optional changed value for persistent event dispatch.
pub struct NodeOutput {
    pub response: Response,
    pub value: Option<String>,
}

/// Widget/resource boundary. Document mutation is deferred until after traversal.
pub trait FramePainter {
    /// Retire removed generations and reconcile retained resource/widget state.
    /// # Errors
    /// * Returns resource or retained state reconciliation failures.
    fn reconcile(
        &mut self,
        document: &Document,
        mutations: &[MutationSummary],
    ) -> Result<(), String>;
    /// Prepare inherited node state even when this parent is fully clipped.
    /// # Errors
    /// * Returns inherited state preparation failures.
    fn prepare_node(
        &mut self,
        _stamp: ElementStamp,
        _authored: &Container,
        _calculated: &Container,
        _geometry: &NodeGeometry<ElementKey>,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Paint one projected node. Must not lay out its `HyperChad` children.
    /// # Errors
    /// * Returns widget or resource preparation failures.
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
    /// # Errors
    /// * Returns invalid retained widget state errors.
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
    /// # Errors
    /// * Returns invalid canvas commands or unavailable canvas adapter errors.
    fn apply_canvas(
        &mut self,
        _document: &Document,
        _command: crate::canvas::CanvasCommand,
    ) -> Result<ElementStamp, String> {
        Err("canvas adapter not installed".to_owned())
    }
    /// Focus an authoritative native control.
    /// # Errors
    /// * Unsupported or unmounted control.
    fn focus(&mut self, _ctx: &egui::Context, _stamp: ElementStamp) -> Result<(), String> {
        Err("native focus adapter unavailable".to_owned())
    }
    /// Blur an authoritative native control.
    /// # Errors
    /// * Unsupported or unmounted control.
    fn blur(&mut self, _ctx: &egui::Context, _stamp: ElementStamp) -> Result<(), String> {
        Err("native blur adapter unavailable".to_owned())
    }
    /// Select all characters in an authoritative native editor.
    /// # Errors
    /// * Unsupported or unmounted editor.
    fn select_all(&mut self, _ctx: &egui::Context, _stamp: ElementStamp) -> Result<(), String> {
        Err("native selection adapter unavailable".to_owned())
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
    terminal_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    host_factory: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<HostFactory<C>>>>>,
    window: std::sync::Arc<std::sync::Mutex<WindowConfiguration>>,
    input_sender: flume::Sender<crate::events::InputEvent>,
    responsive: std::sync::Arc<
        std::sync::Mutex<BTreeMap<String, hyperchad_transformer::ResponsiveTrigger>>,
    >,
    context: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
    handle: DocumentHandle,
    bootstrap: std::sync::Arc<std::sync::Mutex<Option<BackendFrame<C>>>>,
    response_receiver: std::sync::Arc<
        std::sync::Mutex<Option<flume::Receiver<hyperchad_renderer::ResponseMetadata>>>,
    >,
    shutdown: flume::Receiver<()>,
    navigation: flume::Receiver<String>,
    pub(crate) navigation_sender: flume::Sender<String>,
    context_setup: Option<ContextSetup>,
    pub(crate) request_action: flume::Sender<(String, Option<hyperchad_actions::logic::Value>)>,
    pub(crate) on_resize: flume::Sender<(f32, f32)>,
    pub(crate) router: hyperchad_router::Router,
    pub(crate) client_info: std::sync::Arc<hyperchad_router::ClientInfo>,
}

type ContextSetup = std::sync::Arc<dyn Fn(&egui::Context) -> Result<(), String> + Send + Sync>;

impl<C> Clone for EguiRenderer<C> {
    fn clone(&self) -> Self {
        Self {
            context_setup: self.context_setup.clone(),
            terminal_error: self.terminal_error.clone(),
            host_factory: self.host_factory.clone(),
            window: self.window.clone(),
            input_sender: self.input_sender.clone(),
            responsive: self.responsive.clone(),
            context: self.context.clone(),
            handle: self.handle.clone(),
            bootstrap: self.bootstrap.clone(),
            response_receiver: self.response_receiver.clone(),
            shutdown: self.shutdown.clone(),
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
        let terminal_error = std::sync::Arc::new(std::sync::Mutex::new(None));
        frame.terminal_error = terminal_error.clone();
        let (shutdown_sender, shutdown) = flume::bounded(1);
        frame.shutdown_sender = Some(shutdown_sender);
        let (response_sender, response_receiver) = flume::bounded(64);
        frame.response_sender = Some(response_sender);
        let (input_sender, input_receiver) = flume::bounded(256);
        frame.input_receiver = Some(input_receiver);
        let (navigation_sender, navigation) = flume::unbounded();
        let navigation_output = navigation_sender.clone();
        let custom_output = request_action.clone();
        let resize_output = on_resize.clone();
        let host_router = router.clone();
        let factory: std::sync::Arc<HostFactory<C>> = std::sync::Arc::new(move |frame, _ctx| {
            Ok(Box::new(NativeHost {
                app: crate::feedback::FeedbackApp::from_frame(frame)
                    .with_native_capture_from_env()
                    .with_router(host_router.clone(), "/".to_owned()),
                navigation: navigation_output.clone(),
                custom: custom_output.clone(),
                resize: resize_output.clone(),
                size: None,
            }))
        });
        Self {
            context_setup: None,
            terminal_error,
            host_factory: std::sync::Arc::new(std::sync::Mutex::new(Some(factory))),
            window: std::sync::Arc::default(),
            input_sender,
            responsive: std::sync::Arc::default(),
            context: std::sync::Arc::default(),
            handle,
            bootstrap: std::sync::Arc::new(std::sync::Mutex::new(Some(frame))),
            response_receiver: std::sync::Arc::new(std::sync::Mutex::new(Some(response_receiver))),
            shutdown,
            navigation,
            navigation_sender,
            request_action,
            on_resize,
            router,
            client_info,
        }
    }

    /// Transfer the bounded response stream once across all facade clones.
    /// App is the sole redirect consumer; cookies remain rejected before commit.
    /// # Errors
    /// * The receiver was already taken or its lock was poisoned.
    pub fn take_response_receiver(
        &self,
    ) -> Result<flume::Receiver<hyperchad_renderer::ResponseMetadata>, String> {
        self.response_receiver
            .lock()
            .map_err(|error| error.to_string())?
            .take()
            .ok_or_else(|| "response receiver already taken".to_owned())
    }

    /// Configure caller-owned immutable resources before the first pass.
    /// Runs once at native creation, before calculator context initialization and
    /// painter registry retrieval. The default host is retained; callback errors
    /// abort startup. Do not replace fonts after this callback returns.
    #[must_use]
    pub fn with_context_setup(
        mut self,
        setup: impl Fn(&egui::Context) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        self.context_setup = Some(std::sync::Arc::new(setup));
        self
    }

    /// Install the real `WidgetPainter`/event/action/resource native host before taking the runner.
    ///
    /// # Errors
    /// * Returns an error if the shared factory lock is poisoned.
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
    #[allow(
        clippy::result_large_err,
        reason = "preserve the caller's owned bounded mailbox command on rejection"
    )]
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
        configure_exact_hit_testing(ctx);
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
        use futures_util::future::{Either, select};
        match select(
            Box::pin(self.shutdown.recv_async()),
            Box::pin(self.navigation.recv_async()),
        )
        .await
        {
            Either::Left(_) => None,
            Either::Right((result, _)) => result.ok(),
        }
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(feature = "feedback")]
        self.app.native_frame(ctx, frame);
        #[cfg(not(feature = "feedback"))]
        {
            let _ = frame;
            self.app.frame(ctx);
        }
        self.app.flush_responses(ctx);
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
        let mut viewport =
            egui::ViewportBuilder::default().with_inner_size(egui::vec2(size.0, size.1));
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
                renderer: native_renderer(),
                ..Default::default()
            },
            Box::new(move |cc| {
                #[cfg(all(feature = "feedback", feature = "wgpu"))]
                if let Some(state) = &cc.wgpu_render_state {
                    let info = state.adapter.get_info();
                    log::info!("HyperChad native renderer=Wgpu adapter={:?} device_type={:?} backend={:?} driver={:?}", info.name, info.device_type, info.backend, info.driver);
                }
                #[cfg(all(feature = "feedback", feature = "glow"))]
                if cc.gl.is_some() {
                    log::info!("HyperChad native renderer=Glow");
                }
                if let Some(setup) = &renderer.context_setup {
                    setup(&cc.egui_ctx).map_err(std::io::Error::other)?;
                }
                let frame = renderer
                    .take_frame(&cc.egui_ctx)
                    .map_err(std::io::Error::other)?;
                factory(frame, &cc.egui_ctx).map_err(|message| {
                    Box::new(std::io::Error::other(message))
                        as Box<dyn std::error::Error + Send + Sync>
                })
            }),
        )
        .map_err(|e| error(e.to_string()))?;
        let terminal_error = self
            .renderer
            .terminal_error
            .lock()
            .map_err(|e| error(e.to_string()))?
            .take();
        terminal_error.map_or(Ok(()), |message| Err(error(message)))
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
        drop(config);
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
        if !view.response.cookies.is_empty() {
            return Err(Box::new(std::io::Error::other(
                "native cookies require a real host store",
            )));
        }
        self.handle
            .submit_async(crate::runtime::DocumentCommand::Apply {
                view,
                origin: crate::document::ViewOrigin::Navigation,
            })
            .await
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
    terminal_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    command_handle: DocumentHandle,
    pub(crate) route_boundary: Option<crate::routing::PreparedCompletion>,
    pending_mutations: Vec<MutationSummary>,
    shutdown_sender: Option<flume::Sender<()>>,
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
    pub navigation_effects: Vec<(ElementStamp, String)>,
    pub action_effects: Vec<crate::action_runtime::ActionEffects>,
    action_dirty: bool,
    pointer_gesture: u64,
    active_gestures: BTreeMap<u8, (u64, Option<ElementKey>)>,
    events: crate::events::EventRouter,
    keyboard: NativeKeyboardState,
    pub routed_events: Vec<crate::events::UiEvent>,
    /// Already-normalized inputs from the platform adapter, in native order.
    pub pending_inputs: Vec<crate::events::InputEvent>,
    pub diagnostics: Vec<String>,
    pub interactions: Vec<NodeInteraction>,
    /// Response effects are retained for the host; unsupported effects also diagnose.
    response_sender: Option<flume::Sender<hyperchad_renderer::ResponseMetadata>>,
    pub response_effects: Vec<hyperchad_renderer::ResponseMetadata>,
}

impl<C: EguiCalc + Clone> BackendFrame<C> {
    /// Create a single UI owner and its cloneable pre-init command producer.
    #[must_use]
    pub fn new(calculator: C) -> (DocumentHandle, Self) {
        let (handle, inbox) = document_channel();
        (
            handle.clone(),
            Self {
                terminal_error: std::sync::Arc::default(),
                command_handle: handle,
                route_boundary: None,
                pending_mutations: Vec::new(),
                shutdown_sender: None,
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
                navigation_effects: Vec::new(),
                action_effects: Vec::new(),
                action_dirty: false,
                pointer_gesture: 0,
                active_gestures: BTreeMap::new(),
                events: crate::events::EventRouter::new(),
                keyboard: NativeKeyboardState::default(),
                routed_events: Vec::new(),
                pending_inputs: Vec::new(),
                diagnostics: Vec::new(),
                interactions: Vec::new(),
                response_sender: None,
                response_effects: Vec::new(),
            },
        )
    }

    /// Persist a terminal native-host failure for the runner to return after UI exit.
    pub fn fail_native_host(&mut self, ctx: &egui::Context, message: String) {
        if let Ok(mut error) = self.terminal_error.lock() {
            *error = Some(message);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Nonblocking response delivery. Overflow or a missing App consumer is fatal,
    /// closes this native viewport and drops the sole sender, never silently loses effects.
    pub fn flush_responses(&mut self, ctx: &egui::Context) {
        for response in std::mem::take(&mut self.response_effects) {
            if let Some(sender) = &self.response_sender {
                if let Err(error) = sender.try_send(response) {
                    self.diagnostics
                        .push(format!("fatal response delivery: {error}"));
                    self.response_sender = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    break;
                }
            } else {
                self.diagnostics
                    .push("response metadata requires installed App consumer".to_owned());
            }
        }
    }

    pub fn handle(&self) -> DocumentHandle {
        self.command_handle.clone()
    }

    /// Reconcile a route mutation on the next projection before later mailbox work.
    pub fn retain_route_mutation(&mut self, mutation: MutationSummary) {
        self.pending_mutations.push(mutation);
        self.invalidate_document();
    }

    /// UI-host lifecycle access. Call only outside node traversal.
    pub const fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    #[cfg(test)]
    pub(crate) fn reset_event_router_for_test(&mut self) {
        self.events = crate::events::EventRouter::new();
    }

    /// Invalidate after a ticket completion applied authored changes.
    pub fn invalidate_document(&mut self) {
        self.viewport = None;
        self.geometry = None;
    }

    /// Dispatch lifecycle before/terminal events synchronously before host I/O or swaps.
    ///
    /// # Errors
    /// * Missing geometry, invalid event target, or action handler failure.
    pub fn dispatch_host_event(
        &mut self,
        ctx: &egui::Context,
        input: crate::events::InputEvent,
    ) -> Result<(), String> {
        let snapshot = self
            .geometry
            .as_ref()
            .ok_or_else(|| "host lifecycle requires projected geometry".to_owned())?;
        let events = self
            .events
            .route(&self.document, snapshot, input)
            .map_err(|error| error.to_string())?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let now_ms = ctx.input(|input| (input.time.max(0.0) * 1000.0) as u64);
        for event in events {
            self.actions.dispatch(&self.document, &event, now_ms)?;
            let mut effects = self.actions.take_effects();
            self.navigation_effects
                .extend(effects.navigation.drain(..).map(|url| (event.source, url)));
            self.action_dirty |= effects.rebuild;
            self.action_effects.push(effects);
            self.routed_events.push(event);
        }
        let effects = self.actions.take_effects();
        self.action_dirty |= effects.rebuild;
        self.action_effects.push(effects);
        Ok(())
    }

    /// Clear accumulated diagnostics after the host has observed them.
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }

    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }

    #[cfg(feature = "feedback")]
    pub(crate) const fn observation_revision(&self) -> u64 {
        self.revision
    }

    #[cfg(feature = "feedback")]
    pub(crate) fn observation_visible(&self, key: ElementKey, painter: &impl FramePainter) -> bool {
        if !painter.allows_node(key) {
            return false;
        }
        let Some(path) = key_paths(&self.document)
            .into_iter()
            .find_map(|(path, current)| (current == key).then_some(path))
        else {
            return false;
        };
        let mut opacity = 1.0;
        for length in 1..=path.len() {
            let Some(container) = at_path(&self.roots, &path[..length]) else {
                return false;
            };
            opacity *= container.calculated_opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        }
        opacity.is_finite() && opacity > 0.0
    }

    pub const fn geometry(&self) -> Option<&GeometrySnapshot<ElementKey>> {
        self.geometry.as_ref()
    }

    /// Apply queued mutations, calculate owned clones and paint projected geometry.
    /// No network operation, blocking receive, or callback-side document mutation.
    /// # Panics
    /// * The process exhausts its monotonic pointer gesture counter.
    #[allow(
        clippy::too_many_lines,
        reason = "one ordered UI transaction; split only after the integration contract settles"
    )]
    pub fn frame(&mut self, ctx: &egui::Context, painter: &mut impl FramePainter) {
        // Snapshot before TextEdit can consume/reorder IME input. Never mutate the
        // widget stream: editing and committed text remain egui's responsibility.
        let raw_events = ctx.input(|input| input.events.clone());
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
        let batch = self.inbox.drain(
            &mut self.document,
            if self.route_boundary.is_some() {
                0
            } else {
                256
            },
            |document, command| painter.apply_canvas(document, command),
        );
        self.backlog = batch.has_more;
        if self.backlog {
            ctx.request_repaint();
        }
        let mut mutations = std::mem::take(&mut self.pending_mutations);
        for result in batch.results {
            match result {
                crate::runtime::CommandOutcome::Document(Ok(mutation)) => {
                    for diagnostic in &mutation.diagnostics {
                        self.diagnostics.push(format!("document: {diagnostic:?}"));
                    }
                    if !mutation.response.cookies.is_empty() {
                        self.diagnostics.push(
                            "cookie-bearing mailbox response requires a real host store".to_owned(),
                        );
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else if mutation.response != hyperchad_renderer::ResponseMetadata::default() {
                        self.response_effects.push(mutation.response.clone());
                    }
                    mutations.push(mutation);
                }
                crate::runtime::CommandOutcome::Document(Err(error)) => {
                    self.diagnostics.push(error.to_string());
                }
                crate::runtime::CommandOutcome::RouteCompletion(prepared) => {
                    self.route_boundary = Some(prepared);
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
        let projection_dirty = self.viewport.as_ref().is_none_or(|old| {
            old.rect != viewport.rect || old.zoom.to_bits() != viewport.zoom.to_bits()
        }) || self.action_dirty
            || mutations.iter().any(|mutation| mutation.layout_dirty);
        let mut wheel_changed = false;
        // A mailbox/action/viewport change invalidates the old hit targets and
        // scroll limits. Never apply this frame's wheel to that stale projection.
        if !projection_dirty && let Some(snapshot) = &self.geometry {
            let mut cursor_position = ctx.input(|input| input.pointer.hover_pos());
            for event in ctx.input(|input| input.events.clone()) {
                match event {
                    egui::Event::PointerMoved(position) => cursor_position = Some(position),
                    egui::Event::MouseWheel { unit, delta, .. } => {
                        if let Some(point) = cursor_position {
                            let delta = match unit {
                                egui::MouseWheelUnit::Point => delta,
                                egui::MouseWheelUnit::Line => delta * 24.0,
                                egui::MouseWheelUnit::Page => delta * viewport.rect.size(),
                            };
                            wheel_changed |= scroll_wheel(
                                &self.document,
                                snapshot,
                                &mut self.scroll,
                                point,
                                delta,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        let dirty = wheel_changed || projection_dirty;
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
                let Some(revision) = self.revision.checked_add(1) else {
                    self.diagnostics.push("frame revision exhausted".to_owned());
                    return;
                };
                self.revision = revision;
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
        // Resolve inheritance in document preorder, independent of stacking.
        for (path, key) in &paths {
            let (Some(geometry), Some(stamp), Some(authored), Some(calculated)) = (
                snapshot.node(*key),
                self.document.stamp(*key),
                self.document.get(*key),
                at_path(&self.roots, path),
            ) else {
                continue;
            };
            if painter.allows_node(*key)
                && let Err(error) = painter.prepare_node(stamp, authored, calculated, geometry)
            {
                self.diagnostics.push(error);
            }
        }
        let paths_by_key: BTreeMap<_, _> = paths.iter().map(|(path, key)| (*key, path)).collect();
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ctx, |ui| {
                for geometry in snapshot.paint_order() {
                    let key = geometry.key;
                    let Some(path) = paths_by_key.get(&key) else {
                        continue;
                    };
                    let (Some(stamp), Some(authored), Some(calculated)) = (
                        self.document.stamp(key),
                        self.document.get(key),
                        at_path(&self.roots, path),
                    ) else {
                        continue;
                    };
                    if !painter.allows_node(key) {
                        continue;
                    }
                    if geometry.visible_bounds().is_none() {
                        continue;
                    }
                    let mut builder = egui::UiBuilder::new()
                        .id_salt(key)
                        .max_rect(geometry.border_box);
                    if let Some(parent) = self.document.parent(key) {
                        builder = builder.accessibility_parent(egui::Id::new(parent));
                    }
                    ui.scope_builder(builder, |ui| {
                        ui.set_clip_rect(geometry.paint_clip);
                        match painter.paint_node(ui, stamp, authored, calculated, geometry) {
                            Ok(Some(output)) => {
                                self.interactions.push(NodeInteraction { stamp, output });
                            }
                            Ok(None) => {}
                            Err(error) => self.diagnostics.push(error),
                        }
                    });
                }
            });
        // Feed normalized platform facts and authoritative native widget facts to
        // G's persistent router. H consumes routed_events after this traversal.
        let mut released_gestures = BTreeMap::new();
        if let Some(interaction) = self
            .interactions
            .iter()
            .find(|interaction| interaction.output.response.has_focus())
        {
            let editing = self
                .document
                .get(interaction.stamp.key)
                .is_some_and(|node| {
                    matches!(
                        node.element,
                        hyperchad_transformer::Element::Input { .. }
                            | hyperchad_transformer::Element::Textarea { .. }
                    )
                });
            if let Err(error) = self.events.focus(
                &self.document,
                snapshot,
                Some(interaction.stamp.key),
                editing,
            ) {
                self.diagnostics.push(error.to_string());
            }
        }
        for event in raw_events {
            use crate::events::{InputEvent, PointerEvent};
            self.keyboard.observe(&event);
            match event {
                egui::Event::PointerMoved(position) => {
                    self.pending_inputs.push(InputEvent::PointerMoved(position));
                }
                egui::Event::PointerGone => self.pending_inputs.push(InputEvent::PointerDeparted),
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed,
                    repeat,
                    modifiers,
                } => {
                    let native_event = egui::Event::Key {
                        key,
                        physical_key,
                        pressed,
                        repeat,
                        modifiers,
                    };
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
                    }) && (matches!(
                        key,
                        egui::Key::ArrowLeft
                            | egui::Key::ArrowRight
                            | egui::Key::ArrowUp
                            | egui::Key::ArrowDown
                            | egui::Key::Home
                            | egui::Key::End
                            | egui::Key::Backspace
                            | egui::Key::Delete
                            | egui::Key::Space
                    ) || ((modifiers.command || modifiers.ctrl)
                        && matches!(
                            key,
                            egui::Key::A
                                | egui::Key::C
                                | egui::Key::V
                                | egui::Key::X
                                | egui::Key::Y
                                | egui::Key::Z
                        )));
                    if let Some(keyboard) = self.keyboard.key(&native_event, editing_consumed) {
                        self.pending_inputs.push(InputEvent::Key(keyboard));
                    }
                }
                egui::Event::WindowFocused(focus) => {
                    self.pending_inputs.push(InputEvent::WindowFocus(focus));
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    let hit_widget = native_hit_owner(snapshot, pos, &self.interactions, |key| {
                        painter.allows_node(key)
                    });
                    let (gesture, native_widget) = if pressed {
                        self.pointer_gesture = self
                            .pointer_gesture
                            .checked_add(1)
                            .expect("pointer gesture exhausted");
                        let entry = (self.pointer_gesture, hit_widget);
                        self.active_gestures.insert(button as u8, entry);
                        entry
                    } else {
                        self.active_gestures
                            .remove(&(button as u8))
                            .unwrap_or((self.pointer_gesture, None))
                    };
                    if !pressed
                        && button == egui::PointerButton::Primary
                        && let Some(owner) = native_widget
                    {
                        released_gestures.insert(owner, gesture);
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
                    gesture: released_gestures.get(&interaction.stamp.key).copied(),
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
        self.actions.sync_focus(
            self.interactions
                .iter()
                .find(|interaction| interaction.output.response.has_focus())
                .map(|interaction| interaction.stamp.key),
        );
        for event in &self.routed_events {
            if let Err(error) = self.actions.dispatch(&self.document, event, now_ms) {
                self.diagnostics.push(error);
            }
            let mut effects = self.actions.take_effects();
            self.navigation_effects
                .extend(effects.navigation.drain(..).map(|url| (event.source, url)));
            self.action_dirty |= effects.rebuild;
            self.action_effects.push(effects);
        }
        if let Err(error) = self.actions.tick(now_ms) {
            self.diagnostics.push(error);
        }
        let effects = self.actions.take_effects();
        self.action_dirty |= effects.rebuild;
        self.diagnostics.extend(effects.diagnostics.iter().cloned());
        self.action_effects.push(effects);
        for effects in &self.action_effects {
            for (key, focused) in &effects.focus {
                let result = match self.document.stamp(*key) {
                    Some(stamp) if *focused => painter.focus(ctx, stamp),
                    Some(stamp) => painter.blur(ctx, stamp),
                    None => Err("focus effect target is no longer mounted".to_owned()),
                };
                if let Err(error) = result {
                    self.diagnostics.push(error);
                }
            }
            for key in &effects.select_input {
                let result = self
                    .document
                    .stamp(*key)
                    .ok_or_else(|| "selection target is no longer mounted".to_owned())
                    .and_then(|stamp| painter.select_all(ctx, stamp));
                if let Err(error) = result {
                    self.diagnostics.push(error);
                }
            }
        }
        self.actions.sync_focus(
            self.interactions
                .iter()
                .find(|interaction| interaction.output.response.has_focus())
                .map(|interaction| interaction.stamp.key),
        );
        if self.action_dirty {
            ctx.request_repaint();
        }
        if let Some(deadline) = self.actions.next_deadline() {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                deadline.saturating_sub(now_ms),
            ));
        }
    }

    /// Required resource membership from current calculated participation and clipping.
    #[must_use]
    pub fn scene_images(&self, painter: &impl FramePainter) -> Vec<ElementStamp> {
        let Some(snapshot) = &self.geometry else {
            return Vec::new();
        };
        let paths = key_paths(&self.document);
        paths
            .iter()
            .filter_map(|(path, key)| {
                let node = at_path(&self.roots, path)?;
                if !matches!(node.element, hyperchad_transformer::Element::Image { .. })
                    || !painter.allows_node(*key)
                    || snapshot.node(*key)?.visible_bounds().is_none()
                {
                    return None;
                }
                let mut opacity = 1.0;
                for length in 1..=path.len() {
                    opacity *= at_path(&self.roots, &path[..length])?
                        .calculated_opacity
                        .unwrap_or(1.0);
                }
                (opacity.is_finite() && opacity > 0.0)
                    .then(|| self.document.stamp(*key))
                    .flatten()
            })
            .collect()
    }

    /// Current geometry exists and every admitted mailbox item preceding it settled.
    #[must_use]
    pub const fn projection_ready(&self) -> bool {
        !self.backlog && self.geometry.is_some() && self.route_boundary.is_none()
    }

    #[must_use]
    pub fn ready(&self, painter: &impl FramePainter) -> bool {
        !self.backlog && self.geometry.is_some() && painter.pending_resources() == 0
    }
}

fn native_hit_owner(
    snapshot: &GeometrySnapshot<ElementKey>,
    point: egui::Pos2,
    interactions: &[NodeInteraction],
    allows: impl Fn(ElementKey) -> bool,
) -> Option<ElementKey> {
    let mut key = snapshot.hit_test(point, |_| true);
    while let Some(current) = key {
        if allows(current)
            && interactions.iter().any(|interaction| {
                interaction.stamp.key == current
                    && interaction.output.response.enabled()
                    && interaction.output.response.sense.interactive()
            })
        {
            return Some(current);
        }
        key = snapshot.node(current)?.parent;
    }
    None
}

/// Consume screen-space wheel movement from the deepest clipped hit outward.
/// Each axis chains independently; fixed descendants start at their own DOM ancestors.
fn scroll_wheel(
    document: &Document,
    snapshot: &GeometrySnapshot<ElementKey>,
    offsets: &mut BTreeMap<ElementKey, egui::Vec2>,
    point: egui::Pos2,
    delta: egui::Vec2,
) -> bool {
    use hyperchad_transformer::models::LayoutOverflow;
    let mut remaining = delta;
    let mut current = snapshot.hit_test(point, |_| true);
    let mut changed = false;
    while let Some(key) = current {
        let Some(node) = snapshot.node(key) else {
            break;
        };
        current = node.parent;
        let Some(authored) = document.get(key) else {
            continue;
        };
        let zoom = node.transform.scale;
        let old = offsets
            .get(&key)
            .copied()
            .unwrap_or(node.scroll_offset)
            .clamp(egui::Vec2::ZERO, node.scroll_max);
        let mut next = old;
        for (axis, overflow) in [authored.overflow_x, authored.overflow_y]
            .into_iter()
            .enumerate()
        {
            if matches!(overflow, LayoutOverflow::Auto | LayoutOverflow::Scroll) {
                next[axis] = (old[axis] - remaining[axis] / zoom).clamp(0.0, node.scroll_max[axis]);
                remaining[axis] = (next[axis] - old[axis]).mul_add(zoom, remaining[axis]);
            }
        }
        if next != old {
            offsets.insert(key, next);
            changed = true;
        }
    }
    changed
}

fn key_paths(document: &Document) -> BTreeMap<Vec<usize>, ElementKey> {
    fn collect(
        document: &Document,
        key: ElementKey,
        path: &[usize],
        paths: &mut BTreeMap<Vec<usize>, ElementKey>,
    ) {
        paths.insert(path.to_vec(), key);
        for (index, child) in document.children(key).into_iter().enumerate() {
            let mut child_path = path.to_vec();
            child_path.push(index);
            collect(document, child, &child_path, paths);
        }
    }
    let mut paths = BTreeMap::new();
    for (index, key) in document.root_keys().into_iter().enumerate() {
        collect(document, key, &[index], &mut paths);
    }
    paths
}

fn at_path<'a>(roots: &'a [Container], path: &[usize]) -> Option<&'a Container> {
    let (root, children) = path.split_first()?;
    let mut node = roots.get(*root)?;
    for index in children {
        node = node.children.get(*index)?;
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ViewOrigin;
    use crate::runtime::DocumentCommand;

    #[test]
    #[cfg(all(feature = "glow", not(feature = "wgpu")))]
    fn native_renderer_selects_glow_without_wgpu() {
        assert_eq!(native_renderer(), eframe::Renderer::Glow);
    }

    #[test]
    #[cfg(all(feature = "wgpu", not(feature = "glow")))]
    fn native_renderer_selects_wgpu_without_glow() {
        assert_eq!(native_renderer(), eframe::Renderer::Wgpu);
    }

    #[test]
    #[cfg(all(feature = "glow", feature = "wgpu"))]
    fn native_renderer_prefers_wgpu_when_both_are_enabled() {
        assert_eq!(native_renderer(), eframe::Renderer::Wgpu);
    }

    #[test]
    fn wheel_offsets_chain_at_inner_limit_and_respect_clip() {
        use hyperchad_transformer::models::LayoutOverflow;
        fn node(id: usize, y: f32, height: f32) -> Container {
            Container {
                id,
                calculated_x: Some(0.0),
                calculated_y: Some(y),
                calculated_width: Some(100.0),
                calculated_height: Some(height),
                ..Container::default()
            }
        }
        let mut root = node(1, 0.0, 100.0);
        root.overflow_y = LayoutOverflow::Scroll;
        let mut inner = node(2, 0.0, 80.0);
        inner.overflow_y = LayoutOverflow::Scroll;
        inner.children.push(node(3, 0.0, 200.0));
        root.children = vec![inner, node(4, 300.0, 20.0)];
        let mut document = Document::new();
        document
            .apply_view(
                hyperchad_renderer::View {
                    primary: Some(root.clone()),
                    ..Default::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let paths = key_paths(&document);
        let viewport = GeometryViewport {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 100.0)),
            zoom: 1.0,
        };
        let snapshot =
            GeometrySnapshot::build_roots(&[root], viewport, 0, &BTreeMap::new(), |path, _| {
                paths.get(path).copied()
            })
            .unwrap();
        let root_key = paths[&vec![0]];
        let inner_key = paths[&vec![0, 0]];
        let mut offsets = BTreeMap::new();
        assert!(!scroll_wheel(
            &document,
            &snapshot,
            &mut offsets,
            egui::pos2(150.0, 50.0),
            egui::vec2(0.0, -200.0)
        ));
        assert!(scroll_wheel(
            &document,
            &snapshot,
            &mut offsets,
            egui::pos2(20.0, 20.0),
            egui::vec2(0.0, -150.0)
        ));
        assert_eq!(offsets[&inner_key], egui::vec2(0.0, 120.0));
        assert_eq!(offsets[&root_key], egui::vec2(0.0, 30.0));
        assert!(scroll_wheel(
            &document,
            &snapshot,
            &mut offsets,
            egui::pos2(20.0, 20.0),
            egui::vec2(0.0, 150.0)
        ));
        assert_eq!(offsets[&inner_key], egui::Vec2::ZERO);
        assert_eq!(offsets[&root_key], egui::Vec2::ZERO);
    }

    #[test]
    fn facade_render_backpressures_before_first_frame() {
        use hyperchad_renderer::Renderer;
        use std::future::Future;
        let (custom, _) = flume::unbounded();
        let (resize, _) = flume::unbounded();
        let renderer = EguiRenderer::new(
            hyperchad_router::Router::new(),
            custom,
            resize,
            std::sync::Arc::new(hyperchad_router::ClientInfo::default()),
            Calc,
        );
        for _ in 0..256 {
            renderer
                .submit(hyperchad_renderer::View::default(), ViewOrigin::Navigation)
                .unwrap();
        }
        let mut pending = Box::pin(renderer.render(hyperchad_renderer::View::default()));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(pending.as_mut().poll(&mut context).is_pending());
        let mut frame = renderer.take_frame(&egui::Context::default()).unwrap();
        let _ = frame.inbox.drain(&mut frame.document, 1, |_, _| {
            Err("unexpected canvas".to_owned())
        });
        assert!(matches!(
            pending.as_mut().poll(&mut context),
            std::task::Poll::Ready(Ok(()))
        ));
    }

    #[test]
    fn native_shutdown_closes_waiters_despite_live_facade_clones() {
        use std::future::Future;
        let (custom, _) = flume::unbounded();
        let (resize, _) = flume::unbounded();
        let renderer = EguiRenderer::new(
            hyperchad_router::Router::new(),
            custom,
            resize,
            std::sync::Arc::new(hyperchad_router::ClientInfo::default()),
            Calc,
        );
        let retained = renderer.clone();
        let receiver = renderer.take_response_receiver().unwrap();
        assert!(retained.take_response_receiver().is_err());
        let frame = renderer.take_frame(&egui::Context::default()).unwrap();
        let mut waiter = Box::pin(retained.wait_for_navigation());
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        assert!(waiter.as_mut().poll(&mut context).is_pending());
        drop(frame);
        assert_eq!(
            waiter.as_mut().poll(&mut context),
            std::task::Poll::Ready(None)
        );
        assert!(receiver.is_disconnected());
    }

    #[test]
    fn metadata_delivery_is_bounded_and_shutdown_disconnects() {
        let (_, mut frame) = BackendFrame::new(Calc);
        let (sender, receiver) = flume::bounded(1);
        frame.response_sender = Some(sender);
        frame
            .response_effects
            .push(hyperchad_renderer::ResponseMetadata::default());
        let ctx = egui::Context::default();
        frame.flush_responses(&ctx);
        assert_eq!(receiver.len(), 1);
        frame
            .response_effects
            .push(hyperchad_renderer::ResponseMetadata::default());
        frame.flush_responses(&ctx);
        assert!(frame.response_sender.is_none());
        assert!(
            frame
                .diagnostics
                .iter()
                .any(|error| error.contains("fatal response delivery"))
        );
        assert!(receiver.try_recv().is_ok());
        assert_eq!(receiver.try_recv(), Err(flume::TryRecvError::Disconnected));
    }

    #[derive(Clone)]
    struct Calc;
    impl hyperchad_transformer::layout::Calc for Calc {
        fn try_calc(
            &self,
            _: &mut Container,
        ) -> Result<bool, hyperchad_transformer::layout::LayoutError> {
            Ok(false)
        }
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

    fn native_key(logical: egui::Key, physical: Option<egui::Key>) -> egui::Event {
        egui::Event::Key {
            key: logical,
            physical_key: physical,
            pressed: true,
            repeat: true,
            modifiers: egui::Modifiers {
                alt: true,
                ..egui::Modifiers::NONE
            },
        }
    }

    #[test]
    fn native_keyboard_preserves_distinct_physical_identity_without_guessing_location() {
        let keyboard = NativeKeyboardState::default();
        let key = keyboard
            .key(&native_key(egui::Key::A, Some(egui::Key::Q)), false)
            .unwrap();
        assert_eq!(key.key, "A");
        assert_eq!(key.code.as_deref(), Some("KeyQ"));
        assert!(key.repeat && key.modifiers.alt);
        assert!(
            keyboard
                .key(&native_key(egui::Key::A, None), false)
                .unwrap()
                .code
                .is_none()
        );
        for physical in [egui::Key::Num1, egui::Key::Enter, egui::Key::Slash] {
            assert!(
                keyboard
                    .key(&native_key(egui::Key::A, Some(physical)), false)
                    .unwrap()
                    .code
                    .is_none()
            );
        }
    }

    #[test]
    fn native_keyboard_composition_observes_event_order_and_persists_between_batches() {
        let mut keyboard = NativeKeyboardState::default();
        let key = native_key(egui::Key::Enter, None);
        let batch = [
            key.clone(),
            egui::Event::Ime(egui::ImeEvent::Enabled),
            key.clone(),
            egui::Event::Ime(egui::ImeEvent::Preedit("候補".into())),
            key.clone(),
        ];
        let mut states = Vec::new();
        for event in &batch {
            keyboard.observe(event);
            if let Some(key) = keyboard.key(event, false) {
                states.push(key.composing);
            }
        }
        assert_eq!(states, [false, true, true]);
        // A new frame/batch without IME updates must retain composition state.
        assert!(keyboard.key(&key, false).unwrap().composing);
        for terminal in [
            egui::Event::Ime(egui::ImeEvent::Commit("確定".into())),
            egui::Event::Ime(egui::ImeEvent::Disabled),
            egui::Event::WindowFocused(false),
        ] {
            keyboard.observe(&terminal);
            assert!(!keyboard.key(&key, false).unwrap().composing);
            keyboard.observe(&egui::Event::Ime(egui::ImeEvent::Preedit(String::new())));
            assert!(keyboard.key(&key, false).unwrap().composing);
        }
    }

    #[test]
    fn native_frame_uses_prepaint_snapshot_when_widget_consumes_ime_events() {
        struct Consuming;
        impl FramePainter for Consuming {
            fn reconcile(&mut self, _: &Document, _: &[MutationSummary]) -> Result<(), String> {
                Ok(())
            }
            fn pending_resources(&self) -> usize {
                0
            }
            fn paint_node(
                &mut self,
                ui: &mut egui::Ui,
                _: ElementStamp,
                _: &Container,
                _: &Container,
                _: &NodeGeometry<ElementKey>,
            ) -> Result<Option<NodeOutput>, String> {
                // Match egui TextEdit's right to consume IME events. The production
                // adapter must already have its independent observation snapshot.
                ui.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Ime(_)));
                });
                Ok(None)
            }
        }
        let (handle, mut frame) = BackendFrame::new(Calc);
        handle
            .submit(DocumentCommand::Apply {
                view: hyperchad_renderer::View::from(Container {
                    id: 1,
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_width: Some(60.0),
                    calculated_height: Some(60.0),
                    ..Container::default()
                }),
                origin: ViewOrigin::Navigation,
            })
            .unwrap();
        let ctx = egui::Context::default();
        let mut painter = Consuming;
        let mut input = overlap_input(None);
        input.events = vec![
            egui::Event::Ime(egui::ImeEvent::Preedit("候補".into())),
            native_key(egui::Key::Enter, None),
        ];
        let _ = ctx.run(input, |ctx| {
            frame.frame(ctx, &mut painter);
            assert!(!ctx.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
            }));
        });
        assert!(frame.keyboard.composing);
        let _ = ctx.run(overlap_input(None), |ctx| frame.frame(ctx, &mut painter));
        assert!(frame.keyboard.composing);
        let mut input = overlap_input(None);
        input.events = vec![egui::Event::Ime(egui::ImeEvent::Commit("確定".into()))];
        let _ = ctx.run(input, |ctx| frame.frame(ctx, &mut painter));
        assert!(!frame.keyboard.composing);
    }

    fn overlap_input(pressed: Option<bool>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(100.0, 100.0),
            )),
            events: pressed.map_or_else(Vec::new, |pressed| {
                vec![
                    egui::Event::PointerMoved(egui::pos2(20.0, 20.0)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(20.0, 20.0),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            }),
            ..Default::default()
        }
    }

    #[test]
    fn sticky_paint_order_matches_overlap_click_owner_after_preorder_preparation() {
        use hyperchad_transformer::models::Position;
        #[derive(Default)]
        struct Recording {
            prepared: Vec<usize>,
            painted: Vec<usize>,
        }
        impl FramePainter for Recording {
            fn reconcile(&mut self, _: &Document, _: &[MutationSummary]) -> Result<(), String> {
                self.prepared.clear();
                self.painted.clear();
                Ok(())
            }
            fn pending_resources(&self) -> usize {
                0
            }
            fn prepare_node(
                &mut self,
                _: ElementStamp,
                authored: &Container,
                _: &Container,
                _: &NodeGeometry<ElementKey>,
            ) -> Result<(), String> {
                self.prepared.push(authored.id);
                Ok(())
            }
            fn paint_node(
                &mut self,
                ui: &mut egui::Ui,
                stamp: ElementStamp,
                authored: &Container,
                _: &Container,
                geometry: &NodeGeometry<ElementKey>,
            ) -> Result<Option<NodeOutput>, String> {
                self.painted.push(authored.id);
                Ok(Some(NodeOutput {
                    response: ui.interact(
                        geometry.border_box,
                        egui::Id::new(stamp.key),
                        egui::Sense::click(),
                    ),
                    value: None,
                }))
            }
        }
        let node = |id| Container {
            id,
            calculated_x: Some(0.0),
            calculated_y: Some(0.0),
            calculated_width: Some(60.0),
            calculated_height: Some(60.0),
            ..Container::default()
        };
        let mut sticky = node(2);
        sticky.position = Some(Position::Sticky);
        sticky.top = Some(0.into());
        sticky.children.push(node(3));
        let root = Container {
            id: 1,
            children: vec![sticky, node(4)],
            ..Container::default()
        };
        let (handle, mut frame) = BackendFrame::new(Calc);
        handle
            .submit(DocumentCommand::Apply {
                view: hyperchad_renderer::View::from(root),
                origin: ViewOrigin::Navigation,
            })
            .unwrap();
        let ctx = egui::Context::default();
        let mut painter = Recording::default();
        let _ = ctx.run(overlap_input(None), |ctx| frame.frame(ctx, &mut painter));
        assert_eq!(painter.prepared, vec![1, 2, 3, 4]);
        assert_eq!(painter.painted, vec![1, 4, 2, 3]);
        let _ = ctx.run(overlap_input(Some(true)), |ctx| {
            frame.frame(ctx, &mut painter);
        });
        let _ = ctx.run(overlap_input(Some(false)), |ctx| {
            frame.frame(ctx, &mut painter);
        });
        let clicked: Vec<_> = frame
            .interactions
            .iter()
            .filter(|x| x.output.response.clicked())
            .map(|x| frame.document.get(x.stamp.key).unwrap().id)
            .collect();
        assert_eq!(clicked, vec![3]);
        let hit = frame
            .geometry
            .as_ref()
            .unwrap()
            .hit_test(egui::pos2(20.0, 20.0), |_| true)
            .unwrap();
        assert_eq!(frame.document.get(hit).unwrap().id, 3);
    }

    #[test]
    fn invalidated_projection_does_not_consume_wheel_using_old_scroll_limits() {
        use hyperchad_transformer::models::LayoutOverflow;
        let (handle, mut frame) = BackendFrame::new(Calc);
        let root = Container {
            id: 1,
            calculated_width: Some(100.0),
            calculated_height: Some(100.0),
            overflow_y: LayoutOverflow::Scroll,
            children: vec![Container {
                id: 2,
                calculated_x: Some(0.0),
                calculated_y: Some(0.0),
                calculated_width: Some(100.0),
                calculated_height: Some(400.0),
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
        let ctx = egui::Context::default();
        let mut painter = Probe;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(100.0, 100.0),
                )),
                ..Default::default()
            },
            |ctx| {
                frame.frame(ctx, &mut painter);
            },
        );
        assert!(
            frame
                .geometry
                .as_ref()
                .unwrap()
                .paint_order()
                .any(|node| node.scroll_max.y > 0.0)
        );
        frame.action_dirty = true;
        let input = || egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(egui::pos2(20.0, 20.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -30.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let _ = ctx.run(input(), |ctx| frame.frame(ctx, &mut painter));
        assert_eq!(frame.scroll, BTreeMap::new());
        let _ = ctx.run(input(), |ctx| frame.frame(ctx, &mut painter));
        assert!(frame.scroll.values().any(|offset| offset.y > 0.0));
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
            frame.frame(ctx, &mut painter);
        });
        assert!(frame.backlog);
        assert!(!frame.ready(&painter));
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            frame.frame(ctx, &mut painter);
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
