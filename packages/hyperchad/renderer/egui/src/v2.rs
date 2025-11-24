//! Version 2 implementation of the egui renderer.
//!
//! This module provides the second generation egui-based renderer for `HyperChad`,
//! featuring improved rendering architecture and simplified action handling compared
//! to version 1.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, LazyLock, RwLock},
};

use crate::layout::EguiCalc;
use async_trait::async_trait;
use eframe::egui::{self, Color32, Response, Ui, Widget};
use flume::{Receiver, Sender};
use hyperchad_actions::handler::{
    ActionContext, ActionHandler, BTreeMapStyleManager, ElementFinder, LogLevel as ActionLogLevel,
    StyleTrigger,
};
use hyperchad_actions::{ActionTrigger, logic::Value};
use hyperchad_renderer::canvas::CanvasUpdate;
use hyperchad_router::{ClientInfo, Router};
use hyperchad_transformer::{
    Container, Element, Input,
    models::{TextOverflow, Visibility},
};

pub use eframe;
pub use hyperchad_renderer::*;

/// Represents a view to be rendered.
///
/// This enum wraps the different types of views that can be rendered by the egui renderer.
pub enum RenderView {
    /// A container view with its contents.
    ///
    /// Contains a `Container` with layout information and child elements to render.
    View(Container),
}

/// Internal application events for async operations.
#[derive(Debug)]
enum AppEvent {
    /// Event to trigger loading an image from a source URL or path.
    LoadImage {
        /// The source URL or path of the image to load.
        source: String,
    },
}

/// Represents the state of an image being loaded or cached.
#[derive(Clone)]
enum AppImage {
    /// Image is currently being loaded.
    Loading,
    /// Image has been loaded successfully with its byte data.
    Bytes(Arc<[u8]>),
}

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

