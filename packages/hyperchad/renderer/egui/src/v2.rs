#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::layout::EguiCalc;
use async_trait::async_trait;
use eframe::egui::{self, Color32, Response, Ui, Widget};
use flume::{Receiver, Sender};
use hyperchad_actions::{ActionTrigger, ActionType, logic::Value};
use hyperchad_renderer::canvas::CanvasUpdate;
use hyperchad_router::{ClientInfo, Router};
use hyperchad_transformer::{Container, Element, Input, models::Visibility};

pub use eframe;
pub use hyperchad_renderer::*;

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
        _router: Router,
        request_action: Sender<(String, Option<Value>)>,
        on_resize: Sender<(f32, f32)>,
        _client_info: Arc<ClientInfo>,
        calculator: C,
    ) -> Self {
        let (tx, rx) = flume::unbounded();
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            app: EguiApp::new(tx, request_action, on_resize, calculator),
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

impl<C: EguiCalc + Clone + Send + Sync + 'static> hyperchad_renderer::RenderRunner
    for EguiRenderRunner<C>
{
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        let mut viewport =
            egui::ViewportBuilder::default().with_inner_size([self.width, self.height]);

        #[allow(clippy::cast_precision_loss)]
        if let (Some(x), Some(y)) = (self.x, self.y) {
            viewport = viewport.with_position((x as f32, y as f32));
        }

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

        log::debug!("EguiRenderer: starting");
        if let Err(e) = eframe::run_native(
            self.app.title.as_deref().unwrap_or("MoosicBox"),
            options,
            Box::new(|cc| {
                // Initialize fonts and image loaders
                let _ = cc.egui_ctx.run(egui::RawInput::default(), |_| {});
                egui_extras::install_image_loaders(&cc.egui_ctx);

                // Set context in app
                *self.app.ctx.write().unwrap() = Some(cc.egui_ctx.clone());

                // Update calculator with context
                let mut calculator = self.app.calculator.write().unwrap();
                *calculator = calculator.clone().with_context(cc.egui_ctx.clone());
                drop(calculator);

                log::debug!("EguiRenderer: initialized");
                Ok(Box::new(self.app.clone()))
            }),
        ) {
            log::error!("EguiRenderer: eframe error: {e:?}");
        }
        log::debug!("EguiRenderer: finished");

        Ok(())
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> hyperchad_renderer::ToRenderRunner
    for EguiRenderer<C>
{
    fn to_runner(
        self,
        _handle: hyperchad_renderer::Handle,
    ) -> Result<Box<dyn hyperchad_renderer::RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(EguiRenderRunner {
            width: self.width.unwrap(),
            height: self.height.unwrap(),
            x: self.x,
            y: self.y,
            app: self.app,
        }))
    }
}

#[derive(Clone)]
struct EguiApp<C: EguiCalc + Clone + Send + Sync> {
    ctx: Arc<RwLock<Option<egui::Context>>>,
    calculator: Arc<RwLock<C>>,
    container: Arc<RwLock<Option<Container>>>,
    width: Arc<RwLock<Option<f32>>>,
    height: Arc<RwLock<Option<f32>>>,

    // App state
    title: Option<String>,
    description: Option<String>,
    background: Option<Color32>,

    // Communication
    sender: Sender<String>,
    request_action: Sender<(String, Option<Value>)>,
    on_resize: Sender<(f32, f32)>,

