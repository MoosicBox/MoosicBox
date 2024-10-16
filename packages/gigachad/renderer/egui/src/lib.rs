#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use eframe::egui::{self, Response, Ui, Widget};
use flume::{Receiver, Sender};
use gigachad_renderer::viewport::immediate::{Pos, Viewport, ViewportListener};
pub use gigachad_renderer::*;
use gigachad_transformer::{calc::Calc, ContainerElement, Element, LayoutDirection};
use itertools::Itertools;

#[derive(Clone)]
pub struct EguiRenderer {
    width: Option<u16>,
    height: Option<u16>,
    x: Option<i32>,
    y: Option<i32>,
    app: EguiApp,
    receiver: Receiver<String>,
}

impl Default for EguiRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl EguiRenderer {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded();
        let (event_tx, event_rx) = flume::unbounded();
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            app: EguiApp::new(tx, event_tx, event_rx),
            receiver: rx,
        }
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
    }
}

pub struct EguiRenderRunner {
    width: u16,
    height: u16,
    x: Option<i32>,
    y: Option<i32>,
    app: EguiApp,
}

impl RenderRunner for EguiRenderRunner {
    /// # Errors
    ///
    /// Will error if egui fails to run the event loop.
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        let mut viewport = egui::ViewportBuilder::default()
            .with_inner_size([f32::from(self.width), f32::from(self.height)]);

        #[allow(clippy::cast_precision_loss)]
        if let (Some(x), Some(y)) = (self.x, self.y) {
            viewport = viewport.with_position((x as f32, y as f32));
        }

        let options = eframe::NativeOptions {
            viewport,
            centered: true,
            #[cfg(feature = "wgpu")]
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        };

        log::debug!("run: starting");
        if let Err(e) = eframe::run_native(
            "MoosicBox",
            options,
            Box::new(|cc| {
                egui_extras::install_image_loaders(&cc.egui_ctx);
                let app = self.app.clone();
                *app.ctx.write().unwrap() = Some(cc.egui_ctx.clone());
                Ok(Box::new(app))
            }),
        ) {
            log::error!("run: eframe error: {e:?}");
        }
        log::debug!("run: finished");

        Ok(())
    }
}

#[async_trait]
impl Renderer for EguiRenderer {
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    ///
    /// # Errors
    ///
    /// Will error if egui app fails to start
    async fn init(
        &mut self,
        width: u16,
        height: u16,
        x: Option<i32>,
        y: Option<i32>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.width = Some(width);
        self.height = Some(height);
        self.x = x;
        self.y = y;

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
    /// Will error if egui fails to run the event loop.
    async fn to_runner(
        &mut self,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(EguiRenderRunner {
            width: self.width.unwrap(),
            height: self.height.unwrap(),
            x: self.x,
            y: self.y,
            app: self.app.clone(),
        }))
    }

    /// # Errors
    ///
    /// Will error if egui fails to render the elements.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    fn render(
        &mut self,
        mut elements: ContainerElement,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(("render: start"), ("render: start {elements:?}"));

        elements.calculated_width = self.app.width.read().unwrap().or(self.width.map(f32::from));
        elements.calculated_height = self
            .app
            .height
            .read()
            .unwrap()
            .or(self.height.map(f32::from));
        elements.calc();
        *self.app.container.write().unwrap() = elements;
        *self.app.viewport_listeners.write().unwrap() = HashMap::new();
        *self.app.viewports.write().unwrap() = HashMap::new();

        log::debug!("render: finished");
        if let Some(ctx) = &*self.app.ctx.read().unwrap() {
            ctx.request_repaint();
        }

        Ok(())
    }
}

#[derive(Debug)]
enum AppEvent {
    LoadImage { source: String },
}

#[derive(Clone)]
enum AppImage {
    Loading,
    Bytes(Arc<[u8]>),
}

