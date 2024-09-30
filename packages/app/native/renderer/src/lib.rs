#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    collections::HashMap,
    pin::Pin,
    str::FromStr as _,
    sync::{atomic::AtomicI32, Arc, LazyLock, Mutex, RwLock},
};

use fltk::{
    app::{self, App},
    enums::{self, Event},
    frame::{self, Frame},
    group,
    image::{RgbImage, SharedImage},
    prelude::*,
    window::{DoubleWindow, Window},
};
use flume::{Receiver, Sender};
use futures::Future;
use moosicbox_gigachad_transformer::{
    calc::{calc_number, Calc as _},
    ContainerElement, Element, ElementList, HeaderSize, LayoutDirection, LayoutOverflow,
};
use thiserror::Error;

type RouteFunc = Arc<
    Box<
        dyn (Fn() -> Pin<
                Box<dyn Future<Output = Result<ElementList, Box<dyn std::error::Error>>> + Send>,
            >) + Send
            + Sync,
    >,
>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutePath {
    Literal(String),
    Literals(Vec<String>),
}

impl RoutePath {
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        match self {
            Self::Literal(route_path) => route_path == path,
            Self::Literals(route_paths) => route_paths.iter().any(|x| x == path),
        }
    }
}

impl From<&str> for RoutePath {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_owned())
    }
}

