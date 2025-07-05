#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, LazyLock, Mutex, RwLock},
    time::Instant,
};

use async_trait::async_trait;
use canvas::CanvasAction;
use eframe::egui::{self, Color32, CursorIcon, Response, Ui, Widget};
use flume::{Receiver, Sender};
use hyperchad_actions::{
    ActionEffect, ActionTrigger, ActionType, ElementTarget, StyleAction, Target, logic::Value,
};
use hyperchad_renderer::canvas::CanvasUpdate;
use hyperchad_renderer::viewport::immediate::{Pos, Viewport, ViewportListener};
use hyperchad_router::{ClientInfo, RequestInfo, Router};
use hyperchad_transformer::{
    Container, Element, Input, ResponsiveTrigger, TableIter, float_eq,
    models::{
        Cursor, LayoutDirection, LayoutOverflow, LayoutPosition, Position, Route, SwapTarget,
        Visibility,
    },
};
use itertools::Itertools;
use layout::EguiCalc;

pub mod font_metrics;
pub mod layout;

pub use eframe;
pub use hyperchad_renderer::*;

pub enum RenderView {
    View(Container),
    PartialView(PartialView),
}

#[cfg(feature = "debug")]
static DEBUG: LazyLock<RwLock<bool>> = LazyLock::new(|| {
    RwLock::new(
        std::env::var("DEBUG_RENDERER")
            .is_ok_and(|x| ["1", "true"].contains(&x.to_lowercase().as_str())),
    )
});

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
    #[must_use]
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
                request_action,
                on_resize,
                client_info,
                calculator,
            ),
            receiver: rx,
        }
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
    }
}

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
        ElementTarget::StrId(str_id) => {
            let Target::Literal(str_id) = str_id else {
                // FIXME
                return None;
            };
            if let Some(element) = container.find_element_by_str_id(str_id) {
                return Some(func(element));
            }

            log::warn!("Could not find element with str id '{str_id}'");
        }
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
            if let Some(container) = container.find_element_by_id(self_id) {
                if let Some(element) = container.find_element_by_class(class) {
                    return Some(func(element));
                }
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
                        | ElementTarget::StrId(..) => {
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

        moosicbox_task::spawn("render buffer", async move {
            while let Ok(Some(view)) = view_rx.recv_async().await {
                match view {
                    RenderView::View(x) => {
                        let _ = renderer
                            .render(View {
                                immediate: x,
                                future: None,
                            })
                            .await
                            .inspect_err(|e| log::error!("Failed to render: {e:?}"));
                    }
                    RenderView::PartialView(x) => {
                        let _ = renderer
                            .render_partial(x)
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
        moosicbox_task::spawn("renderer_egui::start: listen", {
            let app = self.app.clone();
            async move {
                log::debug!("start: listening");
                app.listen().await;
                Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
            }
        });

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

        moosicbox_task::spawn_blocking("handle_event", move || {
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
        if self.app.ctx.read().unwrap().is_none() {
            self.app
                .render_queue
                .write()
                .unwrap()
                .as_mut()
                .unwrap()
                .push_back(RenderView::View(view.immediate));
            return Ok(());
        }

        let app = self.app.clone();
        let width = self.width;
        let height = self.height;

        moosicbox_task::spawn_blocking("egui render", move || {
            moosicbox_logging::debug_or_trace!(
                ("render: start"),
                ("render: start {:?}", view.immediate)
            );
            let mut element = view.immediate;

            element.calculated_width = app.width.read().unwrap().or(width);
            element.calculated_height = app.height.read().unwrap().or(height);
            app.calculator.read().unwrap().calc(&mut element);
            moosicbox_assert::assert!(element.calculated_font_size.is_some());

            let mut watch_positions = app.watch_positions.write().unwrap();
            watch_positions.clear();
            add_watch_pos(&element, &element, &mut watch_positions);
            drop(watch_positions);

            *app.container.write().unwrap() = Some(element);
            app.images.write().unwrap().clear();
            app.backgrounds.write().unwrap().clear();
            app.visibilities.write().unwrap().clear();
            app.viewport_listeners.write().unwrap().clear();
            app.route_requests.write().unwrap().clear();
            app.checkboxes.write().unwrap().clear();
            app.positions.write().unwrap().clear();
            app.action_delay_off.write().unwrap().clear();
            app.immediate_elements_handled.write().unwrap().clear();

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
    /// Will error if egui fails to render the partial view.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render_partial(
        &self,
        view: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        if self.app.ctx.read().unwrap().is_none() {
            // FIXME: queue the views to process after `ctx` is loaded
            return Ok(());
        }

        let app = self.app.clone();

        moosicbox_task::spawn_blocking("egui render_partial", move || {
            moosicbox_logging::debug_or_trace!(
                ("render_partial: start"),
                ("render_partial: start {:?}", view)
            );

            let mut binding = app.container.write().unwrap();
            let Some(page) = binding.as_mut() else {
                return Ok(());
            };
            let ids = view
                .container
                .children
                .as_slice()
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>();

            let calculator = app.calculator.read().unwrap();

            if let Some(removed) = page.replace_str_id_with_elements_calc(
                &*calculator,
                view.container.children,
                &view.target,
            ) {
                drop(calculator);
                let mut watch_positions = app.watch_positions.write().unwrap();
                watch_positions.clear();
                add_watch_pos(page, page, &mut watch_positions);
                drop(watch_positions);

                let mut visibilities = app.visibilities.write().unwrap();
                if let Some(visibility) = visibilities.remove(&removed.id) {
                    for id in &ids {
                        visibilities.insert(*id, visibility.clone());
                    }
                }
                drop(visibilities);
                let mut backgrounds = app.backgrounds.write().unwrap();
                if let Some(background) = backgrounds.remove(&removed.id) {
                    for id in &ids {
                        backgrounds.insert(*id, background.clone());
                    }
                }
                drop(backgrounds);

                drop(binding);
                if let Some(ctx) = &*app.ctx.read().unwrap() {
                    ctx.request_repaint();
                }
            } else {
                drop(calculator);
                log::warn!("Unable to find element with id {}", view.target);
            }

            moosicbox_logging::debug_or_trace!(("render_partial: end"), ("render_partial: end"));

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

        moosicbox_task::spawn_blocking("egui render_canvas", move || {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StyleTrigger {
    UiEvent,
    CustomEvent,
}

#[derive(Debug, Clone)]
struct StyleOverride<T> {
    trigger: StyleTrigger,
    value: T,
}

struct RenderContext<'a> {
    container: &'a Container,
    viewport_listeners: &'a mut HashMap<usize, ViewportListener>,
    images: &'a mut HashMap<String, AppImage>,
    canvas_actions: &'a mut HashMap<String, Vec<CanvasAction>>,
    route_requests: &'a mut Vec<usize>,
    visibilities: &'a mut HashMap<usize, Vec<StyleOverride<Option<Visibility>>>>,
    displays: &'a mut HashMap<usize, bool>,
    backgrounds: &'a mut HashMap<usize, Vec<StyleOverride<Option<Color>>>>,
    checkboxes: &'a mut HashMap<egui::Id, bool>,
    positions: &'a mut HashMap<usize, egui::Rect>,
    watch_positions: &'a mut HashSet<usize>,
    action_delay_off: &'a mut HashMap<usize, (Instant, u64)>,
    action_throttle: &'a mut HashMap<usize, (Instant, u64)>,
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
    visibilities: Arc<RwLock<HashMap<usize, Vec<StyleOverride<Option<Visibility>>>>>>,
    displays: Arc<RwLock<HashMap<usize, bool>>>,
    backgrounds: Arc<RwLock<HashMap<usize, Vec<StyleOverride<Option<Color>>>>>>,
    checkboxes: Arc<RwLock<HashMap<egui::Id, bool>>>,
    positions: Arc<RwLock<HashMap<usize, egui::Rect>>>,
    watch_positions: Arc<RwLock<HashSet<usize>>>,
    action_delay_off: Arc<RwLock<HashMap<usize, (Instant, u64)>>>,
    action_throttle: Arc<RwLock<HashMap<usize, (Instant, u64)>>>,
    router: Router,
    background: Option<Color32>,
    title: Option<String>,
    description: Option<String>,
    request_action: Sender<(String, Option<Value>)>,
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

impl<C: EguiCalc + Clone + Send + Sync + 'static> EguiApp<C> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        router: Router,
        sender: Sender<String>,
        event: Sender<AppEvent>,
        event_receiver: Receiver<AppEvent>,
        request_action: Sender<(String, Option<Value>)>,
        on_resize: Sender<(f32, f32)>,
        client_info: Arc<ClientInfo>,
        calculator: C,
    ) -> Self {
        Self {
            ctx: Arc::new(RwLock::new(None)),
            calculator: Arc::new(RwLock::new(calculator)),
            render_queue: Arc::new(RwLock::new(Some(VecDeque::new()))),
            view_tx: Arc::new(RwLock::new(None)),
            render_buffer_rx: Arc::new(RwLock::new(None)),
            width: Arc::new(RwLock::new(None)),
            height: Arc::new(RwLock::new(None)),
            container: Arc::new(RwLock::new(None)),
            sender,
            event,
            event_receiver,
            viewport_listeners: Arc::new(RwLock::new(HashMap::new())),
            images: Arc::new(RwLock::new(HashMap::new())),
            canvas_actions: Arc::new(RwLock::new(HashMap::new())),
            route_requests: Arc::new(RwLock::new(vec![])),
            visibilities: Arc::new(RwLock::new(HashMap::new())),
            displays: Arc::new(RwLock::new(HashMap::new())),
            backgrounds: Arc::new(RwLock::new(HashMap::new())),
            checkboxes: Arc::new(RwLock::new(HashMap::new())),
            positions: Arc::new(RwLock::new(HashMap::new())),
            watch_positions: Arc::new(RwLock::new(HashSet::new())),
            action_delay_off: Arc::new(RwLock::new(HashMap::new())),
            action_throttle: Arc::new(RwLock::new(HashMap::new())),
            router,
            background: None,
            title: None,
            description: None,
            request_action,
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
        let mut visibilities = self.visibilities.write().unwrap();
        let mut displays = self.displays.write().unwrap();
        let mut backgrounds = self.backgrounds.write().unwrap();
        let mut checkboxes = self.checkboxes.write().unwrap();
        let mut positions = self.positions.write().unwrap();
        let mut watch_positions = self.watch_positions.write().unwrap();
        let mut action_delay_off = self.action_delay_off.write().unwrap();
        let mut action_throttle = self.action_throttle.write().unwrap();

        let mut render_context = RenderContext {
            container,
            viewport_listeners: &mut viewport_listeners,
            images: &mut images,
            canvas_actions: &mut canvas_actions,
            route_requests: &mut route_requests,
            visibilities: &mut visibilities,
            backgrounds: &mut backgrounds,
            displays: &mut displays,
            checkboxes: &mut checkboxes,
            positions: &mut positions,
            watch_positions: &mut watch_positions,
            action_delay_off: &mut action_delay_off,
            action_throttle: &mut action_throttle,
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
                        moosicbox_task::spawn("renderer: load_image", async move {
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
                                            log::error!("Failed to fetch image ({source}): {e:?}");
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to fetch image ({source}): {e:?}");
                                }
                            }
                        });
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
                    moosicbox_task::spawn("renderer: ProcessRoute", async move {
                        match route {
                            Route::Get {
                                route,
                                trigger,
                                swap,
                            }
                            | Route::Post {
                                route,
                                trigger,
                                swap,
                            }
                            | Route::Put {
                                route,
                                trigger,
                                swap,
                            }
                            | Route::Delete {
                                route,
                                trigger,
                                swap,
                            }
                            | Route::Patch {
                                route,
                                trigger,
                                swap,
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
                                                    Self::swap_elements(
                                                        &swap,
                                                        &ctx,
                                                        &container,
                                                        &calculator.read().unwrap(),
                                                        container_id,
                                                        view.immediate,
                                                    );
                                                    if let Some(future) = view.future {
                                                        let view = future.await;
                                                        Self::swap_elements(
                                                            &swap,
                                                            &ctx,
                                                            &container,
                                                            &calculator.read().unwrap(),
                                                            container_id,
                                                            view,
                                                        );
                                                    }
                                                }
                                                _ => {
                                                    unimplemented!();
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("Failed to process route ({route}): {e:?}");
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn swap_elements(
        swap: &SwapTarget,
        ctx: &egui::Context,
        container: &RwLock<Option<Container>>,
        calculator: &C,
        container_id: usize,
        result: Container,
    ) {
        log::debug!(
            "ProcessRoute: replacing container_id={container_id} with {} elements",
            result.children.len()
        );
        let mut binding = container.write().unwrap();
        let Some(page) = binding.as_mut() else {
            return;
        };
        match swap {
            SwapTarget::This => {
                if page.replace_id_with_elements_calc(calculator, result.children, container_id) {
                    drop(binding);
                    ctx.request_repaint();
                } else {
                    log::warn!("Unable to find element with id {container_id}");
                }
            }
            SwapTarget::Children => {
                if page.replace_id_children_with_elements_calc(
                    calculator,
                    result.children,
                    container_id,
                ) {
                    drop(binding);
                    ctx.request_repaint();
                } else {
                    log::warn!("Unable to find element with id {container_id}");
                }
            }
            SwapTarget::Id(id) => {
                if page.replace_str_id_children_with_elements_calc(calculator, result.children, id)
                {
                    drop(binding);
                    ctx.request_repaint();
                } else {
                    log::warn!("Unable to find element with id {container_id}");
                }
            }
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
        let (width, height) = ctx.input(move |i| (i.screen_rect.width(), i.screen_rect.height()));

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
        Self::get_container_style_override(container, render_context.visibilities)
            .copied()
            .flatten()
            .unwrap_or_else(|| container.visibility.unwrap_or_default())
            == Visibility::Hidden
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
                .allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                    inner(ui, relative_container)
                })
                .inner;
        }

        inner(ui, relative_container)
    }

    fn get_container_style_override<'a, T>(
        container: &'a Container,
        overrides: &'a HashMap<usize, Vec<StyleOverride<T>>>,
    ) -> Option<&'a T> {
        if let Some(overrides) = overrides.get(&container.id) {
            if let Some(StyleOverride { value, .. }) = overrides.last() {
                return Some(value);
            }
        }

        None
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

                if let Some(background) =
                    Self::get_container_style_override(container, render_context.backgrounds)
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
                            if let Some(mut pos) = pos {
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

                                pos.y += rect.height();

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

                                    pos.y += rect.height();
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

                                    pos.y += rect.height();
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

        if let Some(ui) = ui {
            if let Element::Image {
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
                        let loading_image = {
                            matches!(render_context.images.get(source), Some(AppImage::Loading))
                        };

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
                                source: source.to_string(),
                            }) {
                                log::error!("Failed to send LoadImage event: {e:?}");
                            }
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
                let request_action = self.request_action.clone();

                match fx_action.trigger {
                    ActionTrigger::Click | ActionTrigger::ClickOutside => {
                        #[cfg(feature = "profiling")]
                        profiling::scope!("click/clickOutside side effects");
                        let inside = matches!(fx_action.trigger, ActionTrigger::Click);
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let pointer = ctx.input(|x| x.pointer.clone());
                        let ctx = ctx.clone();
                        let responses = responses.clone();
                        let sender = self.sender.clone();
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
                                    &ctx,
                                    id,
                                    &sender,
                                    &request_action,
                                    None,
                                    None,
                                );
                                return !inside;
                            }

                            Self::unhandle_action(
                                &action.action,
                                Some(&action),
                                StyleTrigger::UiEvent,
                                render_context,
                                &ctx,
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
                        let ctx = ctx.clone();
                        let responses = responses.clone();
                        let sender = self.sender.clone();
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
                                    &ctx,
                                    id,
                                    &sender,
                                    &request_action,
                                    None,
                                    None,
                                );
                                return false;
                            }

                            Self::unhandle_action(
                                &action.action,
                                Some(&action),
                                StyleTrigger::UiEvent,
                                render_context,
                                &ctx,
                                id,
                            );

                            true
                        });
                    }
                    ActionTrigger::Hover => {
                        #[cfg(feature = "profiling")]
                        profiling::scope!("hover side effects");
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let responses = responses.clone();
                        let pointer = ctx.input(|x| x.pointer.clone());
                        let ctx = ctx.clone();
                        let sender = self.sender.clone();
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
                                    &ctx,
                                    id,
                                    &sender,
                                    &request_action,
                                    None,
                                    None,
                                );
                            }

                            Self::unhandle_action(
                                &action.action,
                                Some(&action),
                                StyleTrigger::UiEvent,
                                render_context,
                                &ctx,
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
                        let ctx = ctx.clone();
                        let sender = self.sender.clone();
                        self.trigger_side_effect(move |render_context| {
                            if !changed.is_empty() {
                                for value in &changed {
                                    log::trace!("change action: {action}");
                                    if !Self::handle_action(
                                        &action.action,
                                        Some(&action),
                                        StyleTrigger::UiEvent,
                                        render_context,
                                        &ctx,
                                        id,
                                        &sender,
                                        &request_action,
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
                                Some(&action),
                                StyleTrigger::UiEvent,
                                render_context,
                                &ctx,
                                id,
                            );

                            true
                        });
                    }
                    ActionTrigger::Resize => {
                        let request_action = self.request_action.clone();
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let ctx = self.ctx.read().unwrap().clone().unwrap();
                        let sender = self.sender.clone();
                        self.add_resize_handler(move |render_context| {
                            if !Self::handle_action(
                                &action.action,
                                Some(&action),
                                StyleTrigger::CustomEvent,
                                render_context,
                                &ctx,
                                id,
                                &sender,
                                &request_action,
                                None,
                                None,
                            ) {
                                return false;
                            }
                            true
                        });
                    }
                    ActionTrigger::Immediate => {
                        let request_action = self.request_action.clone();
                        let action = fx_action.effect.clone();
                        let id = container.id;
                        let ctx = self.ctx.read().unwrap().clone().unwrap();
                        let sender = self.sender.clone();
                        self.add_immediate_handler(id, move |render_context| {
                            if !Self::handle_action(
                                &action.action,
                                Some(&action),
                                StyleTrigger::CustomEvent,
                                render_context,
                                &ctx,
                                id,
                                &sender,
                                &request_action,
                                None,
                                None,
                            ) {
                                return false;
                            }
                            true
                        });
                    }
                    ActionTrigger::Event(..) => {}
                }
            }
        }
    }

    fn handle_custom_event_side_effects(&self, container: &Container) {
        for fx_action in &container.actions {
            if let ActionTrigger::Event(event_name) = &fx_action.trigger {
                let request_action = self.request_action.clone();
                let action = fx_action.effect.clone();
                let id = container.id;
                let ctx = self.ctx.read().unwrap().clone().unwrap();
                let sender = self.sender.clone();
                self.add_event_handler(event_name.to_string(), move |render_context, value| {
                    let effect = &action;
                    if let ActionType::Event { action, .. } = &action.action {
                        Self::handle_action(
                            action,
                            Some(effect),
                            StyleTrigger::CustomEvent,
                            render_context,
                            &ctx,
                            id,
                            &sender,
                            &request_action,
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
                        {
                            if let Some(href) = href.clone() {
                                if let Err(e) = sender.send(href) {
                                    log::error!("Failed to send href event: {e:?}");
                                }
                            }
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

    #[allow(clippy::too_many_lines)]
    fn calc_value(
        x: &Value,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        id: usize,
        event_value: Option<&str>,
    ) -> Option<Value> {
        use hyperchad_actions::logic::{CalcValue, Value};

        let calc_func = |calc_value: &CalcValue| match calc_value {
            CalcValue::Visibility { target } => Some(Value::Visibility(
                map_element_target(target, id, render_context.container, |element| {
                    Self::get_container_style_override(element, render_context.visibilities)
                        .copied()
                        .unwrap_or(element.visibility)
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
            )),
            CalcValue::Display { target } => Some(Value::Display(
                map_element_target(target, id, render_context.container, |element| {
                    render_context
                        .displays
                        .get(&element.id)
                        .copied()
                        .unwrap_or_else(|| element.hidden.unwrap_or_default())
                })
                .unwrap_or_default(),
            )),
            CalcValue::Id { target } => {
                map_element_target(target, id, render_context.container, |element| {
                    element.str_id.clone()
                })
                .flatten()
                .map(Value::String)
            }
            CalcValue::DataAttrValue { attr, target } => {
                map_element_target(target, id, render_context.container, |element| {
                    element.data.get(attr).cloned()
                })
                .flatten()
                .map(Value::String)
            }
            CalcValue::EventValue => event_value.map(ToString::to_string).map(Value::String),
            CalcValue::WidthPx { target } => {
                let width = map_element_target(target, id, render_context.container, |element| {
                    Value::Real(element.calculated_width.unwrap())
                });
                log::debug!("calc_value: getting width px for element id={id} width={width:?}");
                width
            }
            CalcValue::HeightPx { target } => {
                let height = map_element_target(target, id, render_context.container, |element| {
                    Value::Real(element.calculated_height.unwrap())
                });
                log::debug!("calc_value: getting height px for element id={id} height={height:?}");
                height
            }
            CalcValue::PositionX { target } | CalcValue::PositionY { target } => {
                let position =
                    map_element_target(target, id, render_context.container, |element| {
                        render_context.positions.get(&element.id).map(|rect| {
                            Value::Real(match calc_value {
                                CalcValue::PositionX { .. } => rect.min.x,
                                CalcValue::PositionY { .. } => rect.min.y,
                                _ => unreachable!(),
                            })
                        })
                    })
                    .flatten();
                log::debug!(
                    "calc_value: getting position for element id={id} position={position:?}"
                );
                position
            }
            CalcValue::MouseX { target } | CalcValue::MouseY { target } => {
                let pos = ctx.input(|x| {
                    x.pointer.latest_pos().map_or(0.0, |x| match calc_value {
                        CalcValue::MouseX { .. } => x.x,
                        CalcValue::MouseY { .. } => x.y,
                        _ => unreachable!(),
                    })
                });
                if let Some(target) = target {
                    let position =
                        map_element_target(target, id, render_context.container, |element| {
                            render_context.positions.get(&element.id).map(|rect| {
                                Value::Real(
                                    pos - match calc_value {
                                        CalcValue::MouseX { .. } => rect.min.x,
                                        CalcValue::MouseY { .. } => rect.min.y,
                                        _ => unreachable!(),
                                    },
                                )
                            })
                        })
                        .flatten();
                    log::debug!(
                        "calc_value: getting position for element id={id} position={position:?}"
                    );
                    position
                } else {
                    let global_position = Some(Value::Real(pos));
                    log::debug!("calc_value: got global_position={global_position:?}");
                    global_position
                }
            }
        };

        log::debug!("calc_value: calculating {x:?}");

        match x {
            Value::Calc(x) => calc_func(x),
            Value::Arithmetic(x) => x.as_f32(Some(&calc_func)).map(Value::Real),
            Value::Real(..)
            | Value::Visibility(..)
            | Value::Display(..)
            | Value::String(..)
            | Value::LayoutDirection(..) => Some(x.clone()),
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
        ctx: &egui::Context,
        id: usize,
        sender: &Sender<String>,
        request_action: &Sender<(String, Option<Value>)>,
        event_value: Option<&str>,
        value: Option<&Value>,
    ) -> bool {
        log::trace!("handle_action: action={action}");

        if let Some(ActionEffect {
            throttle: Some(..), ..
        }) = effect
        {
            if let Some((instant, throttle)) = render_context.action_throttle.get(&id) {
                let ms = Instant::now().duration_since(*instant).as_millis();
                if ms < u128::from(*throttle) {
                    log::debug!("handle_action: throttle={throttle} not past throttle yet ms={ms}");
                    ctx.request_repaint();
                    return true;
                }
            }
        }

        let response = match &action {
            ActionType::NoOp => true,
            ActionType::Input { .. } => todo!(),
            ActionType::Style { target, action } => {
                if let Some(ActionEffect {
                    delay_off: Some(delay),
                    ..
                }) = effect
                {
                    if let Some(id) =
                        Self::get_element_target_id(target, id, render_context.container)
                    {
                        render_context
                            .action_delay_off
                            .insert(id, (std::time::Instant::now(), *delay));
                    }
                }

                Self::handle_style_action(
                    action,
                    target,
                    trigger,
                    id,
                    render_context.container,
                    render_context.visibilities,
                    render_context.displays,
                    render_context.backgrounds,
                )
            }
            ActionType::Let { .. } => {
                // let value = Self::calc_value(value, render_context, ctx, id, event_value);
                // render_context.variables.insert(name.clone(), value);
                // true
                unimplemented!("let action");
            }
            ActionType::Navigate { url } => {
                if let Err(e) = sender.send(url.to_owned()) {
                    log::error!("Failed to navigate via action: {e:?}");
                }
                true
            }
            ActionType::Log { message, level } => {
                match level {
                    hyperchad_actions::LogLevel::Error => {
                        log::error!("{message}");
                    }
                    hyperchad_actions::LogLevel::Warn => {
                        log::warn!("{message}");
                    }
                    hyperchad_actions::LogLevel::Info => {
                        log::info!("{message}");
                    }
                    hyperchad_actions::LogLevel::Debug => {
                        log::debug!("{message}");
                    }
                    hyperchad_actions::LogLevel::Trace => {
                        log::trace!("{message}");
                    }
                }

                true
            }
            ActionType::Custom { action } => {
                if let Err(e) = request_action.send((action.clone(), value.cloned())) {
                    moosicbox_assert::die_or_error!("Failed to request action: {action} ({e:?})");
                }
                true
            }
            ActionType::Logic(eval) => {
                use hyperchad_actions::logic::Condition;

                let success = match &eval.condition {
                    Condition::Eq(a, b) => {
                        log::debug!("handle_action: checking eq a={a:?} b={b:?}");

                        let a = Self::calc_value(a, render_context, ctx, id, event_value);
                        let b = Self::calc_value(b, render_context, ctx, id, event_value);

                        log::debug!("handle_action: inner checking eq a={a:?} b={b:?}");

                        a == b
                    }
                    Condition::Bool(b) => *b,
                };

                log::debug!("handle_action: success={success}");

                if success {
                    for action in &eval.actions {
                        if !Self::handle_action(
                            &action.action,
                            Some(action),
                            trigger,
                            render_context,
                            ctx,
                            id,
                            sender,
                            request_action,
                            event_value,
                            value,
                        ) {
                            return false;
                        }
                    }
                } else {
                    for action in &eval.else_actions {
                        if !Self::handle_action(
                            &action.action,
                            Some(action),
                            trigger,
                            render_context,
                            ctx,
                            id,
                            sender,
                            request_action,
                            event_value,
                            value,
                        ) {
                            return false;
                        }
                    }
                }

                true
            }
            ActionType::Event { name, .. } => {
                log::trace!("handle_action: event '{name}' will be handled elsewhere");
                true
            }
            ActionType::Multi(actions) => {
                for action in actions {
                    if !Self::handle_action(
                        action,
                        effect,
                        trigger,
                        render_context,
                        ctx,
                        id,
                        sender,
                        request_action,
                        event_value,
                        value,
                    ) {
                        return false;
                    }
                }

                true
            }
            ActionType::MultiEffect(effects) => {
                for effect in effects {
                    if !Self::handle_action(
                        &effect.action,
                        Some(effect),
                        trigger,
                        render_context,
                        ctx,
                        id,
                        sender,
                        request_action,
                        event_value,
                        value,
                    ) {
                        return false;
                    }
                }

                true
            }
            ActionType::Parameterized { action, value } => {
                let value = Self::calc_value(value, render_context, ctx, id, event_value);
                Self::handle_action(
                    action,
                    effect,
                    trigger,
                    render_context,
                    ctx,
                    id,
                    sender,
                    request_action,
                    event_value,
                    value.as_ref(),
                )
            }
        };

        if let Some(ActionEffect {
            throttle: Some(throttle),
            ..
        }) = effect
        {
            log::debug!("handle_action: beginning action throttle with throttle={throttle}");
            render_context
                .action_throttle
                .insert(id, (std::time::Instant::now(), *throttle));
        }

        response
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::only_used_in_recursion)]
    fn unhandle_action(
        action: &ActionType,
        effect: Option<&ActionEffect>,
        trigger: StyleTrigger,
        render_context: &mut RenderContext,
        ctx: &egui::Context,
        id: usize,
    ) {
        render_context.action_throttle.remove(&id);

        match &action {
            ActionType::Style { target, action } => {
                if let Some(id) = Self::get_element_target_id(target, id, render_context.container)
                {
                    if let Some((instant, delay)) = render_context.action_delay_off.get(&id) {
                        let ms = Instant::now().duration_since(*instant).as_millis();
                        if ms < u128::from(*delay) {
                            log::debug!(
                                "unhandle_action: delay={delay} not past delay yet ms={ms}"
                            );
                            ctx.request_repaint();
                            return;
                        }
                    }
                }

                match action {
                    StyleAction::SetVisibility { .. } => {
                        if let Some(id) =
                            Self::get_element_target_id(target, id, render_context.container)
                        {
                            if let Some(overrides) = render_context.visibilities.get_mut(&id) {
                                overrides.retain(|x| x.trigger != trigger);

                                // TODO: don't delete the corresponding entry. just check for if it
                                // is empty in places this is read from.
                                if overrides.is_empty() {
                                    render_context.visibilities.remove(&id);
                                }
                            }
                        }
                    }
                    StyleAction::SetDisplay { .. } => {
                        if let Some(id) =
                            Self::get_element_target_id(target, id, render_context.container)
                        {
                            if render_context.displays.contains_key(&id) {
                                render_context.displays.remove(&id);
                            }
                        }
                    }
                    StyleAction::SetBackground(..) => {
                        if let Some(id) =
                            Self::get_element_target_id(target, id, render_context.container)
                        {
                            if let Some(overrides) = render_context.backgrounds.get_mut(&id) {
                                overrides.retain(|x| x.trigger != trigger);

                                // TODO: don't delete the corresponding entry. just check for if it
                                // is empty in places this is read from.
                                if overrides.is_empty() {
                                    render_context.backgrounds.remove(&id);
                                }
                            }
                        }
                    }
                    StyleAction::SetFocus(..) => {
                        // FIXME
                        // let mut overrides = render_context.focus_overrides.get_mut(&id).unwrap();
                        // overrides.push(FocusOverride {
                        //     trigger: trigger.clone(),
                        //     focus: *focus,
                        // });
                    }
                }
            }
            ActionType::Multi(actions) => {
                for action in actions {
                    Self::unhandle_action(action, effect, trigger, render_context, ctx, id);
                }
            }
            ActionType::MultiEffect(effects) => {
                for effect in effects {
                    Self::unhandle_action(
                        &effect.action,
                        Some(effect),
                        trigger,
                        render_context,
                        ctx,
                        id,
                    );
                }
            }
            ActionType::Parameterized { action, .. } => {
                Self::unhandle_action(action, effect, trigger, render_context, ctx, id);
            }
            ActionType::Input(..) // FIXME
            | ActionType::NoOp
            | ActionType::Navigate { .. }
            | ActionType::Log { .. }
            | ActionType::Custom { .. }
            | ActionType::Event { .. }
            | ActionType::Let { .. }
            | ActionType::Logic(..) => {}
        }
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    fn get_element_target_id(
        target: &ElementTarget,
        self_id: usize,
        container: &Container,
    ) -> Option<usize> {
        match target {
            ElementTarget::StrId(str_id) => {
                let Target::Literal(str_id) = str_id else {
                    // FIXME
                    return None;
                };
                if let Some(element) = container.find_element_by_str_id(str_id) {
                    return Some(element.id);
                }

                log::warn!("Could not find element with str id '{str_id}'");
            }
            ElementTarget::Class(class) => {
                let Target::Literal(class) = class else {
                    // FIXME
                    return None;
                };
                if let Some(element) = container.find_element_by_class(class) {
                    return Some(element.id);
                }

                log::warn!("Could not find element with class '{class}'");
            }
            ElementTarget::ChildClass(class) => {
                let Target::Literal(class) = class else {
                    // FIXME
                    return None;
                };
                if let Some(container) = container.find_element_by_id(self_id) {
                    if let Some(element) = container.find_element_by_class(class) {
                        return Some(element.id);
                    }
                }

                log::warn!("Could not find element with class '{class}'");
            }
            ElementTarget::Id(id) => {
                return Some(*id);
            }
            ElementTarget::SelfTarget => {
                return Some(self_id);
            }
            ElementTarget::LastChild => {
                if let Some(element) = container
                    .find_element_by_id(self_id)
                    .and_then(|x| x.children.iter().last())
                {
                    return Some(element.id);
                }

                log::warn!("Could not find element last child for id '{self_id}'");
            }
        }

        None
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_arguments)]
    fn handle_style_action(
        action: &StyleAction,
        target: &ElementTarget,
        trigger: StyleTrigger,
        id: usize,
        container: &Container,
        visibilities: &mut HashMap<usize, Vec<StyleOverride<Option<Visibility>>>>,
        displays: &mut HashMap<usize, bool>,
        backgrounds: &mut HashMap<usize, Vec<StyleOverride<Option<Color>>>>,
    ) -> bool {
        match action {
            StyleAction::SetVisibility(visibility) => {
                if let Some(id) = Self::get_element_target_id(target, id, container) {
                    log::trace!(
                        "handle_style_action: set visibility id={id} visibility={visibility} trigger={trigger:?}"
                    );
                    let style_override = StyleOverride {
                        trigger,
                        value: Some(*visibility),
                    };
                    match visibilities.entry(id) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            entry.get_mut().push(style_override);
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(vec![style_override]);
                        }
                    }
                }

                true
            }
            StyleAction::SetDisplay(display) => {
                if let Some(id) = Self::get_element_target_id(target, id, container) {
                    displays.insert(id, *display);
                }

                true
            }
            StyleAction::SetFocus(..) => {
                // if let Some(id) = Self::get_element_target_id(target, id, container) {
                //     focuses.insert(id, *focus);
                // }

                todo!()
            }
            StyleAction::SetBackground(background) => {
                if let Some(id) = Self::get_element_target_id(target, id, container) {
                    if let Some(background) = background {
                        match Color::try_from_hex(background) {
                            Ok(color) => {
                                log::trace!(
                                    "handle_style_action: set background color id={id} color={color} trigger={trigger:?}"
                                );
                                let style_override = StyleOverride {
                                    trigger,
                                    value: Some(color),
                                };
                                match backgrounds.entry(id) {
                                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                                        entry.get_mut().push(style_override);
                                    }
                                    std::collections::hash_map::Entry::Vacant(entry) => {
                                        entry.insert(vec![style_override]);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("handle_style_action: invalid background color: {e:?}");
                            }
                        }
                    } else {
                        log::trace!(
                            "handle_style_action: remove background color id={id} trigger={trigger:?}"
                        );
                        let style_override = StyleOverride {
                            trigger,
                            value: None,
                        };
                        match backgrounds.entry(id) {
                            std::collections::hash_map::Entry::Occupied(mut entry) => {
                                entry.get_mut().push(style_override);
                            }
                            std::collections::hash_map::Entry::Vacant(entry) => {
                                entry.insert(vec![style_override]);
                            }
                        }
                    }
                }

                true
            }
        }
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
            Element::Raw { value } => Some(
                ui.label(
                    egui::RichText::new(value).size(
                        element
                            .calculated_font_size
                            .expect("Missing calculated_font_size"),
                    ),
                ),
            ),
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
        backgrounds: &HashMap<usize, Vec<StyleOverride<Option<Color>>>>,
    ) -> egui::Frame {
        if let Some(background) = Self::get_container_style_override(container, backgrounds)
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
                                render_context.backgrounds,
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
                                render_context.backgrounds,
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
        let mut visibilities = self.visibilities.write().unwrap();
        let mut backgrounds = self.backgrounds.write().unwrap();
        let mut displays = self.displays.write().unwrap();
        let mut checkboxes = self.checkboxes.write().unwrap();
        let mut positions = self.positions.write().unwrap();
        let mut watch_positions = self.watch_positions.write().unwrap();
        let mut action_delay_off = self.action_delay_off.write().unwrap();
        let mut action_throttle = self.action_throttle.write().unwrap();

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
            let mut render_context = RenderContext {
                container,
                viewport_listeners: &mut viewport_listeners,
                images: &mut images,
                canvas_actions: &mut canvas_actions,
                route_requests: &mut route_requests,
                visibilities: &mut visibilities,
                backgrounds: &mut backgrounds,
                displays: &mut displays,
                checkboxes: &mut checkboxes,
                positions: &mut positions,
                watch_positions: &mut watch_positions,
                action_delay_off: &mut action_delay_off,
                action_throttle: &mut action_throttle,
            };

            if resized {
                for handler in self.resize_handlers.write().unwrap().drain(..) {
                    handler(&mut render_context);
                }
            } else {
                self.resize_handlers.write().unwrap().clear();
            }

            ctx.memory_mut(|x| {
                x.options.line_scroll_speed = 100.0;
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
        drop(visibilities);
        drop(displays);
        drop(checkboxes);
        drop(positions);
        drop(watch_positions);
        drop(action_delay_off);
        drop(action_throttle);

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
