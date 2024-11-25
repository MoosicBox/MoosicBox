#![allow(clippy::module_name_repetitions)]

use std::io::Write;

use actix_web::http::header::HeaderMap;
use gigachad_renderer::Color;
use gigachad_router::ContainerElement;
use gigachad_transformer::{
    Calculation, Element, HeaderSize, Input, JustifyContent, LayoutDirection, LayoutOverflow,
    Number,
};

pub trait HtmlTagRenderer {
    /// # Errors
    ///
    /// * If the `HtmlTagRenderer` fails to write the element attributes
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        element: &ContainerElement,
    ) -> Result<(), std::io::Error> {
        element_style_to_html(f, element)?;

        Ok(())
    }

    fn root_html(
        &self,
        _headers: &HeaderMap,
        content: String,
        background: Option<Color>,
    ) -> String {
        format!(
            r#"
            <html>
                <head>
                    <style>
                        body {{
                            margin: 0;{background}
                        }}
                    </style>
                </head>
                <body>{content}</body>
            </html>
            "#,
            background = background
                .map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b))
                .as_deref()
                .unwrap_or("")
        )
    }
}

/// # Errors
///
/// * If any of the elements fail to be written as HTML
pub fn elements_to_html(
    f: &mut dyn Write,
    elements: &[Element],
    tag_renderer: &dyn HtmlTagRenderer,
) -> Result<(), std::io::Error> {
    for element in elements {
        element_to_html(f, element, tag_renderer)?;
    }

    Ok(())
}

/// # Errors
///
/// * If there was an IO error writing the attribute
pub fn write_attr(f: &mut dyn Write, attr: &[u8], value: &[u8]) -> Result<(), std::io::Error> {
    f.write_all(b" ")?;
    f.write_all(attr)?;
    f.write_all(b"=\"")?;
    f.write_all(value)?;
    f.write_all(b"\"")?;
    Ok(())
}

/// # Errors
///
/// * If there was an IO error writing the css attribute
pub fn write_css_attr(f: &mut dyn Write, attr: &[u8], value: &[u8]) -> Result<(), std::io::Error> {
    f.write_all(attr)?;
    f.write_all(b":")?;
    f.write_all(value)?;
    f.write_all(b";")?;
    Ok(())
}

#[must_use]
pub fn number_to_css_string(number: &Number) -> String {
    match number {
        Number::Real(x) => format!("{x}px"),
        Number::Integer(x) => format!("{x}px"),
        Number::RealPercent(x) => format!("{x}%"),
        Number::IntegerPercent(x) => format!("{x}%"),
        Number::Calc(x) => format!("calc({})", calc_to_css_string(x)),
    }
}

#[must_use]
pub fn color_to_css_string(color: Color) -> String {
    color.a.map_or_else(
        || format!("rgb({},{},{})", color.r, color.g, color.b),
        |a| format!("rgba({},{},{},{})", color.r, color.g, color.b, a),
    )
}

#[must_use]
pub fn calc_to_css_string(calc: &Calculation) -> String {
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

/// # Errors
///
/// * If there were any IO errors writing the element style attribute
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
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
        JustifyContent::Center => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"justify-content", b"center")?;
        }
        JustifyContent::End => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"justify-content", b"end")?;
        }
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

    if let Some((color, size)) = &element.border_top {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-top",
            &[
                number_to_css_string(size).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        )?;
    }

    if let Some((color, size)) = &element.border_right {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-right",
            &[
                number_to_css_string(size).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        )?;
    }

    if let Some((color, size)) = &element.border_bottom {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-bottom",
            &[
                number_to_css_string(size).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        )?;
    }

    if let Some((color, size)) = &element.border_left {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-left",
            &[
                number_to_css_string(size).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        )?;
    }

    if printed_start {
        f.write_all(b"\"")?;
    }

    Ok(())
}

/// # Errors
///
/// * If there were any IO errors writing the element as HTML
#[allow(clippy::too_many_lines)]
pub fn element_to_html(
    f: &mut dyn Write,
    element: &Element,
    tag_renderer: &dyn HtmlTagRenderer,
) -> Result<(), std::io::Error> {
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
            tag_renderer.element_attrs_to_html(f, element)?;
            f.write_all(b">")?;
            elements_to_html(f, &element.elements, tag_renderer)?;
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
            tag_renderer.element_attrs_to_html(f, element)?;
            f.write_all(b">")?;
            elements_to_html(f, &element.elements, tag_renderer)?;
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
            tag_renderer.element_attrs_to_html(f, element)?;
            f.write_all(b">")?;
            elements_to_html(f, &element.elements, tag_renderer)?;
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
        tag_renderer.element_attrs_to_html(f, container)?;
        f.write_all(b">")?;
        elements_to_html(f, &container.elements, tag_renderer)?;
        f.write_all(b"</")?;
        f.write_all(tag_name.as_bytes())?;
        f.write_all(b">")?;
    }

    Ok(())
}

/// # Errors
///
/// * If there were any IO errors writing the `ContainerElement` as HTML
pub fn container_element_to_html(
    container: &ContainerElement,
    tag_renderer: &dyn HtmlTagRenderer,
) -> Result<String, std::io::Error> {
    let mut buffer = vec![];

    elements_to_html(&mut buffer, &container.elements, tag_renderer)?;

    Ok(std::str::from_utf8(&buffer)
        .map_err(std::io::Error::other)?
        .to_string())
}

/// # Errors
///
/// * If there were any IO errors writing the `ContainerElement` as an HTML response
#[allow(clippy::similar_names)]
pub fn container_element_to_html_response(
    headers: &HeaderMap,
    container: &ContainerElement,
    background: Option<Color>,
    tag_renderer: &dyn HtmlTagRenderer,
) -> Result<String, std::io::Error> {
    Ok(tag_renderer.root_html(
        headers,
        container_element_to_html(container, tag_renderer)?,
        background,
    ))
}
