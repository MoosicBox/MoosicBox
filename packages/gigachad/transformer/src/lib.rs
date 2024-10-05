#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{fmt::Display, io::Write};

use itertools::Itertools;

#[cfg(feature = "calc")]
pub mod calc;
#[cfg(feature = "html")]
pub mod html;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Number {
    Real(f32),
    Integer(u64),
    RealPercent(f32),
    IntegerPercent(u64),
}

impl Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Real(x) => f.write_fmt(format_args!("{x}")),
            Self::Integer(x) => f.write_fmt(format_args!("{x}")),
            Self::RealPercent(x) => f.write_fmt(format_args!("{x}%")),
            Self::IntegerPercent(x) => f.write_fmt(format_args!("{x}%")),
        }
    }
}

impl Default for Number {
    fn default() -> Self {
        Self::Integer(0)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    Row,
    #[default]
    Column,
}

impl Display for LayoutDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Row => f.write_str("Row"),
            Self::Column => f.write_str("Column"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LayoutOverflow {
    Scroll,
    #[default]
    Squash,
    Wrap,
}

#[cfg(feature = "calc")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LayoutPosition {
    Wrap {
        row: u32,
        col: u32,
    },
    #[default]
    Default,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContainerElement {
    pub elements: Vec<Element>,
    pub direction: LayoutDirection,
    pub overflow: LayoutOverflow,
    pub width: Option<Number>,
    pub height: Option<Number>,
    #[cfg(feature = "calc")]
    pub calculated_width: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_height: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_x: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_y: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_position: Option<LayoutPosition>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Element {
    Raw {
        value: String,
    },
    Div {
        element: ContainerElement,
    },
    Aside {
        element: ContainerElement,
    },
    Main {
        element: ContainerElement,
    },
    Header {
        element: ContainerElement,
    },
    Footer {
        element: ContainerElement,
    },
    Section {
        element: ContainerElement,
    },
    Form {
        element: ContainerElement,
    },
    Span {
        element: ContainerElement,
    },
    Input(Input),
    Button {
        element: ContainerElement,
    },
    Image {
        source: Option<String>,
        element: ContainerElement,
    },
    Anchor {
        element: ContainerElement,
        href: Option<String>,
    },
    Heading {
        element: ContainerElement,
        size: HeaderSize,
    },
    UnorderedList {
        element: ContainerElement,
    },
    OrderedList {
        element: ContainerElement,
    },
    ListItem {
        element: ContainerElement,
    },
}

#[derive(Default)]
struct Attrs {
    values: Vec<(String, Box<dyn Display>)>,
}

impl Attrs {
    fn new() -> Self {
        Self::default()
    }

    #[allow(unused)]
    fn with_attr<K: Into<String>, V: Display + 'static>(mut self, name: K, value: V) -> Self {
        self.add(name, value);
        self
    }

    fn with_attr_opt<K: Into<String>, V: Display + 'static>(
        mut self,
        name: K,
        value: Option<V>,
    ) -> Self {
        self.add_opt(name, value);
        self
    }

    fn to_string_pad_left(&self) -> String {
        if self.values.is_empty() {
            String::new()
        } else {
            format!(
                " {}",
                self.values
                    .iter()
                    .map(|(name, value)| format!("{name}=\"{value}\""))
                    .collect_vec()
                    .join(" ")
            )
        }
    }

    fn add<K: Into<String>, V: Display + 'static>(&mut self, name: K, value: V) {
        self.values.push((name.into(), Box::new(value)));
    }

    fn add_opt<K: Into<String>, V: Display + 'static>(&mut self, name: K, value: Option<V>) {
        if let Some(value) = value {
            self.values.push((name.into(), Box::new(value)));
        }
    }
}

impl ContainerElement {
    fn attrs(&self, with_debug_attrs: bool) -> Attrs {
        let mut attrs = Attrs { values: vec![] };

        if self.direction == LayoutDirection::Row {
            attrs.add("sx-dir", "row");
        }

        if let Some(width) = self.width {
            attrs.add("sx-width", width);
        }
        if let Some(height) = self.height {
            attrs.add("sx-height", height);
        }

        if with_debug_attrs {
            attrs.add_opt("dbg-x", self.calculated_x);
            attrs.add_opt("dbg-y", self.calculated_y);
            attrs.add_opt("dbg-width", self.calculated_width);
            attrs.add_opt("dbg-height", self.calculated_height);
        }

        attrs
    }

