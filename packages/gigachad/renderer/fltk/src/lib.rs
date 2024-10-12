#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    borrow::Cow,
    collections::HashMap,
    ops::Deref,
    str::FromStr as _,
    sync::{
        atomic::{AtomicBool, AtomicI32},
        Arc, LazyLock, Mutex, RwLock,
    },
};

use async_trait::async_trait;
use bytes::Bytes;
use fltk::{
    app::{self, App},
    enums::{self, Event},
    frame::{self, Frame},
    group,
    image::{RgbImage, SharedImage},
    prelude::*,
    widget,
    window::{DoubleWindow, Window},
};
use flume::{Receiver, Sender};
use gigachad_transformer::{
    calc::{calc_number, Calc as _},
    ContainerElement, Element, HeaderSize, LayoutDirection, LayoutOverflow,
};
use thiserror::Error;
use tokio::task::JoinHandle;

pub use gigachad_renderer::*;

#[cfg(feature = "debug")]
static DEBUG: LazyLock<RwLock<bool>> = LazyLock::new(|| {
    RwLock::new(
        std::env::var("DEBUG_RENDERER")
            .is_ok_and(|x| ["1", "true"].contains(&x.to_lowercase().as_str())),
    )
});

#[derive(Debug, Error)]
pub enum LoadImageError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Fltk(#[from] FltkError),
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Resize {},
    MouseWheel {},
    Navigate {
        href: String,
    },
    RegisterImage {
        viewport: Option<Viewport>,
        source: String,
        width: Option<f32>,
        height: Option<f32>,
        frame: Frame,
    },
    LoadImage {
        source: String,
        width: Option<f32>,
        height: Option<f32>,
        frame: Frame,
    },
    UnloadImage {
        frame: Frame,
    },
}

#[derive(Debug, Clone)]
pub struct RegisteredImage {
    source: String,
    width: Option<f32>,
    height: Option<f32>,
    frame: Frame,
}

pub struct ViewportListener {
    widget: widget::Widget,
    viewport: Option<Viewport>,
    visible: bool,
    dist: u32,
    callback: Box<dyn FnMut(bool, u32) + Send + Sync>,
}

impl std::fmt::Debug for ViewportListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportListener")
            .field("widget", &self.widget)
            .field("viewport", &self.viewport)
            .field("visible", &self.visible)
            .finish_non_exhaustive()
    }
}

impl ViewportListener {
    fn new(
        widget: widget::Widget,
        viewport: Option<Viewport>,
        callback: impl FnMut(bool, u32) + Send + Sync + 'static,
    ) -> Self {
        let mut this = Self {
            widget,
            viewport,
            visible: false,
            dist: 0,
            callback: Box::new(callback),
        };

        this.init();
        this
    }

    fn is_visible(&self) -> (bool, u32) {
        if let Some((visible, dist)) = self
            .viewport
            .as_ref()
            .map(|x| x.is_widget_visible(&self.widget))
        {
            (visible, dist)
        } else {
            (true, 0)
        }
    }

    fn init(&mut self) {
        let (visible, dist) = self.is_visible();
        self.visible = visible;
        self.dist = dist;
        (self.callback)(visible, dist);
    }

    pub fn check(&mut self) {
        let (visible, dist) = self.is_visible();

        if visible != self.visible || dist != self.dist {
            self.visible = visible;
            self.dist = dist;
            (self.callback)(visible, dist);
        }
    }
}

type JoinHandleAndCancelled = (JoinHandle<()>, Arc<AtomicBool>);

#[derive(Clone)]
pub struct FltkRenderer {
    app: Option<App>,
    window: Option<DoubleWindow>,
    elements: Arc<Mutex<ContainerElement>>,
    root: Arc<RwLock<Option<group::Flex>>>,
    images: Arc<RwLock<Vec<RegisteredImage>>>,
    viewport_listeners: Arc<RwLock<Vec<ViewportListener>>>,
    width: Arc<AtomicI32>,
    height: Arc<AtomicI32>,
    event_sender: Option<Sender<AppEvent>>,
    event_receiver: Option<Receiver<AppEvent>>,
    viewport_listener_join_handle: Arc<Mutex<Option<JoinHandleAndCancelled>>>,
    sender: Sender<String>,
    receiver: Receiver<String>,
}

