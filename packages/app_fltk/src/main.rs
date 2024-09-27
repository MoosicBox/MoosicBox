use std::str::FromStr;

use fltk::{
    app, enums,
    frame::{self, Frame},
    group,
    image::SharedImage,
    prelude::*,
    window::Window,
};
use moosicbox_htmx_transformer::{
    ContainerElement, Element, ElementList, HeaderSize, LayoutDirection, Number,
};

const WIDTH: i32 = 600;
const HEIGHT: i32 = 400;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    let app = app::App::default();
    let mut win = Window::default()
        .with_size(WIDTH, HEIGHT)
        .with_label("MoosicBox");

    win.end();
    win.make_resizable(true);
    win.show();

    let elements: ElementList = moosicbox_app_fltk_ui::home().into_string().try_into()?;
    let elements: &[Element] = &elements;

    win.begin();
    draw_elements(
        elements,
        Context::new(win.width() as f32, win.height() as f32),
    )?;
    win.end();

    app.run()?;
    Ok(())
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
