//! HTML and CSS conversion utilities for `HyperChad` containers.
//!
//! This module provides low-level functions for converting `HyperChad` containers
//! and elements into HTML markup with CSS styling. It handles element attributes,
//! styles, classes, and supports various HTML elements and layout systems.

#![allow(clippy::module_name_repetitions)]

use std::{collections::BTreeMap, io::Write};

use hyperchad_renderer::{Color, HtmlTagRenderer};
use hyperchad_router::Container;
use hyperchad_transformer::{
    Calculation, Element, HeaderSize, Input, Number,
    models::{
        AlignItems, Cursor, ImageFit, ImageLoading, JustifyContent, LayoutDirection,
        LayoutOverflow, LinkTarget, OverflowWrap, Position, TextAlign, TextDecorationLine,
        TextDecorationStyle, TextOverflow, UserSelect, Visibility, WhiteSpace,
    },
};

/// Writes multiple container elements to HTML output.
///
/// Iterates through the containers and converts each one to its HTML representation.
///
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

/// Writes an HTML attribute with name and value to the output.
///
/// Formats as ` name="value"` with proper escaping.
///
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

/// Writes a CSS property declaration to the output.
///
/// Formats as `property:value;` for use within a style attribute.
///
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

/// Writes a CSS property declaration with `!important` flag to the output.
///
/// Formats as `property:value !important;` for use within style attributes
/// or media queries where higher specificity is needed.
///
/// # Errors
///
/// * If there was an IO error writing the css attribute
pub fn write_css_attr_important(
    f: &mut dyn Write,
    attr: &[u8],
    value: &[u8],
) -> Result<(), std::io::Error> {
    f.write_all(attr)?;
    f.write_all(b":")?;
    f.write_all(value)?;
    f.write_all(b" !important;")?;
    Ok(())
}

/// Converts a number to an HTML/CSS string representation.
///
/// When `px` is true, numeric values are suffixed with `px` for CSS pixel units.
#[must_use]
pub fn number_to_html_string(number: &Number, px: bool) -> String {
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
        Number::RealVw(x) => format!("{x}vw"),
        Number::IntegerVw(x) => format!("{x}vw"),
        Number::RealVh(x) => format!("{x}vh"),
        Number::IntegerVh(x) => format!("{x}vh"),
        Number::RealDvw(x) => format!("{x}dvw"),
        Number::IntegerDvw(x) => format!("{x}dvw"),
        Number::RealDvh(x) => format!("{x}dvh"),
        Number::IntegerDvh(x) => format!("{x}dvh"),
        Number::Calc(x) => format!("calc({})", calc_to_css_string(x, px)),
    }
}

/// Converts a color to a CSS color string (rgb or rgba).
#[must_use]
pub fn color_to_css_string(color: Color) -> String {
    color.a.map_or_else(
        || format!("rgb({},{},{})", color.r, color.g, color.b),
        |a| {
            format!(
                "rgba({},{},{},{})",
                color.r,
                color.g,
                color.b,
                f64::from(a) / f64::from(u8::MAX)
            )
        },
    )
}

/// Converts a calculation expression to a CSS `calc()` string.
///
/// When `px` is true, numeric values are suffixed with `px` for CSS pixel units.
#[must_use]
pub fn calc_to_css_string(calc: &Calculation, px: bool) -> String {
    match calc {
        Calculation::Number(number) => number_to_html_string(number, px),
        Calculation::Add(left, right) => format!(
            "{} + {}",
            calc_to_css_string(left, px),
            calc_to_css_string(right, px)
        ),
        Calculation::Subtract(left, right) => format!(
            "{} - {}",
            calc_to_css_string(left, px),
            calc_to_css_string(right, px)
        ),
        Calculation::Multiply(left, right) => format!(
            "{} * {}",
            calc_to_css_string(left, px),
            calc_to_css_string(right, false)
        ),
        Calculation::Divide(left, right) => format!(
            "{} / {}",
            calc_to_css_string(left, px),
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

const fn is_grid_container(container: &Container) -> bool {
    matches!(container.overflow_x, LayoutOverflow::Wrap { grid: true })
}

/// Writes the style attribute for a container element to the output.
///
/// Converts container properties like dimensions, positioning, flexbox settings,
/// colors, borders, and text styling into inline CSS within a style attribute.
///
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
            write_css_attr!(b"vertical-align", b"top");
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
        | Element::Text { .. }
        | Element::Aside
        | Element::Main
        | Element::Header
        | Element::Footer
        | Element::Section
        | Element::Form { .. }
        | Element::Span
        | Element::Input { .. }
        | Element::Textarea { .. }
        | Element::Button { .. }
        | Element::Anchor { .. }
        | Element::Heading { .. }
        | Element::UnorderedList
        | Element::OrderedList
        | Element::ListItem
        | Element::Table
        | Element::THead
        | Element::TH { .. }
        | Element::TBody
        | Element::TR
        | Element::TD { .. }
        | Element::Canvas
        | Element::Details { .. }
        | Element::Summary
        | Element::Select { .. }
        | Element::Option { .. } => {}
    }

    let is_grid = is_grid_container(container);
    let is_flex = !is_grid && container.is_flex_container();

    if is_flex {
        write_css_attr!(b"display", b"flex");

        if container.direction == LayoutDirection::Column {
            write_css_attr!(b"flex-direction", b"column");
        }
    } else if is_grid {
        write_css_attr!(b"display", b"grid");
    }

    match container.overflow_x {
        LayoutOverflow::Auto => {
            write_css_attr!(b"overflow-x", b"auto");
        }
        LayoutOverflow::Scroll => {
            write_css_attr!(b"overflow-x", b"scroll");
        }
        LayoutOverflow::Wrap { grid } => {
            if grid {
                if let Some(size) = &container.grid_cell_size {
                    write_css_attr!(
                        b"grid-template-columns",
                        format!("repeat(auto-fill, {})", number_to_html_string(size, true))
                            .as_bytes()
                    );
                }
            } else {
                write_css_attr!(b"flex-wrap", b"wrap");
            }
        }
        LayoutOverflow::Hidden => {
            write_css_attr!(b"overflow-x", b"hidden");
        }
        LayoutOverflow::Expand | LayoutOverflow::Squash => {}
    }
    match container.overflow_y {
        LayoutOverflow::Auto => {
            write_css_attr!(b"overflow-y", b"auto");
        }
        LayoutOverflow::Scroll => {
            write_css_attr!(b"overflow-y", b"scroll");
        }
        LayoutOverflow::Wrap { grid } => {
            if grid {
                if let Some(size) = &container.grid_cell_size {
                    write_css_attr!(
                        b"grid-template-columns",
                        format!("repeat(auto-fill, {})", number_to_html_string(size, true))
                            .as_bytes()
                    );
                }
            } else {
                write_css_attr!(b"flex-wrap", b"wrap");
            }
        }
        LayoutOverflow::Hidden => {
            write_css_attr!(b"overflow-y", b"hidden");
        }
        LayoutOverflow::Expand | LayoutOverflow::Squash => {}
    }

    if let Some(position) = container.position {
        match position {
            Position::Relative => {
                write_css_attr!(b"position", b"relative");
            }
            Position::Absolute => {
                write_css_attr!(b"position", b"absolute");
                if container.top.is_none() && container.bottom.is_none() {
                    write_css_attr!(b"top", b"0");
                }
                if container.left.is_none() && container.right.is_none() {
                    write_css_attr!(b"left", b"0");
                }
            }
            Position::Fixed => {
                write_css_attr!(b"position", b"fixed");
                if container.top.is_none() && container.bottom.is_none() {
                    write_css_attr!(b"top", b"0");
                }
                if container.left.is_none() && container.right.is_none() {
                    write_css_attr!(b"left", b"0");
                }
            }
            Position::Sticky => {
                write_css_attr!(b"position", b"sticky");
            }
            Position::Static => {
                write_css_attr!(b"position", b"static");
            }
        }
    }

    if let Some(margin_left) = &container.margin_left {
        write_css_attr!(
            b"margin-left",
            number_to_html_string(margin_left, true).as_bytes(),
        );
    }
    if let Some(margin_right) = &container.margin_right {
        write_css_attr!(
            b"margin-right",
            number_to_html_string(margin_right, true).as_bytes(),
        );
    }
    if let Some(margin_top) = &container.margin_top {
        write_css_attr!(
            b"margin-top",
            number_to_html_string(margin_top, true).as_bytes(),
        );
    }
    if let Some(margin_bottom) = &container.margin_bottom {
        write_css_attr!(
            b"margin-bottom",
            number_to_html_string(margin_bottom, true).as_bytes(),
        );
    }

    if let Some(padding_left) = &container.padding_left {
        write_css_attr!(
            b"padding-left",
            number_to_html_string(padding_left, true).as_bytes(),
        );
    }
    if let Some(padding_right) = &container.padding_right {
        write_css_attr!(
            b"padding-right",
            number_to_html_string(padding_right, true).as_bytes(),
        );
    }
    if let Some(padding_top) = &container.padding_top {
        write_css_attr!(
            b"padding-top",
            number_to_html_string(padding_top, true).as_bytes(),
        );
    }
    if let Some(padding_bottom) = &container.padding_bottom {
        write_css_attr!(
            b"padding-bottom",
            number_to_html_string(padding_bottom, true).as_bytes(),
        );
    }

    if let Some(left) = &container.left {
        write_css_attr!(b"left", number_to_html_string(left, true).as_bytes());
    }
    if let Some(right) = &container.right {
        write_css_attr!(b"right", number_to_html_string(right, true).as_bytes());
    }
    if let Some(top) = &container.top {
        write_css_attr!(b"top", number_to_html_string(top, true).as_bytes());
    }
    if let Some(bottom) = &container.bottom {
        write_css_attr!(b"bottom", number_to_html_string(bottom, true).as_bytes());
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
            number_to_html_string(translate, true).as_bytes()
        );
    }
    if let Some(translate) = &container.translate_y {
        write_transform_attr!(
            b"translateY",
            number_to_html_string(translate, true).as_bytes()
        );
    }

    if printed_transform_start {
        f.write_all(b";")?;
    }

    if let Some(visibility) = container.visibility {
        match visibility {
            Visibility::Visible => {}
            Visibility::Hidden => {
                write_css_attr!(b"visibility", b"hidden");
            }
        }
    }

    if container.hidden == Some(true) {
        write_css_attr!(b"display", b"none");
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

    if let Some(gap) = &container.column_gap {
        write_css_attr!(
            if is_grid {
                b"grid-column-gap"
            } else {
                b"column-gap"
            },
            number_to_html_string(gap, true).as_bytes()
        );
    }
    if let Some(gap) = &container.row_gap {
        write_css_attr!(
            if is_grid { b"grid-row-gap" } else { b"row-gap" },
            number_to_html_string(gap, true).as_bytes()
        );
    }

    if let Some(width) = &container.width {
        write_css_attr!(b"width", number_to_html_string(width, true).as_bytes());
    }
    if let Some(height) = &container.height {
        write_css_attr!(b"height", number_to_html_string(height, true).as_bytes());
    }

    if let Some(width) = &container.min_width {
        write_css_attr!(b"min-width", number_to_html_string(width, true).as_bytes());
    }
    if let Some(width) = &container.max_width {
        write_css_attr!(b"max-width", number_to_html_string(width, true).as_bytes());
    }
    if let Some(height) = &container.min_height {
        write_css_attr!(
            b"min-height",
            number_to_html_string(height, true).as_bytes()
        );
    }
    if let Some(height) = &container.max_height {
        write_css_attr!(
            b"max-height",
            number_to_html_string(height, true).as_bytes()
        );
    }

    if let Some(flex) = &container.flex {
        write_css_attr!(
            b"flex-grow",
            number_to_html_string(&flex.grow, false).as_bytes()
        );
        write_css_attr!(
            b"flex-shrink",
            number_to_html_string(&flex.shrink, false).as_bytes()
        );
        write_css_attr!(
            b"flex-basis",
            number_to_html_string(&flex.basis, false).as_bytes()
        );
    }

    if let Some(background) = container.background {
        write_css_attr!(b"background", color_to_css_string(background).as_bytes());
    }

    if let Some((color, size)) = &container.border_top {
        write_css_attr!(
            b"border-top",
            &[
                number_to_html_string(size, true).as_bytes(),
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
                number_to_html_string(size, true).as_bytes(),
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
                number_to_html_string(size, true).as_bytes(),
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
                number_to_html_string(size, true).as_bytes(),
                b" solid ",
                color_to_css_string(*color).as_bytes(),
            ]
            .concat(),
        );
    }

    if let Some(radius) = &container.border_top_left_radius {
        write_css_attr!(
            b"border-top-left-radius",
            number_to_html_string(radius, true).as_bytes(),
        );
    }

    if let Some(radius) = &container.border_top_right_radius {
        write_css_attr!(
            b"border-top-right-radius",
            number_to_html_string(radius, true).as_bytes(),
        );
    }

    if let Some(radius) = &container.border_bottom_left_radius {
        write_css_attr!(
            b"border-bottom-left-radius",
            number_to_html_string(radius, true).as_bytes(),
        );
    }

    if let Some(radius) = &container.border_bottom_right_radius {
        write_css_attr!(
            b"border-bottom-right-radius",
            number_to_html_string(radius, true).as_bytes(),
        );
    }

    if let Some(font_size) = &container.font_size {
        write_css_attr!(
            b"font-size",
            number_to_html_string(font_size, true).as_bytes(),
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

    if let Some(white_space) = &container.white_space {
        write_css_attr!(
            b"white-space",
            match white_space {
                WhiteSpace::Normal => b"normal",
                WhiteSpace::Preserve => b"pre",
                WhiteSpace::PreserveWrap => b"pre-wrap",
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
                number_to_html_string(thickness, false).as_bytes()
            );
        }
    }

    if let Some(font_family) = &container.font_family {
        write_css_attr!(b"font-family", font_family.join(",").as_bytes());
    }

    if let Some(font_weight) = &container.font_weight {
        write_css_attr!(b"font-weight", font_weight.to_string().as_bytes());
    }

    if let Some(cursor) = &container.cursor {
        write_css_attr!(
            b"cursor",
            match cursor {
                Cursor::Auto => b"auto",
                Cursor::Pointer => b"pointer",
                Cursor::Text => b"text",
                Cursor::Crosshair => b"crosshair",
                Cursor::Move => b"move",
                Cursor::NotAllowed => b"not-allowed",
                Cursor::NoDrop => b"no-drop",
                Cursor::Grab => b"grab",
                Cursor::Grabbing => b"grabbing",
                Cursor::AllScroll => b"all-scroll",
                Cursor::ColResize => b"col-resize",
                Cursor::RowResize => b"row-resize",
                Cursor::NResize => b"n-resize",
                Cursor::EResize => b"e-resize",
                Cursor::SResize => b"s-resize",
                Cursor::WResize => b"w-resize",
                Cursor::NeResize => b"ne-resize",
                Cursor::NwResize => b"nw-resize",
                Cursor::SeResize => b"se-resize",
                Cursor::SwResize => b"sw-resize",
                Cursor::EwResize => b"ew-resize",
                Cursor::NsResize => b"ns-resize",
                Cursor::NeswResize => b"nesw-resize",
                Cursor::ZoomIn => b"zoom-in",
                Cursor::ZoomOut => b"zoom-out",
            }
        );
    }

    if let Some(user_select) = &container.user_select {
        write_css_attr!(
            b"user-select",
            match user_select {
                UserSelect::Auto => b"auto",
                UserSelect::None => b"none",
                UserSelect::Text => b"text",
                UserSelect::All => b"all",
            }
        );
    }

    if let Some(overflow_wrap) = &container.overflow_wrap {
        write_css_attr!(
            b"overflow-wrap",
            match overflow_wrap {
                OverflowWrap::Normal => b"normal",
                OverflowWrap::BreakWord => b"break-word",
                OverflowWrap::Anywhere => b"anywhere",
            }
        );
    }

    if let Some(text_overflow) = &container.text_overflow {
        write_css_attr!(
            b"text-overflow",
            match text_overflow {
                TextOverflow::Clip => b"clip",
                TextOverflow::Ellipsis => b"ellipsis",
            }
        );
    }

    if printed_start {
        f.write_all(b"\"")?;
    }

    Ok(())
}

/// Writes the class attribute for a container element to the output.
///
/// Generates HTML class attribute including default classes for specific elements
/// (like removing button/table default styles) and custom classes from the container.
///
/// # Errors
///
/// * If there were any IO errors writing the element class attribute
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
pub fn element_classes_to_html(
    f: &mut dyn Write,
    container: &Container,
) -> Result<(), std::io::Error> {
    let mut printed_start = false;

    match container.element {
        Element::Button { .. } => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" class=\"")?;
            }
            f.write_all(b"remove-button-styles")?;
        }
        Element::Table => {
            if !printed_start {
                printed_start = true;
                f.write_all(b" class=\"")?;
            }
            f.write_all(b"remove-table-styles")?;
        }
        _ => {}
    }

    if !container.classes.is_empty() {
        if printed_start {
            f.write_all(b" ")?;
        } else {
            printed_start = true;
            f.write_all(b" class=\"")?;
        }

        for class in &container.classes {
            f.write_all(class.as_bytes())?;
            f.write_all(b" ")?;
        }
    }

    if printed_start {
        f.write_all(b"\"")?;
    }

    Ok(())
}

