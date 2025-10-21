use std::{borrow::Cow, collections::BTreeMap, iter::once};

use hyperchad_actions::{Action, ActionEffect, ActionTrigger, ActionType};
use hyperchad_color::{Color, ParseHexError};
use hyperchad_transformer_models::{
    AlignItems, Cursor, FontWeight, ImageFit, ImageLoading, JustifyContent, LayoutDirection,
    LayoutOverflow, LinkTarget, Position, Route, SwapTarget, TextAlign, TextDecorationLine,
    TextDecorationStyle, Visibility, WhiteSpace,
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

#[derive(Debug, Error)]
pub enum ParseAttrWrapperError {
    #[error("Invalid attr value for {name}: '{error:?}'")]
    Parse { name: String, error: ParseAttrError },
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

fn parse_swap(value: &str) -> Result<SwapTarget, ParseAttrError> {
    Ok(match value {
        "this" | "self" => SwapTarget::This,
        "children" => SwapTarget::Children,
        value => {
            if let Some(value) = value.strip_prefix('#') {
                SwapTarget::Id(value.to_string())
            } else {
                return Err(ParseAttrError::InvalidValue(value.to_string()));
            }
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

// TODO: Doesn't support reactive values
fn parse_put_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Put {
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
fn parse_delete_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Delete {
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
fn parse_patch_route(value: &str, tag: &HTMLTag) -> Result<Route, ParseAttrError> {
    Ok(Route::Patch {
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
    use hyperchad_transformer_models::FontWeight;

    fn clean_up_container(container: &mut Container) {
        container.id = 0;

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

        // Clear children for elements that don't allow them
        if !container.element.allows_children() {
            container.children.clear();
        }

        for child in &mut container.children {
            clean_up_container(child);
        }
    }

    #[quickcheck]
    fn display_can_display_and_be_parsed_back_to_original_container(
        mut container: Container,
    ) -> bool {
        log::trace!("container:\n{container:?}");
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
        log::trace!("re_parsed: {re_parsed} ({re_parsed:?})");

        let Some(mut re_parsed) = re_parsed.children.first().cloned() else {
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