    fn attrs_to_string_pad_left(&self, with_debug_attrs: bool) -> String {
        self.attrs(with_debug_attrs).to_string_pad_left()
    }
}

impl Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            &self
                .display_to_string(std::env::var("DEBUG_ATTRS").is_ok_and(|x| &x == "1"))
                .unwrap(),
        )?;

        Ok(())
    }
}

fn display_elements(
    elements: &[Element],
    f: &mut dyn Write,
    with_debug_attrs: bool,
) -> Result<(), std::io::Error> {
    for element in elements {
        element.display(f, with_debug_attrs)?;
    }

    Ok(())
}

impl Element {
    fn display_to_string(
        &self,
        with_debug_attrs: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut data = Vec::new();

        let _ = self.display(&mut data, with_debug_attrs);

        #[cfg(feature = "format")]
        let data = {
            use xml::{reader::ParserConfig, writer::EmitterConfig};
            let data: &[u8] = &data;

            let reader = ParserConfig::new()
                .trim_whitespace(true)
                .ignore_comments(false)
                .create_reader(data);

            let mut dest = Vec::new();

            let mut writer = EmitterConfig::new()
                .perform_indent(true)
                .normalize_empty_elements(false)
                .autopad_comments(false)
                .write_document_declaration(false)
                .create_writer(&mut dest);

            for event in reader {
                if let Some(event) = event?.as_writer_event() {
                    writer.write(event)?;
                }
            }

            dest
        };

        let pretty = String::from_utf8(data)?;
        let Some((_, pretty)) = pretty.split_once('\n') else {
            return Ok(pretty);
        };

        Ok(pretty.to_string())
    }

    #[allow(clippy::too_many_lines)]
    fn display(&self, f: &mut dyn Write, with_debug_attrs: bool) -> Result<(), std::io::Error> {
        match self {
            Self::Raw { value } => {
                f.write_fmt(format_args!("{value}"))?;
            }
            Self::Div { element } => {
                f.write_fmt(format_args!(
                    "<div{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</div>"))?;
            }
            Self::Aside { element } => {
                f.write_fmt(format_args!(
                    "<aside{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</aside>"))?;
            }

            Self::Main { element } => {
                f.write_fmt(format_args!(
                    "<main{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</main>"))?;
            }
            Self::Header { element } => {
                f.write_fmt(format_args!(
                    "<header{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</header>"))?;
            }
            Self::Footer { element } => {
                f.write_fmt(format_args!(
                    "<footer{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</footer>"))?;
            }
            Self::Section { element } => {
                f.write_fmt(format_args!(
                    "<section{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</section>"))?;
            }
            Self::Form { element } => {
                f.write_fmt(format_args!(
                    "<form{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</from>"))?;
            }
            Self::Span { element } => {
                f.write_fmt(format_args!(
                    "<span{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</span>"))?;
            }
            Self::Input(input) => {
                input.display(f, with_debug_attrs)?;
            }
            Self::Button { element } => {
                f.write_fmt(format_args!(
                    "<button{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</button>"))?;
            }
            Self::Image { source, element } => {
                f.write_fmt(format_args!(
                    "<img{src_attr}{attrs} />",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs),
                    src_attr = Attrs::new()
                        .with_attr_opt("src", source.to_owned())
                        .to_string_pad_left()
                ))?;
            }
            Self::Anchor { element, href } => {
                f.write_fmt(format_args!(
                    "<a{href_attr}{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs),
                    href_attr = Attrs::new()
                        .with_attr_opt("href", href.to_owned())
                        .to_string_pad_left(),
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</a>"))?;
            }
            Self::Heading { element, size } => {
                f.write_fmt(format_args!(
                    "<{size}{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</{size}>"))?;
            }
            Self::UnorderedList { element } => {
                f.write_fmt(format_args!(
                    "<ul{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</ul>"))?;
            }
            Self::OrderedList { element } => {
                f.write_fmt(format_args!(
                    "<ol{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</ol>"))?;
            }
            Self::ListItem { element } => {
                f.write_fmt(format_args!(
                    "<li{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</li>"))?;
            }
        }

        Ok(())
    }

    #[must_use]
    pub const fn tag_display_str(&self) -> &'static str {
        match self {
            Self::Raw { .. } => "Raw",
            Self::Div { .. } => "Div",
            Self::Aside { .. } => "Aside",
            Self::Main { .. } => "Main",
            Self::Header { .. } => "Header",
            Self::Footer { .. } => "Footer",
            Self::Section { .. } => "Section",
            Self::Form { .. } => "Form",
            Self::Span { .. } => "Span",
            Self::Input(_) => "Input",
            Self::Button { .. } => "Button",
            Self::Image { .. } => "Image",
            Self::Anchor { .. } => "Anchor",
            Self::Heading { .. } => "Heading",
            Self::UnorderedList { .. } => "UnorderedList",
            Self::OrderedList { .. } => "OrderedList",
            Self::ListItem { .. } => "ListItem",
        }
    }
}

