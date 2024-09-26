use fltk::{
    app, enums,
    frame::{self, Frame},
    group,
    image::SharedImage,
    prelude::*,
    window::Window,
};
use moosicbox_htmx_transformer::{Element, ElementList, HeaderSize};

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

    let col = group::Flex::default_fill().row();

    for element in elements {
        draw_element(element, context.clone())?;
    }

    col.end();

    Ok(())
}

#[derive(Clone)]
struct Context {
    size: u16,
}

impl Default for Context {
    fn default() -> Self {
        Self { size: 12 }
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
        Element::Div { elements } => draw_elements(elements, context)?,
        Element::Aside { elements } => draw_elements(elements, context)?,
        Element::Header { elements } => draw_elements(elements, context)?,
        Element::Footer { elements } => draw_elements(elements, context)?,
        Element::Main { elements } => draw_elements(elements, context)?,
        Element::Section { elements } => draw_elements(elements, context)?,
        Element::Form { elements } => draw_elements(elements, context)?,
        Element::Span { elements } => draw_elements(elements, context)?,
        Element::Input(_) => {}
        Element::Button { elements } => draw_elements(elements, context)?,
        Element::Image { source } => {
            if let Some(source) = source {
                let mut frame = Frame::default_fill();

                let mut image = SharedImage::load(format!("../MoosicBoxUI/public{source}"))?;
                image.scale(36, 36, true, true);

                frame.set_image(Some(image));
            }
        }
        Element::Anchor { elements } => draw_elements(elements, context)?,
        Element::Heading { elements, size } => {
            context.size = match size {
                HeaderSize::H1 => 36,
                HeaderSize::H2 => 30,
                HeaderSize::H3 => 24,
                HeaderSize::H4 => 20,
                HeaderSize::H5 => 16,
                HeaderSize::H6 => 12,
            };
            draw_elements(elements, context)?
        }
    };

    Ok(())
}
