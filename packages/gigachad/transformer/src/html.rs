use std::borrow::Cow;

use gigachad_actions::{Action, ActionTrigger, ActionType};
use gigachad_color::Color;
use gigachad_transformer_models::{
    AlignItems, Cursor, JustifyContent, LayoutDirection, LayoutOverflow, Position, Route,
    SwapTarget, Visibility,
};
use serde_json::Value;
pub use tl::ParseError;
use tl::{Children, HTMLTag, Node, NodeHandle, Parser, ParserOptions};

use crate::{
    parse::{parse_number, GetNumberError},
    Number,
};

impl TryFrom<String> for crate::ContainerElement {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl<'a> TryFrom<&'a str> for crate::ContainerElement {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let result = tl::parse(value, ParserOptions::new())?;

        Ok(Self {
            elements: parse_children(result.children(), result.parser()),
            ..Default::default()
        })
    }
}

fn parse_top_children(
    children: Option<Children<'_, '_>>,
    parser: &Parser<'_>,
) -> Vec<crate::Element> {
    children.map_or_else(Vec::new, |children| {
        parse_children(&children.top().to_vec(), parser)
    })
}

fn parse_children(children: &[NodeHandle], parser: &Parser<'_>) -> Vec<crate::Element> {
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

fn get_tag_attr_value<'a>(tag: &'a HTMLTag, name: &'a str) -> Option<Cow<'a, str>> {
    tag.attributes()
        .iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .find(|(k, _)| k.to_lowercase().as_str() == name)
        .map(|(_, v)| v)
}

fn get_tag_attr_value_owned(tag: &HTMLTag, name: &str) -> Option<String> {
    get_tag_attr_value(tag, name).map(|x| x.to_string())
}

fn get_tag_attr_value_lower(tag: &HTMLTag, name: &str) -> Option<String> {
    get_tag_attr_value(tag, name).map(|x| x.to_lowercase())
}

fn get_direction(tag: &HTMLTag) -> LayoutDirection {
    match get_tag_attr_value_lower(tag, "sx-dir").as_deref() {
        Some("row") => LayoutDirection::Row,
        Some("col") => LayoutDirection::Column,
        _ => LayoutDirection::default(),
    }
}

fn get_color(tag: &HTMLTag, name: &str) -> Option<Color> {
    get_tag_attr_value(tag, name)
        .as_deref()
        .map(Color::from_hex)
}

fn get_border(tag: &HTMLTag, name: &str) -> Option<(Color, Number)> {
    get_tag_attr_value(tag, name)
        .as_deref()
        .and_then(|x| x.split_once(','))
        .map(|(size, color)| (size.trim(), color.trim()))
        .and_then(|(size, color)| parse_number(size).ok().map(|size| (size, color.trim())))
        .and_then(|(size, color)| Color::try_from_hex(color).ok().map(|color| (size, color)))
        .map(|(size, color)| (color, size))
}

fn get_state(tag: &HTMLTag, name: &str) -> Option<Value> {
    get_tag_attr_value(tag, name)
        .as_deref()
        .map(html_escape::decode_html_entities)
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
    if let Some(get) = get_tag_attr_value(tag, "hx-get") {
        Some(Route::Get {
            route: get.to_string(),
            trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
            swap: get_swap(tag).unwrap_or_default(),
        })
    } else if let Some(post) = get_tag_attr_value(tag, "hx-post") {
        Some(Route::Post {
            route: post.to_string(),
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
        Some("show") => LayoutOverflow::Show,
        Some("auto") => LayoutOverflow::Auto,
        _ => LayoutOverflow::default(),
    }
}

fn get_justify_content(tag: &HTMLTag, name: &str) -> JustifyContent {
    match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("start") => JustifyContent::Start,
        Some("center") => JustifyContent::Center,
        Some("end") => JustifyContent::End,
        Some("space-between") => JustifyContent::SpaceBetween,
        Some("space-evenly") => JustifyContent::SpaceEvenly,
        _ => JustifyContent::default(),
    }
}

fn get_align_items(tag: &HTMLTag, name: &str) -> AlignItems {
    match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("center") => AlignItems::Center,
        Some("end") => AlignItems::End,
        _ => AlignItems::default(),
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
            _ => Position::default(),
        })
}

fn get_number(tag: &HTMLTag, name: &str) -> Result<Number, GetNumberError> {
    Ok(if let Some(number) = get_tag_attr_value(tag, name) {
        parse_number(&number)?
    } else {
        return Err(GetNumberError::Parse(String::new()));
    })
}