impl<C: EguiCalc + Clone + Send + Sync + 'static> hyperchad_renderer::RenderRunner
    for EguiRenderRunner<C>
{
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
    /// # Errors
    ///
    /// Will error if egui fails to run the event loop.
    ///
    /// # Panics
    ///
    /// Will panic if width or height were not set during initialization.
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

/// Internal application state for the egui renderer.
///
/// Manages rendering state, UI widgets, and communication channels.
#[derive(Clone)]
struct EguiApp<C: EguiCalc + Clone + Send + Sync> {
    /// The egui context used for rendering.
    ctx: Arc<RwLock<Option<egui::Context>>>,
    /// The layout calculator for computing element dimensions.
    calculator: Arc<RwLock<C>>,
    /// The current container being rendered.
    container: Arc<RwLock<Option<Container>>>,
    /// Current viewport width.
    width: Arc<RwLock<Option<f32>>>,
    /// Current viewport height.
    height: Arc<RwLock<Option<f32>>>,
    /// Queue of views waiting to be rendered.
    render_queue: Arc<RwLock<Option<VecDeque<RenderView>>>>,

    /// Application window title.
    title: Option<String>,
    /// Application window description.
    description: Option<String>,
    /// Application background color.
    background: Option<Color32>,

    /// Sender for navigation events.
    sender: Sender<String>,
    /// Sender for custom action requests.
    request_action: Sender<(String, Option<Value>)>,
    /// Sender for resize events.
    on_resize: Sender<(f32, f32)>,
    /// Sender for internal application events.
    event: Sender<AppEvent>,
    /// Receiver for internal application events.
    event_receiver: flume::Receiver<AppEvent>,

    /// State of checkbox elements by ID.
    checkboxes: Arc<RwLock<BTreeMap<egui::Id, bool>>>,
    /// State of text input elements by ID.
    text_inputs: Arc<RwLock<BTreeMap<egui::Id, String>>>,
    /// Cache of loaded images.
    images: Arc<RwLock<BTreeMap<String, AppImage>>>,
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> EguiApp<C> {
    /// Creates a new `EguiApp` instance with the specified communication channels.
    fn new(
        sender: Sender<String>,
        request_action: Sender<(String, Option<Value>)>,
        on_resize: Sender<(f32, f32)>,
        calculator: C,
    ) -> Self {
        let (event_tx, event_rx) = flume::unbounded();

        Self {
            ctx: Arc::new(RwLock::new(None)),
            calculator: Arc::new(RwLock::new(calculator)),
            container: Arc::new(RwLock::new(None)),
            width: Arc::new(RwLock::new(None)),
            height: Arc::new(RwLock::new(None)),
            render_queue: Arc::new(RwLock::new(Some(VecDeque::new()))),
            title: None,
            description: None,
            background: None,
            sender,
            request_action,
            on_resize,
            event: event_tx,
            event_receiver: event_rx,
            checkboxes: Arc::new(RwLock::new(BTreeMap::new())),
            text_inputs: Arc::new(RwLock::new(BTreeMap::new())),
            images: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Checks if the frame has been resized and updates dimensions accordingly.
    ///
    /// Returns `true` if the frame size changed.
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
                log::error!("Failed to send resize event: {e:?}");
            }
            true
        } else {
            false
        }
    }

    /// Updates the frame size and recalculates the container layout.
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

    /// Renders a container and its children to the UI.
    ///
    /// Returns the UI response if the container was rendered, or `None` if hidden.
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
                let mut label = egui::Label::new(egui::RichText::new(value).size(font_size));

                if matches!(container.text_overflow, Some(TextOverflow::Ellipsis)) {
                    label = label.truncate();
                }

                return Some(label.ui(ui));
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
                    let mut label = egui::Label::new(egui::RichText::new(text).size(font_size))
                        .sense(egui::Sense::click());

                    if matches!(container.text_overflow, Some(TextOverflow::Ellipsis)) {
                        label = label.truncate();
                    }

                    let response = label.ui(ui);

                    if response.clicked()
                        && let Some(href) = href
                    {
                        let _ = self.sender.send(href.clone());
                    }

                    return Some(response);
                }
            }
            Element::Image {
                source: Some(source),
                ..
            } => {
                let mut images = self.images.write().unwrap();
                return Some(Self::render_image(
                    &mut images,
                    ui,
                    source,
                    container,
                    &self.event,
                ));
            }
            _ => {}
        }

        // Render as container with children
        self.render_container_with_children(ui, container)
    }

    /// Renders a container as a frame with its children inside.
    ///
    /// Applies background, padding, borders, and layout direction to the container.
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

    /// Renders an input element (text, password, checkbox, or hidden).
    ///
    /// Maintains input state and returns the UI response if rendered.
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

    /// Handles action triggers (click, hover, change) for a container.
    fn handle_actions(&self, _ui: &Ui, container: &Container, response: &Response) {
        // Use shared action handler system
        for action in &container.actions {
            let should_trigger = match action.trigger {
                ActionTrigger::Click => response.clicked(),
                ActionTrigger::Hover => response.hovered(),
                ActionTrigger::Change => response.changed(),
                _ => false,
            };

            if should_trigger {
                self.handle_action_with_handler(action, container);
            }
        }
    }

    /// Processes an action using the action handler system.
    ///
    /// Creates action context, element finder, and style managers to execute the action.
    fn handle_action_with_handler(
        &self,
        action: &hyperchad_actions::Action,
        root_container: &Container,
    ) {
        // Create action context
        let action_context = EguiActionContext {
            ctx: Arc::new(RwLock::new(None)), // Will be set when context is available
            navigation_sender: Some(self.sender.clone()),
            action_sender: Some(self.request_action.clone()),
        };

        // Create element finder
        let element_finder = EguiElementFinder::new(root_container);

        // Create style managers
        let visibility_manager = BTreeMapStyleManager::default();
        let background_manager = BTreeMapStyleManager::default();
        let display_manager = BTreeMapStyleManager::default();

        // Create action handler
        let mut action_handler = ActionHandler::new(
            element_finder,
            visibility_manager,
            background_manager,
            display_manager,
        );

        // Convert action trigger to style trigger
        let style_trigger = match action.trigger {
            ActionTrigger::Event(_)
            | ActionTrigger::HttpBeforeRequest
            | ActionTrigger::HttpAfterRequest
            | ActionTrigger::HttpRequestSuccess
            | ActionTrigger::HttpRequestError
            | ActionTrigger::HttpRequestAbort
            | ActionTrigger::HttpRequestTimeout => StyleTrigger::CustomEvent,
            ActionTrigger::Click
            | ActionTrigger::Hover
            | ActionTrigger::Change
            | ActionTrigger::ClickOutside
            | ActionTrigger::MouseDown
            | ActionTrigger::KeyDown
            | ActionTrigger::Resize
            | ActionTrigger::Immediate => StyleTrigger::UiEvent,
        };

        // Handle the action
        action_handler.handle_action(
            &action.effect.action,
            Some(&action.effect),
            style_trigger,
            0, // self_id - would need to be determined from context
            &action_context,
            None, // event_value
            None, // value
        );
    }

    /// Extracts text content from a container's first child if it's a raw text element.
    fn get_container_text(container: &Container) -> Option<String> {
        // Look for text in children or raw elements
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            return Some(value.clone());
        }
        None
    }

    /// Sets the font size for all text styles in the egui context.
    fn set_font_size(font_size: f32, ctx: &egui::Context) {
        ctx.style_mut(|style| {
            for font in style.text_styles.values_mut() {
                font.size = font_size;
            }
        });
    }

    /// Renders an image from cache or initiates loading if not cached.
    ///
    /// # Panics
    ///
    /// Will panic if container dimensions are not set.
    fn render_image(
        images: &mut BTreeMap<String, AppImage>,
        ui: &mut Ui,
        source: &str,
        container: &Container,
        event: &Sender<AppEvent>,
    ) -> Response {
        egui::Frame::new()
            .show(ui, |ui| {
                ui.set_width(container.calculated_width.unwrap());
                ui.set_height(container.calculated_height.unwrap());

                match images.get(source) {
                    Some(AppImage::Bytes(bytes)) => {
                        log::trace!(
                            "render_image: showing image for source={source} ({}, {})",
                            container.calculated_width.unwrap(),
                            container.calculated_height.unwrap(),
                        );

                        egui::Image::from_bytes(
                            format!("bytes://{source}"),
                            egui::load::Bytes::Shared(bytes.clone()),
                        )
                        .max_width(container.calculated_width.unwrap())
                        .max_height(container.calculated_height.unwrap())
                        .ui(ui);
                    }
                    Some(AppImage::Loading) => {
                        log::trace!("render_image: image loading for source={source}");
                        ui.label("Loading...");
                    }
                    None => {
                        log::trace!("render_image: triggering image load for source={source}");
                        images.insert(source.to_string(), AppImage::Loading);
                        if let Err(e) = event.send(AppEvent::LoadImage {
                            source: source.to_string(),
                        }) {
                            log::error!("Failed to send LoadImage event: {e:?}");
                        }
                        ui.label("Loading...");
                    }
                }
            })
            .response
    }

    /// Listens for internal application events and processes them asynchronously.
    ///
    /// Handles image loading requests and other application-level events.
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
            }
        }
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

    /// # Errors
    ///
    /// Will error if egui app fails to start.
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

        // Start listening for events
        log::debug!("EguiRenderer: spawning listen thread");
        switchy_async::runtime::Handle::current().spawn_with_name("renderer_egui::init: listen", {
            let app = self.app.clone();
            async move {
                log::debug!("EguiRenderer: listening");
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
        log::debug!("EguiRenderer: emit_event {event_name} = {event_value:?}");
        // Simplified - implement event handling if needed
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if egui fails to render the view.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render(
        &self,
        view: hyperchad_renderer::View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!("EguiRenderer: render called");

        let Some(primary) = view.primary else {
            return Ok(());
        };

        // Check if context is ready
        if self.app.ctx.read().unwrap().is_none() {
            log::debug!("EguiRenderer: context not ready, queuing render");
            self.app
                .render_queue
                .write()
                .unwrap()
                .as_mut()
                .unwrap()
                .push_back(RenderView::View(primary));
            return Ok(());
        }

        let mut container = primary;

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

    /// # Errors
    ///
    /// Will error if egui fails to render the canvas update.
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
        // Process queued renders if context is ready
        let render_queue = self.render_queue.write().unwrap().take();
        if let Some(render_queue) = render_queue {
            for render_view in render_queue {
                match render_view {
                    RenderView::View(view) => {
                        let mut container = view;

                        // Set container size
                        container.calculated_width = self.width.read().unwrap().or(Some(800.0));
                        container.calculated_height = self.height.read().unwrap().or(Some(600.0));

                        // Calculate layout
                        self.calculator.read().unwrap().calc(&mut container);

                        // Store container
                        *self.container.write().unwrap() = Some(container);
                    }
                }
            }
            // Reset the render queue
            *self.render_queue.write().unwrap() = Some(VecDeque::new());
        }

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

/// `ActionContext` implementation for egui renderer.
///
/// Provides context for executing actions including navigation,
/// custom action requests, and logging.
#[derive(Clone)]
struct EguiActionContext {
    /// The egui rendering context.
    ctx: Arc<RwLock<Option<egui::Context>>>,
    /// Channel for sending navigation requests.
    navigation_sender: Option<Sender<String>>,
    /// Channel for sending custom action requests.
    action_sender: Option<Sender<(String, Option<Value>)>>,
}

impl ActionContext for EguiActionContext {
    fn request_repaint(&self) {
        if let Some(ctx) = &*self.ctx.read().unwrap() {
            ctx.request_repaint();
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

    fn navigate(&self, url: String) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.navigation_sender.as_ref().map_or_else(
            || {
                Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Navigation sender not available",
                )) as Box<dyn std::error::Error + Send>)
            },
            |sender| {
                sender
                    .send(url)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
            },
        )
    }

    fn request_custom_action(
        &self,
        action: String,
        value: Option<Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.action_sender.as_ref().map_or_else(
            || {
                Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Action sender not available",
                )) as Box<dyn std::error::Error + Send>)
            },
            |sender| {
                sender
                    .send((action, value))
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
            },
        )
    }

    fn log(&self, level: ActionLogLevel, message: &str) {
        match level {
            ActionLogLevel::Error => log::error!("{message}"),
            ActionLogLevel::Warn => log::warn!("{message}"),
            ActionLogLevel::Info => log::info!("{message}"),
            ActionLogLevel::Debug => log::debug!("{message}"),
            ActionLogLevel::Trace => log::trace!("{message}"),
        }
    }
}

