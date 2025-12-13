//! Version 1 implementation of the egui renderer.
//!
//! This module provides the first generation egui-based renderer for `HyperChad`,
//! including comprehensive UI element rendering, action handling, and viewport
//! management capabilities.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, LazyLock, Mutex, RwLock},
};

use crate::layout::EguiCalc;
use async_trait::async_trait;
use eframe::egui::{self, Color32, CursorIcon, Response, Ui, Widget};
use flume::{Receiver, Sender};
use hyperchad_actions::{
    ActionEffect, ActionTrigger, ActionType, ElementTarget, Target,
    handler::{ActionContext, StyleTrigger},
    logic::Value,
};
use hyperchad_renderer::{
    Color, Content, Handle, RenderRunner, Renderer, ToRenderRunner, View,
    canvas::{self, CanvasAction, CanvasUpdate},
    viewport::immediate::{Pos, Viewport, ViewportListener},
};
use hyperchad_router::{ClientInfo, RequestInfo, Router};
use hyperchad_transformer::{
    Container, Element, Input, ResponsiveTrigger, TableIter, float_eq,
    models::{
        Cursor, LayoutDirection, LayoutOverflow, LayoutPosition, Position, Route, SwapStrategy,
        TextOverflow, Visibility,
    },
};
use itertools::Itertools;

/// Represents a view to be rendered.
///
/// This enum wraps the different types of views that can be rendered by the egui renderer.
pub enum RenderView {
    /// A container view with its contents.
    ///
    /// Contains a `Container` with layout information and child elements to render.
    View(Container),
}

#[cfg(feature = "debug")]
static DEBUG: LazyLock<RwLock<bool>> = LazyLock::new(|| {
    RwLock::new(matches!(
        switchy_env::var("DEBUG_RENDERER").as_deref(),
        Ok("1" | "true")
    ))
});

/// Egui-based renderer for `HyperChad` applications.
///
/// Manages the rendering state and provides access to the underlying egui application.
#[derive(Clone)]
pub struct EguiRenderer<C: EguiCalc + Clone + Send + Sync> {
    width: Option<f32>,
    height: Option<f32>,
    x: Option<i32>,
    y: Option<i32>,
    app: EguiApp<C>,
    receiver: Receiver<String>,
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> EguiRenderer<C> {
    /// Creates a new `EguiRenderer` instance.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        router: Router,
        request_action: Sender<(String, Option<Value>)>,
        on_resize: Sender<(f32, f32)>,
        client_info: Arc<ClientInfo>,
        calculator: C,
    ) -> Self {
        let (tx, rx) = flume::unbounded();
        let (event_tx, event_rx) = flume::unbounded();
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            app: EguiApp::new(
                router,
                tx,
                event_tx,
                event_rx,
                &request_action,
                on_resize,
                client_info,
                calculator,
            ),
            receiver: rx,
        }
    }

    /// Waits for a navigation event and returns the navigation URL if one occurs.
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
    }
}

/// Runner for executing the egui renderer.
///
/// Handles the event loop and window management for the egui application.
pub struct EguiRenderRunner<C: EguiCalc + Clone + Send + Sync> {
    width: f32,
    height: f32,
    x: Option<i32>,
    y: Option<i32>,
    app: EguiApp<C>,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl<C: EguiCalc + Clone + Send + Sync + 'static> RenderRunner for EguiRenderRunner<C> {
    /// # Errors
    ///
    /// Will error if egui fails to run the event loop.
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        let mut viewport =
            egui::ViewportBuilder::default().with_inner_size([self.width, self.height]);

        #[allow(clippy::cast_precision_loss)]
        if let (Some(x), Some(y)) = (self.x, self.y) {
            viewport = viewport.with_position((x as f32, y as f32));
        }

        hyperchad_transformer::layout::set_scrollbar_size(0);

        #[cfg(feature = "wgpu")]
        let renderer = eframe::Renderer::Wgpu;
        #[cfg(not(feature = "wgpu"))]
        let renderer = eframe::Renderer::Glow;

        let options = eframe::NativeOptions {
            viewport,
            centered: true,
            renderer,
            ..Default::default()
        };

        #[cfg(feature = "profiling-tracing")]
        tracing_subscriber::fmt::init();
        #[cfg(feature = "profiling-puffin")]
        start_puffin_server();

        log::debug!("run: starting");
        if let Err(e) = eframe::run_native(
            self.app.title.as_deref().unwrap_or("MoosicBox"),
            options,
            Box::new(|cc| {
                // init fonts
                let _ = cc.egui_ctx.run(egui::RawInput::default(), |_| {});
                egui_extras::install_image_loaders(&cc.egui_ctx);
                *self.app.ctx.write().unwrap() = Some(cc.egui_ctx.clone());
                let mut calculator = self.app.calculator.write().unwrap();
                *calculator = calculator.clone().with_context(cc.egui_ctx.clone());
                log::debug!("run: set calculator context");
                drop(calculator);

                Ok(Box::new(self.app.clone()))
            }),
        ) {
            log::error!("run: eframe error: {e:?}");
        }
        log::debug!("run: finished");

        Ok(())
    }
}

#[cfg_attr(feature = "profiling", profiling::function)]
fn map_element_target<R>(
    target: &ElementTarget,
    self_id: usize,
    container: &Container,
    func: impl Fn(&Container) -> R,
) -> Option<R> {
    match target {
        ElementTarget::Class(class) => {
            let Target::Literal(class) = class else {
                // FIXME
                return None;
            };
            if let Some(element) = container.find_element_by_class(class) {
                return Some(func(element));
            }

            log::warn!("Could not find element with class '{class}'");
        }
        ElementTarget::ChildClass(class) => {
            let Target::Literal(class) = class else {
                // FIXME
                return None;
            };
            if let Some(container) = container.find_element_by_id(self_id)
                && let Some(element) = container.find_element_by_class(class)
            {
                return Some(func(element));
            }

            log::warn!("Could not find element with class '{class}'");
        }
        ElementTarget::Id(id) => {
            if let Some(element) = container.find_element_by_id(self_id) {
                return Some(func(element));
            }

            log::warn!("Could not find element with id '{id}'");
        }
        ElementTarget::SelfTarget => {
            if let Some(element) = container.find_element_by_id(self_id) {
                return Some(func(element));
            }

            log::warn!("Could not find element with id '{self_id}'");
        }
        ElementTarget::LastChild => {
            if let Some(element) = container
                .find_element_by_id(self_id)
                .and_then(|x| x.children.iter().last())
            {
                return Some(func(element));
            }

            log::warn!("Could not find element last child for id '{self_id}'");
        }
        ElementTarget::ById(id) => {
            let Target::Literal(id) = id else {
                // FIXME
                return None;
            };
            if let Some(element) = container.find_element_by_str_id(id) {
                return Some(func(element));
            }

            log::warn!("Could not find element with id '{id}'");
        }
        ElementTarget::Selector(selector) => {
            let Target::Literal(selector) = selector else {
                // FIXME
                return None;
            };
            // For egui, treat selector as a class lookup (best effort)
            // Strip leading '.' or '#' if present
            let selector = selector.trim_start_matches('.').trim_start_matches('#');
            if let Some(element) = container.find_element_by_class(selector) {
                return Some(func(element));
            }

            log::warn!("Could not find element with selector '{selector}'");
        }
    }

    None
}

#[allow(clippy::too_many_lines)]
fn add_watch_pos(root: &Container, container: &Container, watch_positions: &mut HashSet<usize>) {
    fn check_value(
        value: &Value,
        root: &Container,
        watch_positions: &mut HashSet<usize>,
        id: usize,
    ) {
        fn check_calc_value(
            calc: &hyperchad_actions::logic::CalcValue,
            root: &Container,
            watch_positions: &mut HashSet<usize>,
            id: usize,
        ) {
            match calc {
                hyperchad_actions::logic::CalcValue::Visibility { .. }
                | hyperchad_actions::logic::CalcValue::Display { .. }
                | hyperchad_actions::logic::CalcValue::Id { .. }
                | hyperchad_actions::logic::CalcValue::DataAttrValue { .. }
                | hyperchad_actions::logic::CalcValue::EventValue
                | hyperchad_actions::logic::CalcValue::WidthPx { .. }
                | hyperchad_actions::logic::CalcValue::HeightPx { .. }
                | hyperchad_actions::logic::CalcValue::Key { .. }
                | hyperchad_actions::logic::CalcValue::MouseX { target: None }
                | hyperchad_actions::logic::CalcValue::MouseY { target: None } => {}
                hyperchad_actions::logic::CalcValue::PositionX { target }
                | hyperchad_actions::logic::CalcValue::PositionY { target }
                | hyperchad_actions::logic::CalcValue::MouseX {
                    target: Some(target),
                }
                | hyperchad_actions::logic::CalcValue::MouseY {
                    target: Some(target),
                } => {
                    let id = match target {
                        ElementTarget::Id(id) => Some(*id),
                        ElementTarget::SelfTarget => Some(id),
                        ElementTarget::Class(..)
                        | ElementTarget::ChildClass(..)
                        | ElementTarget::LastChild
                        | ElementTarget::ById(..)
                        | ElementTarget::Selector(..) => {
                            map_element_target(target, id, root, |x| x.id)
                        }
                    };
                    log::debug!("add_watch_pos: got id={id:?} for target={target:?}");

                    if let Some(id) = id {
                        watch_positions.insert(id);
                    }
                }
            }
        }

        match value {
            Value::Calc(calc_value) => {
                check_calc_value(calc_value, root, watch_positions, id);
            }
            Value::Arithmetic(arithmetic) => {
                fn check_arithmetic(
                    arithmetic: &hyperchad_actions::logic::Arithmetic,
                    root: &Container,
                    watch_positions: &mut HashSet<usize>,
                    id: usize,
                ) {
                    match arithmetic {
                        hyperchad_actions::logic::Arithmetic::Plus(a, b)
                        | hyperchad_actions::logic::Arithmetic::Minus(a, b)
                        | hyperchad_actions::logic::Arithmetic::Multiply(a, b)
                        | hyperchad_actions::logic::Arithmetic::Divide(a, b)
                        | hyperchad_actions::logic::Arithmetic::Min(a, b)
                        | hyperchad_actions::logic::Arithmetic::Max(a, b) => {
                            check_value(a, root, watch_positions, id);
                            check_value(b, root, watch_positions, id);
                        }
                        hyperchad_actions::logic::Arithmetic::Grouping(x) => {
                            check_arithmetic(x, root, watch_positions, id);
                        }
                    }
                }

                check_arithmetic(arithmetic, root, watch_positions, id);
            }
            Value::Real(..)
            | Value::Visibility(..)
            | Value::Display(..)
            | Value::String(..)
            | Value::Key(..)
            | Value::LayoutDirection(..) => {}
        }
    }

    fn check_action(
        action: &hyperchad_actions::ActionType,
        root: &Container,
        watch_positions: &mut HashSet<usize>,
        id: usize,
    ) {
        match action {
            ActionType::Logic(logic) => {
                match &logic.condition {
                    hyperchad_actions::logic::Condition::Eq(a, b) => {
                        check_value(a, root, watch_positions, id);
                        check_value(b, root, watch_positions, id);
                    }
                    hyperchad_actions::logic::Condition::Bool(_b) => {}
                }

                for action in &logic.actions {
                    check_action(&action.action, root, watch_positions, id);
                }
                for action in &logic.else_actions {
                    check_action(&action.action, root, watch_positions, id);
                }
            }
            ActionType::NoOp
            | ActionType::Style { .. }
            | ActionType::Input { .. }
            | ActionType::Navigate { .. }
            | ActionType::Let { .. }
            | ActionType::Log { .. }
            | ActionType::Custom { .. } => {}
            ActionType::Parameterized { action, value } => {
                check_value(value, root, watch_positions, id);
                check_action(action, root, watch_positions, id);
            }
            ActionType::Event { action, .. } => {
                check_action(action, root, watch_positions, id);
            }
            ActionType::Multi(actions) => {
                for action in actions {
                    check_action(action, root, watch_positions, id);
                }
            }
            ActionType::MultiEffect(effects) => {
                for effect in effects {
                    check_action(&effect.action, root, watch_positions, id);
                }
            }
        }
    }

    for action in &container.actions {
        check_action(&action.effect.action, root, watch_positions, container.id);
    }

    for element in &container.children {
        add_watch_pos(root, element, watch_positions);
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> ToRenderRunner for EguiRenderer<C> {
    /// # Errors
    ///
    /// Will error if egui fails to run the event loop.
    ///
    /// # Panics
    ///
    /// Will panic if the `RwLock` for view transmission or render buffer is poisoned,
    /// or if width or height were not set during initialization.
    fn to_runner(
        self,
        _handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        let (view_tx, view_rx) = flume::unbounded();
        let (render_buffer_tx, render_buffer_rx) = flume::unbounded();

        self.app.view_tx.write().unwrap().replace(view_tx);
        self.app
            .render_buffer_rx
            .write()
            .unwrap()
            .replace(render_buffer_rx);

        let renderer = self.clone();

        switchy_async::runtime::Handle::current().spawn_with_name("render buffer", async move {
            while let Ok(Some(view)) = view_rx.recv_async().await {
                match view {
                    RenderView::View(x) => {
                        let _ = renderer
                            .render(View {
                                primary: Some(x),
                                fragments: vec![],
                                delete_selectors: vec![],
                            })
                            .await
                            .inspect_err(|e| log::error!("Failed to render: {e:?}"));
                    }
                }
            }
            let _ = render_buffer_tx
                .send(())
                .inspect_err(|e| log::error!("Failed to send render buffer finish: {e:?}"));
        });

        Ok(Box::new(EguiRenderRunner {
            width: self.width.unwrap(),
            height: self.height.unwrap(),
            x: self.x,
            y: self.y,
            app: self.app,
        }))
    }
}

#[async_trait]
impl<C: EguiCalc + Clone + Send + Sync + 'static> Renderer for EguiRenderer<C> {
    fn add_responsive_trigger(&mut self, _name: String, _trigger: ResponsiveTrigger) {}

    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    ///
    /// # Errors
    ///
    /// Will error if egui app fails to start
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
        _viewport: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.width = Some(width);
        self.height = Some(height);
        self.x = x;
        self.y = y;
        self.app.title = title.map(Into::into);
        self.app.description = description.map(Into::into);
        self.app.background = background.map(Into::into);

        log::debug!("start: spawning listen thread");
        switchy_async::runtime::Handle::current().spawn_with_name(
            "renderer_egui::start: listen",
            {
                let app = self.app.clone();
                async move {
                    log::debug!("start: listening");
                    app.listen().await;
                    Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
                }
            },
        );

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if egui app fails to emit the event.
    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("emit_event: event_name={event_name} event_value={event_value:?}");

        let app = self.app.clone();

        switchy_async::runtime::Handle::current()
            .spawn_blocking_with_name("handle_event", move || {
                app.handle_event(&event_name, event_value.as_deref());
            })
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + 'static>)?;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if egui fails to render the view.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render(&self, view: View) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        let Some(primary) = view.primary else {
            return Ok(());
        };

        if self.app.ctx.read().unwrap().is_none() {
            self.app
                .render_queue
                .write()
                .unwrap()
                .as_mut()
                .unwrap()
                .push_back(RenderView::View(primary));
            return Ok(());
        }

        let app = self.app.clone();
        let width = self.width;
        let height = self.height;

        switchy_async::runtime::Handle::current()
            .spawn_blocking_with_name("egui render", move || {
                moosicbox_logging::debug_or_trace!(
                    ("render: start"),
                    ("render: start {primary:?}")
                );
                let mut element = primary;

                element.calculated_width = app.width.read().unwrap().or(width);
                element.calculated_height = app.height.read().unwrap().or(height);
                log::debug!(
                    "render: calculated_width={:?} calculated_height={:?}",
                    element.calculated_width,
                    element.calculated_height
                );
                app.calculator.read().unwrap().calc(&mut element);
                moosicbox_assert::assert!(element.calculated_font_size.is_some());

                let mut watch_positions = app.watch_positions.write().unwrap();
                watch_positions.clear();
                add_watch_pos(&element, &element, &mut watch_positions);
                drop(watch_positions);

                *app.container.write().unwrap() = Some(element);
                app.images.write().unwrap().clear();
                app.viewport_listeners.write().unwrap().clear();
                app.route_requests.write().unwrap().clear();
                app.checkboxes.write().unwrap().clear();
                app.positions.write().unwrap().clear();
                app.immediate_elements_handled.write().unwrap().clear();
                // Removed: action_handler field was removed

                log::debug!("render: finished");
                if let Some(ctx) = &*app.ctx.read().unwrap() {
                    ctx.request_repaint();
                }

                Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
            })
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + 'static>)??;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if egui fails to render the canvas update.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render_canvas(
        &self,
        mut update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        let app = self.app.clone();

        switchy_async::runtime::Handle::current()
            .spawn_blocking_with_name("egui render_canvas", move || {
                log::trace!("render_canvas: start");

                let mut binding = app.canvas_actions.write().unwrap();

                let actions = binding
                    .entry(update.target)
                    .or_insert_with(|| Vec::with_capacity(update.canvas_actions.len()));

                actions.append(&mut update.canvas_actions);

                compact_canvas_actions(actions);

                drop(binding);

                if let Some(ctx) = &*app.ctx.read().unwrap() {
                    ctx.request_repaint();
                }

                log::trace!("render_canvas: end");

                Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
            })
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + 'static>)??;

        Ok(())
    }
}

