use std::{borrow::Cow, collections::HashMap};

use gigachad_actions::{Action, ActionEffect, ActionTrigger, ActionType};
use gigachad_color::Color;
use gigachad_transformer_models::{
    AlignItems, Cursor, ImageFit, JustifyContent, LayoutDirection, LayoutOverflow, LinkTarget,
    Position, Route, SwapTarget, TextAlign, TextDecorationLine, TextDecorationStyle, Visibility,
};
use serde_json::Value;
pub use tl::ParseError;
use tl::{Children, HTMLTag, Node, NodeHandle, Parser, ParserOptions};

use crate::{
    parse::{parse_number, GetNumberError},
    Flex, Number, TextDecoration,
};

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

fn get_direction(tag: &HTMLTag) -> LayoutDirection {
    match get_tag_attr_value_lower(tag, "sx-dir").as_deref() {
        Some("row") => LayoutDirection::Row,
        Some("col") => LayoutDirection::Column,
        _ => LayoutDirection::default(),
    }
}

fn get_color(tag: &HTMLTag, name: &str) -> Option<Color> {
    get_tag_attr_value_decoded(tag, name)
        .as_deref()
        .map(Color::from_hex)
}

fn get_border(tag: &HTMLTag, name: &str) -> Option<(Color, Number)> {
    get_tag_attr_value_decoded(tag, name)
        .as_deref()
        .and_then(|x| {
            crate::parse::split_on_char_trimmed(x, ',', 0)
                .ok()
                .flatten()
        })
        .map(|(size, color)| (size.trim(), color.trim()))
        .and_then(|(size, color)| parse_number(size).ok().map(|size| (size, color.trim())))
        .and_then(|(size, color)| Color::try_from_hex(color).ok().map(|color| (size, color)))
        .map(|(size, color)| (color, size))
}

fn get_classes(tag: &HTMLTag) -> Vec<String> {
    get_tag_attr_value_decoded(tag, "class").map_or_else(Vec::new, |x| {
        x.split_whitespace()
            .filter(|x| !x.is_empty())
            .map(ToString::to_string)
            .collect()
    })
}

fn get_link_target(tag: &HTMLTag, name: &str) -> Option<LinkTarget> {
    get_tag_attr_value_decoded(tag, name)
        .as_deref()
        .map(|x| match x {
            "_self" => LinkTarget::SelfTarget,
            "_blank" => LinkTarget::Blank,
            "_parent" => LinkTarget::Parent,
            "_top" => LinkTarget::Top,
            target => LinkTarget::Custom(target.to_string()),
        })
}

fn get_state(tag: &HTMLTag, name: &str) -> Option<Value> {
    get_tag_attr_value_decoded(tag, name)
        .as_deref()
        .and_then(|x| serde_json::from_str(x).ok())
}

fn get_bool(tag: &HTMLTag, name: &str) -> Option<bool> {
    match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("true" | "") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

fn parse_visibility(value: &str) -> Visibility {
    match value {
        "visible" => Visibility::Visible,
        "hidden" => Visibility::Hidden,
        _ => Visibility::default(),
    }
}

fn get_visibility(tag: &HTMLTag) -> Option<Visibility> {
    get_tag_attr_value_lower(tag, "sx-visibility")
        .as_deref()
        .map(parse_visibility)
}

fn parse_swap(value: &str) -> SwapTarget {
    match value {
        "this" | "self" => SwapTarget::This,
        "children" => SwapTarget::Children,
        _ => SwapTarget::default(),
    }
}

fn get_swap(tag: &HTMLTag) -> Option<SwapTarget> {
    get_tag_attr_value_lower(tag, "hx-swap")
        .as_deref()
        .map(parse_swap)
}

fn get_route(tag: &HTMLTag) -> Option<Route> {
    #[allow(clippy::option_if_let_else, clippy::manual_map)]
    if let Some(get) = get_tag_attr_value_owned(tag, "hx-get") {
        Some(Route::Get {
            route: get,
            trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
            swap: get_swap(tag).unwrap_or_default(),
        })
    } else if let Some(post) = get_tag_attr_value_owned(tag, "hx-post") {
        Some(Route::Post {
            route: post,
            trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
            swap: get_swap(tag).unwrap_or_default(),
        })
    } else {
        None
    }
}

fn get_overflow(tag: &HTMLTag, name: &str) -> LayoutOverflow {
    match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("wrap") => LayoutOverflow::Wrap,
        Some("scroll") => LayoutOverflow::Scroll,
        Some("expand") => LayoutOverflow::Expand,
        Some("squash") => LayoutOverflow::Squash,
        Some("auto") => LayoutOverflow::Auto,
        _ => LayoutOverflow::default(),
    }
}

