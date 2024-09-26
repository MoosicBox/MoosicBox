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
    ContainerElement, Element, ElementList, HeaderSize, LayoutDirection,
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
    draw_elements(elements, Context::default())?;
    win.end();

    app.run()?;
    Ok(())
}

fn draw_elements(elements: &[Element], context: Context) -> Result<(), FltkError> {
    log::debug!("draw_elements: elements={elements:?}");

    let flex = group::Flex::default_fill();
    let flex = match context.direction {
        LayoutDirection::Row => flex.row(),
        LayoutDirection::Column => flex.column(),
    };

    for (i, element) in elements.iter().enumerate() {
        if i == elements.len() - 1 {
            draw_element(element, context)?;
            break;
        }
        draw_element(element, context.clone())?;
    }

    flex.end();

    Ok(())
}

#[derive(Clone)]
struct Context {
    size: u16,
    direction: LayoutDirection,
}

impl Context {
    fn with_container(mut self, container: &ContainerElement) -> Context {
        self.direction = container.direction;
        self
    }
}

impl Default for Context {
    fn default() -> Self {
        Self {
            size: 12,
            direction: LayoutDirection::Column,
        }
    }
}

fn draw_element(element: &Element, mut context: Context) -> Result<(), FltkError> {
    log::debug!("draw_element: element={element:?}");

    match element {
        Element::Raw { value } => {
            app::set_font_size(context.size);
            frame::Frame::default()
                .with_label(value)
                .with_align(enums::Align::Inside | enums::Align::Left);
        }
        Element::Div { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Aside { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Header { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Footer { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Main { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Section { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Form { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Span { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Input(_) => {}
        Element::Button { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
        }
        Element::Image { source } => {
            if let Some(source) = source {
                let mut frame = Frame::default_fill();

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
                                    image.scale(36, 36, true, true);

                                    frame.set_image(Some(image));
                                }
                            }
                        }
                    }
                }
            }
        }
        Element::Anchor { element } => {
            context = context.with_container(element);
            draw_elements(&element.elements, context)?;
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
            draw_elements(&element.elements, context)?;
        }
    };

    Ok(())
}
