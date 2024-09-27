#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    str::FromStr as _,
    sync::{Arc, Mutex},
};

use fltk::{
    app::{self, App},
    enums::{self, Event},
    frame::{self, Frame},
    group,
    image::SharedImage,
    prelude::*,
    window::{DoubleWindow, Window},
};
use moosicbox_htmx_transformer::{
    ContainerElement, Element, ElementList, HeaderSize, LayoutDirection, Number,
};

type RouteFunc = Arc<Box<dyn Fn() -> ElementList>>;

#[derive(Clone)]
pub struct Renderer {
    app: App,
    window: DoubleWindow,
    elements: Arc<Mutex<ElementList>>,
    routes: Vec<(String, RouteFunc)>,
}

impl Renderer {
    pub fn new(width: u16, height: u16) -> Result<Self, FltkError> {
        let app = app::App::default();
        let mut window = Window::default()
            .with_size(width as i32, height as i32)
            .with_label("MoosicBox");

        app::set_background_color(24, 26, 27);

        let renderer = Self {
            app,
            window: window.clone(),
            elements: Arc::new(Mutex::new(ElementList::default())),
            routes: vec![],
        };

        window.handle({
            let mut renderer = renderer.clone();
            move |window, ev| match ev {
                Event::Resize => {
                    log::debug!(
                        "event resize: width={} height={}",
                        window.width(),
                        window.height()
                    );
                    if let Err(e) = renderer.perform_render() {
                        log::error!("Failed to draw elements: {e:?}");
                    }
                    true
                }
                _ => false,
            }
        });

        window.end();
        window.make_resizable(true);
        window.show();

        Ok(renderer)
    }

    pub fn with_route(mut self, route: &str, handler: impl Fn() -> ElementList + 'static) -> Self {
        self.routes
            .push((route.to_string(), Arc::new(Box::new(handler))));
        self
    }

    pub fn navigate(&mut self, path: &str) -> Result<(), FltkError> {
        if let Some(handler) = self
            .routes
            .iter()
            .find(|(route, _)| route == path)
            .map(|(_, handler)| handler)
        {
            self.render(handler())?;
        }

        Ok(())
    }

    fn perform_render(&mut self) -> Result<(), FltkError> {
        self.window.clear();
        self.window.begin();
        let elements: &[Element] = &self.elements.lock().unwrap();
        draw_elements(
            elements,
            Context::new(self.window.width() as f32, self.window.height() as f32),
        )?;
        self.window.end();
        self.window.flush();
        Ok(())
    }

    pub fn render(&mut self, elements: ElementList) -> Result<(), FltkError> {
        {
            *self.elements.lock().unwrap() = elements;
        }

        self.perform_render()?;

        Ok(())
    }

    pub fn run(self) -> Result<(), FltkError> {
        self.app.run()?;

        Ok(())
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

fn draw_elements(elements: &[Element], context: Context) -> Result<group::Flex, FltkError> {
    log::debug!("draw_elements: elements={elements:?}");

    let flex = group::Flex::default_fill();
    let mut flex = match context.direction {
        LayoutDirection::Row => flex.row(),
        LayoutDirection::Column => flex.column(),
    };

    for (i, element) in elements.iter().enumerate() {
        if i == elements.len() - 1 {
            draw_element(element, context, &mut flex)?;
            break;
        }
        draw_element(element, context.clone(), &mut flex)?;
    }

    flex.end();

    Ok(flex)
}

fn draw_element(
    element: &Element,
    mut context: Context,
    flex: &mut group::Flex,
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
            other_element = Some(Box::new(
                frame::Frame::default()
                    .with_label(value)
                    .with_align(enums::Align::Inside | enums::Align::Left),
            ));
        }
        Element::Div { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Aside { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Header { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Footer { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Main { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Section { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Form { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Span { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
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
            flex_element = Some(draw_elements(&element.elements, context)?);
        }
        Element::Image {
            source,
            width,
            height,
        } => {
            let mut frame = Frame::default_fill();

            if let Some(source) = source {
                if let Ok(manifest_path) = std::env::var("CARGO_MANIFEST_DIR") {
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
        Element::Anchor { element } => {
            context = context.with_container(element);
            if element.width.is_some() {
                width = Some(context.width);
            }
            if element.height.is_some() {
                height = Some(context.height);
            }
            flex_element = Some(draw_elements(&element.elements, context)?);
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
            flex_element = Some(draw_elements(&element.elements, context)?);
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
