//! HTML parsing and generation utilities.
//!
//! This module provides functionality to parse HTML strings into [`Container`](crate::Container) trees
//! and generate HTML/CSS output from container structures. Requires the `html` feature to be enabled.

use std::{borrow::Cow, collections::BTreeMap, iter::once};

use hyperchad_actions::{Action, ActionEffect, ActionTrigger, ActionType};
use hyperchad_color::{Color, ParseHexError};
use hyperchad_transformer_models::{
    AlignItems, Cursor, FontWeight, ImageFit, ImageLoading, JustifyContent, LayoutDirection,
    LayoutOverflow, LinkTarget, OverflowWrap, Position, Route, Selector, SwapStrategy, TextAlign,
    TextDecorationLine, TextDecorationStyle, TextOverflow, UserSelect, Visibility, WhiteSpace,
};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
pub use tl::ParseError;
use tl::{Children, HTMLTag, Node, NodeHandle, Parser, ParserOptions};

use crate::{
    ConfigOverride, Flex, Number, OverrideItem, TextDecoration,
    parse::{GetNumberError, parse_number},
};

/// Error type for HTML attribute value parsing failures.
#[derive(Debug, Error)]
pub enum ParseAttrError {
    /// The attribute value is not valid for the expected type.
    #[error("Invalid Value: '{0}'")]
    InvalidValue(String),
    /// Failed to parse a number value.
    #[error(transparent)]
    GetNumber(#[from] GetNumberError),
    /// Failed to parse a hex color value.
    #[error(transparent)]
    ParseHex(#[from] ParseHexError),
    /// Failed to deserialize JSON value.
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

/// Wrapper error that includes the attribute name with the parsing error.
#[derive(Debug, Error)]
pub enum ParseAttrWrapperError {
    /// Failed to parse attribute with the given name.
    #[error("Invalid attr value for {name}: '{error:?}'")]
    Parse {
        /// Name of the attribute that failed to parse.
        name: String,
        /// The underlying parse error.
        error: ParseAttrError,
    },
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
        if let Some(node) = node.get(parser)
            && let Some(element) = parse_child(node, parser)
        {
            elements.push(element);
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

fn parse_target(value: &str) -> Result<Selector, ParseAttrError> {
    Ok(match value {
        "this" => Selector::SelfTarget,
        value => {
            if let Some(value) = value.strip_prefix('#') {
                Selector::Id(value.to_string())
            } else if let Some(value) = value.strip_prefix('.') {
                Selector::Class(value.to_string())
            } else if let Some(value) = value.strip_prefix("> .") {
                Selector::ChildClass(value.to_string())
            } else {
                return Err(ParseAttrError::InvalidValue(value.to_string()));
            }
        }
    })
}

fn parse_strategy(value: &str) -> Result<SwapStrategy, ParseAttrError> {
    Ok(match value.to_lowercase().as_str() {
        "children" => SwapStrategy::Children,
        "this" => SwapStrategy::This,
        "beforebegin" => SwapStrategy::BeforeBegin,
        "afterbegin" => SwapStrategy::AfterBegin,
        "beforeend" => SwapStrategy::BeforeEnd,
        "afterend" => SwapStrategy::AfterEnd,
        "delete" => SwapStrategy::Delete,
        "none" => SwapStrategy::None,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn get_route(tag: &HTMLTag) -> Result<Option<Route>, ParseAttrWrapperError> {
    get_tag_attr_value_decoded(tag, "hx-get")
        .as_deref()
        .map(|x| {
            parse_get_route(x, tag).map_err(|e| ParseAttrWrapperError::Parse {
                name: "hx-get".to_string(),
                error: e,
            })
        })
        .or_else(|| {
            get_tag_attr_value_decoded(tag, "hx-post")
                .as_deref()
                .map(|x| {
                    parse_post_route(x, tag).map_err(|e| ParseAttrWrapperError::Parse {
                        name: "hx-post".to_string(),
                        error: e,
                    })
                })
        })
        .or_else(|| {
            get_tag_attr_value_decoded(tag, "hx-put")
                .as_deref()
                .map(|x| {
                    parse_put_route(x, tag).map_err(|e| ParseAttrWrapperError::Parse {
                        name: "hx-put".to_string(),
                        error: e,
                    })
                })
        })
        .or_else(|| {
            get_tag_attr_value_decoded(tag, "hx-delete")
                .as_deref()
                .map(|x| {
                    parse_delete_route(x, tag).map_err(|e| ParseAttrWrapperError::Parse {
                        name: "hx-delete".to_string(),
                        error: e,
                    })
                })
        })
        .or_else(|| {
            get_tag_attr_value_decoded(tag, "hx-patch")
                .as_deref()
                .map(|x| {
                    parse_patch_route(x, tag).map_err(|e| ParseAttrWrapperError::Parse {
                        name: "hx-patch".to_string(),
                        error: e,
                    })
                })
        })
        .transpose()
}

// TODO: Doesn't support reactive values
fn parse_get_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Get {
        route: value.to_string(),
        trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
        target: get_tag_attr_value_decoded(tag, "hx-target")
            .as_deref()
            .map(parse_target)
            .transpose()?
            .unwrap_or_default(),
        strategy: get_tag_attr_value_decoded(tag, "hx-swap")
            .as_deref()
            .map(parse_strategy)
            .transpose()?
            .unwrap_or_default(),
    })
}

// TODO: Doesn't support reactive values
fn parse_post_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Post {
        route: value.to_string(),
        trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
        target: get_tag_attr_value_decoded(tag, "hx-target")
            .as_deref()
            .map(parse_target)
            .transpose()?
            .unwrap_or_default(),
        strategy: get_tag_attr_value_decoded(tag, "hx-swap")
            .as_deref()
            .map(parse_strategy)
            .transpose()?
            .unwrap_or_default(),
    })
}

// TODO: Doesn't support reactive values
fn parse_put_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Put {
        route: value.to_string(),
        trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
        target: get_tag_attr_value_decoded(tag, "hx-target")
            .as_deref()
            .map(parse_target)
            .transpose()?
            .unwrap_or_default(),
        strategy: get_tag_attr_value_decoded(tag, "hx-swap")
            .as_deref()
            .map(parse_strategy)
            .transpose()?
            .unwrap_or_default(),
    })
}

// TODO: Doesn't support reactive values
fn parse_delete_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Delete {
        route: value.to_string(),
        trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
        target: get_tag_attr_value_decoded(tag, "hx-target")
            .as_deref()
            .map(parse_target)
            .transpose()?
            .unwrap_or_default(),
        strategy: get_tag_attr_value_decoded(tag, "hx-swap")
            .as_deref()
            .map(parse_strategy)
            .transpose()?
            .unwrap_or_default(),
    })
}

// TODO: Doesn't support reactive values
fn parse_patch_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Patch {
        route: value.to_string(),
        trigger: get_tag_attr_value_owned(tag, "hx-trigger"),
        target: get_tag_attr_value_decoded(tag, "hx-target")
            .as_deref()
            .map(parse_target)
            .transpose()?
            .unwrap_or_default(),
        strategy: get_tag_attr_value_decoded(tag, "hx-swap")
            .as_deref()
            .map(parse_strategy)
            .transpose()?
            .unwrap_or_default(),
    })
}