impl From<&[&str; 1]> for RoutePath {
    fn from(value: &[&str; 1]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 2]> for RoutePath {
    fn from(value: &[&str; 2]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 3]> for RoutePath {
    fn from(value: &[&str; 3]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 4]> for RoutePath {
    fn from(value: &[&str; 4]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 5]> for RoutePath {
    fn from(value: &[&str; 5]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 6]> for RoutePath {
    fn from(value: &[&str; 6]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 7]> for RoutePath {
    fn from(value: &[&str; 7]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 8]> for RoutePath {
    fn from(value: &[&str; 8]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 9]> for RoutePath {
    fn from(value: &[&str; 9]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 10]> for RoutePath {
    fn from(value: &[&str; 10]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str]> for RoutePath {
    fn from(value: &[&str]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<Vec<&str>> for RoutePath {
    fn from(value: Vec<&str>) -> Self {
        Self::Literals(value.into_iter().map(ToString::to_string).collect())
    }
}

impl From<String> for RoutePath {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}

impl From<&[String]> for RoutePath {
    fn from(value: &[String]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&String]> for RoutePath {
    fn from(value: &[&String]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<Vec<String>> for RoutePath {
    fn from(value: Vec<String>) -> Self {
        Self::Literals(value)
    }
}

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
    Navigate {
        href: String,
    },
    LoadImage {
        source: String,
        width: Option<f32>,
        height: Option<f32>,
        frame: Frame,
    },
}

#[derive(Clone)]
pub struct Renderer {
    app: Option<App>,
    window: Option<DoubleWindow>,
    elements: Arc<Mutex<ElementList>>,
    root: Arc<RwLock<Option<group::Flex>>>,
    routes: Arc<RwLock<Vec<(RoutePath, RouteFunc)>>>,
    width: Arc<AtomicI32>,
    height: Arc<AtomicI32>,
    event_sender: Option<Sender<AppEvent>>,
    event_receiver: Option<Receiver<AppEvent>>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            app: None,
            window: None,
            elements: Arc::new(Mutex::new(ElementList::default())),
            root: Arc::new(RwLock::new(None)),
            routes: Arc::new(RwLock::new(vec![])),
            width: Arc::new(AtomicI32::new(0)),
            height: Arc::new(AtomicI32::new(0)),
            event_sender: None,
            event_receiver: None,
        }
    }

    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    ///
    /// # Errors
    ///
    /// Will error if FLTK app fails to start
    pub fn start(mut self, width: u16, height: u16) -> Result<Self, FltkError> {
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

        let (tx, rx) = flume::unbounded();
        self.event_sender.replace(tx);
        self.event_receiver.replace(rx);

        window.handle({
            let renderer = self.clone();
            move |window, ev| match ev {
                Event::Resize => {
                    if renderer.width.load(std::sync::atomic::Ordering::SeqCst) != window.width()
                        || renderer.height.load(std::sync::atomic::Ordering::SeqCst)
                            != window.height()
                    {
                        renderer
                            .width
                            .store(window.width(), std::sync::atomic::Ordering::SeqCst);
                        renderer
                            .height
                            .store(window.height(), std::sync::atomic::Ordering::SeqCst);
                        log::debug!(
                            "event resize: width={} height={}",
                            window.width(),
                            window.height()
                        );

                        #[allow(clippy::cast_precision_loss)]
                        {
                            renderer
                                .elements
                                .lock()
                                .unwrap()
                                .calc(window.width() as f32, window.height() as f32);
                        }

                        if let Err(e) = renderer.perform_render() {
                            log::error!("Failed to draw elements: {e:?}");
                        }
                    }
                    true
                }
                _ => false,
            }
        });

        window.set_callback(|_| {
            if fltk::app::event() == fltk::enums::Event::Close {
                app::quit();
            }
        });

        window = window.center_screen();
        window.end();
        window.make_resizable(true);
        window.show();

        Ok(self)
    }

    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[must_use]
    pub fn with_route<
        F: Future<Output = Result<ElementList, E>> + Send + 'static,
        E: Into<Box<dyn std::error::Error>>,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn() -> F + Send + Sync + Clone + 'static,
    ) -> Self {
        self.routes.write().unwrap().push((
            route.into(),
            Arc::new(Box::new(move || {
                Box::pin({
                    let handler = handler.clone();
                    async move { handler().await.map_err(Into::into) }
                })
            })),
        ));
        self
    }

    async fn load_image(
        source: String,
        width: Option<f32>,
        height: Option<f32>,
        mut frame: Frame,
    ) -> Result<(), LoadImageError> {
        type ImageCache = LazyLock<Arc<tokio::sync::RwLock<HashMap<String, SharedImage>>>>;
        static IMAGE_CACHE: ImageCache =
            LazyLock::new(|| Arc::new(tokio::sync::RwLock::new(HashMap::new())));

        let key = format!("{source}:{width:?}:{height:?}");

        let cached_image = { IMAGE_CACHE.read().await.get(&key).cloned() };

        let image = if let Some(image) = cached_image {
            image
        } else {
            let data = reqwest::get(source).await?.bytes().await?;
            let image = image::load_from_memory_with_format(&data, image::ImageFormat::WebP)?;
            let image = RgbImage::new(
                image.as_bytes(),
                image.width().try_into().unwrap(),
                image.height().try_into().unwrap(),
                enums::ColorDepth::Rgb8,
            )?;
            let image = SharedImage::from_image(image)?;
            IMAGE_CACHE.write().await.insert(key, image.clone());

            image
        };

        if width.is_some() || height.is_some() {
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_precision_loss)]
            let width = width.unwrap_or(image.width() as f32).round() as i32;
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_precision_loss)]
            let height = height.unwrap_or(image.height() as f32).round() as i32;

            frame.set_size(width, height);
        }

        frame.set_image_scaled(Some(image));
        frame.set_damage(true);
        app::awake();

        Ok(())
    }

    pub async fn listen(&self) {
        let Some(rx) = self.event_receiver.clone() else {
            moosicbox_assert::die_or_panic!("Cannot listen before app is started");
        };
        let mut renderer = self.clone();
        while let Ok(event) = rx.recv_async().await {
            log::debug!("received event {event:?}");
            match event {
                AppEvent::Navigate { href } => {
                    if let Err(e) = renderer.navigate(&href).await {
                        log::error!("Failed to navigate: {e:?}");
                    }
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
            }
        }
    }

    /// # Errors
    ///
    /// Will error if FLTK fails to render the navigation result.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    pub async fn navigate(&mut self, path: &str) -> Result<(), FltkError> {
        let handler = {
            self.routes
                .read()
                .unwrap()
                .iter()
                .find(|(route, _)| route.matches(path))
                .cloned()
                .map(|(_, handler)| handler)
        };
        if let Some(handler) = handler {
            match handler().await {
                Ok(elements) => {
                    self.render(elements)?;
                }
                Err(e) => {
                    log::error!("Failed to fetch route elements: {e:?}");
                }
            }
        } else {
            log::warn!("Invalid navigation path={path:?}");
        }

        Ok(())
    }

    fn perform_render(&self) -> Result<(), FltkError> {
        let (Some(mut window), Some(tx)) = (self.window.clone(), self.event_sender.clone()) else {
            moosicbox_assert::die_or_panic!("Cannot perform_render before app is started");
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
            let elements: &[Element] = &self.elements.lock().unwrap();
            root.replace(draw_elements(
                elements,
                #[allow(clippy::cast_precision_loss)]
                Context::new(window.width() as f32, window.height() as f32),
                tx,
            )?);
        }
        window.end();
        window.flush();
        app::awake();
        log::debug!("perform_render: finished");
        Ok(())
    }

    /// # Errors
    ///
    /// Will error if FLTK fails to render the elements.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    pub fn render(&mut self, mut elements: ElementList) -> Result<(), FltkError> {
        log::debug!("render: {elements:?}");

        #[allow(clippy::cast_precision_loss)]
        {
            elements.calc(
                self.width.load(std::sync::atomic::Ordering::SeqCst) as f32,
                self.height.load(std::sync::atomic::Ordering::SeqCst) as f32,
            );

            *self.elements.lock().unwrap() = elements;
        }

        self.perform_render()?;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if FLTK fails to run the event loop.
    pub fn run(self) -> Result<(), FltkError> {
        let Some(app) = self.app else {
            moosicbox_assert::die_or_panic!("Cannot listen before app is started");
        };
        app.run()
    }
}

#[derive(Clone)]
struct Context {
    size: u16,
    direction: LayoutDirection,
    overflow: LayoutOverflow,
    width: f32,
    height: f32,
}

impl Context {
    fn new(width: f32, height: f32) -> Self {
        Self {
            size: 12,
            direction: LayoutDirection::default(),
            overflow: LayoutOverflow::default(),
            width,
            height,
        }
    }

    fn with_container(mut self, container: &ContainerElement) -> Self {
        self.direction = container.direction;
        self.overflow = container.overflow;
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

fn draw_elements(
    elements: &[Element],
    context: Context,
    event_sender: Sender<AppEvent>,
) -> Result<group::Flex, FltkError> {
    log::debug!("draw_elements: elements={elements:?}");

    let outer_flex = match context.overflow {
        LayoutOverflow::Scroll | LayoutOverflow::Squash => None,
        LayoutOverflow::Wrap => Some(match context.direction {
            LayoutDirection::Row => group::Flex::default_fill().column(),
            LayoutDirection::Column => group::Flex::default_fill().row(),
        }),
    };

    let flex = group::Flex::default_fill();
    let mut flex = match context.direction {
        LayoutDirection::Row => flex.row(),
        LayoutDirection::Column => flex.column(),
    };

    let Some(first) = elements.first() else {
        flex.end();
        if let Some(outer) = outer_flex {
            outer.end();
            return Ok(outer);
        }
        return Ok(flex);
    };

    let (mut row, mut col) = first
        .container_element()
        .and_then(|x| {
            x.calculated_position.as_ref().and_then(|x| match x {
                moosicbox_gigachad_transformer::LayoutPosition::Wrap { row, col } => {
                    Some((*row, *col))
                }
                moosicbox_gigachad_transformer::LayoutPosition::Default => None,
            })
        })
        .unwrap_or((0, 0));

    for (i, element) in elements.iter().enumerate() {
        let (current_row, current_col) = element
            .container_element()
            .and_then(|x| {
                x.calculated_position.as_ref().and_then(|x| match x {
                    moosicbox_gigachad_transformer::LayoutPosition::Wrap { row, col } => {
                        Some((*row, *col))
                    }
                    moosicbox_gigachad_transformer::LayoutPosition::Default => None,
                })
            })
            .unwrap_or((row, col));

        if context.direction == LayoutDirection::Row && row != current_row
            || context.direction == LayoutDirection::Column && col != current_col
        {
            flex.end();

            flex = match context.direction {
                LayoutDirection::Row => group::Flex::default_fill().row(),
                LayoutDirection::Column => group::Flex::default_fill().column(),
            };
        }

        row = current_row;
        col = current_col;

        if i == elements.len() - 1 {
            draw_element(element, context, &mut flex, event_sender)?;
            break;
        }
        draw_element(element, context.clone(), &mut flex, event_sender.clone())?;
    }

    flex.end();
    if let Some(outer) = outer_flex {
        outer.end();
        return Ok(outer);
    }
    Ok(flex)
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
fn draw_element(
    element: &Element,
    mut context: Context,
    flex: &mut group::Flex,
    event_sender: Sender<AppEvent>,
) -> Result<Option<Box<dyn WidgetExt>>, FltkError> {
    log::debug!(
        "draw_element: element={element:?} flex_width={} flex_height={} bounds={:?}",
        flex.width(),
        flex.height(),
        flex.bounds()
    );

    let direction = context.direction;
    let mut width = None;
    let mut height = None;
    let mut flex_element = None;
    let mut other_element: Option<Box<dyn WidgetExt>> = None;

    match element {
        Element::Raw { value } => {
            app::set_font_size(context.size);
            let frame = frame::Frame::default()
                .with_label(value)
                .with_align(enums::Align::Inside | enums::Align::Left);

            other_element = Some(Box::new(frame));
        }
        Element::Div { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Aside { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Header { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Footer { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Main { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Section { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Form { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Span { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Input(_) => {}
        Element::Button { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Image { source, element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            let mut frame = Frame::default_fill();

            if let Some(source) = source {
                if source.starts_with("http") {
                    if let Err(e) = event_sender.send(AppEvent::LoadImage {
                        source: source.to_owned(),
                        width: element.width.map(|_| width.unwrap()),
                        height: element.height.map(|_| height.unwrap()),
                        frame: frame.clone(),
                    }) {
                        log::error!("Failed to send LoadImage event with source={source}: {e:?}");
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
                            .and_then(|x| x.parent())
                            .map(|x| x.join("MoosicBoxUI").join("public").join(source))
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

            other_element = Some(Box::new(frame));
        }
        Element::Anchor { element, href } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            let mut elements = draw_elements(&element.elements, context, event_sender.clone())?;
            if let Some(href) = href.to_owned() {
                elements.handle(move |_, ev| match ev {
                    Event::Push => true,
                    Event::Released => {
                        if let Err(e) = event_sender.send(AppEvent::Navigate { href: href.clone() })
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
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::OrderedList { element } | Element::UnorderedList { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::ListItem { element } => {
            context = context.with_container(element);
            width = element.calculated_width;
            height = element.calculated_height;
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
    }

    if let Some(flex_element) = &flex_element {
        match direction {
            LayoutDirection::Row => {
                if let Some(width) = width {
                    #[allow(clippy::cast_possible_truncation)]
                    flex.fixed(flex_element, width.round() as i32);
                    log::debug!("draw_element: setting fixed width={width}");
                } else {
                    log::debug!(
                        "draw_element: not setting fixed width size width={width:?} height={height:?}"
                    );
                }
            }
            LayoutDirection::Column => {
                if let Some(height) = height {
                    #[allow(clippy::cast_possible_truncation)]
                    flex.fixed(flex_element, height.round() as i32);
                    log::debug!("draw_element: setting fixed height={height}");
                } else {
                    log::debug!(
                        "draw_element: not setting fixed height size width={width:?} height={height:?}"
                    );
                }
            }
        }
    }

    Ok(flex_element
        .map(|x| Box::new(x) as Box<dyn WidgetExt>)
        .or(other_element))
}