/// Renders an option child element within a select, handling the selected state.
///
/// This function renders `<option>` elements that are children of a `<select>`,
/// adding the `selected` attribute when the option's value matches the select's
/// selected value.
///
/// # Errors
///
/// * If there were any IO errors writing the option as HTML
fn render_option_child(
    f: &mut dyn Write,
    child: &Container,
    tag_renderer: &dyn HtmlTagRenderer,
    selected_value: Option<&str>,
) -> Result<(), std::io::Error> {
    if let Element::Option { value, disabled } = &child.element {
        f.write_all(b"<option")?;

        if let Some(value) = value {
            f.write_all(b" value=\"")?;
            f.write_all(value.as_bytes())?;
            f.write_all(b"\"")?;

            // Mark as selected if this option's value matches the select's selected value
            if selected_value == Some(value.as_str()) {
                f.write_all(b" selected")?;
            }
        }
        if *disabled == Some(true) {
            f.write_all(b" disabled")?;
        }

        tag_renderer.element_attrs_to_html(f, child, false)?;
        f.write_all(b">")?;

        elements_to_html(f, &child.children, tag_renderer, false)?;

        f.write_all(b"</option>")?;
    } else {
        // For non-option children, render them normally
        element_to_html(f, child, tag_renderer, false)?;
    }
    Ok(())
}