/// `ElementFinder` implementation for egui renderer.
///
/// Provides methods to find elements within the container tree
/// by ID, class, or other attributes.
struct EguiElementFinder<'a> {
    /// Root container to search within.
    container: &'a Container,
}

impl<'a> EguiElementFinder<'a> {
    /// Creates a new element finder for the given container.
    const fn new(container: &'a Container) -> Self {
        Self { container }
    }

    /// Recursively searches for an element matching the predicate.
    ///
    /// Returns the element ID if found.
    fn find_element_recursive(
        container: &Container,
        predicate: &dyn Fn(&Container) -> bool,
    ) -> Option<usize> {
        if predicate(container) {
            return Some(container.id);
        }

        for child in &container.children {
            if let Some(id) = Self::find_element_recursive(child, predicate) {
                return Some(id);
            }
        }

        None
    }

    /// Finds a container by its numeric ID.
    fn find_by_id(container: &Container, id: usize) -> Option<&Container> {
        if container.id == id {
            return Some(container);
        }
        for child in &container.children {
            if let Some(found) = Self::find_by_id(child, id) {
                return Some(found);
            }
        }
        None
    }
}

impl ElementFinder for EguiElementFinder<'_> {
    fn find_by_str_id(&self, str_id: &str) -> Option<usize> {
        Self::find_element_recursive(self.container, &|container| {
            container.str_id.as_ref().is_some_and(|id| id == str_id)
        })
    }

    fn find_by_class(&self, class: &str) -> Option<usize> {
        Self::find_element_recursive(self.container, &|container| {
            container.classes.iter().any(|c| c == class)
        })
    }

    fn find_child_by_class(&self, parent_id: usize, class: &str) -> Option<usize> {
        let parent = Self::find_by_id(self.container, parent_id)?;

        for child in &parent.children {
            if child.classes.iter().any(|c| c == class) {
                return Some(child.id);
            }
        }

        None
    }

    fn get_last_child(&self, parent_id: usize) -> Option<usize> {
        let parent = Self::find_by_id(self.container, parent_id)?;
        parent.children.last().map(|child| child.id)
    }

    fn get_data_attr(&self, element_id: usize, attr: &str) -> Option<String> {
        let element = Self::find_by_id(self.container, element_id)?;
        element.data.get(attr).cloned()
    }

    fn get_str_id(&self, element_id: usize) -> Option<String> {
        let element = Self::find_by_id(self.container, element_id)?;
        element.str_id.clone()
    }

    fn get_dimensions(&self, element_id: usize) -> Option<(f32, f32)> {
        let element = Self::find_by_id(self.container, element_id)?;
        Some((
            element.calculated_width.unwrap_or(0.0),
            element.calculated_height.unwrap_or(0.0),
        ))
    }

    fn get_position(&self, element_id: usize) -> Option<(f32, f32)> {
        let element = Self::find_by_id(self.container, element_id)?;
        Some((
            element.calculated_x.unwrap_or(0.0),
            element.calculated_y.unwrap_or(0.0),
        ))
    }
}
