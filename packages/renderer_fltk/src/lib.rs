#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

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
use moosicbox_htmx_transformer::{
    ContainerElement, Element, ElementList, HeaderSize, LayoutDirection, Number,
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
        width: Option<Number>,
        height: Option<Number>,
        context_width: f32,
        context_height: f32,
        frame: Frame,
    },
}

#[derive(Clone)]
pub struct Renderer {
    app: App,
    window: DoubleWindow,
    elements: Arc<Mutex<ElementList>>,
    root: Arc<RwLock<Option<group::Flex>>>,
    routes: Arc<RwLock<Vec<(String, RouteFunc)>>>,
    width: Arc<AtomicI32>,
    height: Arc<AtomicI32>,
    event_sender: Sender<AppEvent>,
    event_receiver: Receiver<AppEvent>,
}

impl Renderer {
    pub fn new(width: u16, height: u16) -> Result<Self, FltkError> {
        let app = app::App::default();
        let mut window = Window::default()
            .with_size(width as i32, height as i32)
            .with_label("MoosicBox");

        app::set_background_color(24, 26, 27);

        let (tx, rx) = flume::unbounded();
        let renderer = Self {
            app,
            window: window.clone(),
            elements: Arc::new(Mutex::new(ElementList::default())),
            root: Arc::new(RwLock::new(None)),
            routes: Arc::new(RwLock::new(vec![])),
            width: Arc::new(AtomicI32::new(width as i32)),
            height: Arc::new(AtomicI32::new(height as i32)),
            event_sender: tx,
            event_receiver: rx,
        };

        window.handle({
            let mut renderer = renderer.clone();
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
                        if let Err(e) = renderer.perform_render() {
                            log::error!("Failed to draw elements: {e:?}");
                        }
                    }
                    true
                }
                _ => false,
            }
        });

        window = window.center_screen();
        window.end();
        window.make_resizable(true);
        window.show();

        Ok(renderer)
    }

    pub fn with_route<
        F: Future<Output = Result<ElementList, E>> + Send + 'static,
        E: Into<Box<dyn std::error::Error>>,
    >(
        self,
        route: &str,
        handler: impl Fn() -> F + Send + Sync + Clone + 'static,
    ) -> Self {
        self.routes.write().unwrap().push((
            route.to_string(),
            Arc::new(Box::new(move || {
                Box::pin({
                    let handler = handler.clone();
                    async move { handler().await.map_err(|e| e.into()) }
                })
            })),
        ));
        self
    }

    async fn load_image(
        source: String,
        width: Option<Number>,
        height: Option<Number>,
        context_width: f32,
        context_height: f32,
        mut frame: Frame,
    ) -> Result<(), LoadImageError> {
        type ImageCache = LazyLock<Arc<tokio::sync::RwLock<HashMap<String, (Vec<u8>, u32, u32)>>>>;
        static IMAGE_CACHE: ImageCache =
            LazyLock::new(|| Arc::new(tokio::sync::RwLock::new(HashMap::new())));

        let key = format!("{source}:{width:?}:{height:?}");

        let cached_bytes = { IMAGE_CACHE.read().await.get(&key).cloned() };
        let from_cache = cached_bytes.is_some();

        let (bytes, img_width, img_height) = if let Some((bytes, width, height)) = cached_bytes {
            (bytes, width, height)
        } else {
            let data = reqwest::get(source).await?.bytes().await?;
            let img = image::load_from_memory_with_format(&data, image::ImageFormat::WebP)?;
            (img.as_bytes().to_vec(), img.width(), img.height())
        };

        let mut image = RgbImage::new(
            &bytes,
            img_width as i32,
            img_height as i32,
            enums::ColorDepth::Rgb8,
        )?;

        if !from_cache {
            IMAGE_CACHE
                .write()
                .await
                .insert(key, (bytes, img_width, img_height));
        }

        if width.is_some() || height.is_some() {
            let width = calc_number(width.unwrap_or_default(), context_width).round() as i32;
            let height = calc_number(height.unwrap_or_default(), context_height).round() as i32;

            image.scale(width, height, true, true);
        }

        frame.set_image(Some(image));
        frame.set_damage(true);

        Ok(())
    }

    pub async fn listen(&self) {
        let mut renderer = self.clone();
        while let Ok(event) = renderer.event_receiver.recv_async().await {
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
                    context_width,
                    context_height,
                    frame,
                } => {
                    moosicbox_task::spawn("renderer_fltk: load_image", async move {
                        Self::load_image(
                            source,
                            width,
                            height,
                            context_width,
                            context_height,
                            frame,
                        )
                        .await
                    });
                }
            }
        }
    }

    pub async fn navigate(&mut self, path: &str) -> Result<(), FltkError> {
        let handler = {
            self.routes
                .read()
                .unwrap()
                .iter()
                .find(|(route, _)| route == path)
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

    fn perform_render(&mut self) -> Result<(), FltkError> {
        log::debug!("perform_render: started");
        let mut root = self.root.write().unwrap();
        if let Some(root) = root.take() {
            self.window.remove(&root);
            log::debug!("perform_render: removed root");
        }
        self.window.begin();
        log::debug!("perform_render: begin");
        let elements: &[Element] = &self.elements.lock().unwrap();
        root.replace(draw_elements(
            elements,
            Context::new(self.window.width() as f32, self.window.height() as f32),
            self.event_sender.clone(),
        )?);
        self.window.end();
        self.window.flush();
        log::debug!("perform_render: finished");
        Ok(())
    }

    pub fn render(&mut self, elements: ElementList) -> Result<(), FltkError> {
        log::debug!("render: {elements:?}");
        {
            *self.elements.lock().unwrap() = elements;
        }

        self.perform_render()?;

        Ok(())
    }

    pub fn run(self) -> Result<(), FltkError> {
        self.app.run()
    }
}

#[derive(Clone)]
struct Context {
    size: u16,
    direction: LayoutDirection,
    width: f32,
    height: f32,
}

impl Context {
    fn new(width: f32, height: f32) -> Self {
        Self {
            size: 12,
            direction: LayoutDirection::Column,
            width,
            height,
        }
    }

    fn with_container(mut self, container: &ContainerElement) -> Context {
        self.direction = container.direction;
        self.width = container
            .width
            .map(|x| calc_number(x, self.width))
            .unwrap_or(self.width);
        self.height = container
            .height
            .map(|x| calc_number(x, self.height))
            .unwrap_or(self.height);
        self
    }
}

fn calc_number(number: Number, container: f32) -> f32 {
    match number {
        Number::Real(x) => x,
        Number::Integer(x) => x as f32,
        Number::RealPercent(x) => container * (x / 100.0),
        Number::IntegerPercent(x) => container * (x as f32 / 100.0),
    }
}

fn draw_elements(
    elements: &[Element],
    context: Context,
    event_sender: Sender<AppEvent>,
) -> Result<group::Flex, FltkError> {
    log::debug!("draw_elements: elements={elements:?}");

    let flex = group::Flex::default_fill();
    let mut flex = match context.direction {
        LayoutDirection::Row => flex.row(),
        LayoutDirection::Column => flex.column(),
    };

    for (i, element) in elements.iter().enumerate() {
        if i == elements.len() - 1 {
            draw_element(element, context, &mut flex, event_sender)?;
            break;
        }
        draw_element(element, context.clone(), &mut flex, event_sender.clone())?;
    }

    flex.end();

    Ok(flex)
}

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
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Aside { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Header { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Footer { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Main { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Section { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Form { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Span { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Input(_) => {}
        Element::Button { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Image {
            source,
            width,
            height,
        } => {
            let mut frame = Frame::default_fill();

            if let Some(source) = source {
                if source.starts_with("http") {
                    if let Err(e) = event_sender.send(AppEvent::LoadImage {
                        source: source.to_owned(),
                        width: width.to_owned(),
                        height: height.to_owned(),
                        context_width: context.width,
                        context_height: context.height,
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
                                    let mut image = SharedImage::load(path)?;

                                    if width.is_some() || height.is_some() {
                                        let width =
                                            calc_number(width.unwrap_or_default(), context.width)
                                                .round()
                                                as i32;
                                        let height =
                                            calc_number(height.unwrap_or_default(), context.height)
                                                .round()
                                                as i32;

                                        image.scale(width, height, true, true);
                                    }

                                    frame.set_image(Some(image));
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
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            let mut elements = draw_elements(&element.elements, context, event_sender.clone())?;
            if let Some(href) = href.to_owned() {
                let event_sender = event_sender.clone();
                elements.handle(move |_, ev| match ev {
                    Event::Push => true,
                    Event::Released => {
                        if let Err(e) = event_sender.send(AppEvent::Navigate {
                            href: href.to_owned(),
                        }) {
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
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Ol { element } | Element::Ul { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
        Element::Li { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context, event_sender)?);
        }
    }

    if let Some(flex_element) = &flex_element {
        match direction {
            LayoutDirection::Row => {
                if let Some(width) = width {
                    flex.fixed(flex_element, width.round() as i32);
                    log::debug!("draw_element: setting fixed width={width}")
                } else {
                    log::debug!(
                        "draw_element: not setting fixed width size width={width:?} height={height:?}"
                    )
                }
            }
            LayoutDirection::Column => {
                if let Some(height) = height {
                    flex.fixed(flex_element, height.round() as i32);
                    log::debug!("draw_element: setting fixed height={height}")
                } else {
                    log::debug!(
                        "draw_element: not setting fixed height size width={width:?} height={height:?}"
                    )
                }
            }
        }
    }

    Ok(flex_element
        .map(|x| Box::new(x) as Box<dyn WidgetExt>)
        .or(other_element))
}