fn compact_canvas_actions(actions: &mut Vec<CanvasAction>) {
    let len = actions.len();
    let mut cleared = vec![];
    for i in 0..len {
        let i = len - 1 - i;
        let Some(action) = actions.get(i) else {
            continue;
        };
        match action {
            CanvasAction::StrokeSize(..)
            | CanvasAction::StrokeColor(..)
            | CanvasAction::Line(..) => {}
            CanvasAction::Clear => {
                actions.drain(..=i);
                return;
            }
            CanvasAction::ClearRect(canvas::Pos(x1, y1), canvas::Pos(x2, y2)) => {
                cleared.push(egui::Rect::from_min_max(
                    egui::Pos2 { x: *x1, y: *y1 },
                    egui::Pos2 { x: *x2, y: *y2 },
                ));
            }
            CanvasAction::FillRect(canvas::Pos(x1, y1), canvas::Pos(x2, y2)) => {
                let rect = egui::Rect::from_min_max(
                    egui::Pos2 { x: *x1, y: *y1 },
                    egui::Pos2 { x: *x2, y: *y2 },
                );

                if cleared.iter().any(|x| x.intersects(rect)) {
                    actions.remove(i);
                }
            }
        }
    }
}

#[derive(Debug)]
enum AppEvent {
    LoadImage { source: String },
    ProcessRoute { route: Route, container_id: usize },
}

#[derive(Clone)]
enum AppImage {
    Loading,
    Bytes(Arc<[u8]>),
}

struct RenderContext<'a> {
    viewport_listeners: &'a mut HashMap<usize, ViewportListener>,
    images: &'a mut HashMap<String, AppImage>,
    canvas_actions: &'a mut HashMap<String, Vec<CanvasAction>>,
    route_requests: &'a mut Vec<usize>,
    checkboxes: &'a mut HashMap<egui::Id, bool>,
    positions: &'a mut HashMap<usize, egui::Rect>,
    watch_positions: &'a mut HashSet<usize>,
    // Shared action handler for all action processing
    action_handler: EguiActionHandler<'a>,
    // Action context for UI operations
    action_context: &'a EguiActionContext,
}

#[allow(clippy::type_complexity)]
#[derive(Clone)]
struct EguiApp<C: EguiCalc + Clone + Send + Sync> {
    ctx: Arc<RwLock<Option<egui::Context>>>,
    calculator: Arc<RwLock<C>>,
    render_queue: Arc<RwLock<Option<VecDeque<RenderView>>>>,
    view_tx: Arc<RwLock<Option<Sender<Option<RenderView>>>>>,
    render_buffer_rx: Arc<RwLock<Option<Receiver<()>>>>,
    width: Arc<RwLock<Option<f32>>>,
    height: Arc<RwLock<Option<f32>>>,
    container: Arc<RwLock<Option<Container>>>,
    sender: Sender<String>,
    event: Sender<AppEvent>,
    event_receiver: Receiver<AppEvent>,
    viewport_listeners: Arc<RwLock<HashMap<usize, ViewportListener>>>,
    images: Arc<RwLock<HashMap<String, AppImage>>>,
    canvas_actions: Arc<RwLock<HashMap<String, Vec<CanvasAction>>>>,
    route_requests: Arc<RwLock<Vec<usize>>>,
    // Removed: action_handler - now created locally as needed to avoid lifetime issues
    action_context: EguiActionContext,
    checkboxes: Arc<RwLock<HashMap<egui::Id, bool>>>,
    positions: Arc<RwLock<HashMap<usize, egui::Rect>>>,
    watch_positions: Arc<RwLock<HashSet<usize>>>,
    router: Router,
    background: Option<Color32>,
    title: Option<String>,
    description: Option<String>,
    on_resize: Sender<(f32, f32)>,
    side_effects: Arc<Mutex<VecDeque<Handler>>>,
    event_handlers: Arc<RwLock<Vec<(String, EventHandler)>>>,
    resize_handlers: Arc<RwLock<Vec<Handler>>>,
    immediate_handlers: Arc<RwLock<Vec<Handler>>>,
    immediate_elements_handled: Arc<RwLock<HashSet<usize>>>,
    client_info: Arc<ClientInfo>,
}

type Handler = Box<dyn Fn(&mut RenderContext) -> bool + Send + Sync>;
type EventHandler = Box<dyn Fn(&mut RenderContext, Option<&str>) + Send + Sync>;
type EguiActionHandler<'a> = hyperchad_actions::handler::ActionHandler<
    EguiElementFinder<'a>,
    hyperchad_actions::handler::BTreeMapStyleManager<Option<Visibility>>,
    hyperchad_actions::handler::BTreeMapStyleManager<Option<hyperchad_color::Color>>,
    hyperchad_actions::handler::BTreeMapStyleManager<bool>,
>;

/// `ActionContext` implementation for egui renderer
#[derive(Clone)]
struct EguiActionContext {
    ctx: Arc<RwLock<Option<egui::Context>>>,
    sender: Sender<String>,
    request_action: Sender<(String, Option<Value>)>,
}

impl ActionContext for EguiActionContext {
    fn request_repaint(&self) {
        if let Some(ctx) = &*self.ctx.read().unwrap() {
            ctx.request_repaint();
        }
    }

    fn navigate(&self, url: String) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.sender
            .send(url)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
    }

    fn request_custom_action(
        &self,
        action: String,
        value: Option<Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.request_action
            .send((action, value))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
    }

    fn log(&self, level: hyperchad_actions::handler::LogLevel, message: &str) {
        match level {
            hyperchad_actions::handler::LogLevel::Error => log::error!("{message}"),
            hyperchad_actions::handler::LogLevel::Warn => log::warn!("{message}"),
            hyperchad_actions::handler::LogLevel::Info => log::info!("{message}"),
            hyperchad_actions::handler::LogLevel::Debug => log::debug!("{message}"),
            hyperchad_actions::handler::LogLevel::Trace => log::trace!("{message}"),
        }
    }

    fn get_mouse_position(&self) -> Option<(f32, f32)> {
        // TODO: Implement mouse position tracking
        None
    }

    fn get_mouse_position_relative(&self, _element_id: usize) -> Option<(f32, f32)> {
        // TODO: Implement relative mouse position
        None
    }
}

/// Custom element finder for Container since we can't use the wrapper approach effectively
struct EguiElementFinder<'a> {
    container: &'a Container,
    positions: std::collections::BTreeMap<usize, (f32, f32)>,
    dimensions: std::collections::BTreeMap<usize, (f32, f32)>,
}

impl<'a> EguiElementFinder<'a> {
    const fn new(container: &'a Container) -> Self {
        Self {
            container,
            positions: std::collections::BTreeMap::new(),
            dimensions: std::collections::BTreeMap::new(),
        }
    }
}

impl hyperchad_actions::handler::ElementFinder for EguiElementFinder<'_> {
    fn find_by_str_id(&self, str_id: &str) -> Option<usize> {
        Self::find_element_by_str_id(self.container, str_id).map(|c| c.id)
    }

    fn find_by_class(&self, class: &str) -> Option<usize> {
        Self::find_element_by_class(self.container, class).map(|c| c.id)
    }

    fn find_child_by_class(&self, parent_id: usize, class: &str) -> Option<usize> {
        let parent = Self::find_element_by_id(self.container, parent_id)?;
        Self::find_element_by_class(parent, class).map(|c| c.id)
    }

    fn get_last_child(&self, parent_id: usize) -> Option<usize> {
        let parent = Self::find_element_by_id(self.container, parent_id)?;
        parent.children.last().map(|c| c.id)
    }

    fn get_str_id(&self, element_id: usize) -> Option<String> {
        Self::find_element_by_id(self.container, element_id)?
            .str_id
            .clone()
    }

    fn get_data_attr(&self, element_id: usize, key: &str) -> Option<String> {
        Self::find_element_by_id(self.container, element_id)?
            .data
            .get(key)
            .cloned()
    }

    fn get_dimensions(&self, element_id: usize) -> Option<(f32, f32)> {
        self.dimensions.get(&element_id).copied().or_else(|| {
            let element = Self::find_element_by_id(self.container, element_id)?;
            Some((
                element.calculated_width.unwrap_or(0.0),
                element.calculated_height.unwrap_or(0.0),
            ))
        })
    }

    fn get_position(&self, element_id: usize) -> Option<(f32, f32)> {
        self.positions.get(&element_id).copied().or_else(|| {
            let element = Self::find_element_by_id(self.container, element_id)?;
            Some((
                element.calculated_x.unwrap_or(0.0),
                element.calculated_y.unwrap_or(0.0),
            ))
        })
    }
}

