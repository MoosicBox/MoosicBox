#![allow(clippy::module_name_repetitions)]

use std::io::Write;

use actix_web::http::header::HeaderMap;
use gigachad_renderer::Color;
use gigachad_router::Container;
use gigachad_transformer::{
    models::{AlignItems, JustifyContent, LayoutDirection, LayoutOverflow, Position, Visibility},
    Calculation, Element, HeaderSize, Input, Number,
};

pub trait HtmlTagRenderer {
    /// # Errors
    ///
    /// * If the `HtmlTagRenderer` fails to write the element attributes
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        element_style_to_html(f, container, is_flex_child)?;
        element_classes_to_html(f, container)?;

        Ok(())
    }

    fn root_html(
        &self,
        _headers: &HeaderMap,
        content: String,
        background: Option<Color>,
    ) -> String {
        format!(
            r"
            <html>
                <head>
                    <style>
                        body {{
                            margin: 0;{background};
                            overflow: hidden;
                        }}

                        .remove-button-styles {{
                            background: none;
                            color: inherit;
                            border: none;
                            padding: 0;
                            font: inherit;
                            cursor: pointer;
                            outline: inherit;
                        }}
                    </style>
                </head>
                <body>{content}</body>
            </html>
            ",
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
    containers: &[Container],
    tag_renderer: &dyn HtmlTagRenderer,
    is_flex_child: bool,
) -> Result<(), std::io::Error> {
    for container in containers {
        element_to_html(f, container, tag_renderer, is_flex_child)?;
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

// TODO: handle vertical flex
fn is_flex_container(container: &Container) -> bool {
    container.direction == LayoutDirection::Row
}

/// # Errors
///
/// * If there were any IO errors writing the element style attribute
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn element_style_to_html(
    f: &mut dyn Write,
    container: &Container,
    is_flex_child: bool,
) -> Result<(), std::io::Error> {
    let mut printed_start = false;

    // TODO: handle vertical flex
    if is_flex_child && container.width.is_none() {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"flex", b"1")?;
    }

    if is_flex_container(container) {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"display", b"flex")?;
        write_css_attr(f, b"flex-direction", b"row")?;
    }

    match container.overflow_x {
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
        LayoutOverflow::Expand | LayoutOverflow::Squash => {}
        LayoutOverflow::Wrap => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"flex-wrap", b"wrap")?;
        }
    }
    match container.overflow_y {
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
        LayoutOverflow::Expand | LayoutOverflow::Squash => {}
        LayoutOverflow::Wrap => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"flex-wrap", b"wrap")?;
        }
    }

    if let Some(position) = container.position {
        match position {
            Position::Relative => {
                if !printed_start {
                    printed_start = true;
                    f.write_all(b" style=\"")?;
                }
                write_css_attr(f, b"position", b"relative")?;
            }
            Position::Absolute => {
                if !printed_start {
                    printed_start = true;
                    f.write_all(b" style=\"")?;
                }
                write_css_attr(f, b"position", b"absolute")?;
            }
            Position::Fixed => {
                if !printed_start {
                    printed_start = true;
                    f.write_all(b" style=\"")?;
                }
                write_css_attr(f, b"position", b"fixed")?;
            }
            Position::Static => {
                if !printed_start {
                    printed_start = true;
                    f.write_all(b" style=\"")?;
                }
                write_css_attr(f, b"position", b"static")?;
            }
        }
    }

    if let Some(margin_left) = &container.margin_left {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"margin-left",
            number_to_css_string(margin_left).as_bytes(),
        )?;
    }
    if let Some(margin_right) = &container.margin_right {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"margin-right",
            number_to_css_string(margin_right).as_bytes(),
        )?;
    }
    if let Some(margin_top) = &container.margin_top {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"margin-top",
            number_to_css_string(margin_top).as_bytes(),
        )?;
    }
    if let Some(margin_bottom) = &container.margin_bottom {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"margin-bottom",
            number_to_css_string(margin_bottom).as_bytes(),
        )?;
    }

    if let Some(padding_left) = &container.padding_left {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"padding-left",
            number_to_css_string(padding_left).as_bytes(),
        )?;
    }
    if let Some(padding_right) = &container.padding_right {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"padding-right",
            number_to_css_string(padding_right).as_bytes(),
        )?;
    }
    if let Some(padding_top) = &container.padding_top {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"padding-top",
            number_to_css_string(padding_top).as_bytes(),
        )?;
    }
    if let Some(padding_bottom) = &container.padding_bottom {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"padding-bottom",
            number_to_css_string(padding_bottom).as_bytes(),
        )?;
    }

    if let Some(left) = &container.left {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"left", number_to_css_string(left).as_bytes())?;
    }
    if let Some(right) = &container.right {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"right", number_to_css_string(right).as_bytes())?;
    }
    if let Some(top) = &container.top {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"top", number_to_css_string(top).as_bytes())?;
    }
    if let Some(bottom) = &container.bottom {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"bottom", number_to_css_string(bottom).as_bytes())?;
    }

    if let Some(visibility) = container.visibility {
        match visibility {
            Visibility::Visible => {}
            Visibility::Hidden => {
                if !printed_start {
                    printed_start = true;
                    f.write_all(b" style=\"")?;
                }
                write_css_attr(f, b"display", b"none")?;
            }
        }
    }

    match container.justify_content {
        JustifyContent::Start => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"justify-content", b"start")?;
        }
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
    }

    match container.align_items {
        AlignItems::Start => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"align-items", b"start")?;
        }
        AlignItems::Center => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"align-items", b"center")?;
        }
        AlignItems::End => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, b"align-items", b"end")?;
        }
    }

    if let Some(gap) = &container.gap {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"grid-gap", number_to_css_string(gap).as_bytes())?;
    }

    let mut flex_shrink_0 = false;

    if let Some(width) = &container.width {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"width", number_to_css_string(width).as_bytes())?;
        flex_shrink_0 = true;
    }
    if let Some(height) = &container.height {
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

    if let Some(background) = container.background {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(f, b"background", color_to_css_string(background).as_bytes())?;
    }

    if let Some((color, size)) = &container.border_top {
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

    if let Some((color, size)) = &container.border_right {
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

    if let Some((color, size)) = &container.border_bottom {
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

    if let Some((color, size)) = &container.border_left {
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

    if let Some(radius) = &container.border_top_left_radius {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-top-left-radius",
            number_to_css_string(radius).as_bytes(),
        )?;
    }

    if let Some(radius) = &container.border_top_right_radius {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-top-right-radius",
            number_to_css_string(radius).as_bytes(),
        )?;
    }

    if let Some(radius) = &container.border_bottom_left_radius {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-bottom-left-radius",
            number_to_css_string(radius).as_bytes(),
        )?;
    }

    if let Some(radius) = &container.border_bottom_right_radius {
        if !printed_start {
            printed_start = true;
            f.write_all(b" style=\"")?;
        }
        write_css_attr(
            f,
            b"border-bottom-right-radius",
            number_to_css_string(radius).as_bytes(),
        )?;
    }

    if printed_start {
        f.write_all(b"\"")?;
    }

    Ok(())
}

/// # Errors
///
/// * If there were any IO errors writing the element style attribute
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
pub fn element_classes_to_html(
    f: &mut dyn Write,
    container: &Container,
) -> Result<(), std::io::Error> {
    let mut printed_start = false;

    if container.element == Element::Button {
        if !printed_start {
            printed_start = true;
            f.write_all(b" class=\"")?;
        }
        f.write_all(b"remove-button-styles")?;
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
    container: &Container,
    tag_renderer: &dyn HtmlTagRenderer,
    is_flex_child: bool,
) -> Result<(), std::io::Error> {
    match &container.element {
        Element::Raw { value } => {
            f.write_all(value.as_bytes())?;
            return Ok(());
        }
        Element::Image { source } => {
            const TAG_NAME: &[u8] = b"img";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            if let Some(source) = source {
                f.write_all(b" src=\"")?;
                f.write_all(source.as_bytes())?;
                f.write_all(b"\"")?;
            }
            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;
            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                is_flex_container(container),
            )?;
            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Anchor { href } => {
            const TAG_NAME: &[u8] = b"a";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            if let Some(href) = href {
                f.write_all(b" href=\"")?;
                f.write_all(href.as_bytes())?;
                f.write_all(b"\"")?;
            }
            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;
            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                is_flex_container(container),
            )?;
            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Heading { size } => {
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
            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;
            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                is_flex_container(container),
            )?;
            f.write_all(b"</")?;
            f.write_all(tag_name)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Input { input } => {
            const TAG_NAME: &[u8] = b"input";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            match input {
                Input::Checkbox { checked } => {
                    f.write_all(b" type=\"checkbox\"")?;
                    if *checked == Some(true) {
                        f.write_all(b" checked=\"checked\"")?;
                    }
                }
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

    let tag_name = match &container.element {
        Element::Div => Some("div"),
        Element::Aside => Some("aside"),
        Element::Main => Some("main"),
        Element::Header => Some("header"),
        Element::Footer => Some("footer"),
        Element::Section => Some("section"),
        Element::Form => Some("form"),
        Element::Span => Some("span"),
        Element::Button => Some("button"),
        Element::UnorderedList => Some("ul"),
        Element::OrderedList => Some("ol"),
        Element::ListItem => Some("li"),
        Element::Table => Some("table"),
        Element::THead => Some("thead"),
        Element::TH => Some("th"),
        Element::TBody => Some("tbody"),
        Element::TR => Some("tr"),
        Element::TD => Some("td"),
        _ => None,
    };

    if let Some(tag_name) = tag_name {
        f.write_all(b"<")?;
        f.write_all(tag_name.as_bytes())?;
        tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
        f.write_all(b">")?;
        elements_to_html(
            f,
            &container.children,
            tag_renderer,
            is_flex_container(container),
        )?;
        f.write_all(b"</")?;
        f.write_all(tag_name.as_bytes())?;
        f.write_all(b">")?;
    }

    Ok(())
}

/// # Errors
///
/// * If there were any IO errors writing the `Container` as HTML
pub fn container_element_to_html(
    container: &Container,
    tag_renderer: &dyn HtmlTagRenderer,
) -> Result<String, std::io::Error> {
    let mut buffer = vec![];

    elements_to_html(
        &mut buffer,
        &container.children,
        tag_renderer,
        is_flex_container(container),
    )?;

    Ok(std::str::from_utf8(&buffer)
        .map_err(std::io::Error::other)?
        .to_string())
}

/// # Errors
///
/// * If there were any IO errors writing the `Container` as an HTML response
#[allow(clippy::similar_names)]
pub fn container_element_to_html_response(
    headers: &HeaderMap,
    container: &Container,
    background: Option<Color>,
    tag_renderer: &dyn HtmlTagRenderer,
) -> Result<String, std::io::Error> {
    Ok(tag_renderer.root_html(
        headers,
        container_element_to_html(container, tag_renderer)?,
        background,
    ))
}