#[derive(Clone)]
struct EguiApp {
    ctx: Arc<RwLock<Option<egui::Context>>>,
    width: Arc<RwLock<Option<f32>>>,
    height: Arc<RwLock<Option<f32>>>,
    container: Arc<RwLock<ContainerElement>>,
    sender: Sender<String>,
    event: Sender<AppEvent>,
    event_receiver: Receiver<AppEvent>,
    viewport_listeners: Arc<RwLock<HashMap<usize, ViewportListener>>>,
    viewports: Arc<RwLock<HashMap<usize, Viewport>>>,
    images: Arc<RwLock<HashMap<String, AppImage>>>,
}

type Handler = Box<dyn Fn(&Response)>;

impl EguiApp {
    fn new(
        sender: Sender<String>,
        event: Sender<AppEvent>,
        event_receiver: Receiver<AppEvent>,
    ) -> Self {
        Self {
            ctx: Arc::new(RwLock::new(None)),
            width: Arc::new(RwLock::new(None)),
            height: Arc::new(RwLock::new(None)),
            container: Arc::new(RwLock::new(ContainerElement::default())),
            sender,
            event,
            event_receiver,
            viewport_listeners: Arc::new(RwLock::new(HashMap::new())),
            viewports: Arc::new(RwLock::new(HashMap::new())),
            images: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn listen(&self) {
        while let Ok(event) = self.event_receiver.recv_async().await {
            log::debug!("received event {event:?}");
            match event {
                AppEvent::LoadImage { source } => {
                    let images = self.images.clone();
                    let ctx = self.ctx.clone();
                    moosicbox_task::spawn("renderer: load_image", async move {
                        log::debug!("loading image {source}");
                        match reqwest::get(&source).await {
                            Ok(response) => {
                                if !response.status().is_success() {
                                    return;
                                }

                                match response.bytes().await {
                                    Ok(bytes) => {
                                        images
                                            .write()
                                            .unwrap()
                                            .insert(source, AppImage::Bytes(bytes.to_vec().into()));

                                        if let Some(ctx) = &*ctx.read().unwrap() {
                                            ctx.request_repaint();
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Failed to fetch image: {e:?}");
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to fetch image: {e:?}");
                            }
                        }
                    });
                }
            }
        }
    }

    fn calc(&self, ctx: &egui::Context) {
        ctx.input(move |i| {
            let width = i.screen_rect.width();
            let height = i.screen_rect.height();
            let current_width = *self.width.read().unwrap();
            let current_height = *self.height.read().unwrap();
            if !current_width.is_some_and(|x| (x - width).abs() < 0.01)
                || !current_height.is_some_and(|x| (x - height).abs() < 0.01)
            {
                *self.viewport_listeners.write().unwrap() = HashMap::new();
                *self.viewports.write().unwrap() = HashMap::new();

                log::debug!(
                    "calc: frame size changed from ({:?}, {:?}) -> ({width}, {height})",
                    self.width,
                    self.height
                );

                {
                    let mut container = self.container.write().unwrap();
                    container.calculated_width.replace(width);
                    container.calculated_height.replace(height);
                    container.calc();
                }

                self.width.write().unwrap().replace(width);
                self.height.write().unwrap().replace(height);
            }
        });
    }

    fn get_scroll_container(
        &self,
        ui: &Ui,
        element: &ContainerElement,
        parent: Option<&Viewport>,
    ) -> Viewport {
        let pos = ui.cursor();

        let mut viewport = self
            .viewports
            .write()
            .unwrap()
            .get(&element.id)
            .map_or_else(
                || Viewport {
                    parent: parent.cloned().map(Box::new),
                    pos: Pos {
                        x: 0.0,
                        y: 0.0,
                        w: element.calculated_width.unwrap(),
                        h: element.calculated_height.unwrap(),
                    },
                    viewport: Pos {
                        x: 0.0,
                        y: 0.0,
                        w: element.calculated_width.unwrap(),
                        h: element.calculated_height.unwrap(),
                    },
                },
                Clone::clone,
            );

        viewport.pos.x = pos.left();
        viewport.pos.y = pos.top();
        log::trace!(
            "get_scroll_container: ({}, {})",
            viewport.pos.x,
            viewport.pos.y
        );

        viewport
    }

    fn update_scroll_container(
        &self,
        element: &ContainerElement,
        viewport: Viewport,
        state: egui::scroll_area::State,
    ) {
        let mut binding = self.viewports.write().unwrap();
        let viewport = binding.entry(element.id).or_insert_with(move || viewport);

        log::trace!(
            "update_scroll_container: ({}, {})",
            state.offset.x,
            state.offset.y
        );
        viewport.viewport.x = state.offset.x;
        viewport.viewport.y = state.offset.y;
        drop(binding);
    }

    #[allow(clippy::too_many_lines)]
    fn render_container(
        &self,
        ctx: &egui::Context,
        ui: &mut Ui,
        container: &ContainerElement,
        handler: Option<&Handler>,
        viewport: Option<&Viewport>,
    ) {
        let response = egui::Frame::none().show(ui, move |ui| {
            match (container.overflow_x, container.overflow_y) {
                (
                    gigachad_transformer::LayoutOverflow::Auto,
                    gigachad_transformer::LayoutOverflow::Auto,
                ) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::both()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                self.render_container_contents(
                                    ctx, ui, container, handler, viewport, true,
                                );
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (
                    gigachad_transformer::LayoutOverflow::Scroll,
                    gigachad_transformer::LayoutOverflow::Scroll,
                ) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::both()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                self.render_container_contents(
                                    ctx, ui, container, handler, viewport, true,
                                );
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (
                    gigachad_transformer::LayoutOverflow::Auto,
                    gigachad_transformer::LayoutOverflow::Scroll,
                ) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::vertical()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                let viewport = self.get_scroll_container(ui, container, viewport);
                                let state = egui::ScrollArea::horizontal()
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                    )
                                    .show(ui, {
                                        let viewport = Some(&viewport);
                                        move |ui| {
                                            self.render_container_contents(
                                                ctx, ui, container, handler, viewport, true,
                                            );
                                        }
                                    })
                                    .state;
                                self.update_scroll_container(container, viewport, state);
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (
                    gigachad_transformer::LayoutOverflow::Scroll,
                    gigachad_transformer::LayoutOverflow::Auto,
                ) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::vertical()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                let viewport = self.get_scroll_container(ui, container, viewport);
                                let state = egui::ScrollArea::horizontal()
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                    )
                                    .show(ui, {
                                        let viewport = Some(&viewport);
                                        move |ui| {
                                            self.render_container_contents(
                                                ctx, ui, container, handler, viewport, true,
                                            );
                                        }
                                    })
                                    .state;
                                self.update_scroll_container(container, viewport, state);
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (gigachad_transformer::LayoutOverflow::Auto, _) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::horizontal()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                self.render_container_contents(
                                    ctx, ui, container, handler, viewport, false,
                                );
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (gigachad_transformer::LayoutOverflow::Scroll, _) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::horizontal()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                self.render_container_contents(
                                    ctx, ui, container, handler, viewport, false,
                                );
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (_, gigachad_transformer::LayoutOverflow::Auto) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::vertical()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                self.render_container_contents(
                                    ctx, ui, container, handler, viewport, true,
                                );
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (_, gigachad_transformer::LayoutOverflow::Scroll) => {
                    let viewport = self.get_scroll_container(ui, container, viewport);
                    let state = egui::ScrollArea::vertical()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, {
                            let viewport = Some(&viewport);
                            move |ui| {
                                self.render_container_contents(
                                    ctx, ui, container, handler, viewport, true,
                                );
                            }
                        })
                        .state;
                    self.update_scroll_container(container, viewport, state);
                }
                (_, _) => {
                    self.render_container_contents(ctx, ui, container, handler, viewport, false);
                }
            }
        });

        if let Some(handler) = handler {
            handler(&response.response);
        }
    }

    fn render_container_contents(
        &self,
        ctx: &egui::Context,
        ui: &mut Ui,
        container: &ContainerElement,
        handler: Option<&Handler>,
        viewport: Option<&Viewport>,
        vscroll: bool,
    ) {
        if let Some(width) = container.calculated_width {
            ui.set_width(width);
        }
        if let Some(height) = container.calculated_height {
            ui.set_height(height);

            if vscroll {
                if ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
                    let rect = egui::Rect::from_pos(egui::emath::pos2(0.0, height));
                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                }
                if ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
                    let rect = egui::Rect::from_pos(egui::emath::pos2(0.0, -height));
                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                }
            }
        }
        match container.direction {
            LayoutDirection::Row => {
                let rows = container
                    .elements
                    .iter()
                    .filter_map(|x| x.container_element().map(|y| (x, y)))
                    .filter_map(|(x, y)| y.calculated_position.as_ref().map(|y| (x, y)))
                    .filter_map(|(x, y)| match y {
                        gigachad_transformer::LayoutPosition::Wrap { row, .. } => Some((*row, x)),
                        gigachad_transformer::LayoutPosition::Default => None,
                    })
                    .chunk_by(|(row, _element)| *row);

                let mut rows = rows
                    .into_iter()
                    .map(|(_row, y)| y.into_iter().map(|(_, element)| element).collect_vec())
                    .peekable();

                if rows.peek().is_some() {
                    ui.vertical(move |ui| {
                        for row in rows {
                            ui.horizontal(move |ui| {
                                self.render_elements_ref(ctx, ui, &row, handler, viewport);
                            });
                        }
                    });
                } else {
                    ui.horizontal(move |ui| {
                        self.render_elements(ctx, ui, &container.elements, handler, viewport);
                    });
                }
            }
            LayoutDirection::Column => {
                let cols = container
                    .elements
                    .iter()
                    .filter_map(|x| x.container_element().map(|y| (x, y)))
                    .filter_map(|(x, y)| y.calculated_position.as_ref().map(|y| (x, y)))
                    .filter_map(|(x, y)| match y {
                        gigachad_transformer::LayoutPosition::Wrap { col, .. } => Some((*col, x)),
                        gigachad_transformer::LayoutPosition::Default => None,
                    })
                    .chunk_by(|(col, _element)| *col);

                let mut cols = cols
                    .into_iter()
                    .map(|(_row, y)| y.into_iter().map(|(_, element)| element).collect_vec())
                    .peekable();

                if cols.peek().is_some() {
                    ui.horizontal(move |ui| {
                        for col in cols {
                            ui.vertical(move |ui| {
                                self.render_elements_ref(ctx, ui, &col, handler, viewport);
                            });
                        }
                    });
                } else {
                    ui.vertical(move |ui| {
                        self.render_elements(ctx, ui, &container.elements, handler, viewport);
                    });
                }
            }
        }
    }