    // UI state
    checkboxes: Arc<RwLock<HashMap<egui::Id, bool>>>,
    text_inputs: Arc<RwLock<HashMap<egui::Id, String>>>,
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> EguiApp<C> {
    fn new(
        sender: Sender<String>,
        request_action: Sender<(String, Option<Value>)>,
        on_resize: Sender<(f32, f32)>,
        calculator: C,
    ) -> Self {
        Self {
            ctx: Arc::new(RwLock::new(None)),
            calculator: Arc::new(RwLock::new(calculator)),
            container: Arc::new(RwLock::new(None)),
            width: Arc::new(RwLock::new(None)),
            height: Arc::new(RwLock::new(None)),
            title: None,
            description: None,
            background: None,
            sender,
            request_action,
            on_resize,
            checkboxes: Arc::new(RwLock::new(HashMap::new())),
            text_inputs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn check_frame_resize(&self, ctx: &egui::Context) -> bool {
        let (width, height) = ctx.input(|i| (i.screen_rect.width(), i.screen_rect.height()));

        let current_width = *self.width.read().unwrap();
        let current_height = *self.height.read().unwrap();

        if current_width.is_none_or(|x| (x - width).abs() >= 0.01)
            || current_height.is_none_or(|x| (x - height).abs() >= 0.01)
        {
            self.update_frame_size(width, height);
            if let Err(e) = self.on_resize.send((width, height)) {
                log::error!("Failed to send resize event: {e:?}");
            }
            true
        } else {
            false
        }
    }

    fn update_frame_size(&self, width: f32, height: f32) {
        log::debug!("Frame size changed to: {width}x{height}");

        if let Some(container) = self.container.write().unwrap().as_mut() {
            container.calculated_width = Some(width);
            container.calculated_height = Some(height);
            self.calculator.read().unwrap().calc(container);
        }

        *self.width.write().unwrap() = Some(width);
        *self.height.write().unwrap() = Some(height);
    }

    fn render_container(&self, ui: &mut Ui, container: &Container) -> Option<Response> {
        if container.is_hidden() || container.visibility == Some(Visibility::Hidden) {
            return None;
        }

        // Set font size if specified
        if let Some(font_size) = container.calculated_font_size {
            Self::set_font_size(font_size, ui.ctx());
        }

        // Apply opacity
        if let Some(opacity) = container.calculated_opacity {
            ui.set_opacity(opacity);
        }

        // Handle different element types
        match &container.element {
            Element::Raw { value } => {
                let font_size = container.calculated_font_size.unwrap_or(14.0);
                return Some(ui.label(egui::RichText::new(value).size(font_size)));
            }
            Element::Input { input, .. } => {
                return self.render_input(ui, input, container);
            }
            Element::Button { .. } => {
                if let Some(text) = Self::get_container_text(container) {
                    let font_size = container.calculated_font_size.unwrap_or(14.0);
                    return Some(ui.button(egui::RichText::new(text).size(font_size)));
                }
            }
            Element::Anchor { href, .. } => {
                if let Some(text) = Self::get_container_text(container) {
                    let font_size = container.calculated_font_size.unwrap_or(14.0);
                    let response = ui.link(egui::RichText::new(text).size(font_size));

                    if response.clicked() {
                        if let Some(href) = href {
                            let _ = self.sender.send(href.clone());
                        }
                    }

                    return Some(response);
                }
            }
            _ => {}
        }

        // Render as container with children
        self.render_container_with_children(ui, container)
    }

    fn render_container_with_children(
        &self,
        ui: &mut Ui,
        container: &Container,
    ) -> Option<Response> {
        if container.children.is_empty() {
            return None;
        }

        // Create frame with background and borders
        let mut frame = egui::Frame::new();

        if let Some(background) = container.background {
            frame = frame.fill(background.into());
        }

        // Add padding
        #[allow(clippy::cast_possible_truncation)]
        let padding = egui::Margin {
            left: container.calculated_padding_left.unwrap_or(0.0) as i8,
            right: container.calculated_padding_right.unwrap_or(0.0) as i8,
            top: container.calculated_padding_top.unwrap_or(0.0) as i8,
            bottom: container.calculated_padding_bottom.unwrap_or(0.0) as i8,
        };
        frame = frame.inner_margin(padding);

        // Add corner radius
        if let Some(radius) = container.calculated_border_top_left_radius {
            frame = frame.corner_radius(radius);
        }

        let response = frame.show(ui, |ui| {
            // Set container size
            if let Some(width) = container.calculated_width {
                ui.set_width(width);
            }
            if let Some(height) = container.calculated_height {
                ui.set_height(height);
            }

            // Render children based on direction
            match container.direction {
                hyperchad_transformer::models::LayoutDirection::Row => {
                    ui.horizontal(|ui| {
                        for child in &container.children {
                            self.render_container(ui, child);
                        }
                    });
                }
                hyperchad_transformer::models::LayoutDirection::Column => {
                    ui.vertical(|ui| {
                        for child in &container.children {
                            self.render_container(ui, child);
                        }
                    });
                }
            }
        });

        // Handle actions
        self.handle_actions(ui, container, &response.response);

        Some(response.response)
    }

    #[allow(clippy::significant_drop_tightening)]
    fn render_input(&self, ui: &mut Ui, input: &Input, container: &Container) -> Option<Response> {
        let id = ui.next_auto_id();

        match input {
            Input::Text { value, .. } => {
                let mut text_inputs = self.text_inputs.write().unwrap();
                let text = text_inputs
                    .entry(id)
                    .or_insert_with(|| value.clone().unwrap_or_default());

                let mut text_edit = egui::TextEdit::singleline(text).id(id);

                if let Some(width) = container.calculated_width {
                    text_edit = text_edit.desired_width(width);
                }

                Some(text_edit.ui(ui))
            }
            Input::Password { value, .. } => {
                let mut text_inputs = self.text_inputs.write().unwrap();
                let text = text_inputs
                    .entry(id)
                    .or_insert_with(|| value.clone().unwrap_or_default());

                let mut text_edit = egui::TextEdit::singleline(text).id(id).password(true);

                if let Some(width) = container.calculated_width {
                    text_edit = text_edit.desired_width(width);
                }

                Some(text_edit.ui(ui))
            }
            Input::Checkbox { checked, .. } => {
                let mut checkboxes = self.checkboxes.write().unwrap();
                let checked_value = checkboxes
                    .entry(id)
                    .or_insert_with(|| checked.unwrap_or(false));

                Some(egui::Checkbox::without_text(checked_value).ui(ui))
            }
            Input::Hidden { .. } => None,
        }
    }

    fn handle_actions(&self, _ui: &Ui, container: &Container, response: &Response) {
        for action in &container.actions {
            match action.trigger {
                ActionTrigger::Click => {
                    if response.clicked() {
                        self.execute_action(&action.effect.action);
                    }
                }
                ActionTrigger::Hover => {
                    if response.hovered() {
                        self.execute_action(&action.effect.action);
                    }
                }
                ActionTrigger::Change => {
                    if response.changed() {
                        self.execute_action(&action.effect.action);
                    }
                }
                // Add other triggers as needed
                _ => {}
            }
        }
    }

    fn execute_action(&self, action: &ActionType) {
        match action {
            ActionType::Navigate { url } => {
                let _ = self.sender.send(url.clone());
            }
            ActionType::Custom { action } => {
                let _ = self.request_action.send((action.clone(), None));
            }
            ActionType::Log { message, level } => match level {
                hyperchad_actions::LogLevel::Error => log::error!("{message}"),
                hyperchad_actions::LogLevel::Warn => log::warn!("{message}"),
                hyperchad_actions::LogLevel::Info => log::info!("{message}"),
                hyperchad_actions::LogLevel::Debug => log::debug!("{message}"),
                hyperchad_actions::LogLevel::Trace => log::trace!("{message}"),
            },
            ActionType::Multi(actions) => {
                for action in actions {
                    self.execute_action(action);
                }
            }
            ActionType::MultiEffect(effects) => {
                for effect in effects {
                    self.execute_action(&effect.action);
                }
            }
            // Add other action types as needed
            _ => {
                log::debug!("Unhandled action type: {action:?}");
            }
        }
    }

    fn get_container_text(container: &Container) -> Option<String> {
        // Look for text in children or raw elements
        if let Some(child) = container.children.first() {
            if let Element::Raw { value } = &child.element {
                return Some(value.clone());
            }
        }
        None
    }

    fn set_font_size(font_size: f32, ctx: &egui::Context) {
        ctx.style_mut(|style| {
            for font in style.text_styles.values_mut() {
                font.size = font_size;
            }
        });
    }
}

#[async_trait]
impl<C: EguiCalc + Clone + Send + Sync + 'static> hyperchad_renderer::Renderer for EguiRenderer<C> {
    fn add_responsive_trigger(
        &mut self,
        _name: String,
        _trigger: hyperchad_transformer::ResponsiveTrigger,
    ) {
        // Simplified - implement if needed
    }

    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<hyperchad_renderer::Color>,
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

        log::debug!("EguiRenderer: initialized with size {width}x{height}");
        Ok(())
    }

    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!("EguiRenderer: emit_event {event_name} = {event_value:?}");
        // Simplified - implement event handling if needed
        Ok(())
    }

