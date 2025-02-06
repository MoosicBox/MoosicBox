use std::{borrow::Cow, collections::HashMap};

use gigachad_actions::{Action, ActionEffect, ActionTrigger, ActionType};
use gigachad_color::{Color, ParseHexError};
use gigachad_transformer_models::{
    AlignItems, Cursor, ImageFit, JustifyContent, LayoutDirection, LayoutOverflow, LinkTarget,
    Position, Route, SwapTarget, TextAlign, TextDecorationLine, TextDecorationStyle, Visibility,
};
use serde_json::Value;
use thiserror::Error;
pub use tl::ParseError;
use tl::{Children, HTMLTag, Node, NodeHandle, Parser, ParserOptions};

use crate::{
    parse::{parse_number, GetNumberError},
    Flex, Number, TextDecoration,
};

#[derive(Debug, Error)]
pub enum ParseAttrError {
    #[error("Invalid Value: '{0}'")]
    InvalidValue(String),
    #[error(transparent)]
    GetNumber(#[from] GetNumberError),
    #[error(transparent)]
    ParseHex(#[from] ParseHexError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

impl TryFrom<String> for crate::Container {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl<'a> TryFrom<&'a str> for crate::Container {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let result = tl::parse(value, ParserOptions::new())?;

        Ok(Self {
            children: parse_children(result.children(), result.parser()),
            overflow_x: LayoutOverflow::Squash,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        })
    }
}

fn parse_top_children(
    children: Option<Children<'_, '_>>,
    parser: &Parser<'_>,
) -> Vec<crate::Container> {
    children.map_or_else(Vec::new, |children| {
        parse_children(&children.top().to_vec(), parser)
    })
}

fn parse_children(children: &[NodeHandle], parser: &Parser<'_>) -> Vec<crate::Container> {
    let mut elements = vec![];

    for node in children {
        if let Some(node) = node.get(parser) {
            if let Some(element) = parse_child(node, parser) {
                elements.push(element);
            }
        }
    }

    elements
}

fn get_tag_attr_value_undecoded<'a>(tag: &'a HTMLTag, name: &'a str) -> Option<Cow<'a, str>> {
    tag.attributes()
        .iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .find(|(k, _)| k.to_lowercase().as_str() == name)
        .map(|(_, v)| v)
}

fn get_tag_attr_value_decoded<'a>(tag: &'a HTMLTag, name: &'a str) -> Option<Cow<'a, str>> {
    get_tag_attr_value_undecoded(tag, name).map(|x| match x {
        Cow::Borrowed(x) => html_escape::decode_html_entities(x),
        Cow::Owned(x) => Cow::Owned(html_escape::decode_html_entities(&x).to_string()),
    })
}

fn get_tag_attr_value_owned(tag: &HTMLTag, name: &str) -> Option<String> {
    get_tag_attr_value_decoded(tag, name).map(|x| x.to_string())
}

fn get_tag_attr_value_lower(tag: &HTMLTag, name: &str) -> Option<String> {
    get_tag_attr_value_decoded(tag, name).map(|x| x.to_lowercase())
}

fn parse_direction(value: &str) -> Result<LayoutDirection, ParseAttrError> {
    Ok(match value {
        "row" => LayoutDirection::Row,
        "col" => LayoutDirection::Column,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_border(value: &str) -> Result<(Color, Number), ParseAttrError> {
    let (size, color) = crate::parse::split_on_char_trimmed(value, ',', 0)
        .map_err(|_e| ParseAttrError::InvalidValue(value.to_string()))?
        .ok_or_else(|| ParseAttrError::InvalidValue(value.to_string()))?;
    let (size, color) = (size.trim(), color.trim());
    let size = parse_number(size)?;
    let color = Color::try_from_hex(color)?;

    Ok((color, size))
}

fn parse_classes(value: &str) -> Result<Vec<String>, ParseAttrError> {
    value
        .split_whitespace()
        .filter(|x| !x.is_empty())
        .map(ToString::to_string)
        .map(Ok)
        .collect()
}

fn parse_link_target(value: &str) -> LinkTarget {
    match value {
        "_self" => LinkTarget::SelfTarget,
        "_blank" => LinkTarget::Blank,
        "_parent" => LinkTarget::Parent,
        "_top" => LinkTarget::Top,
        target => LinkTarget::Custom(target.to_string()),
    }
}

fn parse_state(value: &str) -> Result<Value, ParseAttrError> {
    Ok(serde_json::from_str(value)?)
}

fn parse_bool(value: &str) -> Result<bool, ParseAttrError> {
    Ok(match value {
        "true" | "" => true,
        "false" => false,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_visibility(value: &str) -> Result<Visibility, ParseAttrError> {
    Ok(match value {
        "visible" => Visibility::Visible,
        "hidden" => Visibility::Hidden,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_swap(value: &str) -> Result<SwapTarget, ParseAttrError> {
    Ok(match value {
        "this" | "self" => SwapTarget::This,
        "children" => SwapTarget::Children,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn get_route(tag: &HTMLTag) -> Result<Option<Route>, ParseAttrError> {
    get_tag_attr_value_decoded(tag, "hx-get")
        .as_deref()
        .map(|x| parse_get_route(x, tag))
        .or_else(|| {
            get_tag_attr_value_decoded(tag, "hx-post")
                .as_deref()
                .map(|x| parse_post_route(x, tag))
        })
        .transpose()
}

// TODO: Doesn't support reactive values
fn parse_get_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Get {
        route: value.to_string(),
        trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
        swap: get_tag_attr_value_decoded(tag, "hx-swap")
            .as_deref()
            .map(parse_swap)
            .transpose()?
            .unwrap_or_default(),
    })
}

// TODO: Doesn't support reactive values
fn parse_post_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Post {
        route: value.to_string(),
        trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
        swap: get_tag_attr_value_decoded(tag, "hx-swap")
            .as_deref()
            .map(parse_swap)
            .transpose()?
            .unwrap_or_default(),
    })
}

fn parse_overflow(value: &str) -> Result<LayoutOverflow, ParseAttrError> {
    Ok(match value {
        "wrap" => LayoutOverflow::Wrap,
        "scroll" => LayoutOverflow::Scroll,
        "expand" => LayoutOverflow::Expand,
        "squash" => LayoutOverflow::Squash,
        "auto" => LayoutOverflow::Auto,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_justify_content(value: &str) -> Result<JustifyContent, ParseAttrError> {
    Ok(match value {
        "start" => JustifyContent::Start,
        "center" => JustifyContent::Center,
        "end" => JustifyContent::End,
        "space-between" => JustifyContent::SpaceBetween,
        "space-evenly" => JustifyContent::SpaceEvenly,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_align_items(value: &str) -> Result<AlignItems, ParseAttrError> {
    Ok(match value {
        "start" => AlignItems::Start,
        "center" => AlignItems::Center,
        "end" => AlignItems::End,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_text_align(value: &str) -> Result<TextAlign, ParseAttrError> {
    Ok(match value {
        "start" => TextAlign::Start,
        "center" => TextAlign::Center,
        "end" => TextAlign::End,
        "justify" => TextAlign::Justify,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_cursor(value: &str) -> Result<Cursor, ParseAttrError> {
    Ok(match value {
        "auto" => Cursor::Auto,
        "pointer" => Cursor::Pointer,
        "text" => Cursor::Text,
        "crosshair" => Cursor::Crosshair,
        "move" => Cursor::Move,
        "not-allowed" => Cursor::NotAllowed,
        "no-drop" => Cursor::NoDrop,
        "grab" => Cursor::Grab,
        "grabbing" => Cursor::Grabbing,
        "all-scroll" => Cursor::AllScroll,
        "col-resize" => Cursor::ColResize,
        "row-resize" => Cursor::RowResize,
        "n-resize" => Cursor::NResize,
        "e-resize" => Cursor::EResize,
        "s-resize" => Cursor::SResize,
        "w-resize" => Cursor::WResize,
        "ne-resize" => Cursor::NeResize,
        "nw-resize" => Cursor::NwResize,
        "se-resize" => Cursor::SeResize,
        "sw-resize" => Cursor::SwResize,
        "ew-resize" => Cursor::EwResize,
        "ns-resize" => Cursor::NsResize,
        "nesw-resize" => Cursor::NeswResize,
        "zoom-in" => Cursor::ZoomIn,
        "zoom-out" => Cursor::ZoomOut,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_position(value: &str) -> Result<Position, ParseAttrError> {
    Ok(match value {
        "static" => Position::Static,
        "relative" => Position::Relative,
        "absolute" => Position::Absolute,
        "fixed" => Position::Fixed,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn get_data_attrs(tag: &HTMLTag) -> Result<HashMap<String, String>, ParseAttrError> {
    tag.attributes()
        .iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .filter_map(|(k, v)| {
            k.strip_prefix("data-").map(|name| {
                Ok((
                    name.to_string(),
                    pmrv_inner(&html_escape::decode_html_entities(&v), |x| {
                        Ok::<_, ParseAttrError>(x.to_string())
                    })?,
                ))
            })
        })
        .collect::<Result<HashMap<_, _>, _>>()
}

fn parse_text_decoration_lines(value: &str) -> Result<Vec<TextDecorationLine>, ParseAttrError> {
    value
        .split_whitespace()
        .map(parse_text_decoration_line)
        .collect::<Result<Vec<_>, _>>()
}

fn parse_text_decoration_line(value: &str) -> Result<TextDecorationLine, ParseAttrError> {
    Ok(match value {
        "inherit" => TextDecorationLine::Inherit,
        "none" => TextDecorationLine::None,
        "underline" => TextDecorationLine::Underline,
        "overline" => TextDecorationLine::Overline,
        "line-through" => TextDecorationLine::LineThrough,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_text_decoration_style(value: &str) -> Result<TextDecorationStyle, ParseAttrError> {
    Ok(match value {
        "inherit" => TextDecorationStyle::Inherit,
        "solid" => TextDecorationStyle::Solid,
        "double" => TextDecorationStyle::Double,
        "dotted" => TextDecorationStyle::Dotted,
        "dashed" => TextDecorationStyle::Dashed,
        "wavy" => TextDecorationStyle::Wavy,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_text_decoration(value: &str) -> Result<Option<TextDecoration>, GetNumberError> {
    let mut values = value.split_whitespace().peekable();

    if values.peek().is_none() {
        return Ok(None);
    }

    let mut text_decoration = TextDecoration::default();
    let mut parsing_line = true;
    let mut parsing_style = true;
    let mut parsing_color = true;

    for value in values {
        if parsing_line {
            if let Ok(line) = parse_text_decoration_line(value) {
                text_decoration.line.push(line);
                continue;
            }

            parsing_line = false;
        }

        if parsing_style {
            parsing_style = false;

            if let Ok(style) = parse_text_decoration_style(value) {
                text_decoration.style = Some(style);
                continue;
            }
        }

        if parsing_color {
            parsing_color = false;

            if let Ok(color) = Color::try_from_hex(value) {
                text_decoration.color = Some(color);
                continue;
            }
        }

        if text_decoration.thickness.is_some() {
            return Ok(None);
        }

        text_decoration.thickness = Some(parse_number(value)?);
    }

    Ok(Some(text_decoration))
}

fn parse_flex(value: &str) -> Result<Option<Flex>, GetNumberError> {
    match value
        .split_whitespace()
        .map(parse_number)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(values) => {
            let mut iter = values.into_iter();
            match (iter.next(), iter.next(), iter.next(), iter.next()) {
                (Some(grow), None, None, None) => Ok(Some(Flex {
                    grow,
                    ..Flex::default()
                })),
                (Some(grow), Some(shrink), None, None) => Ok(Some(Flex {
                    grow,
                    shrink,
                    ..Flex::default()
                })),
                (Some(grow), Some(shrink), Some(basis), None) => Ok(Some(Flex {
                    grow,
                    shrink,
                    basis,
                })),
                _ => Ok(None),
            }
        }
        Err(e) => Err(e),
    }
}

fn parse_image_fit(value: &str) -> Result<ImageFit, ParseAttrError> {
    Ok(match value {
        "default" => ImageFit::Default,
        "contain" => ImageFit::Contain,
        "cover" => ImageFit::Cover,
        "fill" => ImageFit::Fill,
        "none" => ImageFit::None,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_std_action(action: &str) -> Option<ActionEffect> {
    if let Ok(action) = serde_json::from_str::<ActionEffect>(action) {
        return Some(action);
    };

    if let Ok(action) = serde_json::from_str::<ActionType>(action) {
        return Some(action.into());
    };

    #[cfg(feature = "logic")]
    if let Ok(action) =
        serde_json::from_str::<gigachad_actions::logic::If>(action).map(ActionType::Logic)
    {
        return Some(action.into());
    };

    None
}

fn parse_action(action: String) -> ActionEffect {
    if let Some(action) = parse_std_action(&action) {
        return action;
    }

    ActionType::Custom { action }.into()
}

fn parse_event_action(action: &str) -> (String, ActionEffect) {
    if let Some(ActionEffect {
        action: ActionType::Event { name, action },
        delay_off,
        throttle,
    }) = parse_std_action(action)
    {
        return (
            name.clone(),
            ActionEffect {
                action: ActionType::Event { name, action },
                delay_off,
                throttle,
            },
        );
    }

    let Ok(ActionType::Event { name, action }) = serde_json::from_str::<ActionType>(action) else {
        panic!("Invalid event action: '{action}'");
    };

    (name, action.into())
}

fn get_actions(tag: &HTMLTag) -> Vec<Action> {
    let mut actions = vec![];

    if let Some(action) = get_tag_attr_value_owned(tag, "fx-click") {
        actions.push(Action {
            trigger: ActionTrigger::Click,
            action: parse_action(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-click-outside") {
        actions.push(Action {
            trigger: ActionTrigger::ClickOutside,
            action: parse_action(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-mouse-down") {
        actions.push(Action {
            trigger: ActionTrigger::MouseDown,
            action: parse_action(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-hover") {
        actions.push(Action {
            trigger: ActionTrigger::Hover,
            action: parse_action(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-change") {
        actions.push(Action {
            trigger: ActionTrigger::Change,
            action: parse_action(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-immediate") {
        actions.push(Action {
            trigger: ActionTrigger::Immediate,
            action: parse_action(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-event") {
        let (name, action) = parse_event_action(&action);
        actions.push(Action {
            trigger: ActionTrigger::Event(name),
            action,
        });
    }

    actions
}

/// Parse maybe reactive value
fn pmrv<T, E>(
    tag: &HTMLTag<'_>,
    name: &str,
    func: impl Fn(&str) -> Result<T, E>,
) -> Result<Option<T>, E> {
    get_tag_attr_value_decoded(tag, name)
        .as_deref()
        .map(|x| pmrv_inner(x, func))
        .transpose()
}

fn pmrv_inner<T, E>(value: &str, func: impl Fn(&str) -> Result<T, E>) -> Result<T, E> {
    func(value)
}

#[allow(clippy::too_many_lines)]
fn parse_element(
    tag: &HTMLTag<'_>,
    node: &Node<'_>,
    parser: &Parser<'_>,
) -> Result<crate::Container, ParseAttrError> {
    #[cfg(feature = "id")]
    static CURRENT_ID: std::sync::LazyLock<std::sync::Arc<std::sync::atomic::AtomicUsize>> =
        std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)));

    let border_radius = pmrv(tag, "sx-border-radius", parse_number)?;
    let border_top_left_radius =
        pmrv(tag, "sx-border-top-left-radius", parse_number)?.or_else(|| border_radius.clone());
    let border_top_right_radius =
        pmrv(tag, "sx-border-top-right-radius", parse_number)?.or_else(|| border_radius.clone());
    let border_bottom_left_radius =
        pmrv(tag, "sx-border-bottom-left-radius", parse_number)?.or_else(|| border_radius.clone());
    let border_bottom_right_radius =
        pmrv(tag, "sx-border-bottom-right-radius", parse_number)?.or_else(|| border_radius.clone());

    let margin = pmrv(tag, "sx-margin", parse_number)?;
    let margin_x = pmrv(tag, "sx-margin-x", parse_number)?;
    let margin_y = pmrv(tag, "sx-margin-y", parse_number)?;
    let margin_left = pmrv(tag, "sx-margin-left", parse_number)?
        .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_right = pmrv(tag, "sx-margin-right", parse_number)?
        .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_top = pmrv(tag, "sx-margin-top", parse_number)?
        .or_else(|| margin_y.clone().or_else(|| margin.clone()));
    let margin_bottom = pmrv(tag, "sx-margin-bottom", parse_number)?
        .or_else(|| margin_y.clone().or_else(|| margin.clone()));

    let padding = pmrv(tag, "sx-padding", parse_number)?;
    let padding_x = pmrv(tag, "sx-padding-x", parse_number)?;
    let padding_y = pmrv(tag, "sx-padding-y", parse_number)?;
    let padding_left = pmrv(tag, "sx-padding-left", parse_number)?
        .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_right = pmrv(tag, "sx-padding-right", parse_number)?
        .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_top = pmrv(tag, "sx-padding-top", parse_number)?
        .or_else(|| padding_y.clone().or_else(|| padding.clone()));
    let padding_bottom = pmrv(tag, "sx-padding-bottom", parse_number)?
        .or_else(|| padding_y.clone().or_else(|| padding.clone()));

    let mut text_decoration = pmrv(tag, "sx-text-decoration", parse_text_decoration)?.flatten();

    if let Some(color) = pmrv(tag, "sx-text-decoration-color", Color::try_from_hex)? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.color = Some(color);
        } else {
            text_decoration = Some(TextDecoration {
                color: Some(color),
                ..TextDecoration::default()
            });
        }
    }
    if let Some(line) = pmrv(tag, "sx-text-decoration-line", parse_text_decoration_lines)? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.line = line;
        } else {
            text_decoration = Some(TextDecoration {
                line,
                ..TextDecoration::default()
            });
        }
    }
    if let Some(style) = pmrv(tag, "sx-text-decoration-style", parse_text_decoration_style)? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.style = Some(style);
        } else {
            text_decoration = Some(TextDecoration {
                style: Some(style),
                ..TextDecoration::default()
            });
        }
    }
    if let Some(thickness) = pmrv(tag, "sx-text-decoration-thickness", parse_number)? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.thickness = Some(thickness);
        } else {
            text_decoration = Some(TextDecoration {
                thickness: Some(thickness),
                ..TextDecoration::default()
            });
        }
    }

    let mut flex = pmrv(tag, "sx-flex", parse_flex)?.flatten();

    if let Some(grow) = pmrv(tag, "sx-flex-grow", parse_number)? {
        if let Some(flex) = &mut flex {
            flex.grow = grow;
        } else {
            flex = Some(Flex {
                grow,
                ..Flex::default()
            });
        }
    }
    if let Some(shrink) = pmrv(tag, "sx-flex-shrink", parse_number)? {
        if let Some(flex) = &mut flex {
            flex.shrink = shrink;
        } else {
            flex = Some(Flex {
                shrink,
                ..Flex::default()
            });
        }
    }
    if let Some(basis) = pmrv(tag, "sx-flex-basis", parse_number)? {
        if let Some(flex) = &mut flex {
            flex.basis = basis;
        } else {
            flex = Some(Flex {
                basis,
                ..Flex::default()
            });
        }
    }

    #[allow(clippy::needless_update)]
    Ok(crate::Container {
        #[cfg(feature = "id")]
        id: CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        str_id: get_tag_attr_value_owned(tag, "id"),
        classes: pmrv(tag, "class", parse_classes)?.unwrap_or_else(Vec::new),
        data: get_data_attrs(tag)?,
        direction: pmrv(tag, "sx-dir", parse_direction)?.unwrap_or_default(),
        background: pmrv(tag, "sx-background", Color::try_from_hex)?,
        border_top: pmrv(tag, "sx-border-top", parse_border)?,
        border_right: pmrv(tag, "sx-border-right", parse_border)?,
        border_bottom: pmrv(tag, "sx-border-bottom", parse_border)?,
        border_left: pmrv(tag, "sx-border-left", parse_border)?,
        border_top_left_radius,
        border_top_right_radius,
        border_bottom_left_radius,
        border_bottom_right_radius,
        margin_left,
        margin_right,
        margin_top,
        margin_bottom,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
        font_size: pmrv(tag, "sx-font-size", parse_number)?,
        color: pmrv(tag, "sx-color", Color::try_from_hex)?,
        state: pmrv(tag, "state", parse_state)?,
        hidden: pmrv(tag, "sx-hidden", parse_bool)?,
        visibility: pmrv(tag, "sx-visibility", parse_visibility)?,
        overflow_x: pmrv(tag, "sx-overflow-x", parse_overflow)?.unwrap_or_default(),
        overflow_y: pmrv(tag, "sx-overflow-y", parse_overflow)?.unwrap_or_default(),
        justify_content: pmrv(tag, "sx-justify-content", parse_justify_content)?,
        align_items: pmrv(tag, "sx-align-items", parse_align_items)?,
        text_align: pmrv(tag, "sx-text-align", parse_text_align)?,
        text_decoration,
        font_family: get_tag_attr_value_decoded(tag, "sx-font-family").map(|x| {
            x.split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(ToString::to_string)
                .collect()
        }),
        children: parse_top_children(node.children(), parser),
        width: pmrv(tag, "sx-width", parse_number)?,
        min_width: pmrv(tag, "sx-min-width", parse_number)?,
        max_width: pmrv(tag, "sx-max-width", parse_number)?,
        height: pmrv(tag, "sx-height", parse_number)?,
        min_height: pmrv(tag, "sx-min-height", parse_number)?,
        max_height: pmrv(tag, "sx-max-height", parse_number)?,
        flex,
        left: pmrv(tag, "sx-left", parse_number)?,
        right: pmrv(tag, "sx-right", parse_number)?,
        top: pmrv(tag, "sx-top", parse_number)?,
        bottom: pmrv(tag, "sx-bottom", parse_number)?,
        translate_x: pmrv(tag, "sx-translate-x", parse_number)?,
        translate_y: pmrv(tag, "sx-translate-y", parse_number)?,
        gap: pmrv(tag, "sx-gap", parse_number)?,
        opacity: pmrv(tag, "sx-opacity", parse_number)?,
        debug: pmrv(tag, "debug", parse_bool)?,
        cursor: pmrv(tag, "sx-cursor", parse_cursor)?,
        position: pmrv(tag, "sx-position", parse_position)?,
        route: get_route(tag)?,
        actions: get_actions(tag),
        ..Default::default()
    })
}

#[allow(clippy::too_many_lines)]
fn parse_child(node: &Node<'_>, parser: &Parser<'_>) -> Option<crate::Container> {
    match node {
        Node::Tag(tag) => {
            let mut container = parse_element(tag, node, parser).unwrap();

            match tag.name().as_utf8_str().to_lowercase().as_str() {
                "input" => match get_tag_attr_value_lower(tag, "type").as_deref() {
                    Some("checkbox") => {
                        container.element = crate::Element::Input {
                            input: crate::Input::Checkbox {
                                checked: get_tag_attr_value_lower(tag, "checked")
                                    .as_deref()
                                    .map(|x| matches!(x, "checked" | "true" | "")),
                            },
                        }
                    }
                    Some("text") => {
                        container.element = crate::Element::Input {
                            input: crate::Input::Text {
                                value: get_tag_attr_value_owned(tag, "value"),
                                placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                            },
                        }
                    }
                    Some("password") => {
                        container.element = crate::Element::Input {
                            input: crate::Input::Password {
                                value: get_tag_attr_value_owned(tag, "value"),
                                placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                            },
                        }
                    }
                    Some(_) | None => {
                        return None;
                    }
                },
                "main" => container.element = crate::Element::Main,
                "header" => container.element = crate::Element::Header,
                "footer" => container.element = crate::Element::Footer,
                "aside" => container.element = crate::Element::Aside,
                "div" => container.element = crate::Element::Div,
                "span" => container.element = crate::Element::Span,
                "section" => container.element = crate::Element::Section,
                "form" => container.element = crate::Element::Form,
                "button" => container.element = crate::Element::Button,
                "img" => {
                    container.element = crate::Element::Image {
                        source: get_tag_attr_value_owned(tag, "src"),
                        alt: get_tag_attr_value_owned(tag, "alt"),
                        fit: pmrv(tag, "sx-fit", parse_image_fit).unwrap(),
                        source_set: get_tag_attr_value_owned(tag, "srcset"),
                        sizes: pmrv(tag, "sizes", parse_number).unwrap(),
                    }
                }
                "a" => {
                    container.element = crate::Element::Anchor {
                        target: get_tag_attr_value_decoded(tag, "target")
                            .as_deref()
                            .map(parse_link_target),
                        href: get_tag_attr_value_owned(tag, "href"),
                    }
                }
                "h1" => {
                    container.element = crate::Element::Heading {
                        size: crate::HeaderSize::H1,
                    }
                }
                "h2" => {
                    container.element = crate::Element::Heading {
                        size: crate::HeaderSize::H2,
                    }
                }
                "h3" => {
                    container.element = crate::Element::Heading {
                        size: crate::HeaderSize::H3,
                    }
                }
                "h4" => {
                    container.element = crate::Element::Heading {
                        size: crate::HeaderSize::H4,
                    }
                }
                "h5" => {
                    container.element = crate::Element::Heading {
                        size: crate::HeaderSize::H5,
                    }
                }
                "h6" => {
                    container.element = crate::Element::Heading {
                        size: crate::HeaderSize::H6,
                    }
                }
                "ul" => container.element = crate::Element::UnorderedList,
                "ol" => container.element = crate::Element::OrderedList,
                "li" => container.element = crate::Element::ListItem,
                "table" => container.element = crate::Element::Table,
                "thead" => container.element = crate::Element::THead,
                "th" => container.element = crate::Element::TH,
                "tbody" => container.element = crate::Element::TBody,
                "tr" => {
                    container.element = crate::Element::TR;
                    if get_tag_attr_value_undecoded(tag, "sx-dir").is_none() {
                        container.direction = LayoutDirection::Row;
                    }
                }
                "td" => container.element = crate::Element::TD,
                #[cfg(feature = "canvas")]
                "canvas" => container.element = crate::Element::Canvas,
                _ => {
                    return None;
                }
            }

            Some(container)
        }
        Node::Raw(x) => {
            let value = x.as_utf8_str();
            Some(crate::Container {
                element: crate::Element::Raw {
                    value: value.to_string(),
                },
                ..crate::Container::default()
            })
        }
        Node::Comment(_x) => None,
    }
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
mod test {
    use pretty_assertions::assert_eq;
    use quickcheck_macros::quickcheck;

    use crate::Container;

    fn clean_up_container(container: &mut Container) {
        #[cfg(feature = "id")]
        {
            container.id = 0;
        }

        let mut i = 0;
        let actions = container.actions.clone();
        container.actions.retain(|x| {
            i += 1;
            actions
                .iter()
                .take(i - 1)
                .all(|prev| prev.trigger.trigger_type() != x.trigger.trigger_type())
        });
        container
            .actions
            .sort_by(|a, b| format!("{:?}", a.trigger).cmp(&format!("{:?}", b.trigger)));

        for child in &mut container.children {
            clean_up_container(child);
        }
    }

    #[quickcheck]
    fn display_can_display_and_be_parsed_back_to_original_container(
        mut container: Container,
    ) -> bool {
        clean_up_container(&mut container);

        let markup = container
            .display_to_string(
                true,
                #[cfg(feature = "format")]
                false,
                #[cfg(feature = "syntax-highlighting")]
                false,
            )
            .unwrap();

        let re_parsed: Container = markup.clone().try_into().unwrap();

        let Some(mut re_parsed) = re_parsed.children.first().cloned() else {
            log::trace!("re_parsed: {re_parsed} ({re_parsed:?})");
            panic!("failed to get child from markup: {markup} ({container:?})");
        };

        clean_up_container(&mut re_parsed);

        if re_parsed != container {
            log::trace!("container:\n{container:?}");
            log::trace!("before:\n{container}");
            log::trace!("after:\n{re_parsed}");

            std::thread::sleep(std::time::Duration::from_millis(10));
            assert_eq!(
                re_parsed
                    .display_to_string(
                        true,
                        #[cfg(feature = "format")]
                        true,
                        #[cfg(feature = "syntax-highlighting")]
                        false
                    )
                    .unwrap(),
                container
                    .display_to_string(
                        true,
                        #[cfg(feature = "format")]
                        true,
                        #[cfg(feature = "syntax-highlighting")]
                        false
                    )
                    .unwrap()
            );
            assert_eq!(re_parsed, container);
        }

        true
    }
}