impl EguiElementFinder<'_> {
    fn find_element_by_id(container: &Container, id: usize) -> Option<&Container> {
        if container.id == id {
            return Some(container);
        }

        for child in &container.children {
            if let Some(found) = Self::find_element_by_id(child, id) {
                return Some(found);
            }
        }
        None
    }

    fn find_element_by_str_id<'b>(container: &'b Container, str_id: &str) -> Option<&'b Container> {
        if container.str_id.as_deref() == Some(str_id) {
            return Some(container);
        }

        for child in &container.children {
            if let Some(found) = Self::find_element_by_str_id(child, str_id) {
                return Some(found);
            }
        }
        None
    }

    fn find_element_by_class<'b>(container: &'b Container, class: &str) -> Option<&'b Container> {
        if container.classes.iter().any(|c| c == class) {
            return Some(container);
        }

        for child in &container.children {
            if let Some(found) = Self::find_element_by_class(child, class) {
                return Some(found);
            }
        }
        None
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> EguiApp<C> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        router: Router,
        sender: Sender<String>,
        event: Sender<AppEvent>,
        event_receiver: Receiver<AppEvent>,
        request_action: &Sender<(String, Option<Value>)>,
        on_resize: Sender<(f32, f32)>,
        client_info: Arc<ClientInfo>,
        calculator: C,
    ) -> Self {
        let ctx = Arc::new(RwLock::new(None));
        Self {
            ctx: ctx.clone(),
            calculator: Arc::new(RwLock::new(calculator)),
            render_queue: Arc::new(RwLock::new(Some(VecDeque::new()))),
            view_tx: Arc::new(RwLock::new(None)),
            render_buffer_rx: Arc::new(RwLock::new(None)),
            width: Arc::new(RwLock::new(None)),
            height: Arc::new(RwLock::new(None)),
            container: Arc::new(RwLock::new(None)),
            sender: sender.clone(),
            event,
            event_receiver,
            viewport_listeners: Arc::new(RwLock::new(HashMap::new())),
            images: Arc::new(RwLock::new(HashMap::new())),
            canvas_actions: Arc::new(RwLock::new(HashMap::new())),
            route_requests: Arc::new(RwLock::new(vec![])),
            // Removed: action_handler initialization
            action_context: EguiActionContext {
                ctx,
                sender,
                request_action: request_action.clone(),
            },
            checkboxes: Arc::new(RwLock::new(HashMap::new())),
            positions: Arc::new(RwLock::new(HashMap::new())),
            watch_positions: Arc::new(RwLock::new(HashSet::new())),
            router,
            background: None,
            title: None,
            description: None,
            on_resize,
            side_effects: Arc::new(Mutex::new(VecDeque::new())),
            event_handlers: Arc::new(RwLock::new(vec![])),
            resize_handlers: Arc::new(RwLock::new(vec![])),
            immediate_handlers: Arc::new(RwLock::new(vec![])),
            immediate_elements_handled: Arc::new(RwLock::new(HashSet::new())),
            client_info,
        }
    }

    /// # Errors
    ///
    /// Will error if egui fails to emit the event.
    fn handle_event(&self, event_name: &str, event_value: Option<&str>) {
        log::debug!("handle_event: event_name={event_name} event_value={event_value:?}");

        let container_binding = self.container.write().unwrap();
        let Some(container) = container_binding.as_ref() else {
            return;
        };
        let mut viewport_listeners = self.viewport_listeners.write().unwrap();
        let mut images = self.images.write().unwrap();
        let mut canvas_actions = self.canvas_actions.write().unwrap();
        let mut route_requests = self.route_requests.write().unwrap();
        let mut checkboxes = self.checkboxes.write().unwrap();
        let mut positions = self.positions.write().unwrap();
        let mut watch_positions = self.watch_positions.write().unwrap();
        // Create action handler for event processing
        let element_finder = EguiElementFinder::new(container);
        let action_handler =
            hyperchad_actions::handler::utils::create_default_handler(element_finder);

        let mut render_context = RenderContext {
            viewport_listeners: &mut viewport_listeners,
            images: &mut images,
            canvas_actions: &mut canvas_actions,
            route_requests: &mut route_requests,
            checkboxes: &mut checkboxes,
            positions: &mut positions,
            watch_positions: &mut watch_positions,
            action_handler,
            action_context: &self.action_context,
        };

        let ctx = self.ctx.read().unwrap().clone();
        let binding = self.event_handlers.read().unwrap();
        for handler in binding.iter().filter_map(|(name, handler)| {
            if name == event_name {
                Some(handler)
            } else {
                None
            }
        }) {
            handler(&mut render_context, event_value);

            if let Some(ctx) = &ctx {
                ctx.request_repaint();
            }
        }

        drop(viewport_listeners);
        drop(container_binding);
        drop(binding);
    }

    #[allow(clippy::too_many_lines)]
    async fn listen(&self) {
        while let Ok(event) = self.event_receiver.recv_async().await {
            log::trace!("received event {event:?}");
            match event {
                AppEvent::LoadImage { source } => {
                    let images = self.images.clone();
                    let ctx = self.ctx.clone();
                    if let Some(file) = moosicbox_app_native_image::Asset::get(&source) {
                        log::trace!("loading image {source}");
                        images
                            .write()
                            .unwrap()
                            .insert(source, AppImage::Bytes(file.data.to_vec().into()));

                        if let Some(ctx) = &*ctx.read().unwrap() {
                            ctx.request_repaint();
                        }
                    } else {
                        switchy_async::runtime::Handle::current().spawn_with_name(
                            "renderer: load_image",
                            async move {
                                static CLIENT: LazyLock<switchy_http::Client> =
                                    LazyLock::new(switchy_http::Client::new);

                                log::trace!("loading image {source}");
                                match CLIENT.get(&source).send().await {
                                    Ok(response) => {
                                        if !response.status().is_success() {
                                            log::error!(
                                                "Failed to load image: {}",
                                                response.text().await.unwrap_or_else(|e| {
                                                    format!("(failed to get response text: {e:?})")
                                                })
                                            );
                                            return;
                                        }

                                        match response.bytes().await {
                                            Ok(bytes) => {
                                                let bytes = bytes.to_vec().into();

                                                let mut binding = images.write().unwrap();
                                                binding.insert(source, AppImage::Bytes(bytes));
                                                drop(binding);

                                                if let Some(ctx) = &*ctx.read().unwrap() {
                                                    ctx.request_repaint();
                                                }
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Failed to fetch image ({source}): {e:?}"
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Failed to fetch image ({source}): {e:?}");
                                    }
                                }
                            },
                        );
                    }
                }
                AppEvent::ProcessRoute {
                    route,
                    container_id,
                } => {
                    let container = self.container.clone();
                    {
                        if container.read().unwrap().is_none() {
                            return;
                        }
                    }
                    let router = self.router.clone();
                    let calculator = self.calculator.clone();
                    let ctx = self.ctx.clone();
                    let client = self.client_info.clone();
                    switchy_async::runtime::Handle::current().spawn_with_name(
                        "renderer: ProcessRoute",
                        async move {
                            match route {
                                Route::Get {
                                    route,
                                    trigger,
                                    target,
                                    strategy,
                                }
                                | Route::Post {
                                    route,
                                    trigger,
                                    target,
                                    strategy,
                                }
                                | Route::Put {
                                    route,
                                    trigger,
                                    target,
                                    strategy,
                                }
                                | Route::Delete {
                                    route,
                                    trigger,
                                    target,
                                    strategy,
                                }
                                | Route::Patch {
                                    route,
                                    trigger,
                                    target,
                                    strategy,
                                } => {
                                    if trigger.as_deref() == Some("load") {
                                        let info = RequestInfo { client };
                                        match router.navigate((&route, info)).await {
                                            Ok(content) => {
                                                let Some(content) = content else { return };
                                                let Some(ctx) = ctx.read().unwrap().clone() else {
                                                    moosicbox_assert::die_or_panic!(
                                                        "Context was not set"
                                                    )
                                                };
                                                #[allow(clippy::match_wildcard_for_single_variants)]
                                                match content {
                                                    Content::View(view) => {
                                                        if let Some(primary) = view.primary {
                                                            let element_target = match target {
                                                                hyperchad_transformer::models::Selector::Id(id) => {
                                                                    ElementTarget::ById(Target::Literal(id))
                                                                }
                                                                hyperchad_transformer::models::Selector::Class(class) => {
                                                                    ElementTarget::Class(Target::Literal(class))
                                                                }
                                                                hyperchad_transformer::models::Selector::ChildClass(class) => {
                                                                    ElementTarget::ChildClass(Target::Literal(class))
                                                                }
                                                                hyperchad_transformer::models::Selector::SelfTarget => {
                                                                    ElementTarget::SelfTarget
                                                                }
                                                            };
                                                            Self::swap_elements(
                                                                &element_target,
                                                                &strategy,
                                                                &ctx,
                                                                &container,
                                                                &calculator.read().unwrap(),
                                                                container_id,
                                                                primary,
                                                            );
                                                        }
                                                    }
                                                    _ => {
                                                        unimplemented!();
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Failed to process route ({route}): {e:?}"
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    );
                }
            }
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn swap_elements(
        target: &ElementTarget,
        strategy: &SwapStrategy,
        ctx: &egui::Context,
        container: &RwLock<Option<Container>>,
        calculator: &C,
        container_id: usize,
        result: Container,
    ) {
        log::debug!(
            "ProcessRoute: applying {strategy:?} to target {target:?} for container_id={container_id} with {} elements",
            result.children.len()
        );
        let mut binding = container.write().unwrap();
        let Some(page) = binding.as_mut() else {
            return;
        };

        let target_id = match target {
            ElementTarget::SelfTarget => Some(container_id),
            ElementTarget::ById(Target::Literal(id) | Target::Ref(id)) => {
                page.find_element_by_str_id(id).map(|el| el.id)
            }
            _ => {
                log::warn!("Unsupported target type for egui: {target:?}");
                None
            }
        };

        let Some(target_id) = target_id else {
            log::warn!("Unable to find target element: {target:?}");
            return;
        };

        let success = match strategy {
            SwapStrategy::This => {
                page.replace_id_with_elements_calc(calculator, result.children, target_id)
            }
            SwapStrategy::Children => {
                page.replace_id_children_with_elements_calc(calculator, result.children, target_id)
            }
            SwapStrategy::BeforeBegin => {
                log::warn!("BeforeBegin swap strategy not yet implemented for egui renderer");
                false
            }
            SwapStrategy::AfterBegin => {
                log::warn!("AfterBegin swap strategy not yet implemented for egui renderer");
                false
            }
            SwapStrategy::BeforeEnd => {
                log::warn!("BeforeEnd swap strategy not yet implemented for egui renderer");
                false
            }
            SwapStrategy::AfterEnd => {
                log::warn!("AfterEnd swap strategy not yet implemented for egui renderer");
                false
            }
            SwapStrategy::Delete => {
                log::warn!("Delete swap strategy not yet implemented for egui renderer");
                false
            }
            SwapStrategy::None => true,
        };

        if success {
            drop(binding);
            ctx.request_repaint();
        } else {
            log::warn!("Unable to apply swap strategy {strategy:?} to element with id {target_id}");
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn update_frame_size(&self, width: f32, height: f32) {
        *self.viewport_listeners.write().unwrap() = HashMap::new();

        log::debug!(
            "calc: frame size changed from ({:?}, {:?}) -> ({width}, {height})",
            self.width.read().unwrap(),
            self.height.read().unwrap()
        );

        {
            let mut binding = self.container.write().unwrap();
            if let Some(container) = binding.as_mut() {
                container.calculated_width.replace(width);
                container.calculated_height.replace(height);
                self.calculator.read().unwrap().calc(container);
                drop(binding);
            }
        }

        self.width.write().unwrap().replace(width);
        self.height.write().unwrap().replace(height);
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn check_frame_resize(&self, ctx: &egui::Context) -> bool {
        let (width, height) = ctx.input(move |i| {
            let content_rect = i.content_rect();
            (content_rect.width(), content_rect.height())
        });

        let current_width = *self.width.read().unwrap();
        let current_height = *self.height.read().unwrap();
        if current_width.is_none_or(|x| (x - width).abs() >= 0.01)
            || current_height.is_none_or(|x| (x - height).abs() >= 0.01)
        {
            self.update_frame_size(width, height);
            if let Err(e) = self.on_resize.send((width, height)) {
                moosicbox_assert::die_or_error!(
                    "Failed to send on_resize message: {width}, {height}: {e:?}"
                );
            }
            true
        } else {
            false
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn get_scroll_container(
        rect: egui::Rect,
        pos_x: f32,
        pos_y: f32,
        element: &Container,
        parent: Option<&Viewport>,
    ) -> Viewport {
        let viewport = Viewport {
            parent: parent.cloned().map(Box::new),
            pos: Pos {
                x: pos_x,
                y: pos_y,
                w: element.calculated_width.unwrap(),
                h: element.calculated_height.unwrap(),
            },
            viewport: Pos {
                x: rect.min.x,
                y: rect.min.y,
                w: element.calculated_width.unwrap(),
                h: element.calculated_height.unwrap(),
            },
        };

        log::trace!(
            "get_scroll_container: ({}, {})",
            viewport.pos.x,
            viewport.pos.y
        );

        viewport
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_horizontal_borders(
        ui: &mut Ui,
        container: &Container,
        add_contents: impl FnOnce(&mut Ui) -> Response,
    ) -> Response {
        ui.horizontal(|ui| {
            if let Some((color, size)) = container.calculated_border_left {
                egui::Frame::new().fill(color.into()).show(ui, |ui| {
                    ui.set_width(size);
                    ui.set_height(container.calculated_height.unwrap_or(0.0));
                });
            }

            let response = add_contents(ui);

            if let Some((color, size)) = container.calculated_border_right {
                egui::Frame::new().fill(color.into()).show(ui, |ui| {
                    ui.set_width(size);
                    ui.set_height(container.calculated_height.unwrap_or(0.0));
                });
            }

            response
        })
        .inner
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_vertical_borders(
        ui: &mut Ui,
        container: &Container,
        add_contents: impl FnOnce(&mut Ui) -> Response,
    ) -> Response {
        ui.vertical(|ui| {
            if let Some((color, size)) = container.calculated_border_top {
                egui::Frame::new().fill(color.into()).show(ui, |ui| {
                    ui.set_width(container.calculated_width.unwrap_or(0.0));
                    ui.set_height(size);
                });
            }

            let response = add_contents(ui);

            if let Some((color, size)) = container.calculated_border_bottom {
                egui::Frame::new().fill(color.into()).show(ui, |ui| {
                    ui.set_width(container.calculated_width.unwrap_or(0.0));
                    ui.set_height(size);
                });
            }

            response
        })
        .inner
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_borders(
        ui: &mut Ui,
        container: &Container,
        add_contents: impl FnOnce(&mut Ui) -> Response,
    ) -> Response {
        if container.calculated_border_left.is_some() || container.calculated_border_right.is_some()
        {
            Self::render_horizontal_borders(ui, container, |ui| {
                if container.calculated_border_top.is_some()
                    || container.calculated_border_bottom.is_some()
                {
                    Self::render_vertical_borders(ui, container, add_contents)
                } else {
                    add_contents(ui)
                }
            })
        } else if container.calculated_border_top.is_some()
            || container.calculated_border_bottom.is_some()
        {
            Self::render_vertical_borders(ui, container, add_contents)
        } else {
            add_contents(ui)
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn container_hidden(render_context: &mut RenderContext, container: &Container) -> bool {
        // Check visibility override via action handler
        let visibility = render_context
            .action_handler
            .get_visibility_override(container.id)
            .copied()
            .unwrap_or(container.visibility)
            .unwrap_or_default();

        visibility == Visibility::Hidden
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_arguments)]
    fn handle_scroll_child_out_of_view(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        container: &Container,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &Container)>,
    ) -> bool {
        if container.is_hidden() || Self::container_hidden(render_context, container) {
            return true;
        }

        if let Some(rect) = rect {
            let render_rect =
                Self::get_render_rect(render_context, ui, container, relative_container);
            let width = render_rect.width()
                + container.padding_x().unwrap_or(0.0)
                + container.margin_x().unwrap_or(0.0);
            let height = render_rect.height()
                + container.padding_y().unwrap_or(0.0)
                + container.margin_x().unwrap_or(0.0);
            let (offset_x, offset_y) =
                viewport.map_or((0.0, 0.0), |viewport| (viewport.pos.x, viewport.pos.y));

            if render_rect.min.x + width - offset_x < -1.0
                || render_rect.min.y + height - offset_y < -1.0
                || render_rect.min.x - offset_x >= rect.width() + 1.0
                || render_rect.min.y - offset_y >= rect.height() + 1.0
            {
                log::trace!(
                    "render_container: skipping ({}, {}, {width}, {height})",
                    render_rect.min.x,
                    render_rect.min.y
                );
                self.handle_container_side_effects(
                    render_context,
                    ctx,
                    Some(ui),
                    container,
                    viewport,
                    Some(rect),
                    None,
                    true,
                );
                ui.allocate_space(egui::vec2(width, height));
                return true;
            }
            log::trace!(
                "render_container: showing ({}, {}, {width}, {height})",
                render_rect.min.x,
                render_rect.min.y
            );
        }

        false
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn render_container(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        container: &Container,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &Container)>,
    ) -> Option<Response> {
        if container.debug == Some(true) {
            log::info!("render_container: DEBUG\n{container}");
        }

        if container.is_hidden() || Self::container_hidden(render_context, container) {
            log::trace!("render_container: container is hidden. skipping render");
            self.handle_container_side_effects(
                render_context,
                ctx,
                Some(ui),
                container,
                viewport,
                rect,
                None,
                true,
            );
            return None;
        }

        Self::set_font_size(container, ctx);

        if let Some(opacity) = container.calculated_opacity {
            ui.set_opacity(opacity);
        }

        Some(Self::render_borders(ui, container, |ui| {
            #[allow(clippy::cast_possible_truncation)]
            let (
                render_context,
                response,
            ) = egui::Frame::new()
                .show(ui, {
                    move |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                            egui::Frame::new().show(ui, {
                                move |ui| {
                                    let cursor = ui.cursor();
                                    let (pos_x, pos_y) = (cursor.left(), cursor.top());
                                    match (container.overflow_x, container.overflow_y) {
                                        (
                                            LayoutOverflow::Auto,
                                            LayoutOverflow::Auto,
                                        ) => {
                                            egui::ScrollArea::both()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let response = self.render_container_contents(
                                                            render_context,
                                                            ctx,
                                                            ui,
                                                            container,
                                                            viewport,
                                                            Some(rect),
                                                            relative_container,
                                                            true,
                                                        );

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (
                                            LayoutOverflow::Scroll,
                                            LayoutOverflow::Scroll,
                                        ) => {
                                            egui::ScrollArea::both()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let response = self.render_container_contents(
                                                            render_context,
                                                            ctx,
                                                            ui,
                                                            container,
                                                            viewport,
                                                            Some(rect),
                                                            relative_container,
                                                            true,
                                                        );

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (
                                            LayoutOverflow::Auto,
                                            LayoutOverflow::Scroll,
                                        ) => {
                                            egui::ScrollArea::vertical()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let cursor = ui.cursor();
                                                        let (pos_x, pos_y) = (cursor.left(), cursor.top());
                                                        let (render_context, response) = egui::ScrollArea::horizontal()
                                                            .scroll_bar_visibility(
                                                                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                                            )
                                                            .show_viewport(ui, {
                                                                move |ui, rect| {
                                                                    let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                                    let viewport = Some(&viewport);
                                                                    let response = self.render_container_contents(
                                                                        render_context,
                                                                        ctx,
                                                                        ui,
                                                                        container,
                                                                        viewport,
                                                                        Some(rect),
                                                                        relative_container,
                                                                        true,
                                                                    );

                                                                    (render_context, response)
                                                                }
                                                            }).inner;

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (
                                            LayoutOverflow::Scroll,
                                            LayoutOverflow::Auto,
                                        ) => {
                                            egui::ScrollArea::vertical()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let cursor = ui.cursor();
                                                        let (pos_x, pos_y) = (cursor.left(), cursor.top());
                                                        let (render_context, response) = egui::ScrollArea::horizontal()
                                                            .scroll_bar_visibility(
                                                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                                            )
                                                            .show_viewport(ui, {
                                                                move |ui, rect| {
                                                                    let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                                    let viewport = Some(&viewport);
                                                                    let response = self.render_container_contents(
                                                                        render_context,
                                                                        ctx,
                                                                        ui,
                                                                        container,
                                                                        viewport,
                                                                        Some(rect),
                                                                        relative_container,
                                                                        true,
                                                                    );

                                                                    (render_context, response)
                                                                }
                                                            }).inner;

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (LayoutOverflow::Auto, _) => {
                                            egui::ScrollArea::horizontal()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let response = self.render_container_contents(
                                                            render_context,
                                                            ctx,
                                                            ui,
                                                            container,
                                                            viewport,
                                                            Some(rect),
                                                            relative_container,
                                                            false,
                                                        );

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (LayoutOverflow::Scroll, _) => {
                                            egui::ScrollArea::horizontal()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let response = self.render_container_contents(
                                                            render_context,
                                                            ctx,
                                                            ui,
                                                            container,
                                                            viewport,
                                                            Some(rect),
                                                            relative_container,
                                                            false,
                                                        );

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (_, LayoutOverflow::Auto) => {
                                            egui::ScrollArea::vertical()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let response = self.render_container_contents(
                                                            render_context,
                                                            ctx,
                                                            ui,
                                                            container,
                                                            viewport,
                                                            Some(rect),
                                                            relative_container,
                                                            true,
                                                        );

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (_, LayoutOverflow::Scroll) => {
                                            egui::ScrollArea::vertical()
                                                .scroll_bar_visibility(
                                                    egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                                )
                                                .show_viewport(ui, {
                                                    move |ui, rect| {
                                                        let viewport = Self::get_scroll_container(rect, pos_x, pos_y, container, viewport);
                                                        let viewport = Some(&viewport);
                                                        let response = self.render_container_contents(
                                                            render_context,
                                                            ctx,
                                                            ui,
                                                            container,
                                                            viewport,
                                                            Some(rect),
                                                            relative_container,
                                                            true,
                                                        );

                                                        (render_context, response)
                                                    }
                                                }).inner
                                        }
                                        (_, _) => {
                                            let response = self.render_container_contents(
                                                render_context,
                                                ctx,
                                                ui,
                                                container,
                                                viewport,
                                                rect,
                                                relative_container,
                                                false,
                                            );

                                            (render_context, response)
                                        }
                                    }
                                }
                            })
                        }).inner.inner
                    }
                }).inner;

            ui.set_opacity(1.0);

            if !Self::container_hidden(render_context, container) {
                self.handle_container_side_effects(
                    render_context,
                    ctx,
                    Some(ui),
                    container,
                    viewport,
                    rect,
                    Some(&response),
                    false,
                );
            }

            response
        }))
    }

    fn get_relative_render_rect(
        render_context: &mut RenderContext,
        ui: &Ui,
        container: &Container,
    ) -> egui::Rect {
        moosicbox_assert::assert_or_panic!(
            container.calculated_width.is_some() && container.calculated_height.is_some(),
            "Container size not properly calculated: {container}"
        );

        let rect = egui::Rect::from_min_size(
            ui.cursor().left_top(),
            egui::vec2(
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
        );

        if render_context.watch_positions.contains(&container.id) {
            render_context.positions.insert(container.id, rect);
        }

        rect
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn get_render_rect(
        render_context: &mut RenderContext,
        ui: &Ui,
        container: &Container,
        relative_container: Option<(egui::Rect, &Container)>,
    ) -> egui::Rect {
        match container.position {
            Some(Position::Absolute) => {
                if let Some((relative_rect, ..)) = relative_container {
                    let rect = relative_rect
                        .with_min_x(relative_rect.min.x + container.calculated_x.unwrap())
                        .with_min_y(relative_rect.min.y + container.calculated_y.unwrap())
                        .with_max_x(
                            relative_rect.min.x
                                + container.calculated_x.unwrap()
                                + container.bounding_calculated_width().unwrap(),
                        )
                        .with_max_y(
                            relative_rect.min.y
                                + container.calculated_y.unwrap()
                                + container.bounding_calculated_height().unwrap(),
                        );

                    if render_context.watch_positions.contains(&container.id) {
                        render_context.positions.insert(container.id, rect);
                    }

                    rect
                } else {
                    Self::get_relative_render_rect(render_context, ui, container)
                }
            }
            Some(Position::Fixed) => {
                let (x, y) = (get_left_offset(container), get_top_offset(container));
                let (x, y) = (x.unwrap_or_default(), y.unwrap_or_default());
                let rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(
                        x + get_remaining_offset_width(container),
                        y + get_remaining_offset_height(container),
                    ),
                );

                if render_context.watch_positions.contains(&container.id) {
                    render_context.positions.insert(container.id, rect);
                }

                rect
            }
            Some(Position::Static | Position::Relative | Position::Sticky) | None => {
                Self::get_relative_render_rect(render_context, ui, container)
            }
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_position<'a>(
        render_context: &mut RenderContext,
        ui: &mut Ui,
        ctx: &egui::Context,
        container: &'a Container,
        mut relative_container: Option<(egui::Rect, &'a Container)>,
        inner: impl FnOnce(&mut RenderContext, &mut Ui, Option<(egui::Rect, &'a Container)>) -> Response,
    ) -> Response {
        match container.position {
            Some(Position::Relative | Position::Sticky) => {
                let pos = ui.cursor().left_top();
                let size = egui::vec2(
                    container.calculated_width.unwrap(),
                    container.calculated_height.unwrap(),
                );
                relative_container = Some((egui::Rect::from_min_size(pos, size), container));
            }
            Some(Position::Absolute | Position::Fixed) => {
                let abs_rect =
                    Self::get_render_rect(render_context, ui, container, relative_container);
                relative_container = Some((abs_rect, container));

                let id = ui.next_auto_id();

                return egui::Area::new(id)
                    .movable(false)
                    .kind(egui::UiKind::Frame)
                    .interactable(false)
                    .fixed_pos(abs_rect.min)
                    .show(ctx, |ui| {
                        if let Some(opacity) = container.calculated_opacity {
                            ui.set_opacity(opacity);
                        }
                        inner(render_context, ui, relative_container)
                    })
                    .inner;
            }
            Some(Position::Static) | None => {}
        }

        inner(render_context, ui, relative_container)
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn render_direction<'a>(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        container: &'a Container,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &'a Container)>,
        vscroll: bool,
    ) -> Response {
        for element in container.children.iter().filter(|x| x.is_hidden()) {
            self.handle_container_side_effects(
                render_context,
                ctx,
                None,
                element,
                viewport,
                rect,
                None,
                true,
            );
        }

        match container.direction {
            LayoutDirection::Row => {
                let rows = container
                    .children
                    .iter()
                    .filter_map(|x| x.calculated_position.as_ref().map(|y| (x, y)))
                    .filter_map({
                        |(x, y)| match y {
                            LayoutPosition::Wrap { row, .. } => Some((*row, x)),
                            LayoutPosition::Default => None,
                        }
                    })
                    .chunk_by(|(row, _element)| *row);

                let mut rows = rows
                    .into_iter()
                    .map(|(_row, y)| y.into_iter().map(|(_, element)| element).collect_vec())
                    .peekable();

                if rows.peek().is_some() {
                    ui.vertical(move |ui| {
                        for row in rows {
                            let render_context = &mut *render_context;
                            ui.horizontal(move |ui| {
                                self.render_elements_ref(
                                    render_context,
                                    ctx,
                                    ui,
                                    &row,
                                    viewport,
                                    rect,
                                    relative_container,
                                    !vscroll && rect.is_some(),
                                );
                            });
                        }
                    })
                    .response
                } else {
                    ui.horizontal(move |ui| {
                        self.render_elements(
                            render_context,
                            ctx,
                            ui,
                            &container.children,
                            viewport,
                            rect,
                            relative_container,
                            !vscroll && rect.is_some(),
                        );
                    })
                    .response
                }
            }
            LayoutDirection::Column => {
                let cols = container
                    .children
                    .iter()
                    .filter_map(|x| x.calculated_position.as_ref().map(|y| (x, y)))
                    .filter_map(|(x, y)| match y {
                        LayoutPosition::Wrap { col, .. } => Some((*col, x)),
                        LayoutPosition::Default => None,
                    })
                    .chunk_by(|(col, _element)| *col);

                let mut cols = cols
                    .into_iter()
                    .map(|(_row, y)| y.into_iter().map(|(_, element)| element).collect_vec())
                    .peekable();

                if cols.peek().is_some() {
                    ui.horizontal(move |ui| {
                        for col in cols {
                            let render_context = &mut *render_context;
                            ui.vertical(move |ui| {
                                self.render_elements_ref(
                                    render_context,
                                    ctx,
                                    ui,
                                    &col,
                                    viewport,
                                    rect,
                                    relative_container,
                                    !vscroll && rect.is_some(),
                                );
                            });
                        }
                    })
                    .response
                } else {
                    ui.vertical(move |ui| {
                        self.render_elements(
                            render_context,
                            ctx,
                            ui,
                            &container.children,
                            viewport,
                            rect,
                            relative_container,
                            !vscroll && rect.is_some(),
                        );
                    })
                    .response
                }
            }
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_offset<'a, R>(
        ui: &mut Ui,
        container: &'a Container,
        relative_container: Option<(egui::Rect, &'a Container)>,
        inner: impl FnOnce(&mut Ui, Option<(egui::Rect, &'a Container)>) -> R,
    ) -> R {
        let (offset_x, offset_y) = (get_left_offset(container), get_top_offset(container));

        if offset_x.is_some() || offset_y.is_some() {
            let (offset_x, offset_y) = (offset_x.unwrap_or_default(), offset_y.unwrap_or_default());
            let (x, y) = (ui.cursor().left() + offset_x, ui.cursor().top() + offset_y);
            let rect = egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(
                    get_remaining_offset_width(container),
                    get_remaining_offset_height(container),
                ),
            );

            return ui
                .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    inner(ui, relative_container)
                })
                .inner;
        }

        inner(ui, relative_container)
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn render_container_contents<'a>(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        container: &'a Container,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &'a Container)>,
        vscroll: bool,
    ) -> Response {
        Self::render_position(
            render_context,
            ui,
            ctx,
            container,
            relative_container,
            |render_context, ui, relative_container| {
                #[allow(clippy::cast_possible_truncation)]
                let mut frame = egui::Frame::new().inner_margin(egui::Margin {
                    left: container
                        .calculated_padding_left
                        .map_or(0, |x| x.round() as i8),
                    right: container
                        .calculated_padding_right
                        .map_or(0, |x| x.round() as i8),
                    top: container
                        .calculated_padding_top
                        .map_or(0, |x| x.round() as i8),
                    bottom: container
                        .calculated_padding_bottom
                        .map_or(0, |x| x.round() as i8),
                });

                if let Some(background) = render_context
                    .action_handler
                    .get_background_override(container.id)
                    .copied()
                    .unwrap_or(container.background)
                {
                    frame = frame.fill(background.into());
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                if container.calculated_border_top_left_radius.is_some()
                    || container.calculated_border_top_right_radius.is_some()
                    || container.calculated_border_bottom_left_radius.is_some()
                    || container.calculated_border_bottom_right_radius.is_some()
                {
                    frame = frame.corner_radius(egui::CornerRadius {
                        nw: container
                            .calculated_border_top_left_radius
                            .map_or(0, |x| x.round() as u8),
                        ne: container
                            .calculated_border_top_right_radius
                            .map_or(0, |x| x.round() as u8),
                        sw: container
                            .calculated_border_bottom_left_radius
                            .map_or(0, |x| x.round() as u8),
                        se: container
                            .calculated_border_bottom_right_radius
                            .map_or(0, |x| x.round() as u8),
                    });
                }

                frame
                    .show(ui, {
                        |ui| {
                            let width = container.calculated_width.unwrap();
                            let height = container.calculated_height.unwrap();

                            moosicbox_assert::assert_or_panic!(
                                width >= 0.0,
                                "Width must be >= 0.0. Got {width} for container:\n{container}\n{container:?}"
                            );
                            moosicbox_assert::assert_or_panic!(
                                height >= 0.0,
                                "Height must be >= 0.0. Got {height} for container:\n{container}\n{container:?}"
                            );
                            ui.set_width(width);
                            ui.set_height(height);

                            #[cfg(feature = "debug")]
                            let debug = *DEBUG.read().unwrap();

                            #[cfg(feature = "debug")]
                            let pos = if debug {
                                Some(ui.cursor().left_top())
                            } else {
                                None
                            };

                            if vscroll {
                                if ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
                                    let rect = egui::Rect::from_pos(egui::emath::pos2(0.0, height));
                                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                                }
                                if ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
                                    let rect =
                                        egui::Rect::from_pos(egui::emath::pos2(0.0, -height));
                                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                                }
                            }

                            let response = self.render_direction(
                                render_context,
                                ctx,
                                ui,
                                container,
                                viewport,
                                rect,
                                relative_container,
                                vscroll,
                            );

                            #[cfg(feature = "debug")]
                            if let Some(pos) = pos {
                                let painter = ui.painter();
                                let text = format!("({}, {}, {width}, {height})", pos.x, pos.y);
                                let galley = painter.layout_no_wrap(
                                    text.clone(),
                                    egui::FontId::default(),
                                    Color32::WHITE,
                                );
                                let rect = egui::Align2::LEFT_TOP.anchor_size(pos, galley.size());
                                painter.add(egui::Shape::rect_filled(rect, 0.0, Color32::WHITE));
                                ui.painter().text(
                                    pos,
                                    egui::Align2::LEFT_TOP,
                                    text,
                                    egui::FontId::default(),
                                    Color32::RED,
                                );

                                if container.calculated_padding_left.is_some()
                                    || container.calculated_padding_right.is_some()
                                    || container.calculated_padding_top.is_some()
                                    || container.calculated_padding_bottom.is_some()
                                {
                                    let text = format!(
                                        "p({}, {}, {}, {})",
                                        container.calculated_padding_left.unwrap_or(0.0),
                                        container.calculated_padding_right.unwrap_or(0.0),
                                        container.calculated_padding_top.unwrap_or(0.0),
                                        container.calculated_padding_bottom.unwrap_or(0.0),
                                    );
                                    let galley = painter.layout_no_wrap(
                                        text.clone(),
                                        egui::FontId::default(),
                                        Color32::WHITE,
                                    );
                                    let rect =
                                        egui::Align2::LEFT_TOP.anchor_size(pos, galley.size());
                                    painter.add(egui::Shape::rect_filled(
                                        rect,
                                        0.0,
                                        Color32::WHITE,
                                    ));
                                    ui.painter().text(
                                        pos,
                                        egui::Align2::LEFT_TOP,
                                        text,
                                        egui::FontId::default(),
                                        Color32::RED,
                                    );
                                }

                                if container.calculated_margin_left.is_some()
                                    || container.calculated_margin_right.is_some()
                                    || container.calculated_margin_top.is_some()
                                    || container.calculated_margin_bottom.is_some()
                                {
                                    let text = format!(
                                        "m({}, {}, {}, {})",
                                        container.calculated_margin_left.unwrap_or(0.0),
                                        container.calculated_margin_right.unwrap_or(0.0),
                                        container.calculated_margin_top.unwrap_or(0.0),
                                        container.calculated_margin_bottom.unwrap_or(0.0),
                                    );
                                    let galley = painter.layout_no_wrap(
                                        text.clone(),
                                        egui::FontId::default(),
                                        Color32::WHITE,
                                    );
                                    let rect =
                                        egui::Align2::LEFT_TOP.anchor_size(pos, galley.size());
                                    painter.add(egui::Shape::rect_filled(
                                        rect,
                                        0.0,
                                        Color32::WHITE,
                                    ));
                                    ui.painter().text(
                                        pos,
                                        egui::Align2::LEFT_TOP,
                                        text,
                                        egui::FontId::default(),
                                        Color32::RED,
                                    );
                                }
                            }

                            response
                        }
                    })
                    .response
            },
        )
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_arguments)]
    fn render_elements(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        elements: &[Container],
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &Container)>,
        scroll_child: bool,
    ) {
        log::trace!("render_elements: {} elements", elements.len());
        for element in elements {
            Self::render_offset(ui, element, relative_container, |ui, relative_container| {
                self.render_element(
                    render_context,
                    ctx,
                    ui,
                    element,
                    viewport,
                    rect,
                    relative_container,
                    scroll_child,
                );
            });
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_arguments)]
    fn render_elements_ref(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        elements: &[&Container],
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &Container)>,
        scroll_child: bool,
    ) {
        log::trace!("render_elements_ref: {} elements", elements.len());
        for element in elements {
            Self::render_offset(ui, element, relative_container, |ui, relative_container| {
                self.render_element(
                    render_context,
                    ctx,
                    ui,
                    element,
                    viewport,
                    rect,
                    relative_container,
                    scroll_child,
                );
            });
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn rect_contains_mouse(
        pointer: &egui::PointerState,
        rect: egui::Rect,
        viewport: Option<egui::Rect>,
    ) -> bool {
        pointer.latest_pos().is_some_and(|pos| {
            if viewport.is_some_and(|vp| !vp.contains(pos)) {
                return false;
            }
            rect.contains(pos)
        })
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(
        clippy::too_many_lines,
        clippy::too_many_arguments,
        clippy::cognitive_complexity
    )]
    fn handle_container_side_effects(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: Option<&Ui>,
        container: &Container,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        response: Option<&Response>,
        recurse: bool,
    ) {
        self.handle_element_side_effects(ctx, &container.element, viewport, rect, response);

        if let Some(route) = &container.route {
            #[cfg(feature = "profiling")]
            profiling::scope!("route side effects");
            let processed_route = { render_context.route_requests.contains(&container.id) };
            if !processed_route {
                log::debug!(
                    "processing route route={route:?} container_id={}",
                    container.id
                );
                render_context.route_requests.push(container.id);
                if let Err(e) = self.event.send(AppEvent::ProcessRoute {
                    route: route.to_owned(),
                    container_id: container.id,
                }) {
                    log::error!("Failed to send ProcessRoute event: {e:?}");
                }
            }
        }

        if let Some(response) = response {
            self.handle_ui_event_side_effects(
                container,
                ui,
                ctx,
                viewport,
                rect,
                vec![response.clone()],
            );
        }

        self.handle_custom_event_side_effects(container);

        if let Some(ui) = ui
            && let Element::Image {
                source: Some(source),
                ..
            } = &container.element
        {
            #[cfg(feature = "profiling")]
            profiling::scope!("image side effects");
            let pos = ui.cursor().left_top();
            let listener = render_context
                .viewport_listeners
                .entry(container.id)
                .or_insert_with(|| {
                    ViewportListener::new(
                        viewport.cloned(),
                        0.0,
                        0.0,
                        container.calculated_width.unwrap(),
                        container.calculated_height.unwrap(),
                    )
                });
            listener.viewport = viewport.cloned();
            listener.pos.x = pos.x + viewport.map_or(0.0, |x| x.viewport.x);
            listener.pos.y = pos.y + viewport.map_or(0.0, |x| x.viewport.y);

            let (_, (dist, prev_dist)) = listener.check();

            if prev_dist.is_none_or(|x| x >= 2000.0) && dist < 2000.0 {
                let contains_image =
                    { matches!(render_context.images.get(source), Some(AppImage::Bytes(_))) };
                if !contains_image {
                    let loading_image =
                        { matches!(render_context.images.get(source), Some(AppImage::Loading)) };

                    if !loading_image {
                        log::debug!(
                            "render_element: triggering LoadImage for source={source} ({}, {})",
                            listener.pos.x,
                            listener.pos.y
                        );
                        render_context
                            .images
                            .insert(source.clone(), AppImage::Loading);

                        if let Err(e) = self.event.send(AppEvent::LoadImage {
                            source: source.clone(),
                        }) {
                            log::error!("Failed to send LoadImage event: {e:?}");
                        }
                    }
                }
            }
        }

        if recurse {
            for container in &container.children {
                self.handle_container_side_effects(
                    render_context,
                    ctx,
                    ui,
                    container,
                    viewport,
                    rect,
                    response,
                    recurse,
                );
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_ui_event_side_effects(
        &self,
        container: &Container,
        ui: Option<&Ui>,
        ctx: &egui::Context,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        responses: Vec<Response>,
    ) {
        let responses = Arc::new(responses);
        let viewport_rect = rect.map(|rect| {
            let (offset_x, offset_y) =
                viewport.map_or((0.0, 0.0), |viewport| (viewport.pos.x, viewport.pos.y));
            egui::Rect::from_min_size(egui::pos2(offset_x, offset_y), rect.size())
        });

        if let Some(cursor) = container.cursor {
            #[cfg(feature = "profiling")]
            profiling::scope!("cursor side effects");
            let ctx = ctx.clone();
            let pointer = ctx.input(|x| x.pointer.clone());
            let responses = responses.clone();
            self.trigger_side_effect(move |_render_context| {
                if responses
                    .iter()
                    .any(|r| Self::rect_contains_mouse(&pointer, r.rect, viewport_rect))
                {
                    ctx.output_mut(|x| {
                        x.cursor_icon = cursor_to_cursor_icon(cursor);
                    });
                }

                true
            });
        }

        if container.is_visible() {
            for fx_action in &container.actions {
                match fx_action.trigger {
                    ActionTrigger::Click | ActionTrigger::ClickOutside => {
                        #[cfg(feature = "profiling")]
                        profiling::scope!("click/clickOutside side effects");
                        let inside = matches!(fx_action.trigger, ActionTrigger::Click);
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let pointer = ctx.input(|x| x.pointer.clone());
                        let responses = responses.clone();
                        self.trigger_side_effect(move |render_context| {
                            if responses
                                .iter()
                                .any(|r| Self::rect_contains_mouse(&pointer, r.rect, viewport_rect))
                                == inside
                                && pointer.primary_released()
                            {
                                log::trace!("click action: {action}");
                                Self::handle_action(
                                    &action.action,
                                    Some(&action),
                                    StyleTrigger::UiEvent,
                                    render_context,
                                    id,
                                    None,
                                    None,
                                );
                                return !inside;
                            }

                            Self::unhandle_action(
                                &action.action,
                                StyleTrigger::UiEvent,
                                render_context,
                                id,
                            );

                            true
                        });
                    }
                    ActionTrigger::MouseDown => {
                        #[cfg(feature = "profiling")]
                        profiling::scope!("mouse down side effects");
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let pointer = ctx.input(|x| x.pointer.clone());
                        let responses = responses.clone();
                        self.trigger_side_effect(move |render_context| {
                            if responses
                                .iter()
                                .any(|r| Self::rect_contains_mouse(&pointer, r.rect, viewport_rect))
                                && pointer.primary_down()
                            {
                                log::trace!("mouse down action: {action}");
                                Self::handle_action(
                                    &action.action,
                                    Some(&action),
                                    StyleTrigger::UiEvent,
                                    render_context,
                                    id,
                                    None,
                                    None,
                                );
                                return false;
                            }

                            Self::unhandle_action(
                                &action.action,
                                StyleTrigger::UiEvent,
                                render_context,
                                id,
                            );

                            true
                        });
                    }
                    ActionTrigger::KeyDown => {
                        todo!()
                    }
                    ActionTrigger::Hover => {
                        #[cfg(feature = "profiling")]
                        profiling::scope!("hover side effects");
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let responses = responses.clone();
                        let pointer = ctx.input(|x| x.pointer.clone());
                        self.trigger_side_effect(move |render_context| {
                            if responses
                                .iter()
                                .any(|r| Self::rect_contains_mouse(&pointer, r.rect, viewport_rect))
                            {
                                log::trace!("hover action: {action}");
                                return Self::handle_action(
                                    &action.action,
                                    Some(&action),
                                    StyleTrigger::UiEvent,
                                    render_context,
                                    id,
                                    None,
                                    None,
                                );
                            }

                            Self::unhandle_action(
                                &action.action,
                                StyleTrigger::UiEvent,
                                render_context,
                                id,
                            );

                            true
                        });
                    }
                    ActionTrigger::Change => {
                        #[cfg(feature = "profiling")]
                        profiling::scope!("change side effects");
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let changed = responses
                            .iter()
                            .filter(|x| x.changed())
                            .map(|x| {
                                ui.and_then(|ui| ui.data(|data| data.get_temp::<String>(x.id)))
                            })
                            .collect::<Vec<_>>();
                        self.trigger_side_effect(move |render_context| {
                            if !changed.is_empty() {
                                for value in &changed {
                                    log::trace!("change action: {action}");
                                    if !Self::handle_action(
                                        &action.action,
                                        Some(&action),
                                        StyleTrigger::UiEvent,
                                        render_context,
                                        id,
                                        value.as_deref(),
                                        None,
                                    ) {
                                        return false;
                                    }
                                }

                                return true;
                            }

                            Self::unhandle_action(
                                &action.action,
                                StyleTrigger::UiEvent,
                                render_context,
                                id,
                            );

                            true
                        });
                    }
                    ActionTrigger::Resize => {
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        self.add_resize_handler(move |render_context| {
                            if !Self::handle_action(
                                &action.action,
                                Some(&action),
                                StyleTrigger::CustomEvent,
                                render_context,
                                id,
                                None,
                                None,
                            ) {
                                return false;
                            }
                            true
                        });
                    }
                    ActionTrigger::Immediate => {
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        self.add_immediate_handler(id, move |render_context| {
                            if !Self::handle_action(
                                &action.action,
                                Some(&action),
                                StyleTrigger::CustomEvent,
                                render_context,
                                id,
                                None,
                                None,
                            ) {
                                return false;
                            }
                            true
                        });
                    }
                    ActionTrigger::Event(..)
                    | ActionTrigger::HttpBeforeRequest
                    | ActionTrigger::HttpAfterRequest
                    | ActionTrigger::HttpRequestSuccess
                    | ActionTrigger::HttpRequestError
                    | ActionTrigger::HttpRequestAbort
                    | ActionTrigger::HttpRequestTimeout => {}
                }
            }
        }
    }

    fn handle_custom_event_side_effects(&self, container: &Container) {
        for fx_action in &container.actions {
            if let ActionTrigger::Event(event_name) = &fx_action.trigger {
                let action = fx_action.effect.clone();
                let id = container.id;
                self.add_event_handler(event_name.clone(), move |render_context, value| {
                    let effect = &action;
                    if let ActionType::Event { action, .. } = &action.action {
                        Self::handle_action(
                            action,
                            Some(effect),
                            StyleTrigger::CustomEvent,
                            render_context,
                            id,
                            value,
                            None,
                        );
                    }
                });
            }
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    fn handle_element_side_effects(
        &self,
        ctx: &egui::Context,
        element: &Element,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        response: Option<&Response>,
    ) {
        if let Some(response) = response {
            #[cfg(feature = "profiling")]
            profiling::scope!("button and anchor side effects");
            let viewport_rect = rect.map(|rect| {
                let (offset_x, offset_y) =
                    viewport.map_or((0.0, 0.0), |viewport| (viewport.pos.x, viewport.pos.y));
                egui::Rect::from_min_size(egui::pos2(offset_x, offset_y), rect.size())
            });

            match element {
                Element::Button { .. } => {
                    #[cfg(feature = "profiling")]
                    profiling::scope!("button side effects");
                    let response = response.clone();
                    let pointer = ctx.input(|x| x.pointer.clone());
                    let ctx = ctx.clone();
                    self.trigger_side_effect(move |_render_context| {
                        if Self::rect_contains_mouse(&pointer, response.rect, viewport_rect) {
                            ctx.output_mut(|x| x.cursor_icon = CursorIcon::PointingHand);
                        }

                        true
                    });
                }
                Element::Anchor { href, .. } => {
                    #[cfg(feature = "profiling")]
                    profiling::scope!("anchor side effects");
                    let href = href.to_owned();
                    let sender = self.sender.clone();
                    let response = response.clone();
                    let pointer = ctx.input(|x| x.pointer.clone());
                    let ctx = ctx.clone();
                    self.trigger_side_effect(move |_render_context| {
                        if Self::rect_contains_mouse(&pointer, response.rect, viewport_rect)
                            && pointer.primary_released()
                            && let Some(href) = href.clone()
                            && let Err(e) = sender.send(href)
                        {
                            log::error!("Failed to send href event: {e:?}");
                        }

                        if Self::rect_contains_mouse(&pointer, response.rect, viewport_rect) {
                            ctx.output_mut(|x| x.cursor_icon = CursorIcon::PointingHand);
                        }

                        true
                    });
                }
                _ => {}
            }
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(
        clippy::too_many_lines,
        clippy::cognitive_complexity,
        clippy::too_many_arguments
    )]
    fn handle_action(
        action: &ActionType,
        effect: Option<&ActionEffect>,
        trigger: StyleTrigger,
        render_context: &mut RenderContext,
        id: usize,
        event_value: Option<&str>,
        value: Option<&Value>,
    ) -> bool {
        // Convert local StyleTrigger to shared StyleTrigger
        let shared_trigger = match trigger {
            StyleTrigger::UiEvent => hyperchad_actions::handler::StyleTrigger::UiEvent,
            StyleTrigger::CustomEvent => hyperchad_actions::handler::StyleTrigger::CustomEvent,
        };

        // Use the shared action handler
        render_context.action_handler.handle_action(
            action,
            effect,
            shared_trigger,
            id,
            render_context.action_context,
            event_value,
            value,
        )
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::only_used_in_recursion)]
    fn unhandle_action(
        action: &ActionType,
        trigger: StyleTrigger,
        render_context: &mut RenderContext,
        id: usize,
    ) {
        // Convert local StyleTrigger to shared StyleTrigger
        let shared_trigger = match trigger {
            StyleTrigger::UiEvent => hyperchad_actions::handler::StyleTrigger::UiEvent,
            StyleTrigger::CustomEvent => hyperchad_actions::handler::StyleTrigger::CustomEvent,
        };

        // Use the shared action handler
        render_context.action_handler.unhandle_action(
            action,
            shared_trigger,
            id,
            render_context.action_context,
        );
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn render_element(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        element: &Container,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &Container)>,
        scroll_child: bool,
    ) {
        log::trace!("render_element: rect={rect:?}");

        if element.element == Element::Table {
            self.render_table(
                render_context,
                ctx,
                ui,
                element,
                viewport,
                rect,
                relative_container,
            );
            return;
        }

        if scroll_child
            && self.handle_scroll_child_out_of_view(
                render_context,
                ctx,
                ui,
                element,
                viewport,
                rect,
                relative_container,
            )
        {
            return;
        }

        if render_context.watch_positions.contains(&element.id) {
            Self::get_render_rect(render_context, ui, element, relative_container);
        }

        let response = match &element.element {
            Element::Input { input, .. } => {
                Self::render_input(element, ui, ctx, input, render_context.checkboxes)
            }
            Element::Raw { value } => {
                let font_size = element
                    .calculated_font_size
                    .expect("Missing calculated_font_size");
                let mut label = egui::Label::new(egui::RichText::new(value).size(font_size));

                if matches!(element.text_overflow, Some(TextOverflow::Ellipsis)) {
                    label = label.truncate();
                }

                Some(label.ui(ui))
            }
            Element::Image { source, .. } => source
                .as_ref()
                .map(|source| Self::render_image(render_context, ui, source, element)),
            Element::Canvas => element.str_id.as_ref().map_or_else(
                || None,
                |str_id| Self::render_canvas(render_context, ui, str_id, element),
            ),
            _ => None,
        };

        if let Some(response) = response {
            self.handle_container_side_effects(
                render_context,
                ctx,
                Some(ui),
                element,
                viewport,
                rect,
                Some(&response),
                false,
            );
            return;
        }

        self.render_container(
            render_context,
            ctx,
            ui,
            element,
            viewport,
            rect,
            relative_container,
        );
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_input(
        container: &Container,
        ui: &mut Ui,
        ctx: &egui::Context,
        input: &Input,
        checkboxes: &mut HashMap<egui::Id, bool>,
    ) -> Option<Response> {
        match input {
            Input::Text { .. } | Input::Password { .. } => {
                Some(Self::render_text_input(container, ui, ctx, input))
            }
            Input::Checkbox { .. } => Some(Self::render_checkbox_input(ui, input, checkboxes)),
            Input::Hidden { .. } => None,
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_text_input(
        container: &Container,
        ui: &mut Ui,
        ctx: &egui::Context,
        input: &Input,
    ) -> Response {
        let (Input::Text { value, .. } | Input::Password { value, .. }) = input else {
            unreachable!()
        };

        let id = ui.next_auto_id();
        let mut value_text = ui
            .data_mut(|data| data.remove_temp::<String>(id))
            .unwrap_or_else(|| value.clone().unwrap_or_default());
        let mut text_edit = egui::TextEdit::singleline(&mut value_text).id(id);

        if let Input::Password { .. } = input {
            text_edit = text_edit.password(true);
        }

        let (font_size, _) = Self::set_font_size(container, ctx);

        if container.width.is_some() {
            text_edit = text_edit.desired_width(container.calculated_width.unwrap());
        }

        if container.height.is_some() {
            let height = container.calculated_height.unwrap();
            let remaining_height = height % font_size;
            let rows = (height / font_size).floor();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let rows = rows as usize;
            text_edit = text_edit.desired_rows(rows);

            let margin = (remaining_height / 2.0).round();
            #[allow(clippy::cast_possible_truncation)]
            let margin = margin as i8;

            text_edit = text_edit.margin(egui::Margin {
                left: 0,
                right: 0,
                top: margin,
                bottom: margin,
            });
        }

        let response = text_edit.ui(ui);
        ui.data_mut(|data| data.insert_temp(id, value_text));
        response
    }

    fn set_font_size(container: &Container, ctx: &egui::Context) -> (f32, bool) {
        let body_font_size = ctx
            .style()
            .text_styles
            .get(&egui::TextStyle::Body)
            .expect("Missing body font size")
            .size;

        let font_size = container
            .calculated_font_size
            .unwrap_or_else(|| panic!("Missing calculated_font_size:\n{container}"));

        if float_eq!(body_font_size, font_size) {
            (font_size, false)
        } else {
            ctx.style_mut(|style| {
                for font in style.text_styles.values_mut() {
                    font.size = font_size;
                }
            });

            (font_size, true)
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_checkbox_input(
        ui: &mut Ui,
        input: &Input,
        checkboxes: &mut HashMap<egui::Id, bool>,
    ) -> Response {
        let Input::Checkbox { checked } = input else {
            unreachable!();
        };
        let checked = *checked;

        let id = ui.next_auto_id();

        let contains = { checkboxes.contains_key(&id) };

        let mut checked_value = ui
            .data_mut(|data| {
                let value = data.remove_temp::<bool>(id);

                if !contains {
                    return None;
                }

                value
            })
            .unwrap_or_else(|| checked.unwrap_or_default());

        let checkbox = egui::Checkbox::without_text(&mut checked_value);
        let response = checkbox.ui(ui);

        ui.data_mut(|data| data.insert_temp(id, checked_value));

        if response.changed() {
            checkboxes.insert(id, checked_value);
        }

        response
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_image(
        render_context: &mut RenderContext,
        ui: &mut Ui,
        source: &str,
        container: &Container,
    ) -> Response {
        egui::Frame::new()
            .show(ui, |ui| {
                ui.set_width(container.calculated_width.unwrap());
                ui.set_height(container.calculated_height.unwrap());

                let Some(AppImage::Bytes(bytes)) = render_context.images.get(source).cloned()
                else {
                    return;
                };

                log::trace!(
                    "render_image: showing image for source={source} ({}, {})",
                    container.calculated_width.unwrap(),
                    container.calculated_height.unwrap(),
                );

                egui::Image::from_bytes(
                    format!("bytes://{source}"),
                    egui::load::Bytes::Shared(bytes),
                )
                .max_width(container.calculated_width.unwrap())
                .max_height(container.calculated_height.unwrap())
                .ui(ui);
            })
            .response
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_canvas(
        render_context: &mut RenderContext,
        ui: &mut Ui,
        str_id: &str,
        container: &Container,
    ) -> Option<Response> {
        render_context.canvas_actions.get(str_id).map_or_else(
            || None,
            |actions| {
                let (response, painter) = ui.allocate_painter(
                    egui::Vec2::new(
                        container.calculated_width.unwrap(),
                        container.calculated_height.unwrap(),
                    ),
                    egui::Sense::hover(),
                );

                let pixels_per_point = 1.0; // ctx.pixels_per_point();
                let cursor_px = egui::Pos2::new(
                    response.rect.min.x * pixels_per_point,
                    response.rect.min.y * pixels_per_point,
                )
                .ceil();

                let default_color = Color32::BLACK;
                let stroke = &mut egui::epaint::PathStroke::new(1.0, default_color).inside();
                stroke.color = egui::epaint::ColorMode::Solid(default_color);

                for action in actions {
                    match action {
                        CanvasAction::Clear | CanvasAction::ClearRect(..) => {}
                        CanvasAction::StrokeSize(size) => {
                            stroke.width = *size;
                        }
                        CanvasAction::StrokeColor(color) => {
                            stroke.color = egui::epaint::ColorMode::Solid((*color).into());
                        }
                        CanvasAction::Line(start, end) => {
                            let color = match &stroke.color {
                                egui::epaint::ColorMode::Solid(color) => *color,
                                egui::epaint::ColorMode::UV(_f) => unreachable!(),
                            };
                            painter.line_segment(
                                [
                                    egui::Pos2::new(start.0 + cursor_px.x, start.1 + cursor_px.y),
                                    egui::Pos2::new(end.0 + cursor_px.x, end.1 + cursor_px.y),
                                ],
                                (stroke.width, color),
                            );
                        }
                        CanvasAction::FillRect(start, end) => {
                            let egui::epaint::ColorMode::Solid(color) = stroke.color else {
                                continue;
                            };
                            painter.rect_filled(
                                egui::Rect::from_min_max(
                                    egui::Pos2::new(start.0 + cursor_px.x, start.1 + cursor_px.y),
                                    egui::Pos2::new(end.0 + cursor_px.x, end.1 + cursor_px.y),
                                ),
                                0.0,
                                color,
                            );
                        }
                    }
                }

                Some(response)
            },
        )
    }

    fn apply_container_styles(
        container: &Container,
        mut frame: egui::Frame,
        start: bool,
        end: bool,
        action_handler: &EguiActionHandler,
    ) -> egui::Frame {
        if let Some(background) = action_handler
            .get_background_override(container.id)
            .copied()
            .unwrap_or(container.background)
        {
            frame = frame.fill(background.into());
        }

        if start && end {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            if container.calculated_border_top_left_radius.is_some()
                || container.calculated_border_top_right_radius.is_some()
                || container.calculated_border_bottom_left_radius.is_some()
                || container.calculated_border_bottom_right_radius.is_some()
            {
                frame = frame.corner_radius(egui::CornerRadius {
                    nw: container
                        .calculated_border_top_left_radius
                        .map_or(0, |x| x.round() as u8),
                    ne: container
                        .calculated_border_top_right_radius
                        .map_or(0, |x| x.round() as u8),
                    sw: container
                        .calculated_border_bottom_left_radius
                        .map_or(0, |x| x.round() as u8),
                    se: container
                        .calculated_border_bottom_right_radius
                        .map_or(0, |x| x.round() as u8),
                });
            }
        } else if start {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            if container.calculated_border_top_left_radius.is_some()
                || container.calculated_border_bottom_left_radius.is_some()
            {
                frame = frame.corner_radius(egui::CornerRadius {
                    nw: container
                        .calculated_border_top_left_radius
                        .map_or(0, |x| x.round() as u8),
                    ne: 0,
                    sw: container
                        .calculated_border_bottom_left_radius
                        .map_or(0, |x| x.round() as u8),
                    se: 0,
                });
            }
        } else if end
            && (container.calculated_border_top_right_radius.is_some()
                || container.calculated_border_bottom_right_radius.is_some())
        {
            frame = frame.corner_radius(
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    egui::CornerRadius {
                        nw: 0,
                        ne: container
                            .calculated_border_top_right_radius
                            .map_or(0, |x| x.round() as u8),
                        sw: 0,
                        se: container
                            .calculated_border_bottom_right_radius
                            .map_or(0, |x| x.round() as u8),
                    }
                },
            );
        }

        frame
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    #[cfg_attr(feature = "profiling", profiling::function)]
    fn render_table(
        &self,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        ui: &mut Ui,
        element: &Container,
        viewport: Option<&Viewport>,
        rect: Option<egui::Rect>,
        relative_container: Option<(egui::Rect, &Container)>,
    ) {
        let TableIter { rows, headings } = element.table_iter();

        let mut head_trs = element
            .children
            .iter()
            .filter(|x| matches!(x.element, Element::THead))
            .flat_map(|x| x.children.iter());

        let mut body_trs = element
            .children
            .iter()
            .filter(|x| matches!(x.element, Element::TBody))
            .flat_map(|x| x.children.iter());

        let grid = egui::Grid::new(format!("grid-{}", element.id));

        grid.show(ui, |ui| {
            if let Some(headings) = headings {
                for heading in headings {
                    let tr = head_trs.next();
                    if let Some(tr) = tr {
                        self.handle_container_side_effects(
                            render_context,
                            ctx,
                            Some(ui),
                            tr,
                            viewport,
                            rect,
                            None,
                            false,
                        );
                    }
                    let mut heading = heading.peekable();
                    let mut first = true;
                    let mut responses = vec![];
                    while let Some(th) = heading.next() {
                        let mut frame = egui::Frame::new();

                        if let Some(tr) = tr {
                            frame = Self::apply_container_styles(
                                tr,
                                frame,
                                first,
                                heading.peek().is_none(),
                                &render_context.action_handler,
                            );
                        }

                        let response = frame.show(ui, |ui| {
                            self.render_container(
                                render_context,
                                ctx,
                                ui,
                                th,
                                viewport,
                                rect,
                                relative_container,
                            );
                        });

                        responses.push(response.response);
                        first = false;
                    }

                    if let Some(tr) = tr {
                        self.handle_ui_event_side_effects(
                            tr,
                            Some(ui),
                            ctx,
                            viewport,
                            rect,
                            responses,
                        );
                    }
                    ui.end_row();
                }
            }
            for row in rows {
                let tr = body_trs.next();
                if let Some(tr) = tr {
                    self.handle_container_side_effects(
                        render_context,
                        ctx,
                        Some(ui),
                        tr,
                        viewport,
                        rect,
                        None,
                        false,
                    );
                }
                {
                    let mut row = row.peekable();
                    let mut first = true;
                    let mut responses = vec![];
                    while let Some(td) = row.next() {
                        let mut frame = egui::Frame::new();

                        if let Some(tr) = tr {
                            frame = Self::apply_container_styles(
                                tr,
                                frame,
                                first,
                                row.peek().is_none(),
                                &render_context.action_handler,
                            );
                        }

                        let response = frame.show(ui, |ui| {
                            self.render_container(
                                render_context,
                                ctx,
                                ui,
                                td,
                                viewport,
                                rect,
                                relative_container,
                            );
                        });

                        responses.push(response.response);
                        first = false;
                    }
                    if let Some(tr) = tr {
                        self.handle_ui_event_side_effects(
                            tr,
                            Some(ui),
                            ctx,
                            viewport,
                            rect,
                            responses,
                        );
                    }
                    ui.end_row();
                }
            }
        });
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn add_event_handler(
        &self,
        event_name: String,
        handler: impl Fn(&mut RenderContext, Option<&str>) + Send + Sync + 'static,
    ) {
        self.event_handlers
            .write()
            .unwrap()
            .push((event_name, Box::new(handler)));
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn add_resize_handler(
        &self,
        handler: impl Fn(&mut RenderContext) -> bool + Send + Sync + 'static,
    ) {
        self.resize_handlers
            .write()
            .unwrap()
            .push(Box::new(handler));
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn add_immediate_handler(
        &self,
        id: usize,
        handler: impl Fn(&mut RenderContext) -> bool + Send + Sync + 'static,
    ) {
        if !self
            .immediate_elements_handled
            .read()
            .unwrap()
            .contains(&id)
        {
            self.immediate_elements_handled.write().unwrap().insert(id);
            self.immediate_handlers
                .write()
                .unwrap()
                .push(Box::new(handler));
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn trigger_side_effect(
        &self,
        handler: impl Fn(&mut RenderContext) -> bool + Send + Sync + 'static,
    ) {
        self.side_effects
            .lock()
            .unwrap()
            .push_back(Box::new(handler));
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_lines)]
    fn paint(&self, ctx: &egui::Context) {
        let resized = self.check_frame_resize(ctx);

        self.event_handlers.write().unwrap().clear();

        let container_binding = self.container.write().unwrap();
        let Some(container) = container_binding.as_ref() else {
            return;
        };
        let mut viewport_listeners = self.viewport_listeners.write().unwrap();
        let mut images = self.images.write().unwrap();
        let mut canvas_actions = self.canvas_actions.write().unwrap();
        let mut route_requests = self.route_requests.write().unwrap();
        let mut checkboxes = self.checkboxes.write().unwrap();
        let mut positions = self.positions.write().unwrap();
        let mut watch_positions = self.watch_positions.write().unwrap();
        #[cfg(feature = "debug")]
        if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
            let value = {
                let mut handle = DEBUG.write().unwrap();
                let value = *handle;
                let value = !value;
                *handle = value;
                value
            };
            log::debug!("Set DEBUG to {value}");
        }

        {
            // Create action handler for rendering
            let element_finder = EguiElementFinder::new(container);
            let action_handler =
                hyperchad_actions::handler::utils::create_default_handler(element_finder);

            let mut render_context = RenderContext {
                viewport_listeners: &mut viewport_listeners,
                images: &mut images,
                canvas_actions: &mut canvas_actions,
                route_requests: &mut route_requests,
                checkboxes: &mut checkboxes,
                positions: &mut positions,
                watch_positions: &mut watch_positions,
                action_handler,
                action_context: &self.action_context,
            };

            if resized {
                for handler in self.resize_handlers.write().unwrap().drain(..) {
                    handler(&mut render_context);
                }
            } else {
                self.resize_handlers.write().unwrap().clear();
            }

            ctx.memory_mut(|x| {
                x.options.input_options.line_scroll_speed = 100.0;
            });

            ctx.style_mut(|style| {
                const ZERO_SPACING: egui::Spacing = egui::Spacing {
                    item_spacing: egui::emath::Vec2::ZERO,
                    window_margin: egui::Margin::ZERO,
                    button_padding: egui::emath::Vec2::ZERO,
                    menu_margin: egui::Margin::ZERO,
                    indent: 0.0,
                    interact_size: egui::emath::Vec2::ZERO,
                    slider_width: 0.0,
                    slider_rail_height: 0.0,
                    combo_width: 0.0,
                    text_edit_width: 280.0,
                    icon_width: 14.0,
                    icon_width_inner: 8.0,
                    icon_spacing: 4.0,
                    default_area_size: egui::emath::Vec2::ZERO,
                    tooltip_width: 0.0,
                    menu_width: 0.0,
                    menu_spacing: 0.0,
                    indent_ends_with_horizontal_line: false,
                    combo_height: 0.0,
                    scroll: egui::style::ScrollStyle {
                        floating: true,
                        bar_width: 10.0,
                        foreground_color: true,
                        floating_allocated_width: 0.0,
                        dormant_background_opacity: 0.0,
                        dormant_handle_opacity: 0.0,
                        handle_min_length: 12.0,
                        bar_inner_margin: 4.0,
                        bar_outer_margin: 0.0,
                        floating_width: 2.0,
                        active_background_opacity: 0.4,
                        interact_background_opacity: 0.7,
                        active_handle_opacity: 0.6,
                        interact_handle_opacity: 1.0,
                    },
                };
                style.spacing = ZERO_SPACING;
                #[cfg(all(debug_assertions, feature = "debug"))]
                {
                    style.debug.debug_on_hover = true;
                }
            });

            egui::CentralPanel::default()
                .frame(egui::Frame::new())
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(
                            self.background
                                .unwrap_or_else(|| Color32::from_hex("#181a1b").unwrap()),
                        )
                        .show(ui, |ui| {
                            self.render_container(
                                &mut render_context,
                                ctx,
                                ui,
                                container,
                                None,
                                None,
                                None,
                            );
                        });
                });

            for handler in self.immediate_handlers.write().unwrap().drain(..) {
                if !handler(&mut render_context) {
                    break;
                }
            }

            let mut handlers_count = 0;

            for handler in self.side_effects.lock().unwrap().drain(..) {
                handlers_count += 1;
                if !handler(&mut render_context) {
                    break;
                }
            }

            log::trace!("paint: {handlers_count} handler(s) on render");
        }

        drop(container_binding);
        drop(viewport_listeners);
        drop(images);
        drop(canvas_actions);
        drop(route_requests);
        drop(checkboxes);
        drop(positions);
        drop(watch_positions);
        // Removed: action_handler was removed

        #[cfg(feature = "profiling")]
        profiling::finish_frame!();
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> eframe::App for EguiApp<C> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let render_queue = self.render_queue.write().unwrap().take();
        if let Some(render_queue) = render_queue {
            let view_tx = self.view_tx.write().unwrap().take();
            if let Some(view_tx) = view_tx {
                for view in render_queue {
                    let _ = view_tx
                        .send(Some(view))
                        .inspect_err(|e| log::error!("Failed to send render: {e:?}"));
                }
                let _ = view_tx
                    .send(None)
                    .inspect_err(|e| log::error!("Failed to send render: {e:?}"));
                drop(view_tx);
                let _ = self
                    .render_buffer_rx
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .recv()
                    .inspect_err(|e| log::error!("Failed to send render: {e:?}"));
            }
        }

        self.paint(ctx);
    }
}

const fn cursor_to_cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Auto => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::PointingHand,
        Cursor::Text => CursorIcon::Text,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::NoDrop => CursorIcon::NoDrop,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::AllScroll => CursorIcon::AllScroll,
        Cursor::ColResize => CursorIcon::ResizeColumn,
        Cursor::RowResize => CursorIcon::ResizeRow,
        Cursor::NResize => CursorIcon::ResizeNorth,
        Cursor::EResize => CursorIcon::ResizeEast,
        Cursor::SResize => CursorIcon::ResizeSouth,
        Cursor::WResize => CursorIcon::ResizeWest,
        Cursor::NeResize => CursorIcon::ResizeNorthEast,
        Cursor::NwResize => CursorIcon::ResizeNorthWest,
        Cursor::SeResize => CursorIcon::ResizeSouthEast,
        Cursor::SwResize => CursorIcon::ResizeSouthWest,
        Cursor::EwResize => CursorIcon::ResizeHorizontal,
        Cursor::NsResize => CursorIcon::ResizeVertical,
        Cursor::NeswResize => CursorIcon::ResizeNwSe,
        Cursor::ZoomIn => CursorIcon::ZoomIn,
        Cursor::ZoomOut => CursorIcon::ZoomOut,
    }
}

const EPSILON: f32 = 0.001;

fn get_left_offset(x: impl AsRef<Container>) -> Option<f32> {
    let x = x.as_ref();

    let offset =
        x.calculated_offset_x.unwrap_or_default() + x.calculated_margin_left.unwrap_or_default();

    if offset < EPSILON { None } else { Some(offset) }
}

fn get_top_offset(x: impl AsRef<Container>) -> Option<f32> {
    let x = x.as_ref();

    let offset =
        x.calculated_offset_y.unwrap_or_default() + x.calculated_margin_top.unwrap_or_default();

    if offset < EPSILON { None } else { Some(offset) }
}

fn get_remaining_offset_width(x: impl AsRef<Container>) -> f32 {
    let x = x.as_ref();

    x.bounding_calculated_width().unwrap_or_default() - x.calculated_margin_left.unwrap_or_default()
}

fn get_remaining_offset_height(x: impl AsRef<Container>) -> f32 {
    let x = x.as_ref();

    x.bounding_calculated_height().unwrap_or_default() - x.calculated_margin_top.unwrap_or_default()
}

#[cfg(feature = "profiling-puffin")]
fn start_puffin_server() {
    puffin::set_scopes_on(true);

    match puffin_http::Server::new("127.0.0.1:8585") {
        Ok(puffin_server) => {
            log::info!("Run:  cargo install puffin_viewer && puffin_viewer --url 127.0.0.1:8585");

            std::process::Command::new("puffin_viewer")
                .arg("--url")
                .arg("127.0.0.1:8585")
                .spawn()
                .ok();

            #[allow(clippy::mem_forget)]
            std::mem::forget(puffin_server);
        }
        Err(err) => {
            log::error!("Failed to start puffin server: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_actions::handler::ElementFinder;
    use hyperchad_renderer::canvas::{CanvasAction, Pos};

    #[test_log::test]
    fn test_compact_canvas_actions_removes_actions_before_clear() {
        let mut actions = vec![
            CanvasAction::StrokeSize(1.0),
            CanvasAction::Line(Pos(0.0, 0.0), Pos(10.0, 10.0)),
            CanvasAction::Clear,
            CanvasAction::StrokeSize(2.0),
            CanvasAction::Line(Pos(20.0, 20.0), Pos(30.0, 30.0)),
        ];

        compact_canvas_actions(&mut actions);

        // Everything before and including Clear should be removed,
        // leaving only actions after Clear
        assert_eq!(actions.len(), 2);
        assert!(
            matches!(actions[0], CanvasAction::StrokeSize(s) if (s - 2.0).abs() < f32::EPSILON)
        );
        assert!(matches!(actions[1], CanvasAction::Line(_, _)));
    }

    #[test_log::test]
    fn test_compact_canvas_actions_removes_fill_rect_before_clear_rect() {
        // The function iterates in reverse: FillRect is checked against
        // ClearRects that come AFTER it in array order (since we see
        // them first when going backwards)
        let mut actions = vec![
            CanvasAction::FillRect(Pos(10.0, 10.0), Pos(50.0, 50.0)), // Intersects, will be removed
            CanvasAction::ClearRect(Pos(0.0, 0.0), Pos(100.0, 100.0)),
        ];

        compact_canvas_actions(&mut actions);

        // FillRect should be removed since it intersects with ClearRect
        // (FillRect appears before ClearRect in array order)
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CanvasAction::ClearRect(_, _)));
    }

    #[test_log::test]
    fn test_compact_canvas_actions_keeps_fill_rect_after_clear_rect() {
        // FillRect that appears AFTER ClearRect in array order is NOT removed
        // because when iterating in reverse, we see FillRect before we've
        // added ClearRect to the cleared list
        let mut actions = vec![
            CanvasAction::ClearRect(Pos(0.0, 0.0), Pos(100.0, 100.0)),
            CanvasAction::FillRect(Pos(10.0, 10.0), Pos(50.0, 50.0)), // Not removed
        ];

        compact_canvas_actions(&mut actions);

        // Both actions remain since FillRect is after ClearRect in array order
        assert_eq!(actions.len(), 2);
    }

    #[test_log::test]
    fn test_compact_canvas_actions_keeps_fill_rect_when_not_intersecting() {
        let mut actions = vec![
            CanvasAction::FillRect(Pos(100.0, 100.0), Pos(150.0, 150.0)), // Does not intersect
            CanvasAction::ClearRect(Pos(0.0, 0.0), Pos(50.0, 50.0)),
        ];

        compact_canvas_actions(&mut actions);

        // Both actions should remain since FillRect doesn't intersect ClearRect
        assert_eq!(actions.len(), 2);
    }

    #[test_log::test]
    fn test_compact_canvas_actions_empty_list() {
        let mut actions: Vec<CanvasAction> = vec![];
        compact_canvas_actions(&mut actions);
        assert!(actions.is_empty());
    }

    #[test_log::test]
    fn test_compact_canvas_actions_preserves_stroke_and_line_actions() {
        let mut actions = vec![
            CanvasAction::StrokeSize(2.0),
            CanvasAction::StrokeColor(hyperchad_color::Color {
                r: 255,
                g: 0,
                b: 0,
                a: None,
            }),
            CanvasAction::Line(Pos(0.0, 0.0), Pos(100.0, 100.0)),
        ];

        compact_canvas_actions(&mut actions);

        // No changes since there's no Clear or intersecting rects
        assert_eq!(actions.len(), 3);
    }

    #[test_log::test]
    fn test_compact_canvas_actions_clear_at_end_removes_all() {
        let mut actions = vec![
            CanvasAction::StrokeSize(1.0),
            CanvasAction::Line(Pos(0.0, 0.0), Pos(10.0, 10.0)),
            CanvasAction::FillRect(Pos(0.0, 0.0), Pos(50.0, 50.0)),
            CanvasAction::Clear,
        ];

        compact_canvas_actions(&mut actions);

        // Clear at the end means all actions including Clear are removed
        assert!(actions.is_empty());
    }

    #[test_log::test]
    fn test_compact_canvas_actions_multiple_fill_rects_before_clear_rect() {
        // FillRects that appear BEFORE ClearRect in array order are removed
        // if they intersect with the ClearRect
        let mut actions = vec![
            CanvasAction::FillRect(Pos(10.0, 10.0), Pos(40.0, 40.0)), // Intersects, removed
            CanvasAction::FillRect(Pos(200.0, 200.0), Pos(250.0, 250.0)), // No intersection, kept
            CanvasAction::ClearRect(Pos(0.0, 0.0), Pos(50.0, 50.0)),
        ];

        compact_canvas_actions(&mut actions);

        // The intersecting FillRect should be removed
        assert_eq!(actions.len(), 2);
        // Non-intersecting FillRect and ClearRect remain
        assert!(
            matches!(actions[0], CanvasAction::FillRect(Pos(x1, y1), _) if (x1 - 200.0).abs() < f32::EPSILON && (y1 - 200.0).abs() < f32::EPSILON)
        );
        assert!(matches!(actions[1], CanvasAction::ClearRect(_, _)));
    }

    #[test_log::test]
    fn test_compact_canvas_actions_clear_overrides_clear_rects() {
        let mut actions = vec![
            CanvasAction::ClearRect(Pos(0.0, 0.0), Pos(50.0, 50.0)),
            CanvasAction::FillRect(Pos(10.0, 10.0), Pos(40.0, 40.0)),
            CanvasAction::Clear,
            CanvasAction::Line(Pos(0.0, 0.0), Pos(100.0, 100.0)),
        ];

        compact_canvas_actions(&mut actions);

        // Clear should remove everything before it including itself
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CanvasAction::Line(_, _)));
    }

    #[test_log::test]
    fn test_compact_canvas_actions_multiple_clear_rects_cumulative() {
        // Multiple ClearRects accumulate - FillRects are removed if they
        // intersect with ANY of the ClearRects that come after them
        let mut actions = vec![
            CanvasAction::FillRect(Pos(10.0, 10.0), Pos(40.0, 40.0)), // Intersects first ClearRect
            CanvasAction::FillRect(Pos(110.0, 110.0), Pos(140.0, 140.0)), // Intersects second ClearRect
            CanvasAction::ClearRect(Pos(0.0, 0.0), Pos(50.0, 50.0)),
            CanvasAction::ClearRect(Pos(100.0, 100.0), Pos(150.0, 150.0)),
        ];

        compact_canvas_actions(&mut actions);

        // Both intersecting FillRects should be removed
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], CanvasAction::ClearRect(_, _)));
        assert!(matches!(actions[1], CanvasAction::ClearRect(_, _)));
    }

    // Helper to create a container with specified id, str_id, classes, and children
    fn make_container(
        id: usize,
        str_id: Option<&str>,
        classes: Vec<&str>,
        children: Vec<Container>,
    ) -> Container {
        Container {
            id,
            str_id: str_id.map(String::from),
            classes: classes.into_iter().map(String::from).collect(),
            children,
            ..Default::default()
        }
    }

    // ==================== EguiElementFinder::find_element_by_id tests ====================

    #[test_log::test]
    fn test_find_element_by_id_finds_root_element() {
        let container = make_container(1, None, vec![], vec![]);
        let result = EguiElementFinder::find_element_by_id(&container, 1);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 1);
    }

    #[test_log::test]
    fn test_find_element_by_id_finds_direct_child() {
        let child = make_container(2, None, vec![], vec![]);
        let container = make_container(1, None, vec![], vec![child]);

        let result = EguiElementFinder::find_element_by_id(&container, 2);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 2);
    }

    #[test_log::test]
    fn test_find_element_by_id_finds_deeply_nested_element() {
        // Create a 3-level deep structure
        let grandchild = make_container(3, None, vec![], vec![]);
        let child = make_container(2, None, vec![], vec![grandchild]);
        let container = make_container(1, None, vec![], vec![child]);

        let result = EguiElementFinder::find_element_by_id(&container, 3);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 3);
    }

    #[test_log::test]
    fn test_find_element_by_id_returns_none_when_not_found() {
        let child = make_container(2, None, vec![], vec![]);
        let container = make_container(1, None, vec![], vec![child]);

        let result = EguiElementFinder::find_element_by_id(&container, 999);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_find_element_by_id_searches_all_siblings() {
        // Create container with multiple children, target is in the last sibling subtree
        let grandchild = make_container(5, None, vec![], vec![]);
        let child1 = make_container(2, None, vec![], vec![]);
        let child2 = make_container(3, None, vec![], vec![]);
        let child3 = make_container(4, None, vec![], vec![grandchild]);
        let container = make_container(1, None, vec![], vec![child1, child2, child3]);

        let result = EguiElementFinder::find_element_by_id(&container, 5);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 5);
    }

    // ==================== EguiElementFinder::find_element_by_str_id tests ====================

    #[test_log::test]
    fn test_find_element_by_str_id_finds_root_element() {
        let container = make_container(1, Some("root"), vec![], vec![]);
        let result = EguiElementFinder::find_element_by_str_id(&container, "root");
        assert!(result.is_some());
        assert_eq!(result.unwrap().str_id.as_deref(), Some("root"));
    }

    #[test_log::test]
    fn test_find_element_by_str_id_finds_nested_element() {
        let grandchild = make_container(3, Some("target"), vec![], vec![]);
        let child = make_container(2, Some("child"), vec![], vec![grandchild]);
        let container = make_container(1, Some("root"), vec![], vec![child]);

        let result = EguiElementFinder::find_element_by_str_id(&container, "target");
        assert!(result.is_some());
        assert_eq!(result.unwrap().str_id.as_deref(), Some("target"));
        assert_eq!(result.unwrap().id, 3);
    }

    #[test_log::test]
    fn test_find_element_by_str_id_returns_none_when_not_found() {
        let child = make_container(2, Some("child"), vec![], vec![]);
        let container = make_container(1, Some("root"), vec![], vec![child]);

        let result = EguiElementFinder::find_element_by_str_id(&container, "nonexistent");
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_find_element_by_str_id_skips_elements_without_str_id() {
        let grandchild = make_container(3, Some("target"), vec![], vec![]);
        let child = make_container(2, None, vec![], vec![grandchild]); // No str_id
        let container = make_container(1, None, vec![], vec![child]); // No str_id

        let result = EguiElementFinder::find_element_by_str_id(&container, "target");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 3);
    }

    // ==================== EguiElementFinder::find_element_by_class tests ====================

    #[test_log::test]
    fn test_find_element_by_class_finds_root_element() {
        let container = make_container(1, None, vec!["container", "main"], vec![]);
        let result = EguiElementFinder::find_element_by_class(&container, "main");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 1);
    }

    #[test_log::test]
    fn test_find_element_by_class_finds_nested_element() {
        let grandchild = make_container(3, None, vec!["target-class"], vec![]);
        let child = make_container(2, None, vec!["wrapper"], vec![grandchild]);
        let container = make_container(1, None, vec!["root"], vec![child]);

        let result = EguiElementFinder::find_element_by_class(&container, "target-class");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 3);
    }

    #[test_log::test]
    fn test_find_element_by_class_returns_none_when_not_found() {
        let child = make_container(2, None, vec!["child-class"], vec![]);
        let container = make_container(1, None, vec!["root-class"], vec![child]);

        let result = EguiElementFinder::find_element_by_class(&container, "nonexistent");
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_find_element_by_class_returns_first_match() {
        // Multiple elements have the same class; should return the first one found (DFS)
        let child1 = make_container(2, None, vec!["shared-class"], vec![]);
        let child2 = make_container(3, None, vec!["shared-class"], vec![]);
        let container = make_container(1, None, vec![], vec![child1, child2]);

        let result = EguiElementFinder::find_element_by_class(&container, "shared-class");
        assert!(result.is_some());
        // Should find the first child with the class (DFS order)
        assert_eq!(result.unwrap().id, 2);
    }

    #[test_log::test]
    fn test_find_element_by_class_matches_any_class_in_list() {
        let container = make_container(1, None, vec!["class-a", "class-b", "class-c"], vec![]);

        // All three classes should match
        assert!(EguiElementFinder::find_element_by_class(&container, "class-a").is_some());
        assert!(EguiElementFinder::find_element_by_class(&container, "class-b").is_some());
        assert!(EguiElementFinder::find_element_by_class(&container, "class-c").is_some());
    }

    // ==================== ElementFinder trait implementation tests ====================

    #[test_log::test]
    fn test_find_child_by_class_finds_child_of_specified_parent() {
        let grandchild = make_container(3, None, vec!["target"], vec![]);
        let child = make_container(2, None, vec!["wrapper"], vec![grandchild]);
        let container = make_container(1, None, vec![], vec![child]);

        let finder = EguiElementFinder::new(&container);

        // Find "target" class starting from parent with id=2
        let result = finder.find_child_by_class(2, "target");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 3);
    }

    #[test_log::test]
    fn test_find_child_by_class_returns_none_for_invalid_parent() {
        let child = make_container(2, None, vec!["target"], vec![]);
        let container = make_container(1, None, vec![], vec![child]);

        let finder = EguiElementFinder::new(&container);

        // Parent id 999 doesn't exist
        let result = finder.find_child_by_class(999, "target");
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_last_child_returns_last_child_id() {
        let child1 = make_container(2, None, vec![], vec![]);
        let child2 = make_container(3, None, vec![], vec![]);
        let child3 = make_container(4, None, vec![], vec![]);
        let container = make_container(1, None, vec![], vec![child1, child2, child3]);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_last_child(1);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), 4);
    }

    #[test_log::test]
    fn test_get_last_child_returns_none_for_childless_element() {
        let container = make_container(1, None, vec![], vec![]);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_last_child(1);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_data_attr_returns_attribute_value() {
        let mut container = make_container(1, None, vec![], vec![]);
        container
            .data
            .insert("key1".to_string(), "value1".to_string());
        container
            .data
            .insert("key2".to_string(), "value2".to_string());

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_data_attr(1, "key1");
        assert_eq!(result, Some("value1".to_string()));

        let result = finder.get_data_attr(1, "key2");
        assert_eq!(result, Some("value2".to_string()));
    }

    #[test_log::test]
    fn test_get_data_attr_returns_none_for_missing_key() {
        let mut container = make_container(1, None, vec![], vec![]);
        container
            .data
            .insert("key1".to_string(), "value1".to_string());

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_data_attr(1, "nonexistent");
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_str_id_returns_string_id() {
        let container = make_container(1, Some("my-element-id"), vec![], vec![]);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_str_id(1);
        assert_eq!(result, Some("my-element-id".to_string()));
    }

    #[test_log::test]
    fn test_get_str_id_returns_none_for_missing_str_id() {
        let container = make_container(1, None, vec![], vec![]);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_str_id(1);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_dimensions_returns_calculated_dimensions() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_width = Some(100.0);
        container.calculated_height = Some(50.0);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_dimensions(1);
        assert!(result.is_some());
        let (width, height) = result.unwrap();
        assert!((width - 100.0).abs() < f32::EPSILON);
        assert!((height - 50.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_dimensions_defaults_to_zero_when_not_calculated() {
        let container = make_container(1, None, vec![], vec![]);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_dimensions(1);
        assert!(result.is_some());
        let (width, height) = result.unwrap();
        assert!((width - 0.0).abs() < f32::EPSILON);
        assert!((height - 0.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_position_returns_calculated_position() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_x = Some(25.0);
        container.calculated_y = Some(75.0);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_position(1);
        assert!(result.is_some());
        let (x, y) = result.unwrap();
        assert!((x - 25.0).abs() < f32::EPSILON);
        assert!((y - 75.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_position_defaults_to_zero_when_not_calculated() {
        let container = make_container(1, None, vec![], vec![]);

        let finder = EguiElementFinder::new(&container);

        let result = finder.get_position(1);
        assert!(result.is_some());
        let (x, y) = result.unwrap();
        assert!((x - 0.0).abs() < f32::EPSILON);
        assert!((y - 0.0).abs() < f32::EPSILON);
    }

    // ==================== get_left_offset tests ====================

    #[test_log::test]
    fn test_get_left_offset_returns_none_when_zero() {
        let container = make_container(1, None, vec![], vec![]);
        // Both calculated_offset_x and calculated_margin_left are None (default to 0.0)
        let result = get_left_offset(&container);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_left_offset_returns_none_when_below_epsilon() {
        let mut container = make_container(1, None, vec![], vec![]);
        // Set values that sum to less than EPSILON (0.001)
        container.calculated_offset_x = Some(0.0005);
        container.calculated_margin_left = Some(0.0004);

        let result = get_left_offset(&container);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_left_offset_returns_value_when_above_epsilon() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_offset_x = Some(10.0);
        container.calculated_margin_left = Some(5.0);

        let result = get_left_offset(&container);
        assert!(result.is_some());
        assert!((result.unwrap() - 15.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_left_offset_with_only_offset_x() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_offset_x = Some(20.0);
        // calculated_margin_left is None

        let result = get_left_offset(&container);
        assert!(result.is_some());
        assert!((result.unwrap() - 20.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_left_offset_with_only_margin_left() {
        let mut container = make_container(1, None, vec![], vec![]);
        // calculated_offset_x is None
        container.calculated_margin_left = Some(8.0);

        let result = get_left_offset(&container);
        assert!(result.is_some());
        assert!((result.unwrap() - 8.0).abs() < f32::EPSILON);
    }

    // ==================== get_top_offset tests ====================

    #[test_log::test]
    fn test_get_top_offset_returns_none_when_zero() {
        let container = make_container(1, None, vec![], vec![]);
        let result = get_top_offset(&container);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_top_offset_returns_none_when_below_epsilon() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_offset_y = Some(0.0005);
        container.calculated_margin_top = Some(0.0004);

        let result = get_top_offset(&container);
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_get_top_offset_returns_value_when_above_epsilon() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_offset_y = Some(25.0);
        container.calculated_margin_top = Some(10.0);

        let result = get_top_offset(&container);
        assert!(result.is_some());
        assert!((result.unwrap() - 35.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_top_offset_with_only_offset_y() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_offset_y = Some(15.0);

        let result = get_top_offset(&container);
        assert!(result.is_some());
        assert!((result.unwrap() - 15.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_top_offset_with_only_margin_top() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_margin_top = Some(12.0);

        let result = get_top_offset(&container);
        assert!(result.is_some());
        assert!((result.unwrap() - 12.0).abs() < f32::EPSILON);
    }

    // ==================== get_remaining_offset_width tests ====================

    #[test_log::test]
    fn test_get_remaining_offset_width_returns_zero_when_no_dimensions() {
        let container = make_container(1, None, vec![], vec![]);
        let result = get_remaining_offset_width(&container);
        assert!((result - 0.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_remaining_offset_width_subtracts_margin() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_width = Some(100.0);
        container.calculated_margin_left = Some(20.0);
        // bounding_calculated_width = width + padding_x + scrollbar_right + margin_x
        // With only margin_left set, margin_x = 20.0
        // So bounding = 100 + 0 + 0 + 20 = 120
        // Result = 120 - 20 = 100

        let result = get_remaining_offset_width(&container);
        assert!((result - 100.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_remaining_offset_width_with_padding_and_margin() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_width = Some(200.0);
        container.calculated_padding_left = Some(10.0);
        container.calculated_padding_right = Some(10.0);
        container.calculated_margin_left = Some(15.0);
        container.calculated_margin_right = Some(5.0);
        // bounding = 200 + 20 (padding_x) + 0 (scrollbar) + 20 (margin_x) = 240
        // result = 240 - 15 = 225

        let result = get_remaining_offset_width(&container);
        assert!((result - 225.0).abs() < f32::EPSILON);
    }

    // ==================== get_remaining_offset_height tests ====================

    #[test_log::test]
    fn test_get_remaining_offset_height_returns_zero_when_no_dimensions() {
        let container = make_container(1, None, vec![], vec![]);
        let result = get_remaining_offset_height(&container);
        assert!((result - 0.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_remaining_offset_height_subtracts_margin() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_height = Some(150.0);
        container.calculated_margin_top = Some(25.0);
        // bounding_calculated_height = height + padding_y + scrollbar_bottom + margin_y
        // With only margin_top set, margin_y = 25.0
        // So bounding = 150 + 0 + 0 + 25 = 175
        // Result = 175 - 25 = 150

        let result = get_remaining_offset_height(&container);
        assert!((result - 150.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_get_remaining_offset_height_with_padding_and_margin() {
        let mut container = make_container(1, None, vec![], vec![]);
        container.calculated_height = Some(300.0);
        container.calculated_padding_top = Some(15.0);
        container.calculated_padding_bottom = Some(15.0);
        container.calculated_margin_top = Some(20.0);
        container.calculated_margin_bottom = Some(10.0);
        // bounding = 300 + 30 (padding_y) + 0 (scrollbar) + 30 (margin_y) = 360
        // result = 360 - 20 = 340

        let result = get_remaining_offset_height(&container);
        assert!((result - 340.0).abs() < f32::EPSILON);
    }

    // ==================== map_element_target tests ====================

    #[test_log::test]
    fn test_map_element_target_with_by_id_literal() {
        let target_child = make_container(3, Some("my-target"), vec![], vec![]);
        let child = make_container(2, None, vec![], vec![target_child]);
        let container = make_container(1, None, vec![], vec![child]);

        let target = ElementTarget::ById(Target::Literal("my-target".to_string()));
        let result = map_element_target(&target, 1, &container, |c| c.id);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), 3);
    }

    #[test_log::test]
    fn test_map_element_target_with_by_id_not_found() {
        let container = make_container(1, Some("root"), vec![], vec![]);

        let target = ElementTarget::ById(Target::Literal("nonexistent".to_string()));
        let result = map_element_target(&target, 1, &container, |c| c.id);

        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_map_element_target_with_by_id_ref_returns_none() {
        let container = make_container(1, Some("root"), vec![], vec![]);

        // Target::Ref is not supported, should return None
        let target = ElementTarget::ById(Target::Ref("root".to_string()));
        let result = map_element_target(&target, 1, &container, |c| c.id);

        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_map_element_target_with_class_literal() {
        let target_child = make_container(3, None, vec!["highlight"], vec![]);
        let child = make_container(2, None, vec![], vec![target_child]);
        let container = make_container(1, None, vec![], vec![child]);

        let target = ElementTarget::Class(Target::Literal("highlight".to_string()));
        let result = map_element_target(&target, 1, &container, |c| c.id);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), 3);
    }

    #[test_log::test]
    fn test_map_element_target_with_class_not_found() {
        let container = make_container(1, None, vec!["root-class"], vec![]);

        let target = ElementTarget::Class(Target::Literal("nonexistent".to_string()));
        let result = map_element_target(&target, 1, &container, |c| c.id);

        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_map_element_target_with_child_class_finds_child() {
        let grandchild = make_container(3, None, vec!["target"], vec![]);
        let child = make_container(2, None, vec![], vec![grandchild]);
        let container = make_container(1, None, vec![], vec![child]);

        // Looking for "target" class starting from self_id=2
        let target = ElementTarget::ChildClass(Target::Literal("target".to_string()));
        let result = map_element_target(&target, 2, &container, |c| c.id);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), 3);
    }

    #[test_log::test]
    fn test_map_element_target_with_child_class_invalid_parent() {
        let child = make_container(2, None, vec!["target"], vec![]);
        let container = make_container(1, None, vec![], vec![child]);

        // self_id=999 doesn't exist
        let target = ElementTarget::ChildClass(Target::Literal("target".to_string()));
        let result = map_element_target(&target, 999, &container, |c| c.id);

        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_map_element_target_with_self_target() {
        let child = make_container(2, None, vec![], vec![]);
        let container = make_container(1, None, vec![], vec![child]);

        let target = ElementTarget::SelfTarget;
        let result = map_element_target(&target, 2, &container, |c| c.id);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), 2);
    }

    #[test_log::test]
    fn test_map_element_target_with_self_target_not_found() {
        let container = make_container(1, None, vec![], vec![]);

        let target = ElementTarget::SelfTarget;
        let result = map_element_target(&target, 999, &container, |c| c.id);

        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_map_element_target_with_id_finds_self() {
        let child = make_container(2, None, vec![], vec![]);
        let container = make_container(1, None, vec![], vec![child]);

        // ElementTarget::Id uses self_id to find the element (ignoring the inner id)
        let target = ElementTarget::Id(42); // The inner id is ignored
        let result = map_element_target(&target, 2, &container, |c| c.id);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), 2);
    }

    #[test_log::test]
    fn test_map_element_target_with_last_child() {
        let grandchild1 = make_container(3, None, vec![], vec![]);
        let grandchild2 = make_container(4, None, vec![], vec![]);
        let grandchild3 = make_container(5, None, vec![], vec![]);
        let child = make_container(2, None, vec![], vec![grandchild1, grandchild2, grandchild3]);
        let container = make_container(1, None, vec![], vec![child]);

        let target = ElementTarget::LastChild;
        let result = map_element_target(&target, 2, &container, |c| c.id);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), 5); // Last child of container 2
    }

    #[test_log::test]
    fn test_map_element_target_with_last_child_no_children() {
        let child = make_container(2, None, vec![], vec![]); // No children
        let container = make_container(1, None, vec![], vec![child]);

        let target = ElementTarget::LastChild;
        let result = map_element_target(&target, 2, &container, |c| c.id);

        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_map_element_target_applies_callback_function() {
        let mut target_child =
            make_container(3, Some("target"), vec!["class-a", "class-b"], vec![]);
        target_child.calculated_width = Some(100.0);
        target_child.calculated_height = Some(50.0);
        let container = make_container(1, None, vec![], vec![target_child]);

        let target = ElementTarget::ById(Target::Literal("target".to_string()));

        // Test that callback function is properly applied
        let result = map_element_target(&target, 1, &container, |c| {
            (
                c.calculated_width.unwrap_or(0.0),
                c.calculated_height.unwrap_or(0.0),
            )
        });

        assert!(result.is_some());
        let (width, height) = result.unwrap();
        assert!((width - 100.0).abs() < f32::EPSILON);
        assert!((height - 50.0).abs() < f32::EPSILON);
    }
}
