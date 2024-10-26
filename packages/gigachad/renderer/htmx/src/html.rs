#![allow(clippy::module_name_repetitions)]

use std::io::Write;

use actix_htmx::Htmx;
use gigachad_renderer::Color;
use gigachad_router::ContainerElement;
use gigachad_transformer::{
    Calculation, Element, HeaderSize, Input, JustifyContent, LayoutDirection, LayoutOverflow,
    Number, Route,
};

pub fn elements_to_html(f: &mut dyn Write, elements: &[Element]) -> Result<(), std::io::Error> {
    for element in elements {
        element_to_html(f, element)?;
    }

    Ok(())
}

fn write_attr(f: &mut dyn Write, attr: &[u8], value: &[u8]) -> Result<(), std::io::Error> {
    f.write_all(b" ")?;
    f.write_all(attr)?;
    f.write_all(b"=\"")?;
    f.write_all(value)?;
    f.write_all(b"\"")?;
    Ok(())
}

fn write_css_attr(f: &mut dyn Write, attr: &[u8], value: &[u8]) -> Result<(), std::io::Error> {
    f.write_all(attr)?;
    f.write_all(b":")?;
    f.write_all(value)?;
    f.write_all(b";")?;
    Ok(())
}

fn number_to_css_string(number: &Number) -> String {
    match number {
        Number::Real(x) => format!("{x}px"),
        Number::Integer(x) => format!("{x}px"),
        Number::RealPercent(x) => format!("{x}%"),
        Number::IntegerPercent(x) => format!("{x}%"),
        Number::Calc(x) => format!("calc({})", calc_to_css_string(x)),
    }
}

fn color_to_css_string(color: Color) -> String {
    color.a.map_or_else(
        || format!("rgb({},{},{})", color.r, color.g, color.b),
        |a| format!("rgba({},{},{},{})", color.r, color.g, color.b, a),
    )
}

fn calc_to_css_string(calc: &Calculation) -> String {
    match calc {
        Calculation::Number(number) => number_to_css_string(number),
        Calculation::Add(left, right) => format!(
            "{} + {}",
            calc_to_css_string(left),
            calc_to_css_string(right)
        ),
        Calculation::Subtract(left, right) => format!(
            "{} - {}",
            calc_to_css_string(left),
            calc_to_css_string(right)
        ),
        Calculation::Multiply(left, right) => format!(
            "{} * {}",
            calc_to_css_string(left),
            calc_to_css_string(right)
        ),
        Calculation::Divide(left, right) => format!(
            "{} / {}",
            calc_to_css_string(left),
            calc_to_css_string(right)
        ),
        Calculation::Grouping(value) => format!("({})", calc_to_css_string(value)),
        Calculation::Min(left, right) => format!(
            "min({}, {})",
            calc_to_css_string(left),
            calc_to_css_string(right)
        ),
        Calculation::Max(left, right) => format!(
            "max({}, {})",
            calc_to_css_string(left),
            calc_to_css_string(right)
        ),
    }
}

#[allow(clippy::too_many_lines)]
pub fn element_style_to_html(
    f: &mut dyn Write,
    element: &ContainerElement,
) -> Result<(), std::io::Error> {
    let mut printed_start = false;

    if element.direction == LayoutDirection::Row {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"display", b"flex")?;
        write_css_attr(f, b"flex-direction", b"row")?;
    }

    match element.overflow_x {
        LayoutOverflow::Auto => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"overflow-x", b"auto")?;
        }
        LayoutOverflow::Scroll => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"overflow-x", b"scroll")?;
        }
        LayoutOverflow::Show | LayoutOverflow::Squash => {}
        LayoutOverflow::Wrap => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"flex-wrap", b"wrap")?;
        }
    }
    match element.overflow_y {
        LayoutOverflow::Auto => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"overflow-y", b"auto")?;
        }
        LayoutOverflow::Scroll => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"overflow-y", b"scroll")?;
        }
        LayoutOverflow::Show | LayoutOverflow::Squash => {}
        LayoutOverflow::Wrap => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"flex-wrap", b"wrap")?;
        }
    }

    match element.justify_content {
        JustifyContent::SpaceBetween => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"justify-content", b"space-between")?;
        }
        JustifyContent::SpaceEvenly => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"justify-content", b"space-evenly")?;
        }
        JustifyContent::Default => {}
    }

    if let Some(gap) = &element.gap {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"grid-gap", number_to_css_string(gap).as_bytes())?;
    }

    let mut flex_shrink_0 = false;

    if let Some(width) = &element.width {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"width", number_to_css_string(width).as_bytes())?;
        flex_shrink_0 = true;
    }
    if let Some(height) = &element.height {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"height", number_to_css_string(height).as_bytes())?;
        flex_shrink_0 = true;
    }

    if flex_shrink_0 {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"flex-shrink", b"0")?;
    }

    if let Some(background) = element.background {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"background", color_to_css_string(background).as_bytes())?;
    }

    if printed_start {
        f.write_all(b"\"")?;
    }

    Ok(())
}