    fn render_elements(
        &self,
        ctx: &egui::Context,
        ui: &mut Ui,
        elements: &[Element],
        handler: Option<&Handler>,
        viewport: Option<&Viewport>,
    ) {
        for element in elements {
            self.render_element(ctx, ui, element, handler, viewport);
        }
    }

    fn render_elements_ref(
        &self,
        ctx: &egui::Context,
        ui: &mut Ui,
        elements: &[&Element],
        handler: Option<&Handler>,
        viewport: Option<&Viewport>,
    ) {
        for element in elements {
            self.render_element(ctx, ui, element, handler, viewport);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render_element(
        &self,
        ctx: &egui::Context,
        ui: &mut Ui,
        element: &Element,
        handler: Option<&Handler>,
        viewport: Option<&Viewport>,
    ) {
        let response: Option<Response> = match element {
            Element::Raw { value } => Some(ui.label(value)),
            Element::Image { source, element } => source.clone().map(|source| {
                let listeners: &mut HashMap<_, _> = &mut self.viewport_listeners.write().unwrap();

                let pos = ui.cursor();
                let listener = listeners.entry(element.id).or_insert_with(|| {
                    ViewportListener::new(
                        viewport.cloned(),
                        0.0,
                        0.0,
                        element.calculated_width.unwrap(),
                        element.calculated_height.unwrap(),
                    )
                });
                listener.pos.x = pos.left();
                listener.pos.y = pos.top();

                let (_, (dist, prev_dist)) = listener.check();

                let image = if !prev_dist.is_some_and(|x| x < 1000.0) && dist < 1000.0 {
                    let contains_image = {
                        matches!(
                            self.images.read().unwrap().get(&source),
                            Some(AppImage::Bytes(_))
                        )
                    };
                    if contains_image {
                        let Some(AppImage::Bytes(bytes)) =
                            self.images.read().unwrap().get(&source).cloned()
                        else {
                            unreachable!()
                        };
                        let mut image =
                            egui::Image::from_bytes(source, egui::load::Bytes::Shared(bytes));

                        if element.width.is_some() {
                            image = image.max_width(element.calculated_width.unwrap());
                        }
                        if element.height.is_some() {
                            image = image.max_height(element.calculated_height.unwrap());
                        }

                        Some(image.ui(ui))
                    } else {
                        let loading_image = {
                            matches!(
                                self.images.read().unwrap().get(&source),
                                Some(AppImage::Loading)
                            )
                        };

                        if !loading_image {
                            self.images
                                .write()
                                .unwrap()
                                .insert(source.clone(), AppImage::Loading);

                            if let Err(e) = self.event.send(AppEvent::LoadImage { source }) {
                                log::error!("Failed to send LoadImage event: {e:?}");
                            }
                        }

                        None
                    }
                } else {
                    None
                };

                image.unwrap_or_else(|| {
                    let frame = egui::Frame::none();
                    frame
                        .show(ui, |ui| {
                            if element.width.is_some() {
                                ui.set_width(element.calculated_width.unwrap());
                            }
                            if element.height.is_some() {
                                ui.set_height(element.calculated_height.unwrap());
                            }
                        })
                        .response
                })
            }),
            _ => None,
        };

        if let Some(response) = response {
            if let Some(handler) = handler {
                handler(&response);
            }
            return;
        }

        let immediate_handler: Option<Handler> = match element {
            Element::Button { .. } => Some(Box::new(|response| {
                if response.interact(egui::Sense::click()).clicked() {
                    log::debug!("clicked button!");
                }
            })),
            Element::Anchor { href, .. } => {
                let href = href.to_owned();
                let sender = self.sender.clone();
                Some(Box::new(move |response| {
                    if response.interact(egui::Sense::click()).clicked() {
                        if let Some(href) = href.clone() {
                            if let Err(e) = sender.send(href) {
                                log::error!("Failed to send href event: {e:?}");
                            }
                        }
                    }
                }))
            }
            _ => None,
        };

        if let Some(container) = element.container_element() {
            self.render_container(
                ctx,
                ui,
                container,
                immediate_handler.as_ref().or(handler),
                viewport,
            );
        }
    }

    fn paint(&self, ctx: &egui::Context) {
        self.calc(ctx);

        let container = self.container.clone();
        let container: &ContainerElement = &container.read().unwrap();

        egui::CentralPanel::default().show(ctx, |ui| {
            let style = ui.style_mut();
            style.spacing.window_margin.left = 0.0;
            style.spacing.window_margin.right = 0.0;
            style.spacing.window_margin.top = 0.0;
            style.spacing.window_margin.bottom = 0.0;
            style.spacing.item_spacing = egui::emath::Vec2::splat(0.0);
            #[cfg(feature = "debug")]
            {
                style.debug.debug_on_hover = true;
            }
            self.render_container(ctx, ui, container, None, None);
        });
    }
}

impl eframe::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.paint(ctx);
    }
}