impl Element {
    #[must_use]
    pub const fn container_element(&self) -> Option<&ContainerElement> {
        match self {
            Self::Div { element }
            | Self::Aside { element }
            | Self::Main { element }
            | Self::Header { element }
            | Self::Footer { element }
            | Self::Image { element, .. }
            | Self::Section { element }
            | Self::Form { element }
            | Self::Span { element }
            | Self::Button { element }
            | Self::Anchor { element, .. }
            | Self::Heading { element, .. }
            | Self::UnorderedList { element }
            | Self::OrderedList { element }
            | Self::ListItem { element } => Some(element),
            Self::Raw { .. } | Self::Input(_) => None,
        }
    }

    pub fn container_element_mut(&mut self) -> Option<&mut ContainerElement> {
        match self {
            Self::Div { element }
            | Self::Aside { element }
            | Self::Main { element }
            | Self::Header { element }
            | Self::Footer { element }
            | Self::Image { element, .. }
            | Self::Section { element }
            | Self::Form { element }
            | Self::Span { element }
            | Self::Button { element }
            | Self::Anchor { element, .. }
            | Self::Heading { element, .. }
            | Self::UnorderedList { element }
            | Self::OrderedList { element }
            | Self::ListItem { element } => Some(element),
            Self::Raw { .. } | Self::Input(_) => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HeaderSize {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl Display for HeaderSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::H1 => f.write_str("h1"),
            Self::H2 => f.write_str("h2"),
            Self::H3 => f.write_str("h3"),
            Self::H4 => f.write_str("h4"),
            Self::H5 => f.write_str("h5"),
            Self::H6 => f.write_str("h6"),
        }
    }
}

impl From<HeaderSize> for u8 {
    fn from(value: HeaderSize) -> Self {
        match value {
            HeaderSize::H1 => 1,
            HeaderSize::H2 => 2,
            HeaderSize::H3 => 3,
            HeaderSize::H4 => 4,
            HeaderSize::H5 => 5,
            HeaderSize::H6 => 6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Text {
        value: Option<String>,
        placeholder: Option<String>,
    },
    Password {
        value: Option<String>,
        placeholder: Option<String>,
    },
}

impl Input {
    fn display(&self, f: &mut dyn Write, _with_debug_attrs: bool) -> Result<(), std::io::Error> {
        match self {
            Self::Text { value, placeholder } => {
                let attrs = Attrs::new()
                    .with_attr_opt("value", value.to_owned())
                    .with_attr_opt("placeholder", placeholder.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"text\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))?;
            }
            Self::Password { value, placeholder } => {
                let attrs = Attrs::new()
                    .with_attr_opt("value", value.to_owned())
                    .with_attr_opt("placeholder", placeholder.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"password\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))?;
            }
        }

        Ok(())
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text { value, placeholder } => {
                let attrs = Attrs::new()
                    .with_attr_opt("value", value.to_owned())
                    .with_attr_opt("placeholder", placeholder.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"text\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))
            }
            Self::Password { value, placeholder } => {
                let attrs = Attrs::new()
                    .with_attr_opt("value", value.to_owned())
                    .with_attr_opt("placeholder", placeholder.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"password\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))
            }
        }
    }
}
