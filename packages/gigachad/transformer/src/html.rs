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

fn get_overflow(tag: &HTMLTag, name: &str) -> LayoutOverflow {
    match get_tag_attr_value_lower(tag, name).as_deref() {
        Some("wrap") => LayoutOverflow::Wrap,
        Some("scroll") => LayoutOverflow::Scroll,
        Some("show") => LayoutOverflow::Show,
        Some("auto") => LayoutOverflow::Auto,
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

fn parse_element(
    tag: &HTMLTag<'_>,
    node: &Node<'_>,
    parser: &Parser<'_>,
) -> crate::ContainerElement {
    #[cfg(feature = "id")]
    static CURRENT_ID: std::sync::LazyLock<std::sync::Arc<std::sync::atomic::AtomicUsize>> =
        std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)));

    #[allow(clippy::needless_update)]
    crate::ContainerElement {
        #[cfg(feature = "id")]
        id: CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        direction: get_direction(tag),
        overflow_x: get_overflow(tag, "sx-overflow-x"),
        overflow_y: get_overflow(tag, "sx-overflow-y"),
        elements: parse_top_children(node.children(), parser),
        width: get_number(tag, "sx-width").ok(),
        height: get_number(tag, "sx-height").ok(),
        ..Default::default()
    }
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
