use std::borrow::Cow;

use tl::{Children, HTMLTag, Node, NodeHandle, ParseError, Parser, ParserOptions};

impl TryFrom<String> for crate::ElementList {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl<'a> TryFrom<&'a str> for crate::ElementList {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let result = tl::parse(value, ParserOptions::new())?;

        Ok(crate::ElementList(parse_children(
            result.children(),
            result.parser(),
        )))
    }
}

fn parse_top_children(
    children: Option<Children<'_, '_>>,
    parser: &Parser<'_>,
) -> Vec<crate::Element> {
    if let Some(children) = children {
        parse_children(&children.top().to_vec(), parser)
    } else {
        vec![]
    }
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

fn parse_child(node: &Node<'_>, parser: &Parser<'_>) -> Option<crate::Element> {
    Some(match node {
        Node::Tag(tag) => match tag.name().as_utf8_str().to_lowercase().as_str() {
            "input" => match get_tag_attr_value_lower(tag, "type").as_deref() {
                Some("text") => crate::Element::Input(crate::Input::Text {
                    value: get_tag_attr_value_owned(tag, "value").unwrap_or_default(),
                    placeholder: get_tag_attr_value_owned(tag, "placeholder").unwrap_or_default(),
                }),
                Some("password") => crate::Element::Input(crate::Input::Password {
                    value: get_tag_attr_value_owned(tag, "value").unwrap_or_default(),
                    placeholder: get_tag_attr_value_owned(tag, "placeholder").unwrap_or_default(),
                }),
                Some(_) => {
                    return None;
                }
                None => {
                    return None;
                }
            },
            "main" => crate::Element::Main {
                elements: parse_top_children(node.children(), parser),
            },
            "header" => crate::Element::Header {
                elements: parse_top_children(node.children(), parser),
            },
            "footer" => crate::Element::Footer {
                elements: parse_top_children(node.children(), parser),
            },
            "aside" => crate::Element::Aside {
                elements: parse_top_children(node.children(), parser),
            },
            "div" => crate::Element::Div {
                elements: parse_top_children(node.children(), parser),
            },
            "section" => crate::Element::Section {
                elements: parse_top_children(node.children(), parser),
            },
            "form" => crate::Element::Form {
                elements: parse_top_children(node.children(), parser),
            },
            "button" => crate::Element::Button {
                elements: parse_top_children(node.children(), parser),
            },
            "img" => crate::Element::Image {
                source: get_tag_attr_value_owned(tag, "src"),
            },
            "a" => crate::Element::Anchor {
                elements: parse_top_children(node.children(), parser),
            },
            "h1" => crate::Element::Heading {
                size: crate::HeaderSize::H1,
                elements: parse_top_children(node.children(), parser),
            },
            "h2" => crate::Element::Heading {
                size: crate::HeaderSize::H2,
                elements: parse_top_children(node.children(), parser),
            },
            "h3" => crate::Element::Heading {
                size: crate::HeaderSize::H3,
                elements: parse_top_children(node.children(), parser),
            },
            "h4" => crate::Element::Heading {
                size: crate::HeaderSize::H4,
                elements: parse_top_children(node.children(), parser),
            },
            "h5" => crate::Element::Heading {
                size: crate::HeaderSize::H5,
                elements: parse_top_children(node.children(), parser),
            },
            "h6" => crate::Element::Heading {
                size: crate::HeaderSize::H6,
                elements: parse_top_children(node.children(), parser),
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