/// Writes a complete HTML element for a container to the output.
///
/// Converts a container into its corresponding HTML element with all attributes,
/// styles, and child elements. Handles various element types including images,
/// forms, buttons, tables, and semantic HTML elements.
///
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
    if container.debug == Some(true) {
        log::info!("element_to_html: DEBUG {container}");
    }

    match &container.element {
        Element::Raw { value } => {
            f.write_all(value.as_bytes())?;
            return Ok(());
        }
        Element::Text { value } => {
            f.write_all(html_escape::encode_text(value).as_bytes())?;
            return Ok(());
        }
        Element::Image {
            source,
            alt,
            source_set,
            sizes,
            loading,
            ..
        } => {
            const TAG_NAME: &[u8] = b"img";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            if let Some(source) = source {
                f.write_all(b" src=\"")?;
                f.write_all(source.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(srcset) = source_set {
                f.write_all(b" srcset=\"")?;
                f.write_all(srcset.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(sizes) = sizes {
                f.write_all(b" sizes=\"")?;
                f.write_all(number_to_html_string(sizes, true).as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(alt) = alt {
                f.write_all(b" alt=\"")?;
                f.write_all(alt.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(loading) = loading {
                f.write_all(b" loading=\"")?;
                f.write_all(match loading {
                    ImageLoading::Eager => b"eager",
                    ImageLoading::Lazy => b"lazy",
                })?;
                f.write_all(b"\"")?;
            }
            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;
            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;
            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Anchor { href, target } => {
            const TAG_NAME: &[u8] = b"a";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            if let Some(href) = href {
                f.write_all(b" href=\"")?;
                f.write_all(href.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(target) = target {
                f.write_all(b" target=\"")?;
                f.write_all(match target {
                    LinkTarget::SelfTarget => b"_self",
                    LinkTarget::Blank => b"_blank",
                    LinkTarget::Parent => b"_parent",
                    LinkTarget::Top => b"_top",
                    LinkTarget::Custom(target) => target.as_bytes(),
                })?;
                f.write_all(b"\"")?;
            }
            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;
            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
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
                container.is_flex_container(),
            )?;
            f.write_all(b"</")?;
            f.write_all(tag_name)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Form { action, method } => {
            const TAG_NAME: &[u8] = b"form";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;
            if let Some(action) = action {
                f.write_all(b" action=\"")?;
                f.write_all(action.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(method) = method {
                f.write_all(b" method=\"")?;
                f.write_all(method.as_bytes())?;
                f.write_all(b"\"")?;
            }
            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;
            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;
            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Input {
            name,
            input,
            autofocus,
        } => {
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
                Input::Hidden { value } => {
                    f.write_all(b" type=\"hidden\"")?;
                    if let Some(value) = value {
                        f.write_all(b" value=\"")?;
                        f.write_all(value.as_bytes())?;
                        f.write_all(b"\"")?;
                    }
                }
            }

            if let Some(name) = name {
                f.write_all(b" name=\"")?;
                f.write_all(name.as_bytes())?;
                f.write_all(b"\"")?;
            }

            if matches!(autofocus, Some(true)) {
                f.write_all(b"autofocus")?;
            }

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b"></")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Textarea {
            name,
            placeholder,
            rows,
            cols,
            value,
        } => {
            const TAG_NAME: &[u8] = b"textarea";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;

            if let Some(name) = name {
                f.write_all(b" name=\"")?;
                f.write_all(name.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(placeholder) = placeholder {
                f.write_all(b" placeholder=\"")?;
                f.write_all(placeholder.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(rows) = rows {
                f.write_all(b" rows=\"")?;
                write!(f, "{rows}")?;
                f.write_all(b"\"")?;
            }
            if let Some(cols) = cols {
                f.write_all(b" cols=\"")?;
                write!(f, "{cols}")?;
                f.write_all(b"\"")?;
            }

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;
            f.write_all(value.as_bytes())?;
            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;

            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Button { r#type } => {
            const TAG_NAME: &[u8] = b"button";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;

            f.write_all(b" type=\"")?;
            f.write_all(r#type.as_deref().map_or(b"button", str::as_bytes))?;
            f.write_all(b"\"")?;

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;

            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;

            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::TH { rows, columns } => {
            const TAG_NAME: &[u8] = b"th";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;

            if let Some(rows) = rows {
                f.write_all(b" rowspan=\"")?;
                write!(f, "{rows}")?;
                f.write_all(b"\"")?;
            }
            if let Some(columns) = columns {
                f.write_all(b" colspan=\"")?;
                write!(f, "{columns}")?;
                f.write_all(b"\"")?;
            }

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;

            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;

            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::TD { rows, columns } => {
            const TAG_NAME: &[u8] = b"td";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;

            if let Some(rows) = rows {
                f.write_all(b" rowspan=\"")?;
                write!(f, "{rows}")?;
                f.write_all(b"\"")?;
            }
            if let Some(columns) = columns {
                f.write_all(b" colspan=\"")?;
                write!(f, "{columns}")?;
                f.write_all(b"\"")?;
            }

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;

            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;

            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Details { open } => {
            const TAG_NAME: &[u8] = b"details";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;

            if *open == Some(true) {
                f.write_all(b" open")?;
            }

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;

            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;

            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Select {
            name,
            selected,
            multiple,
            disabled,
            autofocus,
        } => {
            const TAG_NAME: &[u8] = b"select";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;

            if let Some(name) = name {
                f.write_all(b" name=\"")?;
                f.write_all(name.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if let Some(selected) = selected {
                f.write_all(b" data-selected=\"")?;
                f.write_all(selected.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if *multiple == Some(true) {
                f.write_all(b" multiple")?;
            }
            if *disabled == Some(true) {
                f.write_all(b" disabled")?;
            }
            if *autofocus == Some(true) {
                f.write_all(b" autofocus")?;
            }

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;

            // Render option children, marking the selected one
            for child in &container.children {
                render_option_child(f, child, tag_renderer, selected.as_deref())?;
            }

            f.write_all(b"</")?;
            f.write_all(TAG_NAME)?;
            f.write_all(b">")?;
            return Ok(());
        }
        Element::Option { value, disabled } => {
            const TAG_NAME: &[u8] = b"option";
            f.write_all(b"<")?;
            f.write_all(TAG_NAME)?;

            if let Some(value) = value {
                f.write_all(b" value=\"")?;
                f.write_all(value.as_bytes())?;
                f.write_all(b"\"")?;
            }
            if *disabled == Some(true) {
                f.write_all(b" disabled")?;
            }

            tag_renderer.element_attrs_to_html(f, container, is_flex_child)?;
            f.write_all(b">")?;

            elements_to_html(
                f,
                &container.children,
                tag_renderer,
                container.is_flex_container(),
            )?;

            f.write_all(b"</")?;
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
        Element::Span => Some("span"),
        Element::UnorderedList => Some("ul"),
        Element::OrderedList => Some("ol"),
        Element::ListItem => Some("li"),
        Element::Table => Some("table"),
        Element::THead => Some("thead"),
        Element::TBody => Some("tbody"),
        Element::TR => Some("tr"),
        Element::Canvas => Some("canvas"),
        Element::Summary => Some("summary"),
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
            container.is_flex_container(),
        )?;
        f.write_all(b"</")?;
        f.write_all(tag_name.as_bytes())?;
        f.write_all(b">")?;
    }

    Ok(())
}

/// Converts a container's child elements to an HTML string.
///
/// Renders all children of the container and returns the HTML as a string,
/// without wrapping document structure. Useful for generating HTML fragments.
///
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
        container.is_flex_container(),
    )?;

    Ok(std::str::from_utf8(&buffer)
        .map_err(std::io::Error::other)?
        .to_string())
}

/// Converts a container to a complete HTML document response.
///
/// Generates a full HTML page with doctype, head section (including CSS and metadata),
/// and body containing the rendered container. This is used for serving complete
/// HTML pages in web applications.
///
/// # Errors
///
/// * If there were any IO errors writing the `Container` as an HTML response
#[allow(
    clippy::similar_names,
    clippy::implicit_hasher,
    clippy::too_many_arguments
)]
pub fn container_element_to_html_response(
    headers: &BTreeMap<String, String>,
    container: &Container,
    viewport: Option<&str>,
    background: Option<Color>,
    title: Option<&str>,
    description: Option<&str>,
    tag_renderer: &dyn HtmlTagRenderer,
    css_urls: &[String],
    css_paths: &[String],
    inline_css: &[String],
) -> Result<String, std::io::Error> {
    Ok(tag_renderer.root_html(
        headers,
        container,
        container_element_to_html(container, tag_renderer)?,
        viewport,
        background,
        title,
        description,
        css_urls,
        css_paths,
        inline_css,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::override_item_to_css_name;
    use hyperchad_transformer::{Calculation, OverrideItem};

    #[test_log::test]
    fn test_number_to_html_string_real() {
        assert_eq!(number_to_html_string(&Number::Real(10.5), false), "10.5");
        assert_eq!(number_to_html_string(&Number::Real(10.5), true), "10.5px");
        assert_eq!(number_to_html_string(&Number::Real(0.0), true), "0px");
    }

    #[test_log::test]
    fn test_number_to_html_string_integer() {
        assert_eq!(number_to_html_string(&Number::Integer(42), false), "42");
        assert_eq!(number_to_html_string(&Number::Integer(42), true), "42px");
        assert_eq!(number_to_html_string(&Number::Integer(0), true), "0px");
    }

    #[test_log::test]
    fn test_number_to_html_string_percent() {
        assert_eq!(
            number_to_html_string(&Number::RealPercent(50.5), false),
            "50.5%"
        );
        assert_eq!(
            number_to_html_string(&Number::RealPercent(50.5), true),
            "50.5%"
        );
        assert_eq!(
            number_to_html_string(&Number::IntegerPercent(100), false),
            "100%"
        );
        assert_eq!(
            number_to_html_string(&Number::IntegerPercent(100), true),
            "100%"
        );
    }

    #[test_log::test]
    fn test_number_to_html_string_viewport_units() {
        assert_eq!(
            number_to_html_string(&Number::RealVw(50.5), false),
            "50.5vw"
        );
        assert_eq!(
            number_to_html_string(&Number::IntegerVh(100), false),
            "100vh"
        );
        assert_eq!(
            number_to_html_string(&Number::RealDvw(25.0), false),
            "25dvw"
        );
        assert_eq!(
            number_to_html_string(&Number::IntegerDvh(75), false),
            "75dvh"
        );
    }

    #[test_log::test]
    fn test_number_to_html_string_calc() {
        let calc = Number::Calc(Calculation::Add(
            Box::new(Calculation::Number(Box::new(Number::RealPercent(100.0)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(20)))),
        ));
        assert_eq!(number_to_html_string(&calc, true), "calc(100% + 20px)");
    }

    #[test_log::test]
    fn test_color_to_css_string_rgb() {
        let color = Color {
            r: 255,
            g: 128,
            b: 0,
            a: None,
        };
        assert_eq!(color_to_css_string(color), "rgb(255,128,0)");
    }

    #[test_log::test]
    fn test_color_to_css_string_rgba() {
        let color = Color {
            r: 255,
            g: 128,
            b: 0,
            a: Some(128),
        };
        assert_eq!(
            color_to_css_string(color),
            "rgba(255,128,0,0.5019607843137255)"
        );
    }

    #[test_log::test]
    fn test_color_to_css_string_rgba_transparent() {
        let color = Color {
            r: 0,
            g: 0,
            b: 0,
            a: Some(0),
        };
        assert_eq!(color_to_css_string(color), "rgba(0,0,0,0)");
    }

    #[test_log::test]
    fn test_color_to_css_string_rgba_opaque() {
        let color = Color {
            r: 255,
            g: 255,
            b: 255,
            a: Some(255),
        };
        assert_eq!(color_to_css_string(color), "rgba(255,255,255,1)");
    }

    #[test_log::test]
    fn test_calc_to_css_string_number() {
        let calc = Calculation::Number(Box::new(Number::Integer(42)));
        assert_eq!(calc_to_css_string(&calc, true), "42px");
        assert_eq!(calc_to_css_string(&calc, false), "42");
    }

    #[test_log::test]
    fn test_calc_to_css_string_add() {
        let calc = Calculation::Add(
            Box::new(Calculation::Number(Box::new(Number::RealPercent(100.0)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(20)))),
        );
        assert_eq!(calc_to_css_string(&calc, true), "100% + 20px");
    }

    #[test_log::test]
    fn test_calc_to_css_string_subtract() {
        let calc = Calculation::Subtract(
            Box::new(Calculation::Number(Box::new(Number::RealVh(100.0)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(50)))),
        );
        assert_eq!(calc_to_css_string(&calc, true), "100vh - 50px");
    }

    #[test_log::test]
    fn test_calc_to_css_string_multiply() {
        let calc = Calculation::Multiply(
            Box::new(Calculation::Number(Box::new(Number::Integer(10)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(2)))),
        );
        assert_eq!(calc_to_css_string(&calc, true), "10px * 2");
    }

    #[test_log::test]
    fn test_calc_to_css_string_divide() {
        let calc = Calculation::Divide(
            Box::new(Calculation::Number(Box::new(Number::Integer(100)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(2)))),
        );
        assert_eq!(calc_to_css_string(&calc, true), "100px / 2");
    }

    #[test_log::test]
    fn test_calc_to_css_string_grouping() {
        let calc = Calculation::Grouping(Box::new(Calculation::Add(
            Box::new(Calculation::Number(Box::new(Number::Integer(1)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(2)))),
        )));
        assert_eq!(calc_to_css_string(&calc, true), "(1px + 2px)");
    }

    #[test_log::test]
    fn test_calc_to_css_string_min() {
        let calc = Calculation::Min(
            Box::new(Calculation::Number(Box::new(Number::RealPercent(100.0)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(500)))),
        );
        assert_eq!(calc_to_css_string(&calc, true), "min(100%, 500px)");
    }

    #[test_log::test]
    fn test_calc_to_css_string_max() {
        let calc = Calculation::Max(
            Box::new(Calculation::Number(Box::new(Number::Integer(300)))),
            Box::new(Calculation::Number(Box::new(Number::RealPercent(50.0)))),
        );
        assert_eq!(calc_to_css_string(&calc, true), "max(300px, 50%)");
    }

    #[test_log::test]
    fn test_calc_to_css_string_complex() {
        let calc = Calculation::Add(
            Box::new(Calculation::Multiply(
                Box::new(Calculation::Number(Box::new(Number::RealPercent(50.0)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(2)))),
            )),
            Box::new(Calculation::Number(Box::new(Number::Integer(10)))),
        );
        assert_eq!(calc_to_css_string(&calc, true), "50% * 2 + 10px");
    }

    #[test_log::test]
    fn test_write_attr() {
        let mut buffer = Vec::new();
        write_attr(&mut buffer, b"id", b"test-id").unwrap();
        assert_eq!(std::str::from_utf8(&buffer).unwrap(), " id=\"test-id\"");
    }

    #[test_log::test]
    fn test_write_attr_empty_value() {
        let mut buffer = Vec::new();
        write_attr(&mut buffer, b"class", b"").unwrap();
        assert_eq!(std::str::from_utf8(&buffer).unwrap(), " class=\"\"");
    }

    #[test_log::test]
    fn test_write_css_attr() {
        let mut buffer = Vec::new();
        write_css_attr(&mut buffer, b"color", b"red").unwrap();
        assert_eq!(std::str::from_utf8(&buffer).unwrap(), "color:red;");
    }

    #[test_log::test]
    fn test_write_css_attr_with_units() {
        let mut buffer = Vec::new();
        write_css_attr(&mut buffer, b"width", b"100px").unwrap();
        assert_eq!(std::str::from_utf8(&buffer).unwrap(), "width:100px;");
    }

    #[test_log::test]
    fn test_write_css_attr_important() {
        let mut buffer = Vec::new();
        write_css_attr_important(&mut buffer, b"display", b"none").unwrap();
        assert_eq!(
            std::str::from_utf8(&buffer).unwrap(),
            "display:none !important;"
        );
    }

    #[test_log::test]
    fn test_write_css_attr_important_with_value() {
        let mut buffer = Vec::new();
        write_css_attr_important(&mut buffer, b"margin", b"0").unwrap();
        assert_eq!(
            std::str::from_utf8(&buffer).unwrap(),
            "margin:0 !important;"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_dimensions() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::Width(Number::Integer(100))),
            b"width"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::Height(Number::Integer(100))),
            b"height"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::MinWidth(Number::Integer(0))),
            b"min-width"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::MaxHeight(Number::Integer(500))),
            b"max-height"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_margins() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::MarginLeft(Number::Integer(10))),
            b"margin-left"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::MarginRight(Number::Integer(10))),
            b"margin-right"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::MarginTop(Number::Integer(10))),
            b"margin-top"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::MarginBottom(Number::Integer(10))),
            b"margin-bottom"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_padding() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::PaddingLeft(Number::Integer(5))),
            b"padding-left"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::PaddingRight(Number::Integer(5))),
            b"padding-right"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::PaddingTop(Number::Integer(5))),
            b"padding-top"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::PaddingBottom(Number::Integer(5))),
            b"padding-bottom"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_layout() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::Direction(LayoutDirection::Row)),
            b"flex-direction"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::JustifyContent(JustifyContent::Center)),
            b"justify-content"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::AlignItems(AlignItems::Center)),
            b"align-items"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_text() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::FontSize(Number::Integer(16))),
            b"font-size"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::TextAlign(TextAlign::Center)),
            b"text-align"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::WhiteSpace(WhiteSpace::Normal)),
            b"white-space"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_visibility() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::Hidden(true)),
            b"display"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::Visibility(Visibility::Hidden)),
            b"visibility"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_borders() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::BorderTopLeftRadius(Number::Integer(5))),
            b"border-top-left-radius"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::BorderTopRightRadius(Number::Integer(5))),
            b"border-top-right-radius"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::BorderBottomLeftRadius(Number::Integer(5))),
            b"border-bottom-left-radius"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::BorderBottomRightRadius(Number::Integer(5))),
            b"border-bottom-right-radius"
        );
    }

    #[test_log::test]
    fn test_override_item_to_css_name_transform() {
        assert_eq!(
            override_item_to_css_name(&OverrideItem::TranslateX(Number::Integer(10))),
            b"transform"
        );
        assert_eq!(
            override_item_to_css_name(&OverrideItem::TranslateY(Number::Integer(10))),
            b"transform"
        );
    }

    // Tests for element_to_html with various element types
    #[test_log::test]
    fn test_element_to_html_div() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Div,
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<div"));
        assert!(html.ends_with("</div>"));
    }

    #[test_log::test]
    fn test_element_to_html_raw() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Raw {
                value: "Hello <b>World</b>".to_string(),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert_eq!(html, "Hello <b>World</b>");
    }

    #[test_log::test]
    fn test_element_to_html_image_with_source_and_alt() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::models::ImageFit;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Image {
                source: Some("/images/logo.png".to_string()),
                alt: Some("Logo".to_string()),
                fit: Some(ImageFit::Cover),
                source_set: None,
                sizes: None,
                loading: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("src=\"/images/logo.png\""));
        assert!(html.contains("alt=\"Logo\""));
        assert!(html.contains("object-fit:cover"));
    }

    #[test_log::test]
    fn test_element_to_html_anchor_with_href_and_target() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LinkTarget;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Anchor {
                href: Some("https://example.com".to_string()),
                target: Some(LinkTarget::Blank),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<a"));
        assert!(html.contains("href=\"https://example.com\""));
        assert!(html.contains("target=\"_blank\""));
        assert!(html.ends_with("</a>"));
    }

    #[test_log::test]
    fn test_element_to_html_button_with_custom_type() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Button {
                r#type: Some("submit".to_string()),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<button"));
        assert!(html.contains("type=\"submit\""));
        assert!(html.ends_with("</button>"));
    }

    #[test_log::test]
    fn test_element_to_html_button_default_type() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Button { r#type: None },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        // Default type should be "button"
        assert!(html.contains("type=\"button\""));
    }

    #[test_log::test]
    fn test_element_to_html_input_text_with_placeholder() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Text {
                    value: Some("initial".to_string()),
                    placeholder: Some("Enter text...".to_string()),
                },
                name: Some("username".to_string()),
                autofocus: Some(true),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<input"));
        assert!(html.contains("type=\"text\""));
        assert!(html.contains("value=\"initial\""));
        assert!(html.contains("placeholder=\"Enter text...\""));
        assert!(html.contains("name=\"username\""));
        assert!(html.contains("autofocus"));
    }

    #[test_log::test]
    fn test_element_to_html_input_checkbox_checked() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Checkbox {
                    checked: Some(true),
                },
                name: Some("agree".to_string()),
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("checked=\"checked\""));
    }

    #[test_log::test]
    fn test_element_to_html_heading_sizes() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::HeaderSize;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        for (size, expected_tag) in [
            (HeaderSize::H1, "h1"),
            (HeaderSize::H2, "h2"),
            (HeaderSize::H3, "h3"),
            (HeaderSize::H4, "h4"),
            (HeaderSize::H5, "h5"),
            (HeaderSize::H6, "h6"),
        ] {
            let container = Container {
                element: hyperchad_transformer::Element::Heading { size },
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
            let html = std::str::from_utf8(&buffer).unwrap();

            assert!(
                html.starts_with(&format!("<{expected_tag}")),
                "Expected to start with <{expected_tag}, got: {html}"
            );
            assert!(
                html.ends_with(&format!("</{expected_tag}>")),
                "Expected to end with </{expected_tag}>, got: {html}"
            );
        }
    }

    #[test_log::test]
    fn test_element_to_html_table_with_rowspan_colspan() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        // Test TH with rows/columns
        let th_container = Container {
            element: hyperchad_transformer::Element::TH {
                rows: Some(Number::Integer(2)),
                columns: Some(Number::Integer(3)),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &th_container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<th"));
        assert!(html.contains("rowspan=\"2\""));
        assert!(html.contains("colspan=\"3\""));

        // Test TD with rows/columns
        let td_container = Container {
            element: hyperchad_transformer::Element::TD {
                rows: Some(Number::Integer(4)),
                columns: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &td_container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<td"));
        assert!(html.contains("rowspan=\"4\""));
        assert!(!html.contains("colspan"));
    }

    #[test_log::test]
    fn test_element_to_html_details_open() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        // Test open details
        let container = Container {
            element: hyperchad_transformer::Element::Details { open: Some(true) },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<details"));
        assert!(html.contains(" open"));
        assert!(html.ends_with("</details>"));

        // Test closed details
        let container_closed = Container {
            element: hyperchad_transformer::Element::Details { open: Some(false) },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container_closed, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(!html.contains(" open"));
    }

    #[test_log::test]
    fn test_element_to_html_textarea() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Textarea {
                value: "Hello World".to_string(),
                placeholder: Some("Enter message...".to_string()),
                rows: Some(Number::Integer(5)),
                cols: Some(Number::Integer(40)),
                name: Some("message".to_string()),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<textarea"));
        assert!(html.contains("name=\"message\""));
        assert!(html.contains("placeholder=\"Enter message...\""));
        assert!(html.contains("rows=\"5\""));
        assert!(html.contains("cols=\"40\""));
        assert!(html.contains("Hello World"));
        assert!(html.ends_with("</textarea>"));
    }

    // Tests for element_style_to_html
    #[test_log::test]
    fn test_element_style_to_html_flex_container() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LayoutDirection;

        let container = Container {
            direction: LayoutDirection::Column,
            justify_content: Some(hyperchad_transformer::models::JustifyContent::Center),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("display:flex"));
        assert!(style.contains("flex-direction:column"));
        assert!(style.contains("justify-content:center"));
    }

    #[test_log::test]
    fn test_element_style_to_html_position_absolute() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Position;

        let container = Container {
            position: Some(Position::Absolute),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("position:absolute"));
        // Absolute position without explicit top/left should default to 0
        assert!(style.contains("top:0"));
        assert!(style.contains("left:0"));
    }

    #[test_log::test]
    fn test_element_style_to_html_position_with_explicit_values() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Position;

        let container = Container {
            position: Some(Position::Absolute),
            top: Some(Number::Integer(10)),
            right: Some(Number::Integer(20)),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("position:absolute"));
        assert!(style.contains("top:10px"));
        assert!(style.contains("right:20px"));
        // Should NOT add left:0 since right is specified
        assert!(!style.contains("left:0"));
    }

    #[test_log::test]
    fn test_element_style_to_html_margins_and_padding() {
        use hyperchad_router::Container;

        let container = Container {
            margin_left: Some(Number::Integer(5)),
            margin_right: Some(Number::RealPercent(10.0)),
            padding_top: Some(Number::Integer(15)),
            padding_bottom: Some(Number::RealVh(5.0)),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("margin-left:5px"));
        assert!(style.contains("margin-right:10%"));
        assert!(style.contains("padding-top:15px"));
        assert!(style.contains("padding-bottom:5vh"));
    }

    #[test_log::test]
    fn test_element_style_to_html_transforms() {
        use hyperchad_router::Container;

        let container = Container {
            translate_x: Some(Number::Integer(50)),
            translate_y: Some(Number::RealPercent(-25.0)),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("transform:translateX(50px) translateY(-25%)"));
    }

    #[test_log::test]
    fn test_element_style_to_html_hidden() {
        use hyperchad_router::Container;

        let container = Container {
            hidden: Some(true),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("display:none"));
    }

    #[test_log::test]
    fn test_element_style_to_html_overflow() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LayoutOverflow;

        let container = Container {
            overflow_x: LayoutOverflow::Auto,
            overflow_y: LayoutOverflow::Hidden,
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("overflow-x:auto"));
        assert!(style.contains("overflow-y:hidden"));
    }

    #[test_log::test]
    fn test_element_style_to_html_flex_properties() {
        use hyperchad_router::Container;
        use hyperchad_transformer::Flex;

        let container = Container {
            flex: Some(Flex {
                grow: Number::Integer(1),
                shrink: Number::Integer(0),
                basis: Number::RealPercent(50.0),
            }),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("flex-grow:1"));
        assert!(style.contains("flex-shrink:0"));
        assert!(style.contains("flex-basis:50%"));
    }

    #[test_log::test]
    fn test_element_style_to_html_borders() {
        use hyperchad_router::Container;

        let color = hyperchad_renderer::Color {
            r: 255,
            g: 0,
            b: 0,
            a: None,
        };

        let container = Container {
            border_top: Some((color, Number::Integer(2))),
            border_top_left_radius: Some(Number::Integer(5)),
            border_bottom_right_radius: Some(Number::RealPercent(50.0)),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("border-top:2px solid rgb(255,0,0)"));
        assert!(style.contains("border-top-left-radius:5px"));
        assert!(style.contains("border-bottom-right-radius:50%"));
    }

    #[test_log::test]
    fn test_element_style_to_html_text_decoration() {
        use hyperchad_router::Container;
        use hyperchad_transformer::TextDecoration;
        use hyperchad_transformer::models::{TextDecorationLine, TextDecorationStyle};

        let color = hyperchad_renderer::Color {
            r: 0,
            g: 0,
            b: 255,
            a: None,
        };

        let container = Container {
            text_decoration: Some(TextDecoration {
                color: Some(color),
                line: vec![TextDecorationLine::Underline, TextDecorationLine::Overline],
                style: Some(TextDecorationStyle::Wavy),
                thickness: Some(Number::Integer(2)),
            }),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("text-decoration-color:rgb(0,0,255)"));
        assert!(style.contains("text-decoration-line:underline overline"));
        assert!(style.contains("text-decoration-style:wavy"));
        assert!(style.contains("text-decoration-thickness:2"));
    }

    #[test_log::test]
    fn test_element_style_to_html_cursor_types() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Cursor;

        for (cursor, expected) in [
            (Cursor::Pointer, "pointer"),
            (Cursor::Text, "text"),
            (Cursor::Move, "move"),
            (Cursor::NotAllowed, "not-allowed"),
            (Cursor::Grab, "grab"),
        ] {
            let container = Container {
                cursor: Some(cursor),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(&format!("cursor:{expected}")),
                "Expected cursor:{expected}, got: {style}"
            );
        }
    }

    // Tests for element_classes_to_html
    #[test_log::test]
    fn test_element_classes_to_html_button() {
        use hyperchad_router::Container;

        let container = Container {
            element: hyperchad_transformer::Element::Button { r#type: None },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_classes_to_html(&mut buffer, &container).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("class=\""));
        assert!(html.contains("remove-button-styles"));
    }

    #[test_log::test]
    fn test_element_classes_to_html_table() {
        use hyperchad_router::Container;

        let container = Container {
            element: hyperchad_transformer::Element::Table,
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_classes_to_html(&mut buffer, &container).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("class=\""));
        assert!(html.contains("remove-table-styles"));
    }

    #[test_log::test]
    fn test_element_classes_to_html_custom_classes() {
        use hyperchad_router::Container;

        let container = Container {
            element: hyperchad_transformer::Element::Div,
            classes: vec!["my-class".to_string(), "another-class".to_string()],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_classes_to_html(&mut buffer, &container).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("class=\""));
        assert!(html.contains("my-class"));
        assert!(html.contains("another-class"));
    }

    #[test_log::test]
    fn test_element_classes_to_html_button_with_custom_classes() {
        use hyperchad_router::Container;

        let container = Container {
            element: hyperchad_transformer::Element::Button { r#type: None },
            classes: vec!["custom-btn".to_string()],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_classes_to_html(&mut buffer, &container).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("remove-button-styles"));
        assert!(html.contains("custom-btn"));
    }

    #[test_log::test]
    fn test_element_classes_to_html_no_classes() {
        use hyperchad_router::Container;

        let container = Container {
            element: hyperchad_transformer::Element::Div,
            classes: vec![],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_classes_to_html(&mut buffer, &container).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        // Should not have class attribute when no classes
        assert_eq!(html, "");
    }

    // Tests for container_element_to_html
    #[test_log::test]
    fn test_container_element_to_html_with_children() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Div,
            children: vec![
                Container {
                    element: hyperchad_transformer::Element::Span,
                    ..Default::default()
                },
                Container {
                    element: hyperchad_transformer::Element::Raw {
                        value: "text".to_string(),
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let html = container_element_to_html(&container, &tag_renderer).unwrap();

        // container_element_to_html renders children, not the container itself
        assert!(html.contains("<span"));
        assert!(html.contains("text"));
    }

    // Tests for elements_to_html
    #[test_log::test]
    fn test_elements_to_html_multiple() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let containers = vec![
            Container {
                element: hyperchad_transformer::Element::Div,
                ..Default::default()
            },
            Container {
                element: hyperchad_transformer::Element::Span,
                ..Default::default()
            },
        ];

        let mut buffer = Vec::new();
        elements_to_html(&mut buffer, &containers, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("<div"));
        assert!(html.contains("</div>"));
        assert!(html.contains("<span"));
        assert!(html.contains("</span>"));
    }

    // Tests for data attributes
    #[test_log::test]
    fn test_element_to_html_with_data_attributes() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use std::collections::BTreeMap;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let mut data = BTreeMap::new();
        data.insert("test-id".to_string(), "123".to_string());
        data.insert("value".to_string(), "hello".to_string());

        let container = Container {
            element: hyperchad_transformer::Element::Div,
            data,
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("data-test-id=\"123\""));
        assert!(html.contains("data-value=\"hello\""));
    }

    // Test for image with srcset and loading
    #[test_log::test]
    fn test_element_to_html_image_with_srcset_and_loading() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::models::ImageLoading;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Image {
                source: Some("/img.jpg".to_string()),
                alt: None,
                fit: None,
                source_set: Some("/img-small.jpg 300w, /img-large.jpg 600w".to_string()),
                sizes: Some(Number::Integer(300)),
                loading: Some(ImageLoading::Lazy),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("srcset=\"/img-small.jpg 300w, /img-large.jpg 600w\""));
        assert!(html.contains("sizes=\"300px\""));
        assert!(html.contains("loading=\"lazy\""));
    }

    // Test for input types
    #[test_log::test]
    fn test_element_to_html_input_password() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Password {
                    value: None,
                    placeholder: Some("Enter password".to_string()),
                },
                name: Some("password".to_string()),
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"password\""));
        assert!(html.contains("placeholder=\"Enter password\""));
        assert!(html.contains("name=\"password\""));
    }

    #[test_log::test]
    fn test_element_to_html_input_hidden() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Hidden {
                    value: Some("secret".to_string()),
                },
                name: Some("csrf_token".to_string()),
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"hidden\""));
        assert!(html.contains("value=\"secret\""));
        assert!(html.contains("name=\"csrf_token\""));
    }

    // Test anchor with custom target
    #[test_log::test]
    fn test_element_to_html_anchor_with_custom_target() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LinkTarget;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Anchor {
                href: Some("/page".to_string()),
                target: Some(LinkTarget::Custom("my-frame".to_string())),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("target=\"my-frame\""));
    }

    // Test grid layout
    #[test_log::test]
    fn test_element_style_to_html_grid_layout() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LayoutOverflow;

        let container = Container {
            overflow_x: LayoutOverflow::Wrap { grid: true },
            grid_cell_size: Some(Number::Integer(200)),
            column_gap: Some(Number::Integer(10)),
            row_gap: Some(Number::Integer(10)),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("display:grid"));
        assert!(style.contains("grid-template-columns:repeat(auto-fill, 200px)"));
        assert!(style.contains("grid-column-gap:10px"));
        assert!(style.contains("grid-row-gap:10px"));
    }

    // Test visibility
    #[test_log::test]
    fn test_element_style_to_html_visibility_hidden() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Visibility;

        let container = Container {
            visibility: Some(Visibility::Hidden),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("visibility:hidden"));
    }

    // Test all position types
    #[test_log::test]
    fn test_element_style_to_html_all_position_types() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Position;

        for (position, expected) in [
            (Position::Relative, "relative"),
            (Position::Static, "static"),
            (Position::Sticky, "sticky"),
            (Position::Fixed, "fixed"),
        ] {
            let container = Container {
                position: Some(position),
                top: Some(Number::Integer(0)),
                left: Some(Number::Integer(0)),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(&format!("position:{expected}")),
                "Expected position:{expected}, got: {style}"
            );
        }
    }

    // Test semantic HTML elements
    #[test_log::test]
    fn test_element_to_html_semantic_elements() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        for (element, expected_tag) in [
            (hyperchad_transformer::Element::Aside, "aside"),
            (hyperchad_transformer::Element::Main, "main"),
            (hyperchad_transformer::Element::Header, "header"),
            (hyperchad_transformer::Element::Footer, "footer"),
            (hyperchad_transformer::Element::Section, "section"),
            (
                hyperchad_transformer::Element::Form {
                    action: None,
                    method: None,
                },
                "form",
            ),
            (hyperchad_transformer::Element::UnorderedList, "ul"),
            (hyperchad_transformer::Element::OrderedList, "ol"),
            (hyperchad_transformer::Element::ListItem, "li"),
            (hyperchad_transformer::Element::Summary, "summary"),
        ] {
            let container = Container {
                element,
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
            let html = std::str::from_utf8(&buffer).unwrap();

            assert!(
                html.starts_with(&format!("<{expected_tag}")),
                "Expected to start with <{expected_tag}, got: {html}"
            );
            assert!(
                html.ends_with(&format!("</{expected_tag}>")),
                "Expected to end with </{expected_tag}>, got: {html}"
            );
        }
    }

    // Tests for anchor with all target types
    #[test_log::test]
    fn test_element_to_html_anchor_all_target_types() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LinkTarget;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        for (target, expected) in [
            (LinkTarget::SelfTarget, "_self"),
            (LinkTarget::Blank, "_blank"),
            (LinkTarget::Parent, "_parent"),
            (LinkTarget::Top, "_top"),
        ] {
            let container = Container {
                element: hyperchad_transformer::Element::Anchor {
                    href: Some("/page".to_string()),
                    target: Some(target),
                },
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
            let html = std::str::from_utf8(&buffer).unwrap();

            assert!(
                html.contains(&format!("target=\"{expected}\"")),
                "Expected target=\"{expected}\", got: {html}"
            );
        }
    }

    // Test image with eager loading
    #[test_log::test]
    fn test_element_to_html_image_with_eager_loading() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::models::ImageLoading;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Image {
                source: Some("/priority-img.jpg".to_string()),
                alt: Some("Important image".to_string()),
                fit: None,
                source_set: None,
                sizes: None,
                loading: Some(ImageLoading::Eager),
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("loading=\"eager\""));
    }

    // Test image fit modes
    #[test_log::test]
    fn test_element_style_to_html_image_fit_modes() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::ImageFit;

        for (fit, expected_css) in [
            (ImageFit::Default, "object-fit:unset"),
            (ImageFit::Contain, "object-fit:contain"),
            (ImageFit::Cover, "object-fit:cover"),
            (ImageFit::Fill, "object-fit:fill"),
            (ImageFit::None, "object-fit:none"),
        ] {
            let container = Container {
                element: hyperchad_transformer::Element::Image {
                    source: Some("/img.jpg".to_string()),
                    alt: None,
                    fit: Some(fit),
                    source_set: None,
                    sizes: None,
                    loading: None,
                },
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}' for ImageFit::{fit:?}, got: {style}"
            );
        }
    }

    // Test element_style_to_html with user_select
    #[test_log::test]
    fn test_element_style_to_html_user_select() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::UserSelect;

        for (user_select, expected_css) in [
            (UserSelect::Auto, "user-select:auto"),
            (UserSelect::None, "user-select:none"),
            (UserSelect::Text, "user-select:text"),
            (UserSelect::All, "user-select:all"),
        ] {
            let container = Container {
                user_select: Some(user_select),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}', got: {style}"
            );
        }
    }

    // Test element_style_to_html with overflow_wrap
    #[test_log::test]
    fn test_element_style_to_html_overflow_wrap() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::OverflowWrap;

        for (overflow_wrap, expected_css) in [
            (OverflowWrap::Normal, "overflow-wrap:normal"),
            (OverflowWrap::BreakWord, "overflow-wrap:break-word"),
            (OverflowWrap::Anywhere, "overflow-wrap:anywhere"),
        ] {
            let container = Container {
                overflow_wrap: Some(overflow_wrap),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}', got: {style}"
            );
        }
    }

    // Test element_style_to_html with text_overflow
    #[test_log::test]
    fn test_element_style_to_html_text_overflow() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::TextOverflow;

        for (text_overflow, expected_css) in [
            (TextOverflow::Clip, "text-overflow:clip"),
            (TextOverflow::Ellipsis, "text-overflow:ellipsis"),
        ] {
            let container = Container {
                text_overflow: Some(text_overflow),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}', got: {style}"
            );
        }
    }

    // Test element_style_to_html with font_family
    #[test_log::test]
    fn test_element_style_to_html_font_family() {
        use hyperchad_router::Container;

        let container = Container {
            font_family: Some(vec![
                "Arial".to_string(),
                "Helvetica".to_string(),
                "sans-serif".to_string(),
            ]),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("font-family:Arial,Helvetica,sans-serif"));
    }

    // Test element_style_to_html with font_weight
    #[test_log::test]
    fn test_element_style_to_html_font_weight() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::FontWeight;

        for (weight, expected) in [
            (FontWeight::Thin, "font-weight:thin"),
            (FontWeight::Normal, "font-weight:normal"),
            (FontWeight::Bold, "font-weight:bold"),
            (FontWeight::Black, "font-weight:black"),
            (FontWeight::Lighter, "font-weight:lighter"),
            (FontWeight::Bolder, "font-weight:bolder"),
            (FontWeight::Weight100, "font-weight:100"),
            (FontWeight::Weight700, "font-weight:700"),
        ] {
            let container = Container {
                font_weight: Some(weight),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected),
                "Expected '{expected}', got: {style}"
            );
        }
    }

    // Test element_style_to_html with all text_align values
    #[test_log::test]
    fn test_element_style_to_html_text_align_all() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::TextAlign;

        for (text_align, expected_css) in [
            (TextAlign::Start, "text-align:start"),
            (TextAlign::Center, "text-align:center"),
            (TextAlign::End, "text-align:end"),
            (TextAlign::Justify, "text-align:justify"),
        ] {
            let container = Container {
                text_align: Some(text_align),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}', got: {style}"
            );
        }
    }

    // Test element_style_to_html with all white_space values
    #[test_log::test]
    fn test_element_style_to_html_white_space_all() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::WhiteSpace;

        for (white_space, expected_css) in [
            (WhiteSpace::Normal, "white-space:normal"),
            (WhiteSpace::Preserve, "white-space:pre"),
            (WhiteSpace::PreserveWrap, "white-space:pre-wrap"),
        ] {
            let container = Container {
                white_space: Some(white_space),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}', got: {style}"
            );
        }
    }

    // Test element_style_to_html with scroll overflow
    #[test_log::test]
    fn test_element_style_to_html_overflow_scroll() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LayoutOverflow;

        let container = Container {
            overflow_x: LayoutOverflow::Scroll,
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("overflow-x:scroll"));
        assert!(style.contains("overflow-y:scroll"));
    }

    // Test element_style_to_html with flex wrap (non-grid)
    #[test_log::test]
    fn test_element_style_to_html_flex_wrap() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LayoutOverflow;

        let container = Container {
            overflow_x: LayoutOverflow::Wrap { grid: false },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("flex-wrap:wrap"));
    }

    // Test element_style_to_html with justify_content all values
    #[test_log::test]
    fn test_element_style_to_html_justify_content_all() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::JustifyContent;

        for (justify_content, expected_css) in [
            (JustifyContent::Start, "justify-content:start"),
            (JustifyContent::Center, "justify-content:center"),
            (JustifyContent::End, "justify-content:end"),
            (
                JustifyContent::SpaceBetween,
                "justify-content:space-between",
            ),
            (JustifyContent::SpaceEvenly, "justify-content:space-evenly"),
        ] {
            let container = Container {
                justify_content: Some(justify_content),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}', got: {style}"
            );
        }
    }

    // Test element_style_to_html with align_items all values
    #[test_log::test]
    fn test_element_style_to_html_align_items_all() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::AlignItems;

        for (align_items, expected_css) in [
            (AlignItems::Start, "align-items:start"),
            (AlignItems::Center, "align-items:center"),
            (AlignItems::End, "align-items:end"),
        ] {
            let container = Container {
                align_items: Some(align_items),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(expected_css),
                "Expected '{expected_css}', got: {style}"
            );
        }
    }

    // Test container_element_to_html_response
    #[test_log::test]
    fn test_container_element_to_html_response() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_renderer::Color;
        use hyperchad_router::Container;
        use std::collections::BTreeMap;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let headers = BTreeMap::new();

        let container = Container {
            element: hyperchad_transformer::Element::Div,
            children: vec![Container {
                element: hyperchad_transformer::Element::Raw {
                    value: "Hello World".to_string(),
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = container_element_to_html_response(
            &headers,
            &container,
            Some("width=device-width, initial-scale=1"),
            Some(Color {
                r: 240,
                g: 240,
                b: 240,
                a: None,
            }),
            Some("Test Page"),
            Some("A test page description"),
            &tag_renderer,
            &["https://cdn.example.com/style.css".to_string()],
            &["/static/main.css".to_string()],
            &["body { font-size: 16px; }".to_string()],
        )
        .unwrap();

        assert!(result.contains("<!DOCTYPE html>"));
        assert!(result.contains("<title>Test Page</title>"));
        assert!(result.contains("A test page description"));
        assert!(result.contains("Hello World"));
        assert!(result.contains("background:rgb(240,240,240)"));
        assert!(result.contains("https://cdn.example.com/style.css"));
        assert!(result.contains("/static/main.css"));
        assert!(result.contains("body { font-size: 16px; }"));
        assert!(result.contains("width=device-width, initial-scale=1"));
    }

    // Test text_decoration with single line style
    #[test_log::test]
    fn test_element_style_to_html_text_decoration_single_line() {
        use hyperchad_router::Container;
        use hyperchad_transformer::TextDecoration;
        use hyperchad_transformer::models::TextDecorationLine;

        let container = Container {
            text_decoration: Some(TextDecoration {
                color: None,
                line: vec![TextDecorationLine::LineThrough],
                style: None,
                thickness: None,
            }),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("text-decoration-line:line-through"));
    }

    // Test text_decoration with all line styles
    #[test_log::test]
    fn test_element_style_to_html_text_decoration_line_inherit_none() {
        use hyperchad_router::Container;
        use hyperchad_transformer::TextDecoration;
        use hyperchad_transformer::models::TextDecorationLine;

        for (line, expected) in [
            (TextDecorationLine::Inherit, "inherit"),
            (TextDecorationLine::None, "none"),
        ] {
            let container = Container {
                text_decoration: Some(TextDecoration {
                    color: None,
                    line: vec![line],
                    style: None,
                    thickness: None,
                }),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(&format!("text-decoration-line:{expected}")),
                "Expected 'text-decoration-line:{expected}', got: {style}"
            );
        }
    }

    // Test text_decoration_style all values
    #[test_log::test]
    fn test_element_style_to_html_text_decoration_style_all() {
        use hyperchad_router::Container;
        use hyperchad_transformer::TextDecoration;
        use hyperchad_transformer::models::{TextDecorationLine, TextDecorationStyle};

        for (style, expected) in [
            (TextDecorationStyle::Inherit, "inherit"),
            (TextDecorationStyle::Solid, "solid"),
            (TextDecorationStyle::Double, "double"),
            (TextDecorationStyle::Dotted, "dotted"),
            (TextDecorationStyle::Dashed, "dashed"),
        ] {
            let container = Container {
                text_decoration: Some(TextDecoration {
                    color: None,
                    line: vec![TextDecorationLine::Underline],
                    style: Some(style),
                    thickness: None,
                }),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style_str = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style_str.contains(&format!("text-decoration-style:{expected}")),
                "Expected 'text-decoration-style:{expected}', got: {style_str}"
            );
        }
    }

    // Test all cursor types
    #[test_log::test]
    fn test_element_style_to_html_cursor_all_types() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Cursor;

        for (cursor, expected_css) in [
            (Cursor::Auto, "auto"),
            (Cursor::Crosshair, "crosshair"),
            (Cursor::NoDrop, "no-drop"),
            (Cursor::Grabbing, "grabbing"),
            (Cursor::AllScroll, "all-scroll"),
            (Cursor::ColResize, "col-resize"),
            (Cursor::RowResize, "row-resize"),
            (Cursor::NResize, "n-resize"),
            (Cursor::EResize, "e-resize"),
            (Cursor::SResize, "s-resize"),
            (Cursor::WResize, "w-resize"),
            (Cursor::NeResize, "ne-resize"),
            (Cursor::NwResize, "nw-resize"),
            (Cursor::SeResize, "se-resize"),
            (Cursor::SwResize, "sw-resize"),
            (Cursor::EwResize, "ew-resize"),
            (Cursor::NsResize, "ns-resize"),
            (Cursor::NeswResize, "nesw-resize"),
            (Cursor::ZoomIn, "zoom-in"),
            (Cursor::ZoomOut, "zoom-out"),
        ] {
            let container = Container {
                cursor: Some(cursor),
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_style_to_html(&mut buffer, &container, false).unwrap();
            let style = std::str::from_utf8(&buffer).unwrap();

            assert!(
                style.contains(&format!("cursor:{expected_css}")),
                "Expected 'cursor:{expected_css}', got: {style}"
            );
        }
    }

    // Test borders for all sides
    #[test_log::test]
    fn test_element_style_to_html_borders_all_sides() {
        use hyperchad_router::Container;

        let color = hyperchad_renderer::Color {
            r: 128,
            g: 128,
            b: 128,
            a: None,
        };

        let container = Container {
            border_right: Some((color, Number::Integer(1))),
            border_bottom: Some((color, Number::Integer(2))),
            border_left: Some((color, Number::Integer(3))),
            border_top_right_radius: Some(Number::Integer(10)),
            border_bottom_left_radius: Some(Number::Integer(15)),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("border-right:1px solid rgb(128,128,128)"));
        assert!(style.contains("border-bottom:2px solid rgb(128,128,128)"));
        assert!(style.contains("border-left:3px solid rgb(128,128,128)"));
        assert!(style.contains("border-top-right-radius:10px"));
        assert!(style.contains("border-bottom-left-radius:15px"));
    }

    // Test canvas element
    #[test_log::test]
    fn test_element_to_html_canvas() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Canvas,
            str_id: Some("my-canvas".to_string()),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<canvas"));
        assert!(html.contains("id=\"my-canvas\""));
        assert!(html.ends_with("</canvas>"));
    }

    // Test table elements (THead, TBody, TR)
    #[test_log::test]
    fn test_element_to_html_table_elements() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        for (element, expected_tag) in [
            (hyperchad_transformer::Element::Table, "table"),
            (hyperchad_transformer::Element::THead, "thead"),
            (hyperchad_transformer::Element::TBody, "tbody"),
            (hyperchad_transformer::Element::TR, "tr"),
        ] {
            let container = Container {
                element,
                ..Default::default()
            };

            let mut buffer = Vec::new();
            element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
            let html = std::str::from_utf8(&buffer).unwrap();

            assert!(
                html.starts_with(&format!("<{expected_tag}")),
                "Expected <{expected_tag}, got: {html}"
            );
            assert!(
                html.ends_with(&format!("</{expected_tag}>")),
                "Expected </{expected_tag}>, got: {html}"
            );
        }
    }

    // Test flex container detection with row direction
    #[test_log::test]
    fn test_element_style_to_html_flex_row_direction() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LayoutDirection;

        let container = Container {
            direction: LayoutDirection::Row,
            justify_content: Some(hyperchad_transformer::models::JustifyContent::Start),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("display:flex"));
        // Row is default, so flex-direction should not be specified
        assert!(!style.contains("flex-direction"));
    }

    // Test position fixed default top/left
    #[test_log::test]
    fn test_element_style_to_html_position_fixed_defaults() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Position;

        let container = Container {
            position: Some(Position::Fixed),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("position:fixed"));
        assert!(style.contains("top:0"));
        assert!(style.contains("left:0"));
    }

    // Test position fixed with bottom/right (should not add top/left defaults)
    #[test_log::test]
    fn test_element_style_to_html_position_fixed_with_bottom_right() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::Position;

        let container = Container {
            position: Some(Position::Fixed),
            bottom: Some(Number::Integer(20)),
            right: Some(Number::Integer(20)),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        assert!(style.contains("position:fixed"));
        assert!(style.contains("bottom:20px"));
        assert!(style.contains("right:20px"));
        // Should NOT add default top/left since bottom/right are specified
        assert!(!style.contains("top:0"));
        assert!(!style.contains("left:0"));
    }

    // Test input checkbox unchecked
    #[test_log::test]
    fn test_element_to_html_input_checkbox_unchecked() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Checkbox {
                    checked: Some(false),
                },
                name: Some("mybox".to_string()),
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"checkbox\""));
        // checked="checked" attribute should NOT be present
        assert!(!html.contains("checked=\"checked\""));
    }

    // Test details element without open attribute
    #[test_log::test]
    fn test_element_to_html_details_none_open() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Details { open: None },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<details"));
        assert!(!html.contains(" open"));
        assert!(html.ends_with("</details>"));
    }

    // Test for Select element with options and selected value matching
    #[test_log::test]
    fn test_element_to_html_select_with_options_and_selected() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        // Create a select with options where selected value matches one option
        let container = Container {
            element: hyperchad_transformer::Element::Select {
                name: Some("color".to_string()),
                selected: Some("blue".to_string()),
                multiple: None,
                disabled: None,
                autofocus: None,
            },
            children: vec![
                Container {
                    element: hyperchad_transformer::Element::Option {
                        value: Some("red".to_string()),
                        disabled: None,
                    },
                    children: vec![Container {
                        element: hyperchad_transformer::Element::Raw {
                            value: "Red".to_string(),
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                Container {
                    element: hyperchad_transformer::Element::Option {
                        value: Some("blue".to_string()),
                        disabled: None,
                    },
                    children: vec![Container {
                        element: hyperchad_transformer::Element::Raw {
                            value: "Blue".to_string(),
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                Container {
                    element: hyperchad_transformer::Element::Option {
                        value: Some("green".to_string()),
                        disabled: None,
                    },
                    children: vec![Container {
                        element: hyperchad_transformer::Element::Raw {
                            value: "Green".to_string(),
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        // Verify select element structure
        assert!(html.starts_with("<select"));
        assert!(html.contains("name=\"color\""));
        assert!(html.contains("data-selected=\"blue\""));

        // Verify the blue option is marked as selected
        assert!(html.contains("<option value=\"blue\" selected>Blue</option>"));

        // Verify other options are NOT marked as selected
        assert!(html.contains("<option value=\"red\">Red</option>"));
        assert!(html.contains("<option value=\"green\">Green</option>"));
        assert!(html.ends_with("</select>"));
    }

    // Test for Select element with unmatched selected value
    #[test_log::test]
    fn test_element_to_html_select_with_unmatched_selected_value() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        // Selected value doesn't match any option
        let container = Container {
            element: hyperchad_transformer::Element::Select {
                name: Some("size".to_string()),
                selected: Some("xlarge".to_string()), // No option has this value
                multiple: None,
                disabled: None,
                autofocus: None,
            },
            children: vec![
                Container {
                    element: hyperchad_transformer::Element::Option {
                        value: Some("small".to_string()),
                        disabled: None,
                    },
                    ..Default::default()
                },
                Container {
                    element: hyperchad_transformer::Element::Option {
                        value: Some("medium".to_string()),
                        disabled: None,
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        // data-selected should still be present
        assert!(html.contains("data-selected=\"xlarge\""));

        // No option should have selected attribute since none match
        assert!(!html.contains(" selected"));
    }

    // Test for Select element with all attributes
    #[test_log::test]
    fn test_element_to_html_select_with_all_attributes() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        let container = Container {
            element: hyperchad_transformer::Element::Select {
                name: Some("multi-select".to_string()),
                selected: Some("opt1".to_string()),
                multiple: Some(true),
                disabled: Some(true),
                autofocus: Some(true),
            },
            children: vec![Container {
                element: hyperchad_transformer::Element::Option {
                    value: Some("opt1".to_string()),
                    disabled: Some(true),
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("name=\"multi-select\""));
        assert!(html.contains(" multiple"));
        assert!(html.contains(" disabled"));
        assert!(html.contains(" autofocus"));
        // Option should also have disabled
        assert!(html.contains("<option value=\"opt1\" selected disabled>"));
    }

    // Test for Form element with action and method
    #[test_log::test]
    fn test_element_to_html_form_with_action_and_method() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        let container = Container {
            element: hyperchad_transformer::Element::Form {
                action: Some("/submit".to_string()),
                method: Some("POST".to_string()),
            },
            children: vec![Container {
                element: hyperchad_transformer::Element::Input {
                    input: hyperchad_transformer::Input::Text {
                        value: None,
                        placeholder: None,
                    },
                    name: Some("field".to_string()),
                    autofocus: None,
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<form"));
        assert!(html.contains("action=\"/submit\""));
        assert!(html.contains("method=\"POST\""));
        assert!(html.contains("<input"));
        assert!(html.ends_with("</form>"));
    }

    // Test for Form element without action/method (both optional)
    #[test_log::test]
    fn test_element_to_html_form_without_action_method() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        let container = Container {
            element: hyperchad_transformer::Element::Form {
                action: None,
                method: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<form"));
        assert!(!html.contains("action="));
        assert!(!html.contains("method="));
        assert!(html.ends_with("</form>"));
    }

    // Test for checkbox input with None checked (indeterminate state)
    #[test_log::test]
    fn test_element_to_html_input_checkbox_none_checked() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Checkbox { checked: None },
                name: Some("optional".to_string()),
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"checkbox\""));
        // checked attribute should NOT be present when checked is None
        assert!(!html.contains("checked"));
        assert!(html.contains("name=\"optional\""));
    }

    // Test for grid layout without explicit grid_cell_size
    #[test_log::test]
    fn test_element_style_to_html_grid_without_cell_size() {
        use hyperchad_router::Container;
        use hyperchad_transformer::models::LayoutOverflow;

        let container = Container {
            overflow_x: LayoutOverflow::Wrap { grid: true },
            grid_cell_size: None, // No cell size specified
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_style_to_html(&mut buffer, &container, false).unwrap();
        let style = std::str::from_utf8(&buffer).unwrap();

        // Should have display:grid but NOT grid-template-columns
        assert!(style.contains("display:grid"));
        assert!(
            !style.contains("grid-template-columns"),
            "grid-template-columns should not be set without cell size"
        );
    }

    // Test for deeply nested calculations with grouping and mixed units
    #[test_log::test]
    fn test_calc_to_css_string_nested_with_grouping() {
        // calc(100vh - (50px + 10%))
        let calc = Calculation::Subtract(
            Box::new(Calculation::Number(Box::new(Number::IntegerVh(100)))),
            Box::new(Calculation::Grouping(Box::new(Calculation::Add(
                Box::new(Calculation::Number(Box::new(Number::Integer(50)))),
                Box::new(Calculation::Number(Box::new(Number::IntegerPercent(10)))),
            )))),
        );

        let result = calc_to_css_string(&calc, true);
        assert_eq!(result, "100vh - (50px + 10%)");
    }

    // Test for multiply with grouping to ensure correct precedence
    #[test_log::test]
    fn test_calc_to_css_string_multiply_with_grouping() {
        // (100% - 20px) * 0.5 - ensures grouping is preserved before multiplication
        let calc = Calculation::Multiply(
            Box::new(Calculation::Grouping(Box::new(Calculation::Subtract(
                Box::new(Calculation::Number(Box::new(Number::IntegerPercent(100)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(20)))),
            )))),
            Box::new(Calculation::Number(Box::new(Number::Real(0.5)))),
        );

        let result = calc_to_css_string(&calc, true);
        assert_eq!(result, "(100% - 20px) * 0.5");
    }

    // Test for min/max with calculations inside
    #[test_log::test]
    fn test_calc_to_css_string_min_with_calculation() {
        // min(100%, 500px - 20px)
        let calc = Calculation::Min(
            Box::new(Calculation::Number(Box::new(Number::IntegerPercent(100)))),
            Box::new(Calculation::Subtract(
                Box::new(Calculation::Number(Box::new(Number::Integer(500)))),
                Box::new(Calculation::Number(Box::new(Number::Integer(20)))),
            )),
        );

        let result = calc_to_css_string(&calc, true);
        assert_eq!(result, "min(100%, 500px - 20px)");
    }

    // Test for Option element rendered directly (outside of Select)
    #[test_log::test]
    fn test_element_to_html_option_standalone() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        let container = Container {
            element: hyperchad_transformer::Element::Option {
                value: Some("standalone".to_string()),
                disabled: Some(true),
            },
            children: vec![Container {
                element: hyperchad_transformer::Element::Raw {
                    value: "Standalone Option".to_string(),
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<option"));
        assert!(html.contains("value=\"standalone\""));
        assert!(html.contains(" disabled"));
        assert!(html.contains("Standalone Option"));
        assert!(html.ends_with("</option>"));
    }

    // Test for Select with non-option children (mixed content)
    #[test_log::test]
    fn test_element_to_html_select_with_mixed_children() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        // Select with a mix of Option and non-Option children
        let container = Container {
            element: hyperchad_transformer::Element::Select {
                name: Some("mixed".to_string()),
                selected: None,
                multiple: None,
                disabled: None,
                autofocus: None,
            },
            children: vec![
                Container {
                    element: hyperchad_transformer::Element::Option {
                        value: Some("opt1".to_string()),
                        disabled: None,
                    },
                    ..Default::default()
                },
                // A non-option child (like a div) - should be rendered normally
                Container {
                    element: hyperchad_transformer::Element::Div,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        // Should contain the option
        assert!(html.contains("<option value=\"opt1\">"));
        // Should also contain the div (rendered as a non-option child)
        assert!(html.contains("<div"));
        assert!(html.contains("</div>"));
    }

    // Test for TH and TD without rowspan/colspan
    #[test_log::test]
    fn test_element_to_html_table_cells_without_spans() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();

        // TH without spans
        let th_container = Container {
            element: hyperchad_transformer::Element::TH {
                rows: None,
                columns: None,
            },
            children: vec![Container {
                element: hyperchad_transformer::Element::Raw {
                    value: "Header".to_string(),
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &th_container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<th"));
        assert!(!html.contains("rowspan"));
        assert!(!html.contains("colspan"));
        assert!(html.contains("Header"));

        // TD without spans
        let td_container = Container {
            element: hyperchad_transformer::Element::TD {
                rows: None,
                columns: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &td_container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<td"));
        assert!(!html.contains("rowspan"));
        assert!(!html.contains("colspan"));
    }

    // Test for text input without value or placeholder
    #[test_log::test]
    fn test_element_to_html_input_text_minimal() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Text {
                    value: None,
                    placeholder: None,
                },
                name: None,
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"text\""));
        assert!(!html.contains("value="));
        assert!(!html.contains("placeholder="));
        assert!(!html.contains("name="));
        assert!(!html.contains("autofocus"));
    }

    // Test for password input without value or placeholder
    #[test_log::test]
    fn test_element_to_html_input_password_minimal() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Password {
                    value: None,
                    placeholder: None,
                },
                name: None,
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"password\""));
        assert!(!html.contains("value="));
        assert!(!html.contains("placeholder="));
    }

    // Test for hidden input without value
    #[test_log::test]
    fn test_element_to_html_input_hidden_without_value() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;
        use hyperchad_transformer::Input;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Input {
                input: Input::Hidden { value: None },
                name: Some("token".to_string()),
                autofocus: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.contains("type=\"hidden\""));
        assert!(!html.contains("value="));
        assert!(html.contains("name=\"token\""));
    }

    // Test for textarea without optional attributes
    #[test_log::test]
    fn test_element_to_html_textarea_minimal() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Textarea {
                value: String::new(),
                placeholder: None,
                rows: None,
                cols: None,
                name: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<textarea"));
        assert!(!html.contains("name="));
        assert!(!html.contains("placeholder="));
        assert!(!html.contains("rows="));
        assert!(!html.contains("cols="));
        assert!(html.ends_with("</textarea>"));
    }

    // Test anchor without href or target
    #[test_log::test]
    fn test_element_to_html_anchor_minimal() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Anchor {
                href: None,
                target: None,
            },
            children: vec![Container {
                element: hyperchad_transformer::Element::Raw {
                    value: "Click me".to_string(),
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<a"));
        assert!(!html.contains("href="));
        assert!(!html.contains("target="));
        assert!(html.contains("Click me"));
        assert!(html.ends_with("</a>"));
    }

    // Test image without source (edge case)
    #[test_log::test]
    fn test_element_to_html_image_minimal() {
        use crate::DefaultHtmlTagRenderer;
        use hyperchad_router::Container;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let container = Container {
            element: hyperchad_transformer::Element::Image {
                source: None,
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None,
            },
            ..Default::default()
        };

        let mut buffer = Vec::new();
        element_to_html(&mut buffer, &container, &tag_renderer, false).unwrap();
        let html = std::str::from_utf8(&buffer).unwrap();

        assert!(html.starts_with("<img"));
        assert!(!html.contains("src="));
        assert!(!html.contains("alt="));
        assert!(!html.contains("srcset="));
        assert!(!html.contains("sizes="));
        assert!(!html.contains("loading="));
    }
}
