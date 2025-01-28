#![allow(clippy::module_name_repetitions)]

use std::io::Write;

use actix_web::http::header::HeaderMap;
use gigachad_renderer::Color;
use gigachad_router::Container;
use gigachad_transformer::{
    models::{
        AlignItems, ImageFit, JustifyContent, LayoutDirection, LayoutOverflow, Position, TextAlign,
        TextDecorationLine, TextDecorationStyle, Visibility,
    },
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
pub fn number_to_css_string(number: &Number, px: bool) -> String {
    match number {
        Number::Real(x) => {
            if px {
                format!("{x}px")
            } else {
                x.to_string()
            }
        }
        Number::Integer(x) => {
            if px {
                format!("{x}px")
            } else {
                x.to_string()
            }
        }
        Number::RealPercent(x) => format!("{x}%"),
        Number::IntegerPercent(x) => format!("{x}%"),
        Number::RealDvw(x) => format!("{x}dvw"),
        Number::IntegerDvw(x) => format!("{x}dvw"),
        Number::RealDvh(x) => format!("{x}dvh"),
        Number::IntegerDvh(x) => format!("{x}dvh"),
        Number::Calc(x) => format!("calc({})", calc_to_css_string(x, px)),
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
pub fn calc_to_css_string(calc: &Calculation, px: bool) -> String {
    match calc {
        Calculation::Number(number) => number_to_css_string(number, px),
        Calculation::Add(left, right) => format!(
            "{} + {}",
            calc_to_css_string(left, false),
            calc_to_css_string(right, false)
        ),
        Calculation::Subtract(left, right) => format!(
            "{} - {}",
            calc_to_css_string(left, false),
            calc_to_css_string(right, false)
        ),
        Calculation::Multiply(left, right) => format!(
            "{} * {}",
            calc_to_css_string(left, false),
            calc_to_css_string(right, false)
        ),
        Calculation::Divide(left, right) => format!(
            "{} / {}",
            calc_to_css_string(left, false),
            calc_to_css_string(right, false)
        ),
        Calculation::Grouping(value) => format!("({})", calc_to_css_string(value, px)),
        Calculation::Min(left, right) => format!(
            "min({}, {})",
            calc_to_css_string(left, px),
            calc_to_css_string(right, px)
        ),
        Calculation::Max(left, right) => format!(
            "max({}, {})",
            calc_to_css_string(left, px),
            calc_to_css_string(right, px)
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
    _is_flex_child: bool,
) -> Result<(), std::io::Error> {
    let mut printed_start = false;

    macro_rules! write_css_attr {
        ($key:expr, $value:expr $(,)?) => {{
            if !printed_start {
                printed_start = true;
                f.write_all(b" style=\"")?;
            }
            write_css_attr(f, $key, $value)?;
        }};
    }

    match &container.element {
        Element::Image { fit, .. } => {
            if let Some(fit) = fit {
                write_css_attr!(
                    b"object-fit",
                    match fit {
                        ImageFit::Default => b"unset",
                        ImageFit::Contain => b"contain",
                        ImageFit::Cover => b"cover",
                        ImageFit::Fill => b"fill",
                        ImageFit::None => b"none",
                    }
                );
            }
        }
        Element::Div
        | Element::Raw { .. }
        | Element::Aside
        | Element::Main
        | Element::Header
        | Element::Footer
        | Element::Section
        | Element::Form
        | Element::Span
        | Element::Input { .. }
        | Element::Button
        | Element::Anchor { .. }
        | Element::Heading { .. }
        | Element::UnorderedList
        | Element::OrderedList
        | Element::ListItem
        | Element::Table
        | Element::THead
        | Element::TH
        | Element::TBody
        | Element::TR
        | Element::TD
        | Element::Canvas => {}
    }

    if is_flex_container(container) {
        write_css_attr!(b"display", b"flex");
        write_css_attr!(b"flex-direction", b"row");
    }

    match container.overflow_x {
        LayoutOverflow::Auto => {
            write_css_attr!(b"overflow-x", b"auto");
        }
        LayoutOverflow::Scroll => {
            write_css_attr!(b"overflow-x", b"scroll");
        }
        LayoutOverflow::Expand | LayoutOverflow::Squash => {}
        LayoutOverflow::Wrap => {
            write_css_attr!(b"flex-wrap", b"wrap");
        }
    }
    match container.overflow_y {
        LayoutOverflow::Auto => {
            write_css_attr!(b"overflow-y", b"auto");
        }
        LayoutOverflow::Scroll => {
            write_css_attr!(b"overflow-y", b"scroll");
        }
        LayoutOverflow::Expand | LayoutOverflow::Squash => {}
        LayoutOverflow::Wrap => {
            write_css_attr!(b"flex-wrap", b"wrap");
        }
    }

    if let Some(position) = container.position {
        match position {
            Position::Relative => {
                write_css_attr!(b"position", b"relative");
            }
            Position::Absolute => {
                write_css_attr!(b"position", b"absolute");
            }
            Position::Fixed => {
                write_css_attr!(b"position", b"fixed");
            }
            Position::Static => {
                write_css_attr!(b"position", b"static");
            }
        }
    }

    if let Some(margin_left) = &container.margin_left {
        write_css_attr!(
            b"margin-left",
            number_to_css_string(margin_left, true).as_bytes(),
        );
    }
    if let Some(margin_right) = &container.margin_right {
        write_css_attr!(
            b"margin-right",
            number_to_css_string(margin_right, true).as_bytes(),
        );
    }
    if let Some(margin_top) = &container.margin_top {
        write_css_attr!(
            b"margin-top",
            number_to_css_string(margin_top, true).as_bytes(),
        );
    }
    if let Some(margin_bottom) = &container.margin_bottom {
        write_css_attr!(
            b"margin-bottom",
            number_to_css_string(margin_bottom, true).as_bytes(),
        );
    }

    if let Some(padding_left) = &container.padding_left {
        write_css_attr!(
            b"padding-left",
            number_to_css_string(padding_left, true).as_bytes(),
        );
    }
    if let Some(padding_right) = &container.padding_right {
        write_css_attr!(
            b"padding-right",
            number_to_css_string(padding_right, true).as_bytes(),
        );
    }
    if let Some(padding_top) = &container.padding_top {
        write_css_attr!(
            b"padding-top",
            number_to_css_string(padding_top, true).as_bytes(),
        );
    }
    if let Some(padding_bottom) = &container.padding_bottom {
        write_css_attr!(
            b"padding-bottom",
            number_to_css_string(padding_bottom, true).as_bytes(),
        );
    }

    if let Some(left) = &container.left {
        write_css_attr!(b"left", number_to_css_string(left, true).as_bytes());
    }
    if let Some(right) = &container.right {
        write_css_attr!(b"right", number_to_css_string(right, true).as_bytes());
    }
    if let Some(top) = &container.top {
        write_css_attr!(b"top", number_to_css_string(top, true).as_bytes());
    }
    if let Some(bottom) = &container.bottom {
        write_css_attr!(b"bottom", number_to_css_string(bottom, true).as_bytes());
    }

    let mut printed_transform_start = false;

    macro_rules! write_transform_attr {
        ($key:expr, $value:expr $(,)?) => {{
            if !printed_transform_start {
                printed_transform_start = true;
                f.write_all(b"transform:")?;
            } else {
                f.write_all(b" ")?;
            }
            f.write_all($key)?;
            f.write_all(b"(")?;
            f.write_all($value)?;
            f.write_all(b")")?;
        }};
    }

    if let Some(translate) = &container.translate_x {
        write_transform_attr!(
            b"translateX",
            number_to_css_string(translate, true).as_bytes()
        );
    }
    if let Some(translate) = &container.translate_y {
        write_transform_attr!(
            b"translateY",
            number_to_css_string(translate, true).as_bytes()
        );
    }

    if printed_transform_start {
        f.write_all(b";")?;
    }

    if let Some(visibility) = container.visibility {
        match visibility {
            Visibility::Visible => {}
            Visibility::Hidden => {
                write_css_attr!(b"display", b"none");
            }
        }
    }

    if let Some(justify_content) = container.justify_content {
        match justify_content {
            JustifyContent::Start => {
                write_css_attr!(b"justify-content", b"start");
            }
            JustifyContent::Center => {
                write_css_attr!(b"justify-content", b"center");
            }
            JustifyContent::End => {
                write_css_attr!(b"justify-content", b"end");
            }
            JustifyContent::SpaceBetween => {
                write_css_attr!(b"justify-content", b"space-between");
            }
            JustifyContent::SpaceEvenly => {
                write_css_attr!(b"justify-content", b"space-evenly");
            }
        }
    }

    if let Some(align_items) = container.align_items {
        match align_items {
            AlignItems::Start => {
                write_css_attr!(b"align-items", b"start");
            }
            AlignItems::Center => {
                write_css_attr!(b"align-items", b"center");
            }
            AlignItems::End => {
                write_css_attr!(b"align-items", b"end");
            }
        }
    }

    if let Some(gap) = &container.gap {
        write_css_attr!(b"grid-gap", number_to_css_string(gap, true).as_bytes());
    }

    if let Some(width) = &container.width {
        write_css_attr!(b"width", number_to_css_string(width, true).as_bytes());
    }
    if let Some(height) = &container.height {
        write_css_attr!(b"height", number_to_css_string(height, true).as_bytes());
    }

    if let Some(width) = &container.max_width {
        write_css_attr!(b"max-width", number_to_css_string(width, true).as_bytes());
    }
    if let Some(height) = &container.max_height {
        write_css_attr!(b"max-height", number_to_css_string(height, true).as_bytes());
    }

    if let Some(flex) = &container.flex {
        write_css_attr!(
            b"flex-grow",
            number_to_css_string(&flex.grow, false).as_bytes()
        );
        write_css_attr!(
            b"flex-shrink",
            number_to_css_string(&flex.shrink, false).as_bytes()
        );
        write_css_attr!(
            b"flex-basis",
            number_to_css_string(&flex.basis, false).as_bytes()
        );
    }

    if let Some(background) = container.background {
        write_css_attr!(b"background", color_to_css_string(background).as_bytes());
    }

    if let Some((color, size)) = &container.border_top {
        write_css_attr!(
            b"border-top",
            &[
                number_to_css_string(size, true).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        );
    }

    if let Some((color, size)) = &container.border_right {
        write_css_attr!(
            b"border-right",
            &[
                number_to_css_string(size, true).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        );
    }

    if let Some((color, size)) = &container.border_bottom {
        write_css_attr!(
            b"border-bottom",
            &[
                number_to_css_string(size, true).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        );
    }

    if let Some((color, size)) = &container.border_left {
        write_css_attr!(
            b"border-left",
            &[
                number_to_css_string(size, true).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        );
    }

    if let Some(radius) = &container.border_top_left_radius {
        write_css_attr!(
            b"border-top-left-radius",
            number_to_css_string(radius, true).as_bytes(),
        );
    }

    if let Some(radius) = &container.border_top_right_radius {
        write_css_attr!(
            b"border-top-right-radius",
            number_to_css_string(radius, true).as_bytes(),
        );
    }

    if let Some(radius) = &container.border_bottom_left_radius {
        write_css_attr!(
            b"border-bottom-left-radius",
            number_to_css_string(radius, true).as_bytes(),
        );
    }

    if let Some(radius) = &container.border_bottom_right_radius {
        write_css_attr!(
            b"border-bottom-right-radius",
            number_to_css_string(radius, true).as_bytes(),
        );
    }

    if let Some(font_size) = &container.font_size {
        write_css_attr!(
            b"font-size",
            number_to_css_string(font_size, true).as_bytes(),
        );
    }

    if let Some(color) = &container.color {
        write_css_attr!(b"color", color_to_css_string(*color).as_bytes(),);
    }

    if let Some(text_align) = &container.text_align {
        write_css_attr!(
            b"text-align",
            match text_align {
                TextAlign::Start => b"start",
                TextAlign::Center => b"center",
                TextAlign::End => b"end",
                TextAlign::Justify => b"justify",
            }
        );
    }

    if let Some(text_decoration) = &container.text_decoration {
        if let Some(color) = text_decoration.color {
            write_css_attr!(
                b"text-decoration-color",
                color_to_css_string(color).as_bytes()
            );
        }
        if !text_decoration.line.is_empty() {
            write_css_attr!(
                b"text-decoration-line",
                text_decoration
                    .line
                    .iter()
                    .map(|x| match x {
                        TextDecorationLine::Inherit => "inherit",
                        TextDecorationLine::None => "none",
                        TextDecorationLine::Underline => "underline",
                        TextDecorationLine::Overline => "overline",
                        TextDecorationLine::LineThrough => "line-through",
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .as_bytes()
            );
        }
        if let Some(style) = text_decoration.style {
            write_css_attr!(
                b"text-decoration-style",
                match style {
                    TextDecorationStyle::Inherit => b"inherit",
                    TextDecorationStyle::Solid => b"solid",
                    TextDecorationStyle::Double => b"double",
                    TextDecorationStyle::Dotted => b"dotted",
                    TextDecorationStyle::Dashed => b"dashed",
                    TextDecorationStyle::Wavy => b"wavy",
                }
            );
        }

        if let Some(thickness) = &text_decoration.thickness {
            write_css_attr!(
                b"text-decoration-thickness",
                number_to_css_string(thickness, false).as_bytes()
            );
        }
    }

    if let Some(font_family) = &container.font_family {
        write_css_attr!(b"font-family", font_family.join(",").as_bytes());
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
        Element::Image { source, .. } => {
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
