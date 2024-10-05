use std::borrow::Cow;

use thiserror::Error;
use tl::{Children, HTMLTag, Node, NodeHandle, ParseError, Parser, ParserOptions};

use crate::{LayoutDirection, LayoutOverflow, Number};

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

fn get_overflow(tag: &HTMLTag) -> LayoutOverflow {
    match get_tag_attr_value_lower(tag, "sx-overflow").as_deref() {
        Some("wrap") => LayoutOverflow::Wrap,
        Some("scroll") => LayoutOverflow::Scroll,
        _ => LayoutOverflow::default(),
    }
}

#[derive(Debug, Error)]
pub enum GetNumberError {
    #[error("Failed to parse number '{0}'")]
    Parse(String),
}

fn get_number(tag: &HTMLTag, name: &str) -> Result<Number, GetNumberError> {
    Ok(if let Some(number) = get_tag_attr_value(tag, name) {
        if let Some((number, _)) = number.split_once('%') {
            if number.contains('.') {
                Number::RealPercent(
                    number
                        .parse::<f32>()
                        .map_err(|_| GetNumberError::Parse(number.to_string()))?,
                )
            } else {
                Number::IntegerPercent(
                    number
                        .parse::<u64>()
                        .map_err(|_| GetNumberError::Parse(number.to_string()))?,
                )
            }
        } else if number.contains('.') {
            Number::Real(
                number
                    .parse::<f32>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        } else {
            Number::Integer(
                number
                    .parse::<u64>()
                    .map_err(|_| GetNumberError::Parse(number.to_string()))?,
            )
        }
    } else {
        return Err(GetNumberError::Parse(String::new()));
    })
}

#[allow(clippy::too_many_lines)]
fn parse_child(node: &Node<'_>, parser: &Parser<'_>) -> Option<crate::Element> {
    Some(match node {
        Node::Tag(tag) => match tag.name().as_utf8_str().to_lowercase().as_str() {
            "input" => match get_tag_attr_value_lower(tag, "type").as_deref() {
                Some("text") => crate::Element::Input(crate::Input::Text {
                    value: get_tag_attr_value_owned(tag, "value"),
                    placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                }),
                Some("password") => crate::Element::Input(crate::Input::Password {
                    value: get_tag_attr_value_owned(tag, "value"),
                    placeholder: get_tag_attr_value_owned(tag, "placeholder"),
                }),
                Some(_) | None => {
                    return None;
                }
            },
            "main" => crate::Element::Main {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "header" => crate::Element::Header {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "footer" => crate::Element::Footer {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "aside" => crate::Element::Aside {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "div" => crate::Element::Div {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "section" => crate::Element::Section {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "form" => crate::Element::Form {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "button" => crate::Element::Button {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "img" => crate::Element::Image {
                source: get_tag_attr_value_owned(tag, "src"),
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "a" => crate::Element::Anchor {
                href: get_tag_attr_value_owned(tag, "href"),
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "h1" => crate::Element::Heading {
                size: crate::HeaderSize::H1,
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "h2" => crate::Element::Heading {
                size: crate::HeaderSize::H2,
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "h3" => crate::Element::Heading {
                size: crate::HeaderSize::H3,
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "h4" => crate::Element::Heading {
                size: crate::HeaderSize::H4,
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "h5" => crate::Element::Heading {
                size: crate::HeaderSize::H5,
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "h6" => crate::Element::Heading {
                size: crate::HeaderSize::H6,
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "ul" => crate::Element::UnorderedList {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "ol" => crate::Element::OrderedList {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
            },
            "li" => crate::Element::ListItem {
                #[allow(clippy::needless_update)]
                element: crate::ContainerElement {
                    direction: get_direction(tag),
                    overflow: get_overflow(tag),
                    elements: parse_top_children(node.children(), parser),
                    width: get_number(tag, "sx-width").ok(),
                    height: get_number(tag, "sx-height").ok(),
                    ..Default::default()
                },
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