impl Default for FltkRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl FltkRenderer {
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded();
        Self {
            app: None,
            window: None,
            elements: Arc::new(Mutex::new(ContainerElement::default())),
            root: Arc::new(RwLock::new(None)),
            images: Arc::new(RwLock::new(vec![])),
            viewport_listeners: Arc::new(RwLock::new(vec![])),
            width: Arc::new(AtomicI32::new(0)),
            height: Arc::new(AtomicI32::new(0)),
            event_sender: None,
            event_receiver: None,
            viewport_listener_join_handle: Arc::new(Mutex::new(None)),
            sender: tx,
            receiver: rx,
        }
    }

    fn handle_resize(&self, window: &Window) {
        let width = self.width.load(std::sync::atomic::Ordering::SeqCst);
        let height = self.height.load(std::sync::atomic::Ordering::SeqCst);

        if width != window.width() || height != window.height() {
            self.width
                .store(window.width(), std::sync::atomic::Ordering::SeqCst);
            self.height
                .store(window.height(), std::sync::atomic::Ordering::SeqCst);
            log::debug!(
                "event resize: width={width}->{} height={height}->{}",
                window.width(),
                window.height()
            );

            if let Err(e) = self.perform_render() {
                log::error!("Failed to draw elements: {e:?}");
            }
        }
    }

    fn check_viewports(&self, cancelled: &AtomicBool) {
        for listener in self.viewport_listeners.write().unwrap().iter_mut() {
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            listener.check();
        }
    }

    fn trigger_load_image(&self, frame: &Frame) -> Result<(), flume::SendError<AppEvent>> {
        let image = {
            self.images
                .write()
                .unwrap()
                .iter()
                .find(|x| x.frame.is_same(frame))
                .cloned()
        };
        log::debug!("trigger_load_image: image={image:?}");

        if let Some(image) = image {
            if let Some(sender) = &self.event_sender {
                sender.send(AppEvent::LoadImage {
                    source: image.source,
                    width: image.width,
                    height: image.height,
                    frame: frame.to_owned(),
                })?;
            }
        }

        Ok(())
    }

    fn register_image(
        &self,
        viewport: Option<Viewport>,
        source: String,
        width: Option<f32>,
        height: Option<f32>,
        frame: &Frame,
    ) {
        self.images.write().unwrap().push(RegisteredImage {
            source,
            width,
            height,
            frame: frame.clone(),
        });

        let mut frame = frame.clone();
        let renderer = self.clone();
        self.viewport_listeners
            .write()
            .unwrap()
            .push(ViewportListener::new(
                frame.as_base_widget(),
                viewport,
                move |_visible, dist| {
                    if dist < 200 {
                        if let Err(e) = renderer.trigger_load_image(&frame) {
                            log::error!("Failed to trigger_load_image: {e:?}");
                        }
                    } else {
                        Self::set_frame_image(&mut frame, None);
                    }
                },
            ));
    }

    fn set_frame_image(frame: &mut Frame, image: Option<SharedImage>) {
        frame.set_image_scaled(image);
        frame.set_damage(true);
        app::awake();
    }

    async fn load_image(
        source: String,
        width: Option<f32>,
        height: Option<f32>,
        mut frame: Frame,
    ) -> Result<(), LoadImageError> {
        type ImageCache = LazyLock<Arc<tokio::sync::RwLock<HashMap<String, Bytes>>>>;
        static IMAGE_CACHE: ImageCache =
            LazyLock::new(|| Arc::new(tokio::sync::RwLock::new(HashMap::new())));

        let key = format!("{source}:{width:?}:{height:?}");

        let cached_image = { IMAGE_CACHE.read().await.get(&key).cloned() };

        let data = if let Some(data) = cached_image {
            data
        } else {
            let data = reqwest::get(source).await?.bytes().await?;
            IMAGE_CACHE.write().await.insert(key, data.clone());

            data
        };

        let image = image::load_from_memory_with_format(&data, image::ImageFormat::WebP)?;
        let image = RgbImage::new(
            image.as_bytes(),
            image.width().try_into().unwrap(),
            image.height().try_into().unwrap(),
            enums::ColorDepth::Rgb8,
        )?;
        let image = SharedImage::from_image(image)?;

        if width.is_some() || height.is_some() {
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_precision_loss)]
            let width = width.unwrap_or(image.width() as f32).round() as i32;
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_precision_loss)]
            let height = height.unwrap_or(image.height() as f32).round() as i32;

            frame.set_size(width, height);
        }

        Self::set_frame_image(&mut frame, Some(image));

        Ok(())
    }

    fn perform_render(&self) -> Result<(), FltkError> {
        let (Some(mut window), Some(tx)) = (self.window.clone(), self.event_sender.clone()) else {
            moosicbox_assert::die_or_panic!(
                "perform_render: cannot perform_render before app is started"
            );
        };
        log::debug!("perform_render: started");
        {
            let mut root = self.root.write().unwrap();
            if let Some(root) = root.take() {
                window.remove(&root);
                log::debug!("perform_render: removed root");
            }
            window.begin();
            log::debug!("perform_render: begin");
            let container: &mut ContainerElement = &mut self.elements.lock().unwrap();

            #[allow(clippy::cast_precision_loss)]
            let window_width = self.width.load(std::sync::atomic::Ordering::SeqCst) as f32;
            #[allow(clippy::cast_precision_loss)]
            let window_height = self.height.load(std::sync::atomic::Ordering::SeqCst) as f32;

            let recalc = if let (Some(width), Some(height)) =
                (container.calculated_width, container.calculated_height)
            {
                let diff_width = (width - window_width).abs();
                let diff_height = (height - window_height).abs();
                log::trace!("perform_render: diff_width={diff_width} diff_height={diff_height}");
                diff_width > 0.01 || diff_height > 0.01
            } else {
                true
            };

            if recalc {
                container.calculated_width.replace(window_width);
                container.calculated_height.replace(window_height);

                container.calc();
            } else {
                log::debug!("perform_render: ContainerElement had same size, not recalculating");
            }

            log::trace!("perform_render: initialized ContainerElement for rendering {container:?} window_width={window_width} window_height={window_height}");

            {
                log::debug!("perform_render: aborting any existing viewport_listener_join_handle");
                let handle = self.viewport_listener_join_handle.lock().unwrap().take();
                if let Some((handle, cancel)) = handle {
                    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    handle.abort();
                }
                log::debug!("perform_render: clearing images");
                self.images.write().unwrap().clear();
                log::debug!("perform_render: clearing viewport_listeners");
                self.viewport_listeners.write().unwrap().clear();
            }

            root.replace(self.draw_elements(
                Cow::Owned(None),
                container,
                0,
                Context::new(window_width, window_height),
                tx,
            )?);
        }
        window.end();
        window.flush();
        app::awake();
        log::debug!("perform_render: finished");
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    fn draw_elements(
        &self,
        mut viewport: Cow<'_, Option<Viewport>>,
        element: &ContainerElement,
        depth: usize,
        context: Context,
        event_sender: Sender<AppEvent>,
    ) -> Result<group::Flex, FltkError> {
        static SCROLL_LINESIZE: i32 = 40;
        static SCROLLBAR_SIZE: i32 = 16;

        log::debug!("draw_elements: element={element:?} depth={depth} viewport={viewport:?}");

        let (Some(calculated_width), Some(calculated_height)) =
            (element.calculated_width, element.calculated_height)
        else {
            moosicbox_assert::die_or_panic!(
                "draw_elements: missing calculated_width and/or calculated_height value"
            );
        };

        moosicbox_assert::assert!(
        calculated_width > 0.0 && calculated_height > 0.0
            || calculated_width <= 0.0 && calculated_height <= 0.0,
        "Invalid calculated_width/calculated_height: calculated_width={calculated_width} calculated_height={calculated_height}"
    );

        log::debug!(
        "draw_elements: calculated_width={calculated_width} calculated_height={calculated_height}"
    );
        let direction = context.direction;

        #[allow(clippy::cast_possible_truncation)]
        let mut container = group::Flex::default_fill().with_size(
            calculated_width.round() as i32,
            calculated_height.round() as i32,
        );
        container.set_clip_children(false);
        container.set_pad(0);

        #[allow(clippy::cast_possible_truncation)]
        let container_scroll_y: Option<Box<dyn Group>> = match context.overflow_y {
            LayoutOverflow::Auto => Some({
                let mut scroll = group::Scroll::default_fill()
                    .with_size(
                        calculated_width.round() as i32,
                        calculated_height.round() as i32,
                    )
                    .with_type(group::ScrollType::Vertical);
                scroll.set_scrollbar_size(SCROLLBAR_SIZE);
                scroll.scrollbar().set_linesize(SCROLL_LINESIZE);
                let parent = viewport.deref().clone();
                viewport
                    .to_mut()
                    .replace(Viewport::new(parent, scroll.clone()));
                scroll.into()
            }),
            LayoutOverflow::Scroll => Some({
                let mut scroll = group::Scroll::default_fill()
                    .with_size(
                        calculated_width.round() as i32,
                        calculated_height.round() as i32,
                    )
                    .with_type(group::ScrollType::VerticalAlways);
                scroll.set_scrollbar_size(SCROLLBAR_SIZE);
                scroll.scrollbar().set_linesize(SCROLL_LINESIZE);
                let parent = viewport.deref().clone();
                viewport
                    .to_mut()
                    .replace(Viewport::new(parent, scroll.clone()));
                scroll.into()
            }),
            LayoutOverflow::Squash | LayoutOverflow::Show | LayoutOverflow::Wrap => None,
        };
        #[allow(clippy::cast_possible_truncation)]
        let container_scroll_x: Option<Box<dyn Group>> = match context.overflow_x {
            LayoutOverflow::Auto => Some({
                let mut scroll = group::Scroll::default_fill()
                    .with_size(
                        calculated_width.round() as i32,
                        calculated_height.round() as i32,
                    )
                    .with_type(group::ScrollType::Horizontal);
                scroll.set_scrollbar_size(SCROLLBAR_SIZE);
                scroll.hscrollbar().set_linesize(SCROLL_LINESIZE);
                let parent = viewport.deref().clone();
                viewport
                    .to_mut()
                    .replace(Viewport::new(parent, scroll.clone()));
                scroll.into()
            }),
            LayoutOverflow::Scroll => Some({
                let mut scroll = group::Scroll::default_fill()
                    .with_size(
                        calculated_width.round() as i32,
                        calculated_height.round() as i32,
                    )
                    .with_type(group::ScrollType::HorizontalAlways);
                scroll.set_scrollbar_size(SCROLLBAR_SIZE);
                scroll.hscrollbar().set_linesize(SCROLL_LINESIZE);
                let parent = viewport.deref().clone();
                viewport
                    .to_mut()
                    .replace(Viewport::new(parent, scroll.clone()));
                scroll.into()
            }),
            LayoutOverflow::Squash | LayoutOverflow::Show | LayoutOverflow::Wrap => None,
        };

        #[allow(clippy::cast_possible_truncation)]
        let container_wrap_y: Option<Box<dyn Group>> = if context.overflow_y == LayoutOverflow::Wrap
        {
            Some({
                let mut flex = match context.direction {
                    LayoutDirection::Row => group::Flex::default_fill().column(),
                    LayoutDirection::Column => group::Flex::default_fill().row(),
                }
                .with_size(
                    calculated_width.round() as i32,
                    calculated_height.round() as i32,
                );
                flex.set_pad(0);
                flex.set_clip_children(false);
                flex.into()
            })
        } else {
            None
        };
        #[allow(clippy::cast_possible_truncation)]
        let container_wrap_x: Option<Box<dyn Group>> = if context.overflow_x == LayoutOverflow::Wrap
        {
            Some({
                let mut flex = match context.direction {
                    LayoutDirection::Row => group::Flex::default_fill().column(),
                    LayoutDirection::Column => group::Flex::default_fill().row(),
                }
                .with_size(
                    calculated_width.round() as i32,
                    calculated_height.round() as i32,
                );
                flex.set_pad(0);
                flex.set_clip_children(false);
                flex.into()
            })
        } else {
            None
        };

        let contained_width = element.contained_calculated_width();
        let contained_height = element.contained_calculated_height();

        moosicbox_assert::assert!(
            contained_width > 0.0 && contained_height > 0.0
                || contained_width <= 0.0 && contained_height <= 0.0,
            "Invalid contained_width/contained_height: contained_width={contained_width} contained_height={contained_height}"
        );

        log::debug!(
            "draw_elements: contained_width={contained_width} contained_height={contained_height}"
        );
        #[allow(clippy::cast_possible_truncation)]
        let contained_width = contained_width.round() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let contained_height = contained_height.round() as i32;
        log::debug!(
            "draw_elements: rounded contained_width={contained_width} contained_height={contained_height}"
        );

        let inner_container = if contained_width > 0 && contained_height > 0 {
            Some(
                group::Flex::default()
                    .with_size(contained_width, contained_height)
                    .column(),
            )
        } else {
            None
        };
        let flex = group::Flex::default_fill();
        let mut flex = match context.direction {
            LayoutDirection::Row => flex.row(),
            LayoutDirection::Column => flex.column(),
        };

        flex.set_clip_children(false);
        flex.set_pad(0);

        #[cfg(feature = "debug")]
        {
            if *DEBUG.read().unwrap() {
                flex.draw(|w| {
                    fltk::draw::set_draw_color(enums::Color::White);
                    fltk::draw::draw_rect(w.x(), w.y(), w.w(), w.h());
                });
            }
        }

        let (mut row, mut col) = element
            .calculated_position
            .as_ref()
            .and_then(|x| match x {
                gigachad_transformer::LayoutPosition::Wrap { row, col } => Some((*row, *col)),
                gigachad_transformer::LayoutPosition::Default => None,
            })
            .unwrap_or((0, 0));

        let len = element.elements.len();
        for (i, element) in element.elements.iter().enumerate() {
            let (current_row, current_col) = element
                .container_element()
                .and_then(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        gigachad_transformer::LayoutPosition::Wrap { row, col } => {
                            log::debug!("draw_elements: drawing row={row} col={col}");
                            Some((*row, *col))
                        }
                        gigachad_transformer::LayoutPosition::Default => None,
                    })
                })
                .unwrap_or((row, col));

            if context.direction == LayoutDirection::Row && row != current_row
                || context.direction == LayoutDirection::Column && col != current_col
            {
                log::debug!("draw_elements: finished row/col current_row={current_row} current_col={current_col} flex_width={} flex_height={}", flex.w(), flex.h());
                flex.end();

                #[allow(clippy::cast_possible_truncation)]
                {
                    flex = match context.direction {
                        LayoutDirection::Row => group::Flex::default_fill().row(),
                        LayoutDirection::Column => group::Flex::default_fill().column(),
                    };
                    flex.set_clip_children(false);
                    flex.set_pad(0);
                }

                #[cfg(feature = "debug")]
                {
                    if *DEBUG.read().unwrap() {
                        flex.draw(|w| {
                            fltk::draw::set_draw_color(enums::Color::White);
                            fltk::draw::draw_rect(w.x(), w.y(), w.w(), w.h());
                        });
                    }
                }
            }

            row = current_row;
            col = current_col;

            if i == len - 1 {
                if let Some(widget) = self.draw_element(
                    Cow::Borrowed(&viewport),
                    element,
                    i,
                    depth + 1,
                    context,
                    event_sender,
                )? {
                    fixed_size(
                        direction,
                        element.container_element().and_then(|x| x.calculated_width),
                        element
                            .container_element()
                            .and_then(|x| x.calculated_height),
                        &mut flex,
                        &widget,
                    );
                }
                break;
            }
            if let Some(widget) = self.draw_element(
                Cow::Borrowed(&viewport),
                element,
                i,
                depth + 1,
                context.clone(),
                event_sender.clone(),
            )? {
                fixed_size(
                    direction,
                    element.container_element().and_then(|x| x.calculated_width),
                    element
                        .container_element()
                        .and_then(|x| x.calculated_height),
                    &mut flex,
                    &widget,
                );
            }
        }

        log::debug!(
            "draw_elements: finished draw: container_wrap_x={:?} container_wrap_y={:?} container_scroll_x={:?} container_scroll_y={:?} container={}",
            container_wrap_x.as_ref().map(|x| format!("({}, {})", x.wid(), x.hei())),
            container_wrap_y.as_ref().map(|x| format!("({}, {})", x.wid(), x.hei())),
            container_scroll_x.as_ref().map(|x| format!("({}, {})", x.wid(), x.hei())),
            container_scroll_y.as_ref().map(|x| format!("({}, {})", x.wid(), x.hei())),
            format!("({}, {})", container.w(), container.h()),
        );
        flex.end();

        if let Some(container) = inner_container {
            container.end();
        }

        if let Some(mut container) = container_wrap_x {
            log::debug!(
                "draw_elements: ending container_wrap_x {} ({}, {})",
                container.type_str(),
                container.wid(),
                container.hei(),
            );
            container.end();
        }
        if let Some(mut container) = container_wrap_y {
            log::debug!(
                "draw_elements: ending container_wrap_y {} ({}, {})",
                container.type_str(),
                container.wid(),
                container.hei(),
            );
            container.end();
        }
        if let Some(mut container) = container_scroll_x {
            log::debug!(
                "draw_elements: ending container_scroll_x {} ({}, {})",
                container.type_str(),
                container.wid(),
                container.hei(),
            );
            container.end();
        }
        if let Some(mut container) = container_scroll_y {
            log::debug!(
                "draw_elements: ending container_scroll_y {} ({}, {})",
                container.type_str(),
                container.wid(),
                container.hei(),
            );
            container.end();
        }
        log::debug!(
            "draw_elements: ending container {} ({}, {})",
            container.type_str(),
            container.wid(),
            container.hei(),
        );
        container.end();

        if log::log_enabled!(log::Level::Trace) {
            let mut hierarchy = String::new();

            let mut current = Some(flex.as_base_widget());
            while let Some(widget) = current.take() {
                hierarchy.push_str(&format!(
                    "\n\t({}, {}, {}, {})",
                    widget.x(),
                    widget.y(),
                    widget.w(),
                    widget.h()
                ));
                current = widget.parent().map(|x| x.as_base_widget());
            }

            log::trace!("draw_elements: hierarchy:{hierarchy}");
        }

        Ok(container)
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    fn draw_element(
        &self,
        viewport: Cow<'_, Option<Viewport>>,
        element: &Element,
        index: usize,
        depth: usize,
        mut context: Context,
        event_sender: Sender<AppEvent>,
    ) -> Result<Option<widget::Widget>, FltkError> {
        log::debug!("draw_element: element={element:?} index={index} depth={depth}");

        let mut flex_element = None;
        let mut other_element: Option<widget::Widget> = None;

        match element {
            Element::Raw { value } => {
                app::set_font_size(context.size);
                #[allow(unused_mut)]
                let mut frame = frame::Frame::default()
                    .with_label(value)
                    .with_align(enums::Align::Inside | enums::Align::Left);

                #[cfg(feature = "debug")]
                {
                    if *DEBUG.read().unwrap() {
                        frame.draw(|w| {
                            fltk::draw::set_draw_color(enums::Color::White);
                            fltk::draw::draw_rect(w.x(), w.y(), w.w(), w.h());
                        });
                    }
                }

                other_element = Some(frame.as_base_widget());
            }
            Element::Div { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Aside { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Header { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Footer { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Main { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Section { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Form { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Span { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Input(_) => {}
            Element::Button { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::Image { source, element } => {
                context = context.with_container(element);
                let width = element.calculated_width;
                let height = element.calculated_height;
                let mut frame = Frame::default_fill();

                #[cfg(feature = "debug")]
                {
                    if *DEBUG.read().unwrap() {
                        frame.draw(|w| {
                            fltk::draw::set_draw_color(enums::Color::White);
                            fltk::draw::draw_rect(w.x(), w.y(), w.w(), w.h());
                        });
                    }
                }

                if let Some(source) = source {
                    if source.starts_with("http") {
                        if let Err(e) = event_sender.send(AppEvent::RegisterImage {
                            viewport: viewport.deref().clone(),
                            source: source.to_owned(),
                            width: element.width.map(|_| width.unwrap()),
                            height: element.height.map(|_| height.unwrap()),
                            frame: frame.clone(),
                        }) {
                            log::error!(
                                "Failed to send LoadImage event with source={source}: {e:?}"
                            );
                        }
                    } else if let Ok(manifest_path) = std::env::var("CARGO_MANIFEST_DIR") {
                        if let Ok(path) = std::path::PathBuf::from_str(&manifest_path) {
                            let source = source
                                .chars()
                                .skip_while(|x| *x == '/' || *x == '\\')
                                .collect::<String>();

                            if let Some(path) = path
                                .parent()
                                .and_then(|x| x.parent())
                                .map(|x| x.join("app-website").join("public").join(source))
                            {
                                if let Ok(path) = path.canonicalize() {
                                    if path.is_file() {
                                        let image = SharedImage::load(path)?;

                                        if width.is_some() || height.is_some() {
                                            #[allow(clippy::cast_possible_truncation)]
                                            let width = calc_number(
                                                element.width.unwrap_or_default(),
                                                context.width,
                                            )
                                            .round()
                                                as i32;
                                            #[allow(clippy::cast_possible_truncation)]
                                            let height = calc_number(
                                                element.height.unwrap_or_default(),
                                                context.height,
                                            )
                                            .round()
                                                as i32;

                                            frame.set_size(width, height);
                                        }

                                        frame.set_image_scaled(Some(image));
                                    }
                                }
                            }
                        }
                    }
                }

                other_element = Some(frame.as_base_widget());
            }
            Element::Anchor { element, href } => {
                context = context.with_container(element);
                let mut elements =
                    self.draw_elements(viewport, element, depth, context, event_sender.clone())?;
                if let Some(href) = href.to_owned() {
                    elements.handle(move |_, ev| match ev {
                        Event::Push => true,
                        Event::Released => {
                            if let Err(e) =
                                event_sender.send(AppEvent::Navigate { href: href.clone() })
                            {
                                log::error!("Failed to navigate to href={href}: {e:?}");
                            }
                            true
                        }
                        _ => false,
                    });
                }
                flex_element = Some(elements);
            }
            Element::Heading { element, size } => {
                context = context.with_container(element);
                context.size = match size {
                    HeaderSize::H1 => 36,
                    HeaderSize::H2 => 30,
                    HeaderSize::H3 => 24,
                    HeaderSize::H4 => 20,
                    HeaderSize::H5 => 16,
                    HeaderSize::H6 => 12,
                };
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::OrderedList { element } | Element::UnorderedList { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
            Element::ListItem { element } => {
                context = context.with_container(element);
                flex_element =
                    Some(self.draw_elements(viewport, element, depth, context, event_sender)?);
            }
        }

        #[cfg(feature = "debug")]
        if let Some(flex_element) = &mut flex_element {
            if *DEBUG.read().unwrap() && (depth == 1 || index > 0) {
                let mut element_info = vec![];

                let mut child = Some(element);

                while let Some(element) = child.take() {
                    let element_name = element.tag_display_str();
                    let text = element_name.to_string();
                    let first = element_info.is_empty();

                    element_info.push(text);

                    if let Some(container) = element.container_element() {
                        let text = format!(
                            "    ({}, {}, {}, {})",
                            container.calculated_x.unwrap_or(0.0),
                            container.calculated_y.unwrap_or(0.0),
                            container.calculated_width.unwrap_or(0.0),
                            container.calculated_height.unwrap_or(0.0),
                        );

                        element_info.push(text);
                    }

                    if first {
                        let text = format!(
                            "    ({}, {}, {}, {})",
                            flex_element.x(),
                            flex_element.y(),
                            flex_element.w(),
                            flex_element.h(),
                        );

                        element_info.push(text);
                    }

                    if let Some(container) = element.container_element() {
                        child = container.elements.first();
                    }
                }

                flex_element.draw({
                    move |w| {
                        use fltk::draw;

                        draw::set_draw_color(enums::Color::Red);
                        draw::draw_rect(w.x(), w.y(), w.w(), w.h());
                        draw::set_font(fltk::draw::font(), 8);

                        let mut y_offset = 0;

                        for text in &element_info {
                            let (_t_x, _t_y, _t_w, t_h) = draw::text_extents(text);
                            y_offset += t_h;
                            draw::draw_text(text, w.x(), w.y() + y_offset);
                        }
                    }
                });
            }
        }

        Ok(flex_element.map(|x| x.as_base_widget()).or(other_element))
    }

    async fn listen(&self) {
        let Some(rx) = self.event_receiver.clone() else {
            moosicbox_assert::die_or_panic!("Cannot listen before app is started");
        };
        let renderer = self.clone();
        while let Ok(event) = rx.recv_async().await {
            log::debug!("received event {event:?}");
            match event {
                AppEvent::Navigate { href } => {
                    if let Err(e) = self.sender.send(href) {
                        log::error!("Failed to send navigation href: {e:?}");
                    }
                }
                AppEvent::Resize {} => {}
                AppEvent::MouseWheel {} => {
                    {
                        let values = {
                            let value = renderer
                                .viewport_listener_join_handle
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .take();
                            if let Some((handle, cancel)) = value {
                                Some((handle, cancel))
                            } else {
                                None
                            }
                        };
                        if let Some((handle, cancel)) = values {
                            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                            let _ = handle.await;
                        }
                    }

                    let cancel = Arc::new(AtomicBool::new(false));
                    let handle = moosicbox_task::spawn("check_viewports", {
                        let renderer = renderer.clone();
                        let cancel = cancel.clone();
                        async move {
                            renderer.check_viewports(&cancel);
                        }
                    });

                    renderer
                        .viewport_listener_join_handle
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .replace((handle, cancel));
                }
                AppEvent::RegisterImage {
                    viewport,
                    source,
                    width,
                    height,
                    frame,
                } => {
                    renderer.register_image(viewport, source, width, height, &frame);
                }
                AppEvent::LoadImage {
                    source,
                    width,
                    height,
                    frame,
                } => {
                    moosicbox_task::spawn("renderer: load_image", async move {
                        Self::load_image(source, width, height, frame).await
                    });
                }
                AppEvent::UnloadImage { mut frame } => {
                    Self::set_frame_image(&mut frame, None);
                }
            }
        }
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
    }
}

pub struct FltkRenderRunner {
    app: App,
}

impl RenderRunner for FltkRenderRunner {
    /// # Errors
    ///
    /// Will error if FLTK fails to run the event loop.
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        let app = self.app;
        log::debug!("run: starting");
        app.run()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
        log::debug!("run: finished");
        Ok(())
    }
}

#[async_trait]
impl Renderer for FltkRenderer {
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    ///
    /// # Errors
    ///
    /// Will error if FLTK app fails to start
    async fn init(
        &mut self,
        width: u16,
        height: u16,
        x: Option<i32>,
        y: Option<i32>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        let app = app::App::default();
        self.app.replace(app);

        let mut window = Window::default()
            .with_size(i32::from(width), i32::from(height))
            .with_label("MoosicBox");

        self.window.replace(window.clone());
        self.width
            .store(i32::from(width), std::sync::atomic::Ordering::SeqCst);
        self.height
            .store(i32::from(height), std::sync::atomic::Ordering::SeqCst);

        app::set_background_color(24, 26, 27);
        app::set_foreground_color(255, 255, 255);
        app::set_frame_type(enums::FrameType::NoBox);
        fltk::image::Image::set_scaling_algorithm(fltk::image::RgbScaling::Bilinear);
        RgbImage::set_scaling_algorithm(fltk::image::RgbScaling::Bilinear);

        let (tx, rx) = flume::unbounded();
        self.event_sender.replace(tx);
        self.event_receiver.replace(rx);

        window.handle({
            let renderer = self.clone();
            move |window, ev| {
                log::trace!("Received event: {ev}");
                match ev {
                    Event::Resize => {
                        renderer.handle_resize(window);
                        if let Some(sender) = &renderer.event_sender {
                            let _ = sender.send(AppEvent::Resize {});
                        }
                        true
                    }
                    Event::MouseWheel => {
                        if let Some(sender) = &renderer.event_sender {
                            let _ = sender.send(AppEvent::MouseWheel {});
                        }
                        false
                    }
                    #[cfg(feature = "debug")]
                    Event::KeyUp => {
                        let key = app::event_key();
                        log::debug!("Received key press {key:?}");
                        if key == enums::Key::F3 {
                            let value = {
                                let mut handle = DEBUG.write().unwrap();
                                let value = *handle;
                                let value = !value;
                                *handle = value;
                                value
                            };
                            log::debug!("Set DEBUG to {value}");
                            if let Err(e) = renderer.perform_render() {
                                log::error!("Failed to draw elements: {e:?}");
                            }
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
        });

        window.set_callback(|_| {
            if fltk::app::event() == fltk::enums::Event::Close {
                app::quit();
            }
        });

        if let (Some(x), Some(y)) = (x, y) {
            log::debug!("start: positioning window x={x} y={y}");
            window = window.with_pos(x, y);
        } else {
            log::debug!("start: centering window");
            window = window.center_screen();
        }
        window.end();
        window.make_resizable(true);
        window.show();
        log::debug!("start: started");

        log::debug!("start: spawning listen thread");
        moosicbox_task::spawn("renderer_fltk::start: listen", {
            let renderer = self.clone();
            async move {
                log::debug!("start: listening");
                renderer.listen().await;
                Ok::<_, Box<dyn std::error::Error + Send + 'static>>(())
            }
        });

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if FLTK fails to run the event loop.
    async fn to_runner(
        &mut self,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        let Some(app) = self.app else {
            moosicbox_assert::die_or_panic!("Cannot listen before app is started");
        };

        Ok(Box::new(FltkRenderRunner { app }))
    }

    /// # Errors
    ///
    /// Will error if FLTK fails to render the elements.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    fn render(
        &mut self,
        elements: ContainerElement,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!("render: {elements:?}");

        {
            *self.elements.lock().unwrap() = elements;
        }

        self.perform_render()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + 'static>)?;

        Ok(())
    }
}

#[derive(Clone)]
struct Context {
    size: u16,
    direction: LayoutDirection,
    overflow_x: LayoutOverflow,
    overflow_y: LayoutOverflow,
    width: f32,
    height: f32,
}

impl Context {
    fn new(width: f32, height: f32) -> Self {
        Self {
            size: 12,
            direction: LayoutDirection::default(),
            overflow_x: LayoutOverflow::default(),
            overflow_y: LayoutOverflow::default(),
            width,
            height,
        }
    }

    fn with_container(mut self, container: &ContainerElement) -> Self {
        self.direction = container.direction;
        self.overflow_x = container.overflow_x;
        self.overflow_y = container.overflow_y;
        self.width = container
            .calculated_width
            .or_else(|| container.width.map(|x| calc_number(x, self.width)))
            .unwrap_or(self.width);
        self.height = container
            .calculated_height
            .or_else(|| container.height.map(|x| calc_number(x, self.height)))
            .unwrap_or(self.height);
        self
    }
}

#[derive(Clone)]
pub struct Viewport {
    widget: widget::Widget,
    parent: Option<Box<Viewport>>,
    position: Arc<Box<dyn ViewportPosition + Send + Sync>>,
}

impl std::fmt::Debug for Viewport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut binding = f.debug_struct("Viewport");
        let x = binding
            .field("x", &self.x())
            .field("y", &self.y())
            .field("w", &self.w())
            .field("h", &self.h());

        if let Some(parent) = &self.parent {
            x.field("parent", &parent);
        }

        x.finish_non_exhaustive()
    }
}

impl Viewport {
    fn new(parent: Option<Self>, position: impl ViewportPosition + Send + Sync + 'static) -> Self {
        Self {
            widget: position.viewport_as_base_widget(),
            parent: parent.map(Box::new),
            position: Arc::new(Box::new(position)),
        }
    }

    fn x(&self) -> i32 {
        self.position.viewport_x()
    }

    fn y(&self) -> i32 {
        self.position.viewport_y()
    }

    fn w(&self) -> i32 {
        self.position.viewport_w()
    }

    fn h(&self) -> i32 {
        self.position.viewport_h()
    }

    fn is_widget_visible(&self, widget: &widget::Widget) -> (bool, u32) {
        let (visible_in_current_viewport, dist) =
            self.position.is_widget_visible(&self.widget, widget);

        // FIXME: This doesn't correctly check the position leaf widget (the param above)
        // within this viewport itself, but this probably isn't a huge issue since nested
        // `Viewport`s isn't super likely yet.
        if visible_in_current_viewport {
            self.parent
                .as_ref()
                .map_or((visible_in_current_viewport, dist), |parent| {
                    let (parent_visible, parent_dist) = parent.is_widget_visible(&self.widget);

                    (
                        visible_in_current_viewport && parent_visible,
                        dist + parent_dist,
                    )
                })
        } else {
            (false, dist)
        }
    }
}

trait ViewportPosition {
    fn viewport_x(&self) -> i32;
    fn viewport_y(&self) -> i32;
    fn viewport_w(&self) -> i32;
    fn viewport_h(&self) -> i32;
    fn viewport_as_base_widget(&self) -> widget::Widget;

    fn is_widget_visible(
        &self,
        this_widget: &widget::Widget,
        widget: &widget::Widget,
    ) -> (bool, u32) {
        let mut x = widget.x();
        let mut y = widget.y();
        let w = widget.w();
        let h = widget.h();
        log::trace!("is_widget_visible: widget x={x} y={y} w={w} h={h}");

        log::trace!(
            "is_widget_visible: {x} -= {} = {}",
            this_widget.x(),
            x - this_widget.x()
        );
        x -= this_widget.x();
        log::trace!(
            "is_widget_visible: {y} -= {} = {}",
            this_widget.y(),
            y - this_widget.y()
        );
        y -= this_widget.y();

        let viewport_w = self.viewport_w();
        let viewport_h = self.viewport_h();

        #[allow(clippy::cast_sign_loss)]
        let dist_x = std::cmp::max(0, std::cmp::max(-(x + w), x - viewport_w)) as u32;
        #[allow(clippy::cast_sign_loss)]
        let dist_y = std::cmp::max(0, std::cmp::max(-(y + h), y - viewport_h)) as u32;

        let dist = std::cmp::max(dist_x, dist_y);

        log::trace!(
            "is_widget_visible:\n\t\
            {dist_x} == 0 &&\n\t\
            {dist_y} == 0"
        );

        if dist_x == 0 && dist_y == 0 {
            log::trace!("is_widget_visible: visible");
            return (true, dist);
        }

        log::trace!("is_widget_visible: not visible");

        (false, dist)
    }
}

impl std::fmt::Debug for dyn ViewportPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportPosition")
            .field("x", &self.viewport_x())
            .field("y", &self.viewport_y())
            .field("w", &self.viewport_w())
            .field("h", &self.viewport_h())
            .finish()
    }
}

impl std::fmt::Debug for Box<dyn ViewportPosition + Send + Sync> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportPosition")
            .field("x", &self.viewport_x())
            .field("y", &self.viewport_y())
            .field("w", &self.viewport_w())
            .field("h", &self.viewport_h())
            .finish()
    }
}

impl ViewportPosition for group::Scroll {
    fn viewport_x(&self) -> i32 {
        self.xposition()
    }

    fn viewport_y(&self) -> i32 {
        self.yposition()
    }

    fn viewport_w(&self) -> i32 {
        self.w()
    }

    fn viewport_h(&self) -> i32 {
        self.h()
    }

    fn viewport_as_base_widget(&self) -> widget::Widget {
        self.as_base_widget()
    }
}

impl From<group::Scroll> for Box<dyn ViewportPosition + Send + Sync> {
    fn from(value: group::Scroll) -> Self {
        Box::new(value)
    }
}

trait Group {
    fn end(&mut self);
    fn type_str(&self) -> &'static str;
    fn wid(&self) -> i32;
    fn hei(&self) -> i32;
}

impl Group for group::Flex {
    fn end(&mut self) {
        <Self as GroupExt>::end(self);
    }

    fn type_str(&self) -> &'static str {
        "Flex"
    }

    fn wid(&self) -> i32 {
        <Self as fltk::prelude::WidgetExt>::w(self)
    }

    fn hei(&self) -> i32 {
        <Self as fltk::prelude::WidgetExt>::h(self)
    }
}

impl From<group::Flex> for Box<dyn Group> {
    fn from(value: group::Flex) -> Self {
        Box::new(value)
    }
}

impl Group for group::Scroll {
    fn end(&mut self) {
        <Self as GroupExt>::end(self);
    }

    fn type_str(&self) -> &'static str {
        "Scroll"
    }

    fn wid(&self) -> i32 {
        <Self as fltk::prelude::WidgetExt>::w(self)
    }

    fn hei(&self) -> i32 {
        <Self as fltk::prelude::WidgetExt>::h(self)
    }
}

impl From<group::Scroll> for Box<dyn Group> {
    fn from(value: group::Scroll) -> Self {
        Box::new(value)
    }
}

fn fixed_size<W: WidgetExt>(
    direction: LayoutDirection,
    width: Option<f32>,
    height: Option<f32>,
    container: &mut group::Flex,
    element: &W,
) {
    call_fixed_size(direction, width, height, move |size| {
        container.fixed(element, size);
    });
}

#[inline]
fn call_fixed_size<F: FnMut(i32)>(
    direction: LayoutDirection,
    width: Option<f32>,
    height: Option<f32>,
    mut f: F,
) {
    match direction {
        LayoutDirection::Row => {
            if let Some(width) = width {
                #[allow(clippy::cast_possible_truncation)]
                f(width.round() as i32);
                log::debug!("call_fixed_size: setting fixed width={width}");
            } else {
                log::debug!(
                    "call_fixed_size: not setting fixed width size width={width:?} height={height:?}"
                );
            }
        }
        LayoutDirection::Column => {
            if let Some(height) = height {
                #[allow(clippy::cast_possible_truncation)]
                f(height.round() as i32);
                log::debug!("call_fixed_size: setting fixed height={height})");
            } else {
                log::debug!(
                    "call_fixed_size: not setting fixed height size width={width:?} height={height:?}"
                );
            }
        }
    }
}
