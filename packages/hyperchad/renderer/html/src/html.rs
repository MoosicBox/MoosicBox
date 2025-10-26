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
        | Element::Aside
        | Element::Main
        | Element::Header
        | Element::Footer
        | Element::Section
        | Element::Form
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
        | Element::Summary => {}
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

/// # Errors
///
/// * If there were any IO errors writing the `Container` as an HTML response
#[allow(clippy::similar_names, clippy::implicit_hasher)]
pub fn container_element_to_html_response(
    headers: &BTreeMap<String, String>,
    container: &Container,
    viewport: Option<&str>,
    background: Option<Color>,
    title: Option<&str>,
    description: Option<&str>,
    tag_renderer: &dyn HtmlTagRenderer,
) -> Result<String, std::io::Error> {
    Ok(tag_renderer.root_html(
        headers,
        container,
        container_element_to_html(container, tag_renderer)?,
        viewport,
        background,
        title,
        description,
    ))
}