fn get_justify_content(tag: &HTMLTag, name: &str) -> Option<JustifyContent> {
    Some(match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("start") => JustifyContent::Start,
        Some("center") => JustifyContent::Center,
        Some("end") => JustifyContent::End,
        Some("space-between") => JustifyContent::SpaceBetween,
        Some("space-evenly") => JustifyContent::SpaceEvenly,
        _ => {
            return None;
        }
    })
}

fn get_align_items(tag: &HTMLTag, name: &str) -> Option<AlignItems> {
    Some(match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("start") => AlignItems::Start,
        Some("center") => AlignItems::Center,
        Some("end") => AlignItems::End,
        _ => {
            return None;
        }
    })
}

fn get_text_align(tag: &HTMLTag, name: &str) -> Option<TextAlign> {
    match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("start") => Some(TextAlign::Start),
        Some("center") => Some(TextAlign::Center),
        Some("end") => Some(TextAlign::End),
        Some("justify") => Some(TextAlign::Justify),
        _ => None,
    }
}

fn get_cursor(tag: &HTMLTag) -> Option<Cursor> {
    get_tag_attr_value_lower(tag, "sx-cursor")
        .as_deref()
        .map(|x| match x {
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
            _ => Cursor::default(),
        })
}

fn get_position(tag: &HTMLTag) -> Option<Position> {
    get_tag_attr_value_lower(tag, "sx-position")
        .as_deref()
        .map(|x| match x {
            "static" => Position::Static,
            "relative" => Position::Relative,
            "absolute" => Position::Absolute,
            "fixed" => Position::Fixed,
            _ => Position::default(),
        })
}

fn get_data_attrs(tag: &HTMLTag) -> HashMap<String, String> {
    tag.attributes()
        .iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .filter_map(|(k, v)| {
            k.strip_prefix("data-").map(|name| {
                (
                    name.to_string(),
                    html_escape::decode_html_entities(&v).to_string(),
                )
            })
        })
        .collect()
}

fn get_number(tag: &HTMLTag, name: &str) -> Result<Option<Number>, GetNumberError> {
    Ok(
        if let Some(number) = get_tag_attr_value_decoded(tag, name) {
            Some(parse_number(&number)?)
        } else {
            None
        },
    )
}

fn parse_text_decoration_line(value: &str) -> Option<TextDecorationLine> {
    Some(match value {
        "inherit" => TextDecorationLine::Inherit,
        "none" => TextDecorationLine::None,
        "underline" => TextDecorationLine::Underline,
        "overline" => TextDecorationLine::Overline,
        "line-through" => TextDecorationLine::LineThrough,
        _ => {
            return None;
        }
    })
}

fn get_text_decoration_line(tag: &HTMLTag, name: &str) -> Option<Vec<TextDecorationLine>> {
    get_tag_attr_value_decoded(tag, name).and_then(|x| {
        x.split_whitespace()
            .map(parse_text_decoration_line)
            .collect::<Option<Vec<_>>>()
    })
}

fn parse_text_decoration_style(value: &str) -> Option<TextDecorationStyle> {
    Some(match value {
        "inherit" => TextDecorationStyle::Inherit,
        "solid" => TextDecorationStyle::Solid,
        "double" => TextDecorationStyle::Double,
        "dotted" => TextDecorationStyle::Dotted,
        "dashed" => TextDecorationStyle::Dashed,
        "wavy" => TextDecorationStyle::Wavy,
        _ => {
            return None;
        }
    })
}

fn get_text_decoration_style(tag: &HTMLTag, name: &str) -> Option<TextDecorationStyle> {
    get_tag_attr_value_decoded(tag, name)
        .as_deref()
        .and_then(parse_text_decoration_style)
}