fn parse_action(action: String) -> ActionType {
    if let Ok(action) = serde_json::from_str::<ActionType>(&action) {
        return action;
    };

    #[cfg(feature = "logic")]
    if let Ok(action) =
        serde_json::from_str::<gigachad_actions::logic::If>(&action).map(ActionType::Logic)
    {
        return action;
    };

    ActionType::Custom { action }
}

fn get_actions(tag: &HTMLTag) -> Vec<Action> {
    let mut actions = vec![];

    if let Some(action) = get_tag_attr_value(tag, "fx-click") {
        actions.push(Action {
            trigger: ActionTrigger::Click,
            action: parse_action(html_escape::decode_html_entities(&action).to_string()),
        });
    }
    if let Some(action) = get_tag_attr_value(tag, "fx-click-outside") {
        actions.push(Action {
            trigger: ActionTrigger::ClickOutside,
            action: parse_action(html_escape::decode_html_entities(&action).to_string()),
        });
    }
    if let Some(action) = get_tag_attr_value(tag, "fx-hover") {
        actions.push(Action {
            trigger: ActionTrigger::Hover,
            action: parse_action(html_escape::decode_html_entities(&action).to_string()),
        });
    }
    if let Some(action) = get_tag_attr_value(tag, "fx-change") {
        actions.push(Action {
            trigger: ActionTrigger::Change,
            action: parse_action(html_escape::decode_html_entities(&action).to_string()),
        });
    }

    actions
}

fn parse_element(
    tag: &HTMLTag<'_>,
    node: &Node<'_>,
    parser: &Parser<'_>,
) -> crate::ContainerElement {
    #[cfg(feature = "id")]
    static CURRENT_ID: std::sync::LazyLock<std::sync::Arc<std::sync::atomic::AtomicUsize>> =
        std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)));

    let border_radius = get_number(tag, "sx-border-radius").ok();
    let border_top_left_radius = get_number(tag, "sx-border-top-left-radius")
        .ok()
        .or_else(|| border_radius.clone());
    let border_top_right_radius = get_number(tag, "sx-border-top-right-radius")
        .ok()
        .or_else(|| border_radius.clone());
    let border_bottom_left_radius = get_number(tag, "sx-border-bottom-left-radius")
        .ok()
        .or_else(|| border_radius.clone());
    let border_bottom_right_radius = get_number(tag, "sx-border-bottom-right-radius")
        .ok()
        .or_else(|| border_radius.clone());

    let margin = get_number(tag, "sx-margin").ok();
    let margin_x = get_number(tag, "sx-margin-x").ok();
    let margin_y = get_number(tag, "sx-margin-y").ok();
    let margin_left = get_number(tag, "sx-margin-left")
        .ok()
        .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_right = get_number(tag, "sx-margin-right")
        .ok()
        .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_top = get_number(tag, "sx-margin-top")
        .ok()
        .or_else(|| margin_y.clone().or_else(|| margin.clone()));
    let margin_bottom = get_number(tag, "sx-margin-bottom")
        .ok()
        .or_else(|| margin_y.clone().or_else(|| margin.clone()));

    let padding = get_number(tag, "sx-padding").ok();
    let padding_x = get_number(tag, "sx-padding-x").ok();
    let padding_y = get_number(tag, "sx-padding-y").ok();
    let padding_left = get_number(tag, "sx-padding-left")
        .ok()
        .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_right = get_number(tag, "sx-padding-right")
        .ok()
        .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_top = get_number(tag, "sx-padding-top")
        .ok()
        .or_else(|| padding_y.clone().or_else(|| padding.clone()));
    let padding_bottom = get_number(tag, "sx-padding-bottom")
        .ok()
        .or_else(|| padding_y.clone().or_else(|| padding.clone()));

    #[allow(clippy::needless_update)]
    crate::ContainerElement {
        #[cfg(feature = "id")]
        id: CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        str_id: get_tag_attr_value_owned(tag, "id"),
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
        state: get_state(tag, "state"),
        hidden: get_bool(tag, "sx-hidden"),
        visibility: get_visibility(tag),
        overflow_x: get_overflow(tag, "sx-overflow-x"),
        overflow_y: get_overflow(tag, "sx-overflow-y"),
        justify_content: get_justify_content(tag, "sx-justify-content"),
        align_items: get_align_items(tag, "sx-align-items"),
        elements: parse_top_children(node.children(), parser),
        width: get_number(tag, "sx-width").ok(),
        height: get_number(tag, "sx-height").ok(),
        left: get_number(tag, "sx-left").ok(),
        right: get_number(tag, "sx-right").ok(),
        top: get_number(tag, "sx-top").ok(),
        bottom: get_number(tag, "sx-bottom").ok(),
        gap: get_number(tag, "sx-gap").ok(),
        opacity: get_number(tag, "sx-opacity").ok(),
        debug: get_bool(tag, "debug"),
        cursor: get_cursor(tag),
        position: get_position(tag),
        route: get_route(tag),
        actions: get_actions(tag),
        ..Default::default()
    }
}