    async fn render(
        &self,
        view: hyperchad_renderer::View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!("EguiRenderer: render called");

        let mut container = view.immediate;

        // Set container size
        container.calculated_width = self.app.width.read().unwrap().or(self.width);
        container.calculated_height = self.app.height.read().unwrap().or(self.height);

        // Calculate layout
        self.app.calculator.read().unwrap().calc(&mut container);

        // Store container
        *self.app.container.write().unwrap() = Some(container);

        // Request repaint
        if let Some(ctx) = &*self.app.ctx.read().unwrap() {
            ctx.request_repaint();
        }

        Ok(())
    }

    async fn render_partial(
        &self,
        view: hyperchad_renderer::PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!(
            "EguiRenderer: render_partial called for target: {}",
            view.target
        );

        // Simplified partial rendering - replace element with matching ID
        if let Some(container) = self.app.container.write().unwrap().as_mut() {
            let value = container.replace_str_id_with_elements_calc(
                &*self.app.calculator.read().unwrap(),
                view.container.children,
                &view.target,
            );
            if value.is_some() {
                log::debug!("EguiRenderer: replaced element with ID: {}", view.target);
            } else {
                log::warn!(
                    "EguiRenderer: could not find element with ID: {}",
                    view.target
                );
            }
        }

        // Request repaint
        if let Some(ctx) = &*self.app.ctx.read().unwrap() {
            ctx.request_repaint();
        }

        Ok(())
    }

    async fn render_canvas(
        &self,
        _update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!("EguiRenderer: render_canvas called");
        // Simplified - implement canvas rendering if needed
        Ok(())
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> eframe::App for EguiApp<C> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for resize
        self.check_frame_resize(ctx);

        // Set up minimal styling
        ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::Vec2::ZERO;
            style.spacing.window_margin = egui::Margin::ZERO;
            style.spacing.button_padding = egui::Vec2::ZERO;
        });

        // Render the main container
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new().fill(
                    self.background
                        .unwrap_or_else(|| Color32::from_hex("#181a1b").unwrap()),
                ),
            )
            .show(ctx, |ui| {
                if let Some(container) = &*self.container.read().unwrap() {
                    self.render_container(ui, container);
                }
            });
    }
}