pub fn element_attrs_to_html(
    f: &mut dyn Write,
    element: &ContainerElement,
) -> Result<(), std::io::Error> {
    if let Some(route) = &element.route {
        match route {
            Route::Get { route, trigger } => {
                write_attr(f, b"hx-swap", b"outerHTML")?;
                write_attr(f, b"hx-get", route.as_bytes())?;
                if let Some(trigger) = trigger {
                    write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                }
            }
            Route::Post { route, trigger } => {
                write_attr(f, b"hx-swap", b"outerHTML")?;
                write_attr(f, b"hx-post", route.as_bytes())?;
                if let Some(trigger) = trigger {
                    write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                }
            }
        }
    }

    element_style_to_html(f, element)?;

    let mut is_flex_grid = false;
    if let LayoutOverflow::Wrap { .. } = element.overflow_x {
        is_flex_grid = true;
    } else if let LayoutOverflow::Wrap { .. } = element.overflow_y {
        is_flex_grid = true;
    }

    if is_flex_grid {
        write_attr(f, b"class", b"flex-grid-fill-last-row")?;
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
pub fn element_to_html(f: &mut dyn Write, element: &Element) -> Result<(), std::io::Error> {
    match element {
        Element::Raw { value } => {
            f.write_all(value.as_bytes())?;
            return Ok(());
        }
        Element::Image { source, element } => {
            const TAG_NAME: &[u8] = b"img";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            if let Some(source) = source {
                f.write_all(b" src=\"")?;
                f.write_all(source.as_bytes())?;
                f.write_all(b"\"")?;
            }
            element_attrs_to_html(f, element)?;
            f.write_all(b">")?;
            elements_to_html(f, &element.elements)?;
            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Anchor { element, href } => {
            const TAG_NAME: &[u8] = b"a";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            if let Some(href) = href {
                f.write_all(b" href=\"")?;
                f.write_all(href.as_bytes())?;
                f.write_all(b"\"")?;
            }
            element_attrs_to_html(f, element)?;
            f.write_all(b">")?;
            elements_to_html(f, &element.elements)?;
            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Heading { element, size } => {
            let tag_name = match size {
                HeaderSize::H1 => b"h1",
                HeaderSize::H2 => b"h2",
                HeaderSize::H3 => b"h3",
                HeaderSize::H4 => b"h4",
                HeaderSize::H5 => b"h5",
                HeaderSize::H6 => b"h6",
            };
            f.write_all(b"<")?;
            f.write_all(tag_name)?;
            element_attrs_to_html(f, element)?;
            f.write_all(b">")?;
            elements_to_html(f, &element.elements)?;
            f.write_all(b"</")?;
            f.write_all(tag_name)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Input(input) => {
            const TAG_NAME: &[u8] = b"input";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            match input {
                Input::Text { value, placeholder } => {
                    f.write_all(b" type=\"text\"")?;
                    if let Some(value) = value {
                        f.write_all(b" value=\"")?;
                        f.write_all(value.as_bytes())?;
                        f.write_all(b"\"")?;
                    }
                    if let Some(placeholder) = placeholder {
                        f.write_all(b" placeholder=\"")?;
                        f.write_all(placeholder.as_bytes())?;
                        f.write_all(b"\"")?;
                    }
                }
                Input::Password { value, placeholder } => {
                    f.write_all(b" type=\"password\"")?;
                    if let Some(value) = value {
                        f.write_all(b" value=\"")?;
                        f.write_all(value.as_bytes())?;
                        f.write_all(b"\"")?;
                    }
                    if let Some(placeholder) = placeholder {
                        f.write_all(b" placeholder=\"")?;
                        f.write_all(placeholder.as_bytes())?;
                        f.write_all(b"\"")?;
                    }
                }
            }
            f.write_all(b"></")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        _ => {}
    }

    let tag_name = match element {
        Element::Div { element } => Some(("div", element)),
        Element::Aside { element } => Some(("aside", element)),
        Element::Main { element } => Some(("main", element)),
        Element::Header { element } => Some(("header", element)),
        Element::Footer { element } => Some(("footer", element)),
        Element::Section { element } => Some(("section", element)),
        Element::Form { element } => Some(("form", element)),
        Element::Span { element } => Some(("span", element)),
        Element::Button { element } => Some(("button", element)),
        Element::UnorderedList { element } => Some(("ul", element)),
        Element::OrderedList { element } => Some(("ol", element)),
        Element::ListItem { element } => Some(("li", element)),
        Element::Table { element } => Some(("table", element)),
        Element::THead { element } => Some(("thead", element)),
        Element::TH { element } => Some(("th", element)),
        Element::TBody { element } => Some(("tbody", element)),
        Element::TR { element } => Some(("tr", element)),
        Element::TD { element } => Some(("td", element)),
        _ => None,
    };

    if let Some((tag_name, container)) = tag_name {
        f.write_all(b"<")?;
        f.write_all(tag_name.as_bytes())?;
        element_attrs_to_html(f, container)?;
        f.write_all(b">")?;
        elements_to_html(f, &container.elements)?;
        f.write_all(b"</")?;
        f.write_all(tag_name.as_bytes())?;
        f.write_all(b">")?;
    }

    Ok(())
}

pub fn container_element_to_html(container: &ContainerElement) -> Result<String, std::io::Error> {
    let mut buffer = vec![];

    elements_to_html(&mut buffer, &container.elements)?;

    Ok(std::str::from_utf8(&buffer)
        .map_err(std::io::Error::other)?
        .to_string())
}

#[allow(clippy::similar_names)]
pub fn container_element_to_html_response(
    container: &ContainerElement,
    background: Option<Color>,
    htmx: &Htmx,
) -> Result<String, std::io::Error> {
    let html = container_element_to_html(container)?;

    Ok(if htmx.is_htmx {
        html
    } else {
        format!(
            r#"
            <html>
                <head>
                    <script
                        src="https://unpkg.com/htmx.org@2.0.2"
                        integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ"
                        crossorigin="anonymous">
                    </script>
                    <style>
                        body {{
                            margin: 0;{background}
                        }}

                        .flex-grid-fill-last-row::after {{
                            content: "";
                            flex: auto;
                        }}
                    </style>
                </head>
                <body>{html}</body>
            </html>
            "#,
            background = background
                .map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b))
                .as_deref()
                .unwrap_or("")
        )
    })
}