fn parse_overflow(value: &str) -> Result<LayoutOverflow, ParseAttrError> {
    Ok(match value {
        "wrap" => LayoutOverflow::Wrap { grid: false },
        "wrap-grid" => LayoutOverflow::Wrap { grid: true },
        "scroll" => LayoutOverflow::Scroll,
        "expand" => LayoutOverflow::Expand,
        "squash" => LayoutOverflow::Squash,
        "hidden" => LayoutOverflow::Hidden,
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
        _ => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_white_space(value: &str) -> Result<WhiteSpace, ParseAttrError> {
    Ok(match value {
        "normal" => WhiteSpace::Normal,
        "preserve" | "pre" => WhiteSpace::Preserve,
        "preserve-wrap" | "pre-wrap" => WhiteSpace::PreserveWrap,
        _ => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_user_select(value: &str) -> Result<UserSelect, ParseAttrError> {
    Ok(match value {
        "auto" => UserSelect::Auto,
        "none" => UserSelect::None,
        "text" => UserSelect::Text,
        "all" => UserSelect::All,
        _ => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_overflow_wrap(value: &str) -> Result<OverflowWrap, ParseAttrError> {
    Ok(match value {
        "normal" => OverflowWrap::Normal,
        "break-word" => OverflowWrap::BreakWord,
        "anywhere" => OverflowWrap::Anywhere,
        _ => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_text_overflow(value: &str) -> Result<TextOverflow, ParseAttrError> {
    Ok(match value {
        "clip" => TextOverflow::Clip,
        "ellipsis" => TextOverflow::Ellipsis,
        _ => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_font_weight(value: &str) -> Result<FontWeight, ParseAttrError> {
    Ok(match value {
        // Named variants
        "normal" => FontWeight::Normal,
        "bold" => FontWeight::Bold,
        "lighter" => FontWeight::Lighter,
        "bolder" => FontWeight::Bolder,
        "thin" => FontWeight::Thin,
        "extra-light" | "extralight" => FontWeight::ExtraLight,
        "light" => FontWeight::Light,
        "medium" => FontWeight::Medium,
        "semi-bold" | "semibold" => FontWeight::SemiBold,
        "extra-bold" | "extrabold" => FontWeight::ExtraBold,
        "black" => FontWeight::Black,
        // Numeric variants - preserve user intent by using Weight* variants
        "100" => FontWeight::Weight100,
        "200" => FontWeight::Weight200,
        "300" => FontWeight::Weight300,
        "400" => FontWeight::Weight400,
        "500" => FontWeight::Weight500,
        "600" => FontWeight::Weight600,
        "700" => FontWeight::Weight700,
        "800" => FontWeight::Weight800,
        "900" => FontWeight::Weight900,
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
        "sticky" => Position::Sticky,
        "relative" => Position::Relative,
        "absolute" => Position::Absolute,
        "fixed" => Position::Fixed,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn get_data_attrs(tag: &HTMLTag) -> BTreeMap<String, String> {
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
        .collect::<BTreeMap<_, _>>()
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

fn parse_text_decoration(value: &str) -> Result<TextDecoration, ParseAttrError> {
    let mut values = value.split_whitespace().peekable();

    if values.peek().is_none() {
        return Err(ParseAttrError::InvalidValue(value.to_string()));
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
            // Already parsed a thickness. throw out the whole thing
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }

        text_decoration.thickness = Some(parse_number(value)?);
    }

    Ok(text_decoration)
}

fn parse_flex(value: &str) -> Result<Flex, ParseAttrError> {
    let values = value
        .split_whitespace()
        .map(parse_number)
        .collect::<Result<Vec<_>, _>>()?;

    let mut iter = values.into_iter();

    Ok(match (iter.next(), iter.next(), iter.next(), iter.next()) {
        (Some(grow), None, None, None) => Flex {
            grow,
            ..Flex::default()
        },
        (Some(grow), Some(shrink), None, None) => Flex {
            grow,
            shrink,
            ..Flex::default()
        },
        (Some(grow), Some(shrink), Some(basis), None) => Flex {
            grow,
            shrink,
            basis,
        },
        _ => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
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

fn parse_image_loading(value: &str) -> Result<ImageLoading, ParseAttrError> {
    Ok(match value {
        "eager" => ImageLoading::Eager,
        "lazy" => ImageLoading::Lazy,
        value => {
            return Err(ParseAttrError::InvalidValue(value.to_string()));
        }
    })
}

fn parse_std_action(action: &str) -> Option<ActionEffect> {
    if let Ok(action) = serde_json::from_str::<ActionEffect>(action) {
        return Some(action);
    }

    if let Ok(action) = serde_json::from_str::<ActionType>(action) {
        return Some(action.into());
    }

    #[cfg(feature = "logic")]
    if let Ok(action) =
        serde_json::from_str::<hyperchad_actions::logic::If>(action).map(ActionType::Logic)
    {
        return Some(action.into());
    }

    None
}

fn parse_effect(effect: String) -> ActionEffect {
    if let Some(effect) = parse_std_action(&effect) {
        return effect;
    }

    ActionType::Custom { action: effect }.into()
}

fn parse_event_action(action: &str) -> (String, ActionEffect) {
    if let Some(ActionEffect {
        action: ActionType::Event { name, action },
        delay_off,
        throttle,
        unique,
    }) = parse_std_action(action)
    {
        return (
            name.clone(),
            ActionEffect {
                action: ActionType::Event { name, action },
                delay_off,
                throttle,
                unique,
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
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-click-outside") {
        actions.push(Action {
            trigger: ActionTrigger::ClickOutside,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-mouse-down") {
        actions.push(Action {
            trigger: ActionTrigger::MouseDown,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-hover") {
        actions.push(Action {
            trigger: ActionTrigger::Hover,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-change") {
        actions.push(Action {
            trigger: ActionTrigger::Change,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-resize") {
        actions.push(Action {
            trigger: ActionTrigger::Resize,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-immediate") {
        actions.push(Action {
            trigger: ActionTrigger::Immediate,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-http-before-request") {
        actions.push(Action {
            trigger: ActionTrigger::HttpBeforeRequest,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-http-after-request") {
        actions.push(Action {
            trigger: ActionTrigger::HttpAfterRequest,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-http-success") {
        actions.push(Action {
            trigger: ActionTrigger::HttpRequestSuccess,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-http-error") {
        actions.push(Action {
            trigger: ActionTrigger::HttpRequestError,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-http-abort") {
        actions.push(Action {
            trigger: ActionTrigger::HttpRequestAbort,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-http-timeout") {
        actions.push(Action {
            trigger: ActionTrigger::HttpRequestTimeout,
            effect: parse_effect(action),
        });
    }
    if let Some(action) = get_tag_attr_value_owned(tag, "fx-event") {
        let (name, action) = parse_event_action(&action);
        actions.push(Action {
            trigger: ActionTrigger::Event(name),
            effect: action,
        });
    }

    actions
}

/// Parse maybe reactive value
fn pmrv<
    'a,
    T: for<'de> Deserialize<'de> + Clone + std::fmt::Debug,
    O: IntoIterator<Item = OverrideItem>,
    E: Into<ParseAttrError>,
>(
    tag: &HTMLTag<'_>,
    mut names: impl Iterator<Item = &'a str>,
    overrides: &mut Vec<ConfigOverride>,
    func: impl Fn(&str) -> Result<T, E>,
    to_overrides: impl Fn(T) -> O,
) -> Result<Option<T>, ParseAttrWrapperError> {
    names
        .find_map(|name| get_tag_attr_value_decoded(tag, name).map(|x| (name, x)))
        .map(|(name, value)| {
            pmrv_inner(name, value.as_ref(), overrides, func, to_overrides).map_err(|e| {
                ParseAttrWrapperError::Parse {
                    name: name.to_string(),
                    error: e.into(),
                }
            })
        })
        .transpose()
        .map(Option::flatten)
}

#[allow(unused_variables, clippy::ptr_arg, clippy::needless_pass_by_ref_mut)]
fn pmrv_inner<
    T: for<'de> Deserialize<'de> + Clone + std::fmt::Debug,
    O: IntoIterator<Item = OverrideItem>,
    E: Into<ParseAttrError>,
>(
    name: &str,
    value: &str,
    overrides: &mut Vec<ConfigOverride>,
    func: impl Fn(&str) -> Result<T, E>,
    to_overrides: impl Fn(T) -> O,
) -> Result<Option<T>, E> {
    #[cfg(feature = "logic")]
    {
        Ok(
            if let Ok(x) = serde_json::from_str::<
                hyperchad_actions::logic::IfExpression<T, hyperchad_actions::logic::Responsive>,
            >(value)
            {
                if let Some(value) = x.value {
                    match x.condition {
                        hyperchad_actions::logic::Responsive::Target(target) => {
                            overrides.push(ConfigOverride {
                                condition: crate::OverrideCondition::ResponsiveTarget {
                                    name: target,
                                },
                                overrides: to_overrides(value).into_iter().collect::<Vec<_>>(),
                                default: x
                                    .default
                                    .clone()
                                    .and_then(|x| to_overrides(x).into_iter().next()),
                            });
                        }
                        hyperchad_actions::logic::Responsive::Targets(targets) => {
                            overrides.extend(targets.into_iter().map(|target| {
                                ConfigOverride {
                                    condition: crate::OverrideCondition::ResponsiveTarget {
                                        name: target,
                                    },
                                    overrides: to_overrides(value.clone())
                                        .into_iter()
                                        .collect::<Vec<_>>(),
                                    default: x
                                        .default
                                        .clone()
                                        .and_then(|x| to_overrides(x).into_iter().next()),
                                }
                            }));
                        }
                    }
                }

                x.default
            } else {
                Some(func(value)?)
            },
        )
    }
    #[cfg(not(feature = "logic"))]
    Ok(Some(func(value)?))
}

macro_rules! iter_once {
    ($val:expr) => {{ |x| std::iter::once($val(x)) }};
}

#[allow(clippy::too_many_lines)]
fn parse_element(
    tag: &HTMLTag<'_>,
    node: &Node<'_>,
    parser: &Parser<'_>,
) -> Result<crate::Container, ParseAttrWrapperError> {
    static CURRENT_ID: std::sync::LazyLock<std::sync::Arc<std::sync::atomic::AtomicUsize>> =
        std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)));

    let mut overrides = vec![];

    let border_radius = pmrv(
        tag,
        once("sx-border-radius"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::BorderTopLeftRadius(x.clone())).chain(
                std::iter::once(OverrideItem::BorderTopRightRadius(x.clone())).chain(
                    std::iter::once(OverrideItem::BorderBottomLeftRadius(x.clone()))
                        .chain(std::iter::once(OverrideItem::BorderBottomRightRadius(x))),
                ),
            )
        },
    )?;
    let border_radius_top = pmrv(
        tag,
        once("sx-border-top-radius"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::BorderTopLeftRadius(x.clone()))
                .chain(std::iter::once(OverrideItem::BorderTopRightRadius(x)))
        },
    )?;
    let border_radius_right = pmrv(
        tag,
        once("sx-border-right-radius"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::BorderTopRightRadius(x.clone()))
                .chain(std::iter::once(OverrideItem::BorderBottomRightRadius(x)))
        },
    )?;
    let border_radius_bottom = pmrv(
        tag,
        once("sx-border-bottom-radius"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::BorderBottomLeftRadius(x.clone()))
                .chain(std::iter::once(OverrideItem::BorderBottomRightRadius(x)))
        },
    )?;
    let border_radius_left = pmrv(
        tag,
        once("sx-border-left-radius"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::BorderTopLeftRadius(x.clone()))
                .chain(std::iter::once(OverrideItem::BorderBottomLeftRadius(x)))
        },
    )?;
    let border_top_left_radius = pmrv(
        tag,
        once("sx-border-top-left-radius"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::BorderTopLeftRadius),
    )?
    .or_else(|| border_radius_top.clone())
    .or_else(|| border_radius_left.clone())
    .or_else(|| border_radius.clone());
    let border_top_right_radius = pmrv(
        tag,
        once("sx-border-top-right-radius"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::BorderTopRightRadius),
    )?
    .or_else(|| border_radius_top.clone())
    .or_else(|| border_radius_right.clone())
    .or_else(|| border_radius.clone());
    let border_bottom_left_radius = pmrv(
        tag,
        once("sx-border-bottom-left-radius"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::BorderBottomLeftRadius),
    )?
    .or_else(|| border_radius_bottom.clone())
    .or_else(|| border_radius_left.clone())
    .or_else(|| border_radius.clone());
    let border_bottom_right_radius = pmrv(
        tag,
        once("sx-border-bottom-right-radius"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::BorderBottomRightRadius),
    )?
    .or_else(|| border_radius_bottom.clone())
    .or_else(|| border_radius_right.clone())
    .or_else(|| border_radius.clone());

    let margin = pmrv(tag, once("sx-margin"), &mut overrides, parse_number, |x| {
        std::iter::once(OverrideItem::MarginTop(x.clone())).chain(
            std::iter::once(OverrideItem::MarginRight(x.clone())).chain(
                std::iter::once(OverrideItem::MarginBottom(x.clone()))
                    .chain(std::iter::once(OverrideItem::MarginLeft(x))),
            ),
        )
    })?;
    let margin_x = pmrv(
        tag,
        once("sx-margin-x"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::MarginLeft(x.clone()))
                .chain(std::iter::once(OverrideItem::MarginRight(x)))
        },
    )?;
    let margin_y = pmrv(
        tag,
        once("sx-margin-y"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::MarginTop(x.clone()))
                .chain(std::iter::once(OverrideItem::MarginBottom(x)))
        },
    )?;
    let margin_left = pmrv(
        tag,
        once("sx-margin-left"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::MarginLeft),
    )?
    .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_right = pmrv(
        tag,
        once("sx-margin-right"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::MarginRight),
    )?
    .or_else(|| margin_x.clone().or_else(|| margin.clone()));
    let margin_top = pmrv(
        tag,
        once("sx-margin-top"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::MarginTop),
    )?
    .or_else(|| margin_y.clone().or_else(|| margin.clone()));
    let margin_bottom = pmrv(
        tag,
        once("sx-margin-bottom"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::MarginBottom),
    )?
    .or_else(|| margin_y.clone().or_else(|| margin.clone()));

    let padding = pmrv(tag, once("sx-padding"), &mut overrides, parse_number, |x| {
        std::iter::once(OverrideItem::PaddingTop(x.clone())).chain(
            std::iter::once(OverrideItem::PaddingRight(x.clone())).chain(
                std::iter::once(OverrideItem::PaddingBottom(x.clone()))
                    .chain(std::iter::once(OverrideItem::PaddingLeft(x))),
            ),
        )
    })?;
    let padding_x = pmrv(
        tag,
        once("sx-padding-x"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::PaddingLeft(x.clone()))
                .chain(std::iter::once(OverrideItem::PaddingRight(x)))
        },
    )?;
    let padding_y = pmrv(
        tag,
        once("sx-padding-y"),
        &mut overrides,
        parse_number,
        |x| {
            std::iter::once(OverrideItem::PaddingTop(x.clone()))
                .chain(std::iter::once(OverrideItem::PaddingBottom(x)))
        },
    )?;
    let padding_left = pmrv(
        tag,
        once("sx-padding-left"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::PaddingLeft),
    )?
    .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_right = pmrv(
        tag,
        once("sx-padding-right"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::PaddingRight),
    )?
    .or_else(|| padding_x.clone().or_else(|| padding.clone()));
    let padding_top = pmrv(
        tag,
        once("sx-padding-top"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::PaddingTop),
    )?
    .or_else(|| padding_y.clone().or_else(|| padding.clone()));
    let padding_bottom = pmrv(
        tag,
        once("sx-padding-bottom"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::PaddingBottom),
    )?
    .or_else(|| padding_y.clone().or_else(|| padding.clone()));

    let mut text_decoration = pmrv(
        tag,
        once("sx-text-decoration"),
        &mut overrides,
        parse_text_decoration,
        iter_once!(OverrideItem::TextDecoration),
    )?;

    if let Some(color) = pmrv(
        tag,
        once("sx-text-decoration-color"),
        &mut overrides,
        Color::try_from_hex,
        |_| std::iter::empty(),
    )? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.color = Some(color);
        } else {
            text_decoration = Some(TextDecoration {
                color: Some(color),
                ..TextDecoration::default()
            });
        }
    }
    if let Some(line) = pmrv(
        tag,
        once("sx-text-decoration-line"),
        &mut overrides,
        parse_text_decoration_lines,
        |_| std::iter::empty(),
    )? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.line = line;
        } else {
            text_decoration = Some(TextDecoration {
                line,
                ..TextDecoration::default()
            });
        }
    }
    if let Some(style) = pmrv(
        tag,
        once("sx-text-decoration-style"),
        &mut overrides,
        parse_text_decoration_style,
        |_| std::iter::empty(),
    )? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.style = Some(style);
        } else {
            text_decoration = Some(TextDecoration {
                style: Some(style),
                ..TextDecoration::default()
            });
        }
    }
    if let Some(thickness) = pmrv(
        tag,
        once("sx-text-decoration-thickness"),
        &mut overrides,
        parse_number,
        |_| std::iter::empty(),
    )? {
        if let Some(text_decoration) = &mut text_decoration {
            text_decoration.thickness = Some(thickness);
        } else {
            text_decoration = Some(TextDecoration {
                thickness: Some(thickness),
                ..TextDecoration::default()
            });
        }
    }

    let mut flex = pmrv(
        tag,
        once("sx-flex"),
        &mut overrides,
        parse_flex,
        iter_once!(OverrideItem::Flex),
    )?;

    if let Some(grow) = pmrv(
        tag,
        once("sx-flex-grow"),
        &mut overrides,
        parse_number,
        |_| std::iter::empty(),
    )? {
        if let Some(flex) = &mut flex {
            flex.grow = grow;
        } else {
            flex = Some(Flex {
                grow,
                ..Flex::default()
            });
        }
    }
    if let Some(shrink) = pmrv(
        tag,
        once("sx-flex-shrink"),
        &mut overrides,
        parse_number,
        |_| std::iter::empty(),
    )? {
        if let Some(flex) = &mut flex {
            flex.shrink = shrink;
        } else {
            flex = Some(Flex {
                shrink,
                ..Flex::default()
            });
        }
    }
    if let Some(basis) = pmrv(
        tag,
        once("sx-flex-basis"),
        &mut overrides,
        parse_number,
        |_| std::iter::empty(),
    )? {
        if let Some(flex) = &mut flex {
            flex.basis = basis;
        } else {
            flex = Some(Flex {
                basis,
                ..Flex::default()
            });
        }
    }

    let gap = pmrv(
        tag,
        once("sx-gap"),
        &mut overrides,
        parse_number,
        iter_once!(OverrideItem::ColumnGap),
    )?;

    let border = pmrv(tag, once("sx-border"), &mut overrides, parse_border, |x| {
        std::iter::once(OverrideItem::BorderTop(x.clone())).chain(
            std::iter::once(OverrideItem::BorderRight(x.clone())).chain(
                std::iter::once(OverrideItem::BorderBottom(x.clone()))
                    .chain(std::iter::once(OverrideItem::BorderLeft(x))),
            ),
        )
    })?;

    let border_x = pmrv(
        tag,
        once("sx-border-x"),
        &mut overrides,
        parse_border,
        |x| {
            std::iter::once(OverrideItem::BorderRight(x.clone()))
                .chain(std::iter::once(OverrideItem::BorderLeft(x)))
        },
    )?;

    let border_y = pmrv(
        tag,
        once("sx-border-y"),
        &mut overrides,
        parse_border,
        |x| {
            std::iter::once(OverrideItem::BorderTop(x.clone()))
                .chain(std::iter::once(OverrideItem::BorderBottom(x)))
        },
    )?;

    let border_top = pmrv(
        tag,
        once("sx-border-top"),
        &mut overrides,
        parse_border,
        iter_once!(OverrideItem::BorderTop),
    )?
    .or_else(|| border_y.clone().or_else(|| border.clone()));
    let border_right = pmrv(
        tag,
        once("sx-border-right"),
        &mut overrides,
        parse_border,
        iter_once!(OverrideItem::BorderRight),
    )?
    .or_else(|| border_x.clone().or_else(|| border.clone()));
    let border_bottom = pmrv(
        tag,
        once("sx-border-bottom"),
        &mut overrides,
        parse_border,
        iter_once!(OverrideItem::BorderBottom),
    )?
    .or_else(|| border_y.clone().or_else(|| border.clone()));
    let border_left = pmrv(
        tag,
        once("sx-border-left"),
        &mut overrides,
        parse_border,
        iter_once!(OverrideItem::BorderLeft),
    )?
    .or_else(|| border_x.clone().or_else(|| border.clone()));

    #[allow(clippy::needless_update)]
    Ok(crate::Container {
        id: CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        str_id: pmrv(
            tag,
            once("id"),
            &mut overrides,
            |x| Ok::<_, ParseAttrError>(x.to_string()),
            iter_once!(OverrideItem::StrId),
        )?,
        classes: pmrv(
            tag,
            once("class"),
            &mut overrides,
            parse_classes,
            iter_once!(OverrideItem::Classes),
        )?
        .unwrap_or_else(Vec::new),
        data: get_data_attrs(tag),
        direction: pmrv(
            tag,
            once("sx-dir"),
            &mut overrides,
            parse_direction,
            iter_once!(OverrideItem::Direction),
        )?
        .unwrap_or_default(),
        background: pmrv(
            tag,
            once("sx-background"),
            &mut overrides,
            Color::try_from_hex,
            iter_once!(OverrideItem::Background),
        )?,
        border_top,
        border_right,
        border_bottom,
        border_left,
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
        font_size: pmrv(
            tag,
            once("sx-font-size"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::FontSize),
        )?,
        color: pmrv(
            tag,
            once("sx-color"),
            &mut overrides,
            Color::try_from_hex,
            iter_once!(OverrideItem::Color),
        )?,
        state: get_tag_attr_value_decoded(tag, "state")
            .as_deref()
            .map(parse_state)
            .transpose()
            .map_err(|e| ParseAttrWrapperError::Parse {
                name: "state".to_string(),
                error: e,
            })?,
        hidden: pmrv(
            tag,
            once("sx-hidden"),
            &mut overrides,
            parse_bool,
            iter_once!(OverrideItem::Hidden),
        )?,
        visibility: pmrv(
            tag,
            once("sx-visibility"),
            &mut overrides,
            parse_visibility,
            iter_once!(OverrideItem::Visibility),
        )?,
        overflow_x: pmrv(
            tag,
            once("sx-overflow-x"),
            &mut overrides,
            parse_overflow,
            iter_once!(OverrideItem::OverflowX),
        )?
        .unwrap_or_default(),
        overflow_y: pmrv(
            tag,
            once("sx-overflow-y"),
            &mut overrides,
            parse_overflow,
            iter_once!(OverrideItem::OverflowY),
        )?
        .unwrap_or_default(),
        grid_cell_size: pmrv(
            tag,
            once("sx-grid-cell-size"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::GridCellSize),
        )?,
        justify_content: pmrv(
            tag,
            once("sx-justify-content"),
            &mut overrides,
            parse_justify_content,
            iter_once!(OverrideItem::JustifyContent),
        )?,
        align_items: pmrv(
            tag,
            once("sx-align-items"),
            &mut overrides,
            parse_align_items,
            iter_once!(OverrideItem::AlignItems),
        )?,
        text_align: pmrv(
            tag,
            once("sx-text-align"),
            &mut overrides,
            parse_text_align,
            iter_once!(OverrideItem::TextAlign),
        )?,
        white_space: pmrv(
            tag,
            once("sx-white-space"),
            &mut overrides,
            parse_white_space,
            iter_once!(OverrideItem::WhiteSpace),
        )?,
        text_decoration,
        font_family: pmrv(
            tag,
            once("sx-font-family"),
            &mut overrides,
            |x| {
                Ok::<_, ParseAttrError>(
                    x.split(',')
                        .map(str::trim)
                        .filter(|x| !x.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                )
            },
            iter_once!(OverrideItem::FontFamily),
        )?,
        font_weight: pmrv(
            tag,
            once("sx-font-weight"),
            &mut overrides,
            parse_font_weight,
            iter_once!(OverrideItem::FontWeight),
        )?,
        children: parse_top_children(node.children(), parser),
        width: pmrv(
            tag,
            once("sx-width"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::Width),
        )?,
        min_width: pmrv(
            tag,
            once("sx-min-width"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::MinWidth),
        )?,
        max_width: pmrv(
            tag,
            once("sx-max-width"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::MaxWidth),
        )?,
        height: pmrv(
            tag,
            once("sx-height"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::Height),
        )?,
        min_height: pmrv(
            tag,
            once("sx-min-height"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::MinHeight),
        )?,
        max_height: pmrv(
            tag,
            once("sx-max-height"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::MaxHeight),
        )?,
        flex,
        left: pmrv(
            tag,
            once("sx-left"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::Left),
        )?,
        right: pmrv(
            tag,
            once("sx-right"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::Right),
        )?,
        top: pmrv(
            tag,
            once("sx-top"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::Top),
        )?,
        bottom: pmrv(
            tag,
            once("sx-bottom"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::Bottom),
        )?,
        translate_x: pmrv(
            tag,
            once("sx-translate-x"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::TranslateX),
        )?,
        translate_y: pmrv(
            tag,
            once("sx-translate-y"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::TranslateY),
        )?,
        column_gap: pmrv(
            tag,
            once("sx-column-gap").chain(once("sx-col-gap")),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::ColumnGap),
        )?
        .or_else(|| gap.clone()),
        row_gap: pmrv(
            tag,
            once("sx-row-gap"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::RowGap),
        )?
        .or(gap),
        opacity: pmrv(
            tag,
            once("sx-opacity"),
            &mut overrides,
            parse_number,
            iter_once!(OverrideItem::Opacity),
        )?,
        debug: get_tag_attr_value_decoded(tag, "debug")
            .as_deref()
            .map(parse_bool)
            .transpose()
            .map_err(|e| ParseAttrWrapperError::Parse {
                name: "debug".to_string(),
                error: e,
            })?,
        cursor: pmrv(
            tag,
            once("sx-cursor"),
            &mut overrides,
            parse_cursor,
            iter_once!(OverrideItem::Cursor),
        )?,
        user_select: pmrv(
            tag,
            once("sx-user-select"),
            &mut overrides,
            parse_user_select,
            iter_once!(OverrideItem::UserSelect),
        )?,
        overflow_wrap: pmrv(
            tag,
            once("sx-overflow-wrap"),
            &mut overrides,
            parse_overflow_wrap,
            iter_once!(OverrideItem::OverflowWrap),
        )?,
        text_overflow: pmrv(
            tag,
            once("sx-text-overflow"),
            &mut overrides,
            parse_text_overflow,
            iter_once!(OverrideItem::TextOverflow),
        )?,
        position: pmrv(
            tag,
            once("sx-position"),
            &mut overrides,
            parse_position,
            iter_once!(OverrideItem::Position),
        )?,
        route: get_route(tag)?,
        actions: get_actions(tag),
        overrides,
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
                            name: get_tag_attr_value_owned(tag, "name"),
                            autofocus: get_tag_attr_value_decoded(tag, "autofocus")
                                .as_deref()
                                .map(parse_bool)
                                .transpose()
                                .ok()
                                .flatten(),
                            input: crate::Input::Checkbox {
                                checked: get_tag_attr_value_lower(tag, "checked")
                                    .as_deref()
                                    .map(|x| matches!(x, "checked" | "true" | "")),
                            },
                        }
                    }
                    Some("text") => {
                        container.element = crate::Element::Input {
                            name: get_tag_attr_value_owned(tag, "name"),
                            autofocus: get_tag_attr_value_decoded(tag, "autofocus")
                                .as_deref()
                                .map(parse_bool)
                                .transpose()
                                .ok()
                                .flatten(),
                            input: crate::Input::Text {
                                value: get_tag_attr_value_owned(tag, "value"),
                                placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                            },
                        }
                    }
                    Some("password") => {
                        container.element = crate::Element::Input {
                            name: get_tag_attr_value_owned(tag, "name"),
                            autofocus: get_tag_attr_value_decoded(tag, "autofocus")
                                .as_deref()
                                .map(parse_bool)
                                .transpose()
                                .ok()
                                .flatten(),
                            input: crate::Input::Password {
                                value: get_tag_attr_value_owned(tag, "value"),
                                placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                            },
                        }
                    }
                    Some("hidden") => {
                        container.element = crate::Element::Input {
                            name: get_tag_attr_value_owned(tag, "name"),
                            autofocus: get_tag_attr_value_decoded(tag, "autofocus")
                                .as_deref()
                                .map(parse_bool)
                                .transpose()
                                .ok()
                                .flatten(),
                            input: crate::Input::Hidden {
                                value: get_tag_attr_value_owned(tag, "value"),
                            },
                        }
                    }
                    Some(_) | None => {
                        return None;
                    }
                },
                "textarea" => {
                    // Extract text content from child Raw nodes (textarea value is text content, not an attribute)
                    let raw_value = node
                        .children()
                        .map(|children| {
                            children
                                .top()
                                .iter()
                                .filter_map(|child| {
                                    child.get(parser).and_then(|node| match node {
                                        Node::Raw(x) => Some(x.as_utf8_str().to_string()),
                                        _ => None,
                                    })
                                })
                                .collect::<String>()
                        })
                        .unwrap_or_default();

                    // Decode HTML entities in the textarea value
                    let value = html_escape::decode_html_entities(&raw_value).to_string();

                    container.element = crate::Element::Textarea {
                        name: get_tag_attr_value_owned(tag, "name"),
                        placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                        value,
                        rows: get_tag_attr_value_decoded(tag, "rows")
                            .as_deref()
                            .map(parse_number)
                            .transpose()
                            .ok()
                            .flatten(),
                        cols: get_tag_attr_value_decoded(tag, "cols")
                            .as_deref()
                            .map(parse_number)
                            .transpose()
                            .ok()
                            .flatten(),
                    };
                }
                "main" => container.element = crate::Element::Main,
                "header" => container.element = crate::Element::Header,
                "footer" => container.element = crate::Element::Footer,
                "aside" => container.element = crate::Element::Aside,
                "div" => container.element = crate::Element::Div,
                "span" => container.element = crate::Element::Span,
                "section" => container.element = crate::Element::Section,
                "form" => container.element = crate::Element::Form,
                "button" => {
                    container.element = crate::Element::Button {
                        r#type: get_tag_attr_value_owned(tag, "type"),
                    }
                }
                "img" => {
                    container.element = crate::Element::Image {
                        source: get_tag_attr_value_owned(tag, "src"),
                        alt: get_tag_attr_value_owned(tag, "alt"),
                        fit: get_tag_attr_value_decoded(tag, "sx-fit")
                            .as_deref()
                            .map(parse_image_fit)
                            .transpose()
                            .unwrap(),
                        loading: get_tag_attr_value_decoded(tag, "loading")
                            .as_deref()
                            .map(parse_image_loading)
                            .transpose()
                            .unwrap(),
                        source_set: get_tag_attr_value_owned(tag, "srcset"),
                        sizes: get_tag_attr_value_decoded(tag, "sizes")
                            .as_deref()
                            .map(parse_number)
                            .transpose()
                            .unwrap(),
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
                "th" => {
                    container.element = crate::Element::TH {
                        rows: get_tag_attr_value_decoded(tag, "rows")
                            .as_deref()
                            .map(parse_number)
                            .transpose()
                            .ok()
                            .flatten(),
                        columns: get_tag_attr_value_decoded(tag, "columns")
                            .as_deref()
                            .map(parse_number)
                            .transpose()
                            .ok()
                            .flatten(),
                    };
                }
                "tbody" => container.element = crate::Element::TBody,
                "tr" => {
                    container.element = crate::Element::TR;
                    if get_tag_attr_value_undecoded(tag, "sx-dir").is_none() {
                        container.direction = LayoutDirection::Row;
                    }
                }
                "td" => {
                    container.element = crate::Element::TD {
                        rows: get_tag_attr_value_decoded(tag, "rows")
                            .as_deref()
                            .map(parse_number)
                            .transpose()
                            .ok()
                            .flatten(),
                        columns: get_tag_attr_value_decoded(tag, "columns")
                            .as_deref()
                            .map(parse_number)
                            .transpose()
                            .ok()
                            .flatten(),
                    };
                }
                "details" => {
                    container.element = crate::Element::Details {
                        open: get_tag_attr_value_undecoded(tag, "open").map(|_| true),
                    };
                }
                "summary" => {
                    container.element = crate::Element::Summary;
                }
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
mod test_parse_helpers {
    use super::*;
    use pretty_assertions::assert_eq;

    // Tests for parse_text_decoration
    #[test_log::test]
    fn parse_text_decoration_parses_single_line_type() {
        let result = parse_text_decoration("underline").unwrap();
        assert_eq!(result.line, vec![TextDecorationLine::Underline]);
        assert!(result.style.is_none());
        assert!(result.color.is_none());
        assert!(result.thickness.is_none());
    }

    #[test_log::test]
    fn parse_text_decoration_parses_multiple_line_types() {
        let result = parse_text_decoration("underline overline").unwrap();
        assert_eq!(
            result.line,
            vec![TextDecorationLine::Underline, TextDecorationLine::Overline]
        );
    }

    #[test_log::test]
    fn parse_text_decoration_parses_line_with_style() {
        let result = parse_text_decoration("underline wavy").unwrap();
        assert_eq!(result.line, vec![TextDecorationLine::Underline]);
        assert_eq!(result.style, Some(TextDecorationStyle::Wavy));
    }

    #[test_log::test]
    fn parse_text_decoration_parses_line_with_color() {
        let result = parse_text_decoration("underline #ff0000").unwrap();
        assert_eq!(result.line, vec![TextDecorationLine::Underline]);
        assert!(result.color.is_some());
    }

    #[test_log::test]
    fn parse_text_decoration_parses_line_style_color() {
        let result = parse_text_decoration("underline solid #ff0000").unwrap();
        assert_eq!(result.line, vec![TextDecorationLine::Underline]);
        assert_eq!(result.style, Some(TextDecorationStyle::Solid));
        assert!(result.color.is_some());
    }

    #[test_log::test]
    fn parse_text_decoration_parses_line_style_color_thickness() {
        let result = parse_text_decoration("underline solid #ff0000 2").unwrap();
        assert_eq!(result.line, vec![TextDecorationLine::Underline]);
        assert_eq!(result.style, Some(TextDecorationStyle::Solid));
        assert!(result.color.is_some());
        assert!(result.thickness.is_some());
    }

    #[test_log::test]
    fn parse_text_decoration_returns_error_for_empty_string() {
        let result = parse_text_decoration("");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn parse_text_decoration_returns_error_for_double_thickness() {
        // After parsing line, style, color, any remaining token is thickness.
        // A second thickness value should fail.
        let result = parse_text_decoration("underline solid #ff0000 2 3");
        assert!(result.is_err());
    }

    // Tests for parse_flex
    #[test_log::test]
    fn parse_flex_parses_single_grow_value() {
        let result = parse_flex("1").unwrap();
        assert_eq!(result.grow, crate::Number::Integer(1));
        // shrink and basis should be defaults
        assert_eq!(result.shrink, Flex::default().shrink);
        assert_eq!(result.basis, Flex::default().basis);
    }

    #[test_log::test]
    fn parse_flex_parses_grow_and_shrink() {
        let result = parse_flex("2 3").unwrap();
        assert_eq!(result.grow, crate::Number::Integer(2));
        assert_eq!(result.shrink, crate::Number::Integer(3));
        assert_eq!(result.basis, Flex::default().basis);
    }

    #[test_log::test]
    fn parse_flex_parses_grow_shrink_and_basis() {
        let result = parse_flex("1 0 100").unwrap();
        assert_eq!(result.grow, crate::Number::Integer(1));
        assert_eq!(result.shrink, crate::Number::Integer(0));
        assert_eq!(result.basis, crate::Number::Integer(100));
    }

    #[test_log::test]
    fn parse_flex_parses_with_percent_basis() {
        let result = parse_flex("1 1 50%").unwrap();
        assert_eq!(result.grow, crate::Number::Integer(1));
        assert_eq!(result.shrink, crate::Number::Integer(1));
        assert_eq!(result.basis, crate::Number::IntegerPercent(50));
    }

    #[test_log::test]
    fn parse_flex_returns_error_for_empty_string() {
        let result = parse_flex("");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn parse_flex_returns_error_for_too_many_values() {
        let result = parse_flex("1 2 3 4");
        assert!(result.is_err());
    }

    // Tests for parse_border
    #[test_log::test]
    fn parse_border_parses_size_and_color() {
        let result = parse_border("2, #ff0000").unwrap();
        assert_eq!(result.1, crate::Number::Integer(2));
        // Color should be parsed
        assert_eq!(result.0, hyperchad_color::Color::from_hex("#ff0000"));
    }

    #[test_log::test]
    fn parse_border_returns_error_for_missing_comma() {
        let result = parse_border("2 #ff0000");
        assert!(result.is_err());
    }

    #[test_log::test]
    fn parse_border_returns_error_for_invalid_color() {
        let result = parse_border("2, not-a-color");
        assert!(result.is_err());
    }

    // Tests for parse_classes
    #[test_log::test]
    fn parse_classes_parses_single_class() {
        let result = parse_classes("my-class").unwrap();
        assert_eq!(result, vec!["my-class".to_string()]);
    }

    #[test_log::test]
    fn parse_classes_parses_multiple_classes() {
        let result = parse_classes("class1 class2 class3").unwrap();
        assert_eq!(
            result,
            vec![
                "class1".to_string(),
                "class2".to_string(),
                "class3".to_string()
            ]
        );
    }

    #[test_log::test]
    fn parse_classes_handles_extra_whitespace() {
        let result = parse_classes("  class1   class2  ").unwrap();
        assert_eq!(result, vec!["class1".to_string(), "class2".to_string()]);
    }

    #[test_log::test]
    fn parse_classes_returns_empty_vec_for_empty_string() {
        let result = parse_classes("").unwrap();
        assert!(result.is_empty());
    }

    // Tests for parse_link_target
    #[test_log::test]
    fn parse_link_target_parses_self_target() {
        let result = parse_link_target("_self");
        assert!(matches!(result, LinkTarget::SelfTarget));
    }

    #[test_log::test]
    fn parse_link_target_parses_blank_target() {
        let result = parse_link_target("_blank");
        assert!(matches!(result, LinkTarget::Blank));
    }

    #[test_log::test]
    fn parse_link_target_parses_parent_target() {
        let result = parse_link_target("_parent");
        assert!(matches!(result, LinkTarget::Parent));
    }

    #[test_log::test]
    fn parse_link_target_parses_top_target() {
        let result = parse_link_target("_top");
        assert!(matches!(result, LinkTarget::Top));
    }

    #[test_log::test]
    fn parse_link_target_parses_custom_target() {
        let result = parse_link_target("my-frame");
        assert!(matches!(result, LinkTarget::Custom(ref s) if s == "my-frame"));
    }

    // Tests for parse_target (Selector)
    #[test_log::test]
    fn parse_target_parses_self_target() {
        let result = parse_target("this").unwrap();
        assert!(matches!(result, Selector::SelfTarget));
    }

    #[test_log::test]
    fn parse_target_parses_id_selector() {
        let result = parse_target("#my-id").unwrap();
        assert!(matches!(result, Selector::Id(ref id) if id == "my-id"));
    }

    #[test_log::test]
    fn parse_target_parses_class_selector() {
        let result = parse_target(".my-class").unwrap();
        assert!(matches!(result, Selector::Class(ref class) if class == "my-class"));
    }

    #[test_log::test]
    fn parse_target_parses_child_class_selector() {
        let result = parse_target("> .my-class").unwrap();
        assert!(matches!(result, Selector::ChildClass(ref class) if class == "my-class"));
    }

    #[test_log::test]
    fn parse_target_returns_error_for_invalid_selector() {
        let result = parse_target("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_strategy (SwapStrategy)
    #[test_log::test]
    fn parse_strategy_parses_children() {
        let result = parse_strategy("children").unwrap();
        assert!(matches!(result, SwapStrategy::Children));
    }

    #[test_log::test]
    fn parse_strategy_parses_this() {
        let result = parse_strategy("this").unwrap();
        assert!(matches!(result, SwapStrategy::This));
    }

    #[test_log::test]
    fn parse_strategy_parses_beforebegin() {
        let result = parse_strategy("beforebegin").unwrap();
        assert!(matches!(result, SwapStrategy::BeforeBegin));
    }

    #[test_log::test]
    fn parse_strategy_parses_case_insensitive() {
        let result = parse_strategy("BEFOREEND").unwrap();
        assert!(matches!(result, SwapStrategy::BeforeEnd));
    }

    #[test_log::test]
    fn parse_strategy_parses_delete() {
        let result = parse_strategy("delete").unwrap();
        assert!(matches!(result, SwapStrategy::Delete));
    }

    #[test_log::test]
    fn parse_strategy_parses_none() {
        let result = parse_strategy("none").unwrap();
        assert!(matches!(result, SwapStrategy::None));
    }

    #[test_log::test]
    fn parse_strategy_returns_error_for_invalid() {
        let result = parse_strategy("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_overflow
    #[test_log::test]
    fn parse_overflow_parses_wrap() {
        let result = parse_overflow("wrap").unwrap();
        assert!(matches!(result, LayoutOverflow::Wrap { grid: false }));
    }

    #[test_log::test]
    fn parse_overflow_parses_wrap_grid() {
        let result = parse_overflow("wrap-grid").unwrap();
        assert!(matches!(result, LayoutOverflow::Wrap { grid: true }));
    }

    #[test_log::test]
    fn parse_overflow_parses_scroll() {
        let result = parse_overflow("scroll").unwrap();
        assert!(matches!(result, LayoutOverflow::Scroll));
    }

    #[test_log::test]
    fn parse_overflow_parses_expand() {
        let result = parse_overflow("expand").unwrap();
        assert!(matches!(result, LayoutOverflow::Expand));
    }

    #[test_log::test]
    fn parse_overflow_parses_squash() {
        let result = parse_overflow("squash").unwrap();
        assert!(matches!(result, LayoutOverflow::Squash));
    }

    #[test_log::test]
    fn parse_overflow_parses_hidden() {
        let result = parse_overflow("hidden").unwrap();
        assert!(matches!(result, LayoutOverflow::Hidden));
    }

    #[test_log::test]
    fn parse_overflow_parses_auto() {
        let result = parse_overflow("auto").unwrap();
        assert!(matches!(result, LayoutOverflow::Auto));
    }

    #[test_log::test]
    fn parse_overflow_returns_error_for_invalid() {
        let result = parse_overflow("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_image_fit
    #[test_log::test]
    fn parse_image_fit_parses_default() {
        let result = parse_image_fit("default").unwrap();
        assert!(matches!(result, ImageFit::Default));
    }

    #[test_log::test]
    fn parse_image_fit_parses_contain() {
        let result = parse_image_fit("contain").unwrap();
        assert!(matches!(result, ImageFit::Contain));
    }

    #[test_log::test]
    fn parse_image_fit_parses_cover() {
        let result = parse_image_fit("cover").unwrap();
        assert!(matches!(result, ImageFit::Cover));
    }

    #[test_log::test]
    fn parse_image_fit_parses_fill() {
        let result = parse_image_fit("fill").unwrap();
        assert!(matches!(result, ImageFit::Fill));
    }

    #[test_log::test]
    fn parse_image_fit_parses_none() {
        let result = parse_image_fit("none").unwrap();
        assert!(matches!(result, ImageFit::None));
    }

    #[test_log::test]
    fn parse_image_fit_returns_error_for_invalid() {
        let result = parse_image_fit("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_image_loading
    #[test_log::test]
    fn parse_image_loading_parses_eager() {
        let result = parse_image_loading("eager").unwrap();
        assert!(matches!(result, ImageLoading::Eager));
    }

    #[test_log::test]
    fn parse_image_loading_parses_lazy() {
        let result = parse_image_loading("lazy").unwrap();
        assert!(matches!(result, ImageLoading::Lazy));
    }

    #[test_log::test]
    fn parse_image_loading_returns_error_for_invalid() {
        let result = parse_image_loading("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_bool
    #[test_log::test]
    fn parse_bool_parses_true() {
        let result = parse_bool("true").unwrap();
        assert!(result);
    }

    #[test_log::test]
    fn parse_bool_parses_false() {
        let result = parse_bool("false").unwrap();
        assert!(!result);
    }

    #[test_log::test]
    fn parse_bool_parses_empty_as_true() {
        let result = parse_bool("").unwrap();
        assert!(result);
    }

    #[test_log::test]
    fn parse_bool_returns_error_for_invalid() {
        let result = parse_bool("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_visibility
    #[test_log::test]
    fn parse_visibility_parses_visible() {
        let result = parse_visibility("visible").unwrap();
        assert!(matches!(result, Visibility::Visible));
    }

    #[test_log::test]
    fn parse_visibility_parses_hidden() {
        let result = parse_visibility("hidden").unwrap();
        assert!(matches!(result, Visibility::Hidden));
    }

    #[test_log::test]
    fn parse_visibility_returns_error_for_invalid() {
        let result = parse_visibility("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_direction
    #[test_log::test]
    fn parse_direction_parses_row() {
        let result = parse_direction("row").unwrap();
        assert!(matches!(result, LayoutDirection::Row));
    }

    #[test_log::test]
    fn parse_direction_parses_col() {
        let result = parse_direction("col").unwrap();
        assert!(matches!(result, LayoutDirection::Column));
    }

    #[test_log::test]
    fn parse_direction_returns_error_for_invalid() {
        let result = parse_direction("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_cursor
    #[test_log::test]
    fn parse_cursor_parses_auto() {
        let result = parse_cursor("auto").unwrap();
        assert!(matches!(result, Cursor::Auto));
    }

    #[test_log::test]
    fn parse_cursor_parses_pointer() {
        let result = parse_cursor("pointer").unwrap();
        assert!(matches!(result, Cursor::Pointer));
    }

    #[test_log::test]
    fn parse_cursor_parses_grab() {
        let result = parse_cursor("grab").unwrap();
        assert!(matches!(result, Cursor::Grab));
    }

    #[test_log::test]
    fn parse_cursor_parses_grabbing() {
        let result = parse_cursor("grabbing").unwrap();
        assert!(matches!(result, Cursor::Grabbing));
    }

    #[test_log::test]
    fn parse_cursor_returns_error_for_invalid() {
        let result = parse_cursor("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_position
    #[test_log::test]
    fn parse_position_parses_static() {
        let result = parse_position("static").unwrap();
        assert!(matches!(result, Position::Static));
    }

    #[test_log::test]
    fn parse_position_parses_sticky() {
        let result = parse_position("sticky").unwrap();
        assert!(matches!(result, Position::Sticky));
    }

    #[test_log::test]
    fn parse_position_parses_relative() {
        let result = parse_position("relative").unwrap();
        assert!(matches!(result, Position::Relative));
    }

    #[test_log::test]
    fn parse_position_parses_absolute() {
        let result = parse_position("absolute").unwrap();
        assert!(matches!(result, Position::Absolute));
    }

    #[test_log::test]
    fn parse_position_parses_fixed() {
        let result = parse_position("fixed").unwrap();
        assert!(matches!(result, Position::Fixed));
    }

    #[test_log::test]
    fn parse_position_returns_error_for_invalid() {
        let result = parse_position("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_text_decoration_line
    #[test_log::test]
    fn parse_text_decoration_line_parses_inherit() {
        let result = parse_text_decoration_line("inherit").unwrap();
        assert!(matches!(result, TextDecorationLine::Inherit));
    }

    #[test_log::test]
    fn parse_text_decoration_line_parses_none() {
        let result = parse_text_decoration_line("none").unwrap();
        assert!(matches!(result, TextDecorationLine::None));
    }

    #[test_log::test]
    fn parse_text_decoration_line_parses_underline() {
        let result = parse_text_decoration_line("underline").unwrap();
        assert!(matches!(result, TextDecorationLine::Underline));
    }

    #[test_log::test]
    fn parse_text_decoration_line_parses_overline() {
        let result = parse_text_decoration_line("overline").unwrap();
        assert!(matches!(result, TextDecorationLine::Overline));
    }

    #[test_log::test]
    fn parse_text_decoration_line_parses_line_through() {
        let result = parse_text_decoration_line("line-through").unwrap();
        assert!(matches!(result, TextDecorationLine::LineThrough));
    }

    #[test_log::test]
    fn parse_text_decoration_line_returns_error_for_invalid() {
        let result = parse_text_decoration_line("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_text_decoration_style
    #[test_log::test]
    fn parse_text_decoration_style_parses_inherit() {
        let result = parse_text_decoration_style("inherit").unwrap();
        assert!(matches!(result, TextDecorationStyle::Inherit));
    }

    #[test_log::test]
    fn parse_text_decoration_style_parses_solid() {
        let result = parse_text_decoration_style("solid").unwrap();
        assert!(matches!(result, TextDecorationStyle::Solid));
    }

    #[test_log::test]
    fn parse_text_decoration_style_parses_double() {
        let result = parse_text_decoration_style("double").unwrap();
        assert!(matches!(result, TextDecorationStyle::Double));
    }

    #[test_log::test]
    fn parse_text_decoration_style_parses_dotted() {
        let result = parse_text_decoration_style("dotted").unwrap();
        assert!(matches!(result, TextDecorationStyle::Dotted));
    }

    #[test_log::test]
    fn parse_text_decoration_style_parses_dashed() {
        let result = parse_text_decoration_style("dashed").unwrap();
        assert!(matches!(result, TextDecorationStyle::Dashed));
    }

    #[test_log::test]
    fn parse_text_decoration_style_parses_wavy() {
        let result = parse_text_decoration_style("wavy").unwrap();
        assert!(matches!(result, TextDecorationStyle::Wavy));
    }

    #[test_log::test]
    fn parse_text_decoration_style_returns_error_for_invalid() {
        let result = parse_text_decoration_style("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_text_decoration_lines
    #[test_log::test]
    fn parse_text_decoration_lines_parses_single() {
        let result = parse_text_decoration_lines("underline").unwrap();
        assert_eq!(result, vec![TextDecorationLine::Underline]);
    }

    #[test_log::test]
    fn parse_text_decoration_lines_parses_multiple() {
        let result = parse_text_decoration_lines("underline overline line-through").unwrap();
        assert_eq!(
            result,
            vec![
                TextDecorationLine::Underline,
                TextDecorationLine::Overline,
                TextDecorationLine::LineThrough
            ]
        );
    }

    #[test_log::test]
    fn parse_text_decoration_lines_returns_error_for_invalid() {
        let result = parse_text_decoration_lines("underline invalid");
        assert!(result.is_err());
    }

    // Tests for parse_justify_content
    #[test_log::test]
    fn parse_justify_content_parses_start() {
        let result = parse_justify_content("start").unwrap();
        assert!(matches!(result, JustifyContent::Start));
    }

    #[test_log::test]
    fn parse_justify_content_parses_center() {
        let result = parse_justify_content("center").unwrap();
        assert!(matches!(result, JustifyContent::Center));
    }

    #[test_log::test]
    fn parse_justify_content_parses_end() {
        let result = parse_justify_content("end").unwrap();
        assert!(matches!(result, JustifyContent::End));
    }

    #[test_log::test]
    fn parse_justify_content_parses_space_between() {
        let result = parse_justify_content("space-between").unwrap();
        assert!(matches!(result, JustifyContent::SpaceBetween));
    }

    #[test_log::test]
    fn parse_justify_content_parses_space_evenly() {
        let result = parse_justify_content("space-evenly").unwrap();
        assert!(matches!(result, JustifyContent::SpaceEvenly));
    }

    #[test_log::test]
    fn parse_justify_content_returns_error_for_invalid() {
        let result = parse_justify_content("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_align_items
    #[test_log::test]
    fn parse_align_items_parses_start() {
        let result = parse_align_items("start").unwrap();
        assert!(matches!(result, AlignItems::Start));
    }

    #[test_log::test]
    fn parse_align_items_parses_center() {
        let result = parse_align_items("center").unwrap();
        assert!(matches!(result, AlignItems::Center));
    }

    #[test_log::test]
    fn parse_align_items_parses_end() {
        let result = parse_align_items("end").unwrap();
        assert!(matches!(result, AlignItems::End));
    }

    #[test_log::test]
    fn parse_align_items_returns_error_for_invalid() {
        let result = parse_align_items("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_text_align
    #[test_log::test]
    fn parse_text_align_parses_start() {
        let result = parse_text_align("start").unwrap();
        assert!(matches!(result, TextAlign::Start));
    }

    #[test_log::test]
    fn parse_text_align_parses_center() {
        let result = parse_text_align("center").unwrap();
        assert!(matches!(result, TextAlign::Center));
    }

    #[test_log::test]
    fn parse_text_align_parses_end() {
        let result = parse_text_align("end").unwrap();
        assert!(matches!(result, TextAlign::End));
    }

    #[test_log::test]
    fn parse_text_align_parses_justify() {
        let result = parse_text_align("justify").unwrap();
        assert!(matches!(result, TextAlign::Justify));
    }

    #[test_log::test]
    fn parse_text_align_returns_error_for_invalid() {
        let result = parse_text_align("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_white_space
    #[test_log::test]
    fn parse_white_space_parses_normal() {
        let result = parse_white_space("normal").unwrap();
        assert!(matches!(result, WhiteSpace::Normal));
    }

    #[test_log::test]
    fn parse_white_space_parses_preserve() {
        let result = parse_white_space("preserve").unwrap();
        assert!(matches!(result, WhiteSpace::Preserve));
    }

    #[test_log::test]
    fn parse_white_space_parses_pre() {
        let result = parse_white_space("pre").unwrap();
        assert!(matches!(result, WhiteSpace::Preserve));
    }

    #[test_log::test]
    fn parse_white_space_parses_preserve_wrap() {
        let result = parse_white_space("preserve-wrap").unwrap();
        assert!(matches!(result, WhiteSpace::PreserveWrap));
    }

    #[test_log::test]
    fn parse_white_space_parses_pre_wrap() {
        let result = parse_white_space("pre-wrap").unwrap();
        assert!(matches!(result, WhiteSpace::PreserveWrap));
    }

    #[test_log::test]
    fn parse_white_space_returns_error_for_invalid() {
        let result = parse_white_space("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_user_select
    #[test_log::test]
    fn parse_user_select_parses_auto() {
        let result = parse_user_select("auto").unwrap();
        assert!(matches!(result, UserSelect::Auto));
    }

    #[test_log::test]
    fn parse_user_select_parses_none() {
        let result = parse_user_select("none").unwrap();
        assert!(matches!(result, UserSelect::None));
    }

    #[test_log::test]
    fn parse_user_select_parses_text() {
        let result = parse_user_select("text").unwrap();
        assert!(matches!(result, UserSelect::Text));
    }

    #[test_log::test]
    fn parse_user_select_parses_all() {
        let result = parse_user_select("all").unwrap();
        assert!(matches!(result, UserSelect::All));
    }

    #[test_log::test]
    fn parse_user_select_returns_error_for_invalid() {
        let result = parse_user_select("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_overflow_wrap
    #[test_log::test]
    fn parse_overflow_wrap_parses_normal() {
        let result = parse_overflow_wrap("normal").unwrap();
        assert!(matches!(result, OverflowWrap::Normal));
    }

    #[test_log::test]
    fn parse_overflow_wrap_parses_break_word() {
        let result = parse_overflow_wrap("break-word").unwrap();
        assert!(matches!(result, OverflowWrap::BreakWord));
    }

    #[test_log::test]
    fn parse_overflow_wrap_parses_anywhere() {
        let result = parse_overflow_wrap("anywhere").unwrap();
        assert!(matches!(result, OverflowWrap::Anywhere));
    }

    #[test_log::test]
    fn parse_overflow_wrap_returns_error_for_invalid() {
        let result = parse_overflow_wrap("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_text_overflow
    #[test_log::test]
    fn parse_text_overflow_parses_clip() {
        let result = parse_text_overflow("clip").unwrap();
        assert!(matches!(result, TextOverflow::Clip));
    }

    #[test_log::test]
    fn parse_text_overflow_parses_ellipsis() {
        let result = parse_text_overflow("ellipsis").unwrap();
        assert!(matches!(result, TextOverflow::Ellipsis));
    }

    #[test_log::test]
    fn parse_text_overflow_returns_error_for_invalid() {
        let result = parse_text_overflow("invalid");
        assert!(result.is_err());
    }

    // Tests for parse_font_weight
    #[test_log::test]
    fn parse_font_weight_parses_normal() {
        let result = parse_font_weight("normal").unwrap();
        assert!(matches!(result, FontWeight::Normal));
    }

    #[test_log::test]
    fn parse_font_weight_parses_bold() {
        let result = parse_font_weight("bold").unwrap();
        assert!(matches!(result, FontWeight::Bold));
    }

    #[test_log::test]
    fn parse_font_weight_parses_lighter() {
        let result = parse_font_weight("lighter").unwrap();
        assert!(matches!(result, FontWeight::Lighter));
    }

    #[test_log::test]
    fn parse_font_weight_parses_bolder() {
        let result = parse_font_weight("bolder").unwrap();
        assert!(matches!(result, FontWeight::Bolder));
    }

    #[test_log::test]
    fn parse_font_weight_parses_numeric_100() {
        let result = parse_font_weight("100").unwrap();
        assert!(matches!(result, FontWeight::Weight100));
    }

    #[test_log::test]
    fn parse_font_weight_parses_numeric_400() {
        let result = parse_font_weight("400").unwrap();
        assert!(matches!(result, FontWeight::Weight400));
    }

    #[test_log::test]
    fn parse_font_weight_parses_numeric_700() {
        let result = parse_font_weight("700").unwrap();
        assert!(matches!(result, FontWeight::Weight700));
    }

    #[test_log::test]
    fn parse_font_weight_parses_numeric_900() {
        let result = parse_font_weight("900").unwrap();
        assert!(matches!(result, FontWeight::Weight900));
    }

    #[test_log::test]
    fn parse_font_weight_parses_thin() {
        let result = parse_font_weight("thin").unwrap();
        assert!(matches!(result, FontWeight::Thin));
    }

    #[test_log::test]
    fn parse_font_weight_parses_extra_light() {
        let result = parse_font_weight("extra-light").unwrap();
        assert!(matches!(result, FontWeight::ExtraLight));
    }

    #[test_log::test]
    fn parse_font_weight_parses_extralight() {
        let result = parse_font_weight("extralight").unwrap();
        assert!(matches!(result, FontWeight::ExtraLight));
    }

    #[test_log::test]
    fn parse_font_weight_parses_light() {
        let result = parse_font_weight("light").unwrap();
        assert!(matches!(result, FontWeight::Light));
    }

    #[test_log::test]
    fn parse_font_weight_parses_medium() {
        let result = parse_font_weight("medium").unwrap();
        assert!(matches!(result, FontWeight::Medium));
    }

    #[test_log::test]
    fn parse_font_weight_parses_semi_bold() {
        let result = parse_font_weight("semi-bold").unwrap();
        assert!(matches!(result, FontWeight::SemiBold));
    }

    #[test_log::test]
    fn parse_font_weight_parses_semibold() {
        let result = parse_font_weight("semibold").unwrap();
        assert!(matches!(result, FontWeight::SemiBold));
    }

    #[test_log::test]
    fn parse_font_weight_parses_extra_bold() {
        let result = parse_font_weight("extra-bold").unwrap();
        assert!(matches!(result, FontWeight::ExtraBold));
    }

    #[test_log::test]
    fn parse_font_weight_parses_extrabold() {
        let result = parse_font_weight("extrabold").unwrap();
        assert!(matches!(result, FontWeight::ExtraBold));
    }

    #[test_log::test]
    fn parse_font_weight_parses_black() {
        let result = parse_font_weight("black").unwrap();
        assert!(matches!(result, FontWeight::Black));
    }

    #[test_log::test]
    fn parse_font_weight_returns_error_for_invalid() {
        let result = parse_font_weight("invalid");
        assert!(result.is_err());
    }
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
mod test {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use crate::Container;
    use hyperchad_transformer_models::FontWeight;

    /// Module for diff generation between Container values.
    ///
    /// Uses the existing `PartialEq` implementations which already handle semantic
    /// equality for `Number` types (e.g., `Real(0.0)` == `Integer(0)`).
    mod semantic_diff {
        use crate::Container;

        /// A difference found between two Containers.
        #[derive(Debug)]
        pub struct Diff {
            pub path: String,
            pub left: String,
            pub right: String,
        }

        impl std::fmt::Display for Diff {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}: {} vs {}", self.path, self.left, self.right)
            }
        }

        /// Collect all differences between two Containers.
        pub fn diff_containers(left: &Container, right: &Container) -> Vec<Diff> {
            diff_containers_inner(left, right, String::new())
        }

        fn diff_containers_inner(left: &Container, right: &Container, prefix: String) -> Vec<Diff> {
            let mut diffs = Vec::new();

            macro_rules! check_field {
                ($field:ident) => {
                    if left.$field != right.$field {
                        diffs.push(Diff {
                            path: if prefix.is_empty() {
                                stringify!($field).to_string()
                            } else {
                                format!("{}.{}", prefix, stringify!($field))
                            },
                            left: format!("{:?}", left.$field),
                            right: format!("{:?}", right.$field),
                        });
                    }
                };
            }

            // All fields use the same check - PartialEq handles semantic equality
            check_field!(id);
            check_field!(str_id);
            check_field!(classes);
            check_field!(data);
            check_field!(element);
            check_field!(direction);
            check_field!(overflow_x);
            check_field!(overflow_y);
            check_field!(grid_cell_size);
            check_field!(justify_content);
            check_field!(align_items);
            check_field!(text_align);
            check_field!(white_space);
            check_field!(text_decoration);
            check_field!(font_family);
            check_field!(font_weight);
            check_field!(width);
            check_field!(min_width);
            check_field!(max_width);
            check_field!(height);
            check_field!(min_height);
            check_field!(max_height);
            check_field!(flex);
            check_field!(column_gap);
            check_field!(row_gap);
            check_field!(opacity);
            check_field!(left);
            check_field!(right);
            check_field!(top);
            check_field!(bottom);
            check_field!(translate_x);
            check_field!(translate_y);
            check_field!(cursor);
            check_field!(user_select);
            check_field!(overflow_wrap);
            check_field!(text_overflow);
            check_field!(position);
            check_field!(background);
            check_field!(border_top);
            check_field!(border_right);
            check_field!(border_bottom);
            check_field!(border_left);
            check_field!(border_top_left_radius);
            check_field!(border_top_right_radius);
            check_field!(border_bottom_left_radius);
            check_field!(border_bottom_right_radius);
            check_field!(margin_left);
            check_field!(margin_right);
            check_field!(margin_top);
            check_field!(margin_bottom);
            check_field!(padding_left);
            check_field!(padding_right);
            check_field!(padding_top);
            check_field!(padding_bottom);
            check_field!(font_size);
            check_field!(color);
            check_field!(state);
            check_field!(route);
            check_field!(hidden);
            check_field!(debug);
            check_field!(visibility);

            // Actions - compare element by element for better diff reporting
            if left.actions.len() == right.actions.len() {
                for (i, (la, ra)) in left.actions.iter().zip(&right.actions).enumerate() {
                    if la != ra {
                        diffs.push(Diff {
                            path: if prefix.is_empty() {
                                format!("actions[{i}]")
                            } else {
                                format!("{prefix}.actions[{i}]")
                            },
                            left: format!("{la:?}"),
                            right: format!("{ra:?}"),
                        });
                    }
                }
            } else {
                diffs.push(Diff {
                    path: if prefix.is_empty() {
                        "actions.len".to_string()
                    } else {
                        format!("{prefix}.actions.len")
                    },
                    left: format!("{}", left.actions.len()),
                    right: format!("{}", right.actions.len()),
                });
            }

            // Overrides - compare element by element for better diff reporting
            if left.overrides.len() == right.overrides.len() {
                for (i, (lo, ro)) in left.overrides.iter().zip(&right.overrides).enumerate() {
                    if lo != ro {
                        diffs.push(Diff {
                            path: if prefix.is_empty() {
                                format!("overrides[{i}]")
                            } else {
                                format!("{prefix}.overrides[{i}]")
                            },
                            left: format!("{lo:?}"),
                            right: format!("{ro:?}"),
                        });
                    }
                }
            } else {
                diffs.push(Diff {
                    path: if prefix.is_empty() {
                        "overrides.len".to_string()
                    } else {
                        format!("{prefix}.overrides.len")
                    },
                    left: format!("{}", left.overrides.len()),
                    right: format!("{}", right.overrides.len()),
                });
            }

            // Children - recurse for detailed diff reporting
            if left.children.len() == right.children.len() {
                for (i, (lc, rc)) in left.children.iter().zip(&right.children).enumerate() {
                    let child_prefix = if prefix.is_empty() {
                        format!("children[{i}]")
                    } else {
                        format!("{prefix}.children[{i}]")
                    };
                    diffs.extend(diff_containers_inner(lc, rc, child_prefix));
                }
            } else {
                diffs.push(Diff {
                    path: if prefix.is_empty() {
                        "children.len".to_string()
                    } else {
                        format!("{prefix}.children.len")
                    },
                    left: format!("{}", left.children.len()),
                    right: format!("{}", right.children.len()),
                });
            }

            diffs
        }
    }

    fn clean_up_container(container: &mut Container) {
        container.id = 0;

        // Overrides are only serialized when logic feature is enabled
        #[cfg(not(feature = "logic"))]
        container.overrides.clear();

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

        let mut i = 0;
        let overrides = container.overrides.clone();
        container.overrides.retain(|x| {
            i += 1;
            overrides
                .iter()
                .take(i - 1)
                .all(|prev| prev.condition != x.condition)
        });
        container
            .overrides
            .sort_by(|a, b| format!("{:?}", a.condition).cmp(&format!("{:?}", b.condition)));

        // Normalize override defaults to match base values.
        // During serialization, if config.default is None, the base value is used.
        // This ensures round-trip consistency.
        // We collect the updates first to avoid borrow conflicts.
        let default_updates: Vec<_> = container
            .overrides
            .iter()
            .enumerate()
            .filter_map(|(i, config)| {
                config.overrides.first().map(|first| {
                    let base_value = container.get_base_value_for_override(first);
                    (i, base_value)
                })
            })
            .collect();

        for (i, default) in default_updates {
            container.overrides[i].default = default;
        }

        // Clear children for elements that don't allow them
        if !container.element.allows_children() {
            container.children.clear();
        }

        for child in &mut container.children {
            clean_up_container(child);
        }
    }

    proptest! {
        #[test_log::test]
        fn display_can_display_and_be_parsed_back_to_original_container(
            mut container: Container,
        ) {
            log::trace!("container:\n{container}");
            clean_up_container(&mut container);

            let markup = container
                .display_to_string(
                    true,
                    false,
                    #[cfg(feature = "format")]
                    false,
                    #[cfg(feature = "syntax-highlighting")]
                    false,
                )
                .unwrap();
            log::trace!("markup:\n{markup}");

            let re_parsed: Container = markup.clone().try_into().unwrap();
            log::trace!("re_parsed:\n{re_parsed}");

            let Some(mut re_parsed) = re_parsed.children.first().cloned() else {
                panic!("failed to get child from markup: {markup} ({container:?})");
            };

            clean_up_container(&mut re_parsed);

            // Use semantic diff to find actual differences
            // (ignores equivalent Number variants like Integer(0) vs Real(0.0))
            let diffs = semantic_diff::diff_containers(&re_parsed, &container);

            if !diffs.is_empty() {
                // Print a clean summary of semantic differences
                log::debug!("\n=== SEMANTIC DIFFERENCES FOUND ===");
                log::debug!("Markup:\n{markup}");
                let diff_count = diffs.len();
                log::debug!("\n--- Differences ({diff_count} total) ---");
                for diff in &diffs {
                    log::debug!("  {diff}");
                }
                log::debug!("=================================\n");

                // Fail the test with the first difference
                prop_assert!(
                    false,
                    "Container round-trip failed with {diff_count} semantic difference(s). First: {}",
                    diffs[0]
                );
            }
        }
    }

    #[test]
    fn test_font_weight_parsing() {
        let html = r#"<div sx-font-weight="bold">Bold text</div>"#;
        let container: Container = html.try_into().unwrap();
        let child = &container.children[0];
        assert_eq!(child.font_weight, Some(FontWeight::Bold));

        let html = r#"<div sx-font-weight="400">Normal text</div>"#;
        let container: Container = html.try_into().unwrap();
        let child = &container.children[0];
        assert_eq!(child.font_weight, Some(FontWeight::Weight400));

        let html = r#"<div sx-font-weight="lighter">Lighter text</div>"#;
        let container: Container = html.try_into().unwrap();
        let child = &container.children[0];
        assert_eq!(child.font_weight, Some(FontWeight::Lighter));
    }

    #[test]
    fn test_textarea_value_from_text_content() {
        let html = r#"<textarea name="message">Hello World</textarea>"#;
        let container: Container = html.try_into().unwrap();
        let child = &container.children[0];

        if let crate::Element::Textarea { value, name, .. } = &child.element {
            assert_eq!(value, "Hello World");
            assert_eq!(name.as_deref(), Some("message"));
        } else {
            panic!("Expected Textarea element");
        }

        let html = r#"<textarea placeholder="Enter text"></textarea>"#;
        let container: Container = html.try_into().unwrap();
        let child = &container.children[0];

        if let crate::Element::Textarea {
            value, placeholder, ..
        } = &child.element
        {
            assert_eq!(value, "");
            assert_eq!(placeholder.as_deref(), Some("Enter text"));
        } else {
            panic!("Expected Textarea element");
        }

        let html = r#"<textarea rows="10" cols="50">Line 1
Line 2
Line 3</textarea>"#;
        let container: Container = html.try_into().unwrap();
        let child = &container.children[0];

        if let crate::Element::Textarea { value, .. } = &child.element {
            assert_eq!(value, "Line 1\nLine 2\nLine 3");
        } else {
            panic!("Expected Textarea element");
        }
    }

    #[test]
    fn test_textarea_html_escaping() {
        let textarea = Container {
            element: crate::Element::Textarea {
                value: "Test < > & \" value".to_string(),
                placeholder: None,
                name: None,
                rows: None,
                cols: None,
            },
            ..Default::default()
        };

        let html = textarea
            .display_to_string(
                false,
                false,
                #[cfg(feature = "format")]
                false,
                #[cfg(feature = "syntax-highlighting")]
                false,
            )
            .unwrap();
        assert!(html.contains("&lt;"));
        assert!(html.contains("&gt;"));
        assert!(html.contains("&amp;"));

        let parsed: Container = html.as_str().try_into().unwrap();
        let child = &parsed.children[0];
        if let crate::Element::Textarea { value, .. } = &child.element {
            assert_eq!(value, "Test < > & \" value");
        } else {
            panic!("Expected Textarea element, got: {:?}", child.element);
        }
    }
}