fn get_text_decoration(
    tag: &HTMLTag,
    name: &str,
) -> Result<Option<TextDecoration>, GetNumberError> {
    Ok(get_tag_attr_value_undecoded(tag, name)
        .as_deref()
        .map(|x| html_escape::decode_html_entities(x))
        .as_deref()
        .map(|x| x.split_whitespace().collect::<Vec<_>>())
        .map(|values| {
            if values.is_empty() {
                return Ok(None);
            }

            let mut text_decoration = TextDecoration::default();
            let mut parsing_line = true;
            let mut parsing_style = true;
            let mut parsing_color = true;

            for value in values {
                if parsing_line {
                    if let Some(line) = parse_text_decoration_line(value) {
                        text_decoration.line.push(line);
                        continue;
                    }

                    parsing_line = false;
                }

                if parsing_style {
                    parsing_style = false;

                    if let Some(style) = parse_text_decoration_style(value) {
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
        })
        .transpose()?
        .flatten())
}

fn get_flex(tag: &HTMLTag, name: &str) -> Result<Option<Flex>, GetNumberError> {
    match get_tag_attr_value_undecoded(tag, name)
        .as_deref()
        .map(|x| html_escape::decode_html_entities(x))
        .as_deref()
        .map(|x| {
            x.split_whitespace()
                .map(parse_number)
                .collect::<Result<Vec<_>, _>>()
        }) {
        Some(Ok(values)) => {
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
        Some(Err(e)) => Err(e),
        None => Ok(None),
    }
}

fn get_image_fit(tag: &HTMLTag, name: &str) -> Option<ImageFit> {
    get_tag_attr_value_lower(tag, name)
        .as_deref()
        .and_then(|x| match x {
            "default" => Some(ImageFit::Default),
            "contain" => Some(ImageFit::Contain),
            "cover" => Some(ImageFit::Cover),
            "fill" => Some(ImageFit::Fill),
            "none" => Some(ImageFit::None),
            _ => None,
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

#[allow(clippy::too_many_lines)]
fn parse_element(tag: &HTMLTag<'_>, node: &Node<'_>, parser: &Parser<'_>) -> crate::Container {
    #[cfg(feature = "id")]
    static CURRENT_ID: std::sync::LazyLock<std::sync::Arc<std::sync::atomic::AtomicUsize>> =
        std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)));

    let border_radius = get_number(tag, "sx-border-radius").unwrap();
    let border_top_left_radius = get_number(tag, "sx-border-top-left-radius")
        .unwrap()
        .or_else(|| border_radius.clone());
    let border_top_right_radius = get_number(tag, "sx-border-top-right-radius")
        .unwrap()
        .or_else(|| border_radius.clone());
    let border_bottom_left_radius = get_number(tag, "sx-border-bottom-left-radius")
        .unwrap()
        .or_else(|| border_radius.clone());
    let border_bottom_right_radius = get_number(tag, "sx-border-bottom-right-radius")
        .unwrap()
        .or_else(|| border_radius.clone());

    let margin = get_number(tag, "sx-margin").unwrap();
    let margin_x = get_number(tag, "sx-margin-x").unwrap();
    let margin_y = get_number(tag, "sx-margin-y").unwrap();
    let margin_left = get_number(tag, "sx-margin-left")
        .unwrap()
        .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_right = get_number(tag, "sx-margin-right")
        .unwrap()
        .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_top = get_number(tag, "sx-margin-top")
        .unwrap()
        .or_else(|| margin_y.clone().or_else(|| margin.clone()));
    let margin_bottom = get_number(tag, "sx-margin-bottom")
        .unwrap()
        .or_else(|| margin_y.clone().or_else(|| margin.clone()));

    let padding = get_number(tag, "sx-padding").unwrap();
    let padding_x = get_number(tag, "sx-padding-x").unwrap();
    let padding_y = get_number(tag, "sx-padding-y").unwrap();
    let padding_left = get_number(tag, "sx-padding-left")
        .unwrap()
        .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_right = get_number(tag, "sx-padding-right")
        .unwrap()
        .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_top = get_number(tag, "sx-padding-top")
        .unwrap()
        .or_else(|| padding_y.clone().or_else(|| padding.clone()));
    let padding_bottom = get_number(tag, "sx-padding-bottom")
        .unwrap()
        .or_else(|| padding_y.clone().or_else(|| padding.clone()));

    let mut text_decoration = get_text_decoration(tag, "sx-text-decoration").unwrap();

    if let Some(color) = get_color(tag, "sx-text-decoration-color") {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.color = Some(color);
        } else {
            text_decoration = Some(TextDecoration {
                color: Some(color),
                ..TextDecoration::default()
            });
        }
    }
    if let Some(line) = get_text_decoration_line(tag, "sx-text-decoration-line") {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.line = line;
        } else {
            text_decoration = Some(TextDecoration {
                line,
                ..TextDecoration::default()
            });
        }
    }
    if let Some(style) = get_text_decoration_style(tag, "sx-text-decoration-style") {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.style = Some(style);
        } else {
            text_decoration = Some(TextDecoration {
                style: Some(style),
                ..TextDecoration::default()
            });
        }
    }
    if let Some(thickness) = get_number(tag, "sx-text-decoration-thickness").unwrap() {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.thickness = Some(thickness);
        } else {
            text_decoration = Some(TextDecoration {
                thickness: Some(thickness),
                ..TextDecoration::default()
            });
        }
    }

    let mut flex = get_flex(tag, "sx-flex").unwrap();

    if let Some(grow) = get_number(tag, "sx-flex-grow").unwrap() {
        if let Some(flex) = &mut flex {
            flex.grow = grow;
        } else {
            flex = Some(Flex {
                grow,
                ..Flex::default()
            });
        }
    }
    if let Some(shrink) = get_number(tag, "sx-flex-shrink").unwrap() {
        if let Some(flex) = &mut flex {
            flex.shrink = shrink;
        } else {
            flex = Some(Flex {
                shrink,
                ..Flex::default()
            });
        }
    }
    if let Some(basis) = get_number(tag, "sx-flex-basis").unwrap() {
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
    crate::Container {
        #[cfg(feature = "id")]
        id: CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        str_id: get_tag_attr_value_owned(tag, "id"),
        classes: get_classes(tag),
        data: get_data_attrs(tag),
        direction: get_direction(tag),
        background: get_color(tag, "sx-background"),
        border_top: get_border(tag, "sx-border-top"),
        border_right: get_border(tag, "sx-border-right"),
        border_bottom: get_border(tag, "sx-border-bottom"),
        border_left: get_border(tag, "sx-border-left"),
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
        font_size: get_number(tag, "sx-font-size").unwrap(),
        color: get_color(tag, "sx-color"),
        state: get_state(tag, "state"),
        hidden: get_bool(tag, "sx-hidden"),
        visibility: get_visibility(tag),
        overflow_x: get_overflow(tag, "sx-overflow-x"),
        overflow_y: get_overflow(tag, "sx-overflow-y"),
        justify_content: get_justify_content(tag, "sx-justify-content"),
        align_items: get_align_items(tag, "sx-align-items"),
        text_align: get_text_align(tag, "sx-text-align"),
        text_decoration,
        font_family: get_tag_attr_value_decoded(tag, "sx-font-family").map(|x| {
            x.split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(ToString::to_string)
                .collect()
        }),
        children: parse_top_children(node.children(), parser),
        width: get_number(tag, "sx-width").unwrap(),
        min_width: get_number(tag, "sx-min-width").unwrap(),
        max_width: get_number(tag, "sx-max-width").unwrap(),
        height: get_number(tag, "sx-height").unwrap(),
        min_height: get_number(tag, "sx-min-height").unwrap(),
        max_height: get_number(tag, "sx-max-height").unwrap(),
        flex,
        left: get_number(tag, "sx-left").unwrap(),
        right: get_number(tag, "sx-right").unwrap(),
        top: get_number(tag, "sx-top").unwrap(),
        bottom: get_number(tag, "sx-bottom").unwrap(),
        translate_x: get_number(tag, "sx-translate-x").unwrap(),
        translate_y: get_number(tag, "sx-translate-y").unwrap(),
        gap: get_number(tag, "sx-gap").unwrap(),
        opacity: get_number(tag, "sx-opacity").unwrap(),
        debug: get_bool(tag, "debug"),
        cursor: get_cursor(tag),
        position: get_position(tag),
        route: get_route(tag),
        actions: get_actions(tag),
        ..Default::default()
    }
}

#[allow(clippy::too_many_lines)]
fn parse_child(node: &Node<'_>, parser: &Parser<'_>) -> Option<crate::Container> {
    match node {
        Node::Tag(tag) => {
            let mut container = parse_element(tag, node, parser);

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
                        fit: get_image_fit(tag, "sx-fit"),
                        source_set: get_tag_attr_value_owned(tag, "srcset"),
                        sizes: get_number(tag, "sizes").unwrap(),
                    }
                }
                "a" => {
                    container.element = crate::Element::Anchor {
                        target: get_link_target(tag, "target"),
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