#[allow(clippy::too_many_lines)]
fn parse_child(node: &Node<'_>, parser: &Parser<'_>) -> Option<crate::Element> {
    Some(match node {
        Node::Tag(tag) => match tag.name().as_utf8_str().to_lowercase().as_str() {
            "input" => match get_tag_attr_value_lower(tag, "type").as_deref() {
                Some("checkbox") => crate::Element::Input {
                    input: crate::Input::Checkbox {
                        checked: get_tag_attr_value_lower(tag, "checked")
                            .as_deref()
                            .map(|x| matches!(x, "checked" | "true" | "")),
                    },
                    element: parse_element(tag, node, parser),
                },
                Some("text") => crate::Element::Input {
                    input: crate::Input::Text {
                        value: get_tag_attr_value_owned(tag, "value"),
                        placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                    },
                    element: parse_element(tag, node, parser),
                },
                Some("password") => crate::Element::Input {
                    input: crate::Input::Password {
                        value: get_tag_attr_value_owned(tag, "value"),
                        placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                    },
                    element: parse_element(tag, node, parser),
                },
                Some(_) | None => {
                    return None;
                }
            },
            "main" => crate::Element::Main {
                element: parse_element(tag, node, parser),
            },
            "header" => crate::Element::Header {
                element: parse_element(tag, node, parser),
            },
            "footer" => crate::Element::Footer {
                element: parse_element(tag, node, parser),
            },
            "aside" => crate::Element::Aside {
                element: parse_element(tag, node, parser),
            },
            "div" => crate::Element::Div {
                element: parse_element(tag, node, parser),
            },
            "section" => crate::Element::Section {
                element: parse_element(tag, node, parser),
            },
            "form" => crate::Element::Form {
                element: parse_element(tag, node, parser),
            },
            "button" => crate::Element::Button {
                element: parse_element(tag, node, parser),
            },
            "img" => crate::Element::Image {
                source: get_tag_attr_value_owned(tag, "src"),
                element: parse_element(tag, node, parser),
            },
            "a" => crate::Element::Anchor {
                href: get_tag_attr_value_owned(tag, "href"),
                element: parse_element(tag, node, parser),
            },
            "h1" => crate::Element::Heading {
                size: crate::HeaderSize::H1,
                element: parse_element(tag, node, parser),
            },
            "h2" => crate::Element::Heading {
                size: crate::HeaderSize::H2,
                element: parse_element(tag, node, parser),
            },
            "h3" => crate::Element::Heading {
                size: crate::HeaderSize::H3,
                element: parse_element(tag, node, parser),
            },
            "h4" => crate::Element::Heading {
                size: crate::HeaderSize::H4,
                element: parse_element(tag, node, parser),
            },
            "h5" => crate::Element::Heading {
                size: crate::HeaderSize::H5,
                element: parse_element(tag, node, parser),
            },
            "h6" => crate::Element::Heading {
                size: crate::HeaderSize::H6,
                element: parse_element(tag, node, parser),
            },
            "ul" => crate::Element::UnorderedList {
                element: parse_element(tag, node, parser),
            },
            "ol" => crate::Element::OrderedList {
                element: parse_element(tag, node, parser),
            },
            "li" => crate::Element::ListItem {
                element: parse_element(tag, node, parser),
            },
            "table" => crate::Element::Table {
                element: parse_element(tag, node, parser),
            },
            "thead" => crate::Element::THead {
                element: parse_element(tag, node, parser),
            },
            "th" => crate::Element::TH {
                element: parse_element(tag, node, parser),
            },
            "tbody" => crate::Element::TBody {
                element: parse_element(tag, node, parser),
            },
            "tr" => crate::Element::TR {
                element: parse_element(tag, node, parser),
            },
            "td" => crate::Element::TD {
                element: parse_element(tag, node, parser),
            },
            #[cfg(feature = "canvas")]
            "canvas" => crate::Element::Canvas {
                element: parse_element(tag, node, parser),
            },
            _ => {
                return None;
            }
        },
        Node::Raw(x) => crate::Element::Raw {
            value: x.as_utf8_str().to_string(),
        },
        Node::Comment(_x) => {
            return None;
        }
    })
}
