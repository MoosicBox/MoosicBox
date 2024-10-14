#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use eframe::egui::{self, Response, Ui};
use flume::{Receiver, Sender};
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
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            app: EguiApp::new(tx),
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
            ..Default::default()
        };

        log::debug!("run: starting");
        if let Err(e) = eframe::run_native(
            "MoosicBox",
            options,
            Box::new(|_cc| Ok(Box::new(self.app.clone()))),
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
        log::debug!("render: {elements:?}");

        elements.calculated_width = self.width.map(f32::from);
        elements.calculated_height = self.height.map(f32::from);
        elements.calc();
        *self.app.container.write().unwrap() = elements;

        Ok(())
    }
}

#[derive(Clone)]
struct EguiApp {
    width: Option<f32>,
    height: Option<f32>,
    container: Arc<RwLock<ContainerElement>>,
    sender: Sender<String>,
}

type Handler = Box<dyn Fn(&Response)>;

impl EguiApp {
    fn new(sender: Sender<String>) -> Self {
        Self {
            width: None,
            height: None,
            container: Arc::new(RwLock::new(ContainerElement::default())),
            sender,
        }
    }
    fn calc(&mut self, ctx: &egui::Context) {
        ctx.input(move |i| {
            let width = i.screen_rect.width();
            let height = i.screen_rect.height();
            if !self.width.is_some_and(|x| (x - width).abs() < 0.01)
                || !self.height.is_some_and(|x| (x - height).abs() < 0.01)
            {
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

                self.width.replace(width);
                self.height.replace(height);
            }
        });
    }

    fn render_container(
        &self,
        ui: &mut Ui,
        container: &ContainerElement,
        handler: Option<&Handler>,
    ) {
        egui::Frame::none().show(ui, move |ui| {
            if let Some(width) = container.calculated_width {
                ui.set_width(width);
            }
            if let Some(height) = container.calculated_height {
                ui.set_height(height);
            }
            match container.direction {
                LayoutDirection::Row => {
                    let rows = container
                        .elements
                        .iter()
                        .filter_map(|x| x.container_element().map(|y| (x, y)))
                        .filter_map(|(x, y)| y.calculated_position.as_ref().map(|y| (x, y)))
                        .filter_map(|(x, y)| match y {
                            gigachad_transformer::LayoutPosition::Wrap { row, .. } => {
                                Some((*row, x))
                            }
                            gigachad_transformer::LayoutPosition::Default => None,
                        })
                        .chunk_by(|(row, _element)| *row);

                    let mut rows = rows
                        .into_iter()
                        .map(|(_row, y)| y.into_iter().map(|(_, element)| element).collect_vec())
                        .peekable();

                    if rows.peek().is_some() {
                        for row in rows {
                            ui.vertical(move |ui| {
                                ui.horizontal(move |ui| {
                                    self.render_elements_ref(ui, &row, handler);
                                });
                            });
                        }
                    } else {
                        ui.horizontal(move |ui| {
                            self.render_elements(ui, &container.elements, handler);
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
                            gigachad_transformer::LayoutPosition::Wrap { col, .. } => {
                                Some((*col, x))
                            }
                            gigachad_transformer::LayoutPosition::Default => None,
                        })
                        .chunk_by(|(col, _element)| *col);

                    let mut cols = cols
                        .into_iter()
                        .map(|(_row, y)| y.into_iter().map(|(_, element)| element).collect_vec())
                        .peekable();

                    if cols.peek().is_some() {
                        for col in cols {
                            ui.horizontal(move |ui| {
                                ui.vertical(move |ui| {
                                    self.render_elements_ref(ui, &col, handler);
                                });
                            });
                        }
                    } else {
                        ui.vertical(move |ui| {
                            self.render_elements(ui, &container.elements, handler);
                        });
                    }
                }
            }
        });
    }

    fn render_elements(&self, ui: &mut Ui, elements: &[Element], handler: Option<&Handler>) {
        for element in elements {
            self.render_element(ui, element, handler);
        }
    }

    fn render_elements_ref(&self, ui: &mut Ui, elements: &[&Element], handler: Option<&Handler>) {
        for element in elements {
            self.render_element(ui, element, handler);
        }
    }

    fn render_element(&self, ui: &mut Ui, element: &Element, handler: Option<&Handler>) {
        let response: Option<Response> = match element {
            Element::Raw { value } => Some(ui.label(value)),
            _ => None,
        };

        if let (Some(handler), Some(response)) = (handler, response) {
            handler(&response);
        }

        let handler: Option<Handler> = match element {
            Element::Button { .. } => Some(Box::new(|response| {
                if response.clicked() {
                    log::debug!("clicked button!");
                }
            })),
            Element::Anchor { href, .. } => {
                let href = href.to_owned();
                let sender = self.sender.clone();
                Some(Box::new(move |response| {
                    if response.clicked() {
                        log::debug!("clicked link {href:?}!");
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
            self.render_container(ui, container, handler.as_ref());
        }
    }
}

impl eframe::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.calc(ctx);

        let container = self.container.clone();
        let container: &ContainerElement = &container.read().unwrap();

        egui::CentralPanel::default().show(ctx, |ui| {
            let style = ui.style_mut();
            style.spacing.window_margin.left = 0.0;
            style.spacing.window_margin.right = 0.0;
            style.spacing.window_margin.top = 0.0;
            style.spacing.window_margin.bottom = 0.0;
            #[cfg(feature = "debug")]
            {
                style.debug.debug_on_hover = true;
            }
            self.render_container(ui, container, None);
        });
    }
}
