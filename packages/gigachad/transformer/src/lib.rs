#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{fmt::Display, io::Write};

#[cfg(feature = "calc")]
pub mod calc;
#[cfg(feature = "html")]
pub mod html;

#[allow(clippy::module_name_repetitions)]
#[must_use]
pub fn calc_number(number: &Number, container: f32) -> f32 {
    match number {
        Number::Real(x) => *x,
        #[allow(clippy::cast_precision_loss)]
        Number::Integer(x) => *x as f32,
        Number::RealPercent(x) => container * (*x / 100.0),
        #[allow(clippy::cast_precision_loss)]
        Number::IntegerPercent(x) => container * (*x as f32 / 100.0),
        Number::Calc(x) => x.calc(container),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Calculation {
    Number(Box<Number>),
    Add(Box<Calculation>, Box<Calculation>),
    Subtract(Box<Calculation>, Box<Calculation>),
    Multiply(Box<Calculation>, Box<Calculation>),
    Divide(Box<Calculation>, Box<Calculation>),
}

impl Calculation {
    fn calc(&self, container: f32) -> f32 {
        match self {
            Self::Number(number) => calc_number(number, container),
            Self::Add(left, right) => left.calc(container) + right.calc(container),
            Self::Subtract(left, right) => left.calc(container) - right.calc(container),
            Self::Multiply(left, right) => left.calc(container) * right.calc(container),
            Self::Divide(left, right) => left.calc(container) / right.calc(container),
        }
    }
}

impl Display for Calculation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(number) => f.write_str(&number.to_string()),
            Self::Add(left, right) => f.write_fmt(format_args!("{left} + {right}")),
            Self::Subtract(left, right) => f.write_fmt(format_args!("{left} - {right}")),
            Self::Multiply(left, right) => f.write_fmt(format_args!("{left} * {right}")),
            Self::Divide(left, right) => f.write_fmt(format_args!("{left} / {right}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Number {
    Real(f32),
    Integer(u64),
    RealPercent(f32),
    IntegerPercent(u64),
    Calc(Calculation),
}

impl Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Real(x) => f.write_fmt(format_args!("{x}")),
            Self::Integer(x) => f.write_fmt(format_args!("{x}")),
            Self::RealPercent(x) => f.write_fmt(format_args!("{x}%")),
            Self::IntegerPercent(x) => f.write_fmt(format_args!("{x}%")),
            Self::Calc(x) => f.write_fmt(format_args!("calc({x})")),
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
    Auto,
    Scroll,
    Show,
    #[default]
    Squash,
    Wrap,
}

impl Display for LayoutOverflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("Auto"),
            Self::Scroll => f.write_str("Scroll"),
            Self::Show => f.write_str("Show"),
            Self::Squash => f.write_str("Squash"),
            Self::Wrap => f.write_str("Wrap"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum JustifyContent {
    SpaceEvenly,
    #[default]
    Default,
}

impl Display for JustifyContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpaceEvenly => f.write_str("SpaceEvenly"),
            Self::Default => f.write_str("Default"),
        }
    }
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

#[cfg(feature = "calc")]
impl LayoutPosition {
    #[must_use]
    pub const fn row(&self) -> Option<u32> {
        match self {
            Self::Wrap { row, .. } => Some(*row),
            Self::Default => None,
        }
    }

    #[must_use]
    pub const fn column(&self) -> Option<u32> {
        match self {
            Self::Wrap { col, .. } => Some(*col),
            Self::Default => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Get {
        route: String,
        trigger: Option<String>,
    },
    Post {
        route: String,
        trigger: Option<String>,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContainerElement {
    #[cfg(feature = "id")]
    pub id: usize,
    pub elements: Vec<Element>,
    pub direction: LayoutDirection,
    pub overflow_x: LayoutOverflow,
    pub overflow_y: LayoutOverflow,
    pub justify_content: JustifyContent,
    pub width: Option<Number>,
    pub height: Option<Number>,
    pub route: Option<Route>,
    pub padding_left: Option<f32>,
    pub padding_right: Option<f32>,
    pub padding_top: Option<f32>,
    pub padding_bottom: Option<f32>,
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

impl ContainerElement {
    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_element_by_id(&self, id: usize) -> Option<&Self> {
        if self.id == id {
            Some(self)
        } else {
            self.elements
                .iter()
                .filter_map(|x| x.container_element())
                .find_map(|x| x.find_element_by_id(id))
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_element_by_id_mut(&mut self, id: usize) -> Option<&mut Self> {
        if self.id == id {
            Some(self)
        } else {
            self.elements
                .iter_mut()
                .filter_map(|x| x.container_element_mut())
                .find_map(|x| x.find_element_by_id_mut(id))
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_parent<'a>(&self, root: &'a mut Self) -> Option<&'a Self> {
        if root
            .elements
            .iter()
            .filter_map(|x| x.container_element())
            .any(|x| x.id == self.id)
        {
            Some(root)
        } else {
            root.elements
                .iter()
                .filter_map(|x| x.container_element())
                .find(|x| {
                    x.elements
                        .iter()
                        .filter_map(|x| x.container_element())
                        .any(|x| x.id == self.id)
                })
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_parent_by_id(&self, id: usize) -> Option<&Self> {
        if self
            .elements
            .iter()
            .filter_map(|x| x.container_element())
            .any(|x| x.id == id)
        {
            Some(self)
        } else {
            self.elements
                .iter()
                .filter_map(|x| x.container_element())
                .find_map(|x| x.find_parent_by_id(id))
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_parent_by_id_mut(&mut self, id: usize) -> Option<&mut Self> {
        if self
            .elements
            .iter()
            .filter_map(|x| x.container_element())
            .any(|x| x.id == id)
        {
            Some(self)
        } else {
            self.elements
                .iter_mut()
                .filter_map(|x| x.container_element_mut())
                .find_map(|x| x.find_parent_by_id_mut(id))
        }
    }

    pub fn replace_with(&mut self, replacement: Self) {
        *self = replacement;
    }

    /// # Panics
    ///
    /// * If the `ContainerElement` is the root node
    /// * If the `ContainerElement` is not properly attached to the tree
    #[cfg(feature = "id")]
    pub fn replace_with_elements(&mut self, replacement: Vec<Element>, root: &mut Self) {
        let Some(parent) = &mut root.find_parent_by_id_mut(self.id) else {
            panic!("Cannot replace the root node with multiple elements");
        };

        let index = parent
            .elements
            .iter()
            .enumerate()
            .find_map(|(i, x)| {
                if let Some(container) = x.container_element() {
                    if container.id == self.id {
                        Some(i)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("ContainerElement is not attached properly to tree"));

        parent.elements.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.elements.insert(index + i, element);
        }
    }

    /// # Panics
    ///
    /// * If the `ContainerElement` is not properly attached to the tree
    #[cfg(feature = "id")]
    pub fn replace_id_with_elements(&mut self, replacement: Vec<Element>, id: usize) -> bool {
        let Some(parent) = &mut self.find_parent_by_id_mut(id) else {
            return false;
        };

        let index = parent
            .elements
            .iter()
            .enumerate()
            .find_map(|(i, x)| {
                x.container_element().and_then(
                    |container| {
                        if container.id == id {
                            Some(i)
                        } else {
                            None
                        }
                    },
                )
            })
            .unwrap_or_else(|| panic!("ContainerElement is not attached properly to tree"));

        parent.elements.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.elements.insert(index + i, element);
        }

        true
    }

    /// # Panics
    ///
    /// * If the `ContainerElement` is not properly attached to the tree
    #[cfg(feature = "id")]
    pub fn replace_ids_with_elements(&mut self, replacement: Vec<Element>, ids: &[usize]) -> bool {
        let Some(parent) = &mut self.find_parent_by_id_mut(ids[0]) else {
            return false;
        };

        let index = parent
            .elements
            .iter()
            .enumerate()
            .find_map(|(i, x)| {
                x.container_element().and_then(|container| {
                    if container.id == ids[0] {
                        Some(i)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| panic!("ContainerElement is not attached properly to tree"));

        for _ in 0..ids.len() {
            parent.elements.remove(index);
        }

        for (i, element) in replacement.into_iter().enumerate() {
            parent.elements.insert(index + i, element);
        }

        true
    }
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
    Table {
        element: ContainerElement,
    },
    THead {
        element: ContainerElement,
    },
    TH {
        element: ContainerElement,
    },
    TBody {
        element: ContainerElement,
    },
    TR {
        element: ContainerElement,
    },
    TD {
        element: ContainerElement,
    },
}

#[derive(Default)]
struct Attrs {
    values: Vec<(String, String)>,
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
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        }
    }

    fn add<K: Into<String>, V: Display>(&mut self, name: K, value: V) {
        self.values.push((name.into(), value.to_string()));
    }

    fn add_opt<K: Into<String>, V: Display>(&mut self, name: K, value: Option<V>) {
        if let Some(value) = value {
            self.values.push((name.into(), value.to_string()));
        }
    }
}

impl ContainerElement {
    fn attrs(&self, with_debug_attrs: bool) -> Attrs {
        let mut attrs = Attrs { values: vec![] };

        if let Some(route) = &self.route {
            match route {
                Route::Get { route, trigger } => {
                    attrs.add("hx-get", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                }
                Route::Post { route, trigger } => {
                    attrs.add("hx-post", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                }
            }
        }

        if self.direction == LayoutDirection::Row {
            attrs.add("sx-dir", "row");
        }

        if let Some(width) = &self.width {
            attrs.add("sx-width", width);
        }
        if let Some(height) = &self.height {
            attrs.add("sx-height", height);
        }
        match self.overflow_x {
            LayoutOverflow::Auto => {
                attrs.add("sx-overflow-x", "auto");
            }
            LayoutOverflow::Scroll => {
                attrs.add("sx-overflow-x", "scroll");
            }
            LayoutOverflow::Show => {
                attrs.add("sx-overflow-x", "show");
            }
            LayoutOverflow::Squash => {}
            LayoutOverflow::Wrap => {
                attrs.add("sx-overflow-x", "wrap");
            }
        }
        match self.overflow_y {
            LayoutOverflow::Auto => {
                attrs.add("sx-overflow-y", "auto");
            }
            LayoutOverflow::Scroll => {
                attrs.add("sx-overflow-y", "scroll");
            }
            LayoutOverflow::Show => {
                attrs.add("sx-overflow-y", "show");
            }
            LayoutOverflow::Squash => {}
            LayoutOverflow::Wrap => {
                attrs.add("sx-overflow-y", "wrap");
            }
        }

        if with_debug_attrs {
            #[cfg(feature = "calc")]
            {
                attrs.add_opt("dbg-x", self.calculated_x);
                attrs.add_opt("dbg-y", self.calculated_y);
                attrs.add_opt("dbg-width", self.calculated_width);
                attrs.add_opt("dbg-height", self.calculated_height);
                attrs.add_opt("dbg-padding-left", self.padding_left);
                attrs.add_opt("dbg-padding-right", self.padding_right);
                attrs.add_opt("dbg-padding-top", self.padding_top);
                attrs.add_opt("dbg-padding-bottom", self.padding_bottom);

                if let Some(LayoutPosition::Wrap { row, col }) = &self.calculated_position {
                    attrs.add("dbg-row", *row);
                    attrs.add("dbg-col", *col);
                }
            }
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
                .display_to_string(
                    std::env::var("DEBUG_ATTRS")
                        .is_ok_and(|x| ["1", "true"].contains(&x.to_lowercase().as_str())),
                )
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
            if data[0] == b'<' {
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
            } else {
                data
            }
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
            Self::Table { element } => {
                f.write_fmt(format_args!(
                    "<table{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</table>"))?;
            }
            Self::THead { element } => {
                f.write_fmt(format_args!(
                    "<thead{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</thead>"))?;
            }
            Self::TH { element } => {
                f.write_fmt(format_args!(
                    "<th{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</th>"))?;
            }
            Self::TBody { element } => {
                f.write_fmt(format_args!(
                    "<tbody{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</tbody>"))?;
            }
            Self::TR { element } => {
                f.write_fmt(format_args!(
                    "<tr{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</tr>"))?;
            }
            Self::TD { element } => {
                f.write_fmt(format_args!(
                    "<td{attrs}>",
                    attrs = element.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&element.elements, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</td>"))?;
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
            Self::Table { .. } => "Table",
            Self::THead { .. } => "THead",
            Self::TH { .. } => "TH",
            Self::TBody { .. } => "TBody",
            Self::TR { .. } => "TR",
            Self::TD { .. } => "TD",
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
            | Self::Table { element }
            | Self::THead { element }
            | Self::TH { element }
            | Self::TBody { element }
            | Self::TR { element }
            | Self::TD { element }
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
            | Self::Table { element }
            | Self::THead { element }
            | Self::TH { element }
            | Self::TBody { element }
            | Self::TR { element }
            | Self::TD { element }
            | Self::ListItem { element } => Some(element),
            Self::Raw { .. } | Self::Input(_) => None,
        }
    }
}

pub struct TableIter<'a> {
    pub headings:
        Option<Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a ContainerElement> + 'a>> + 'a>>,
    pub rows: Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a ContainerElement> + 'a>> + 'a>,
}

pub struct TableIterMut<'a> {
    pub headings: Option<
        Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a mut ContainerElement> + 'a>> + 'a>,
    >,
    pub rows:
        Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a mut ContainerElement> + 'a>> + 'a>,
}

impl Element {
    /// # Panics
    ///
    /// Will panic if `Element` is not a table
    #[must_use]
    pub fn table_iter<'a, 'b>(&'a self) -> TableIter<'b>
    where
        'a: 'b,
    {
        let Self::Table { element: container } = self else {
            panic!("Not a table");
        };

        let mut rows_builder: Option<Vec<Box<dyn Iterator<Item = &'b ContainerElement>>>> = None;
        let mut headings: Option<
            Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b ContainerElement>>>>,
        > = None;
        let mut rows: Box<
            dyn Iterator<Item = Box<dyn Iterator<Item = &'b ContainerElement>>> + 'b,
        > = Box::new(std::iter::empty());

        for element in &container.elements {
            match element {
                Self::THead { element } => {
                    headings = Some(Box::new(
                        element
                            .elements
                            .iter()
                            .filter_map(|x| x.container_element())
                            .map(|x| {
                                Box::new(x.elements.iter().filter_map(|x| x.container_element()))
                                    as Box<dyn Iterator<Item = &ContainerElement>>
                            }),
                    )
                        as Box<
                            dyn Iterator<Item = Box<dyn Iterator<Item = &'b ContainerElement>>>
                                + 'b,
                        >);
                }
                Self::TBody { element } => {
                    rows = Box::new(
                        element
                            .elements
                            .iter()
                            .filter_map(|x| x.container_element())
                            .map(|x| {
                                Box::new(x.elements.iter().filter_map(|x| x.container_element()))
                                    as Box<dyn Iterator<Item = &ContainerElement>>
                            }),
                    )
                        as Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b ContainerElement>>>>;
                }
                Self::TR { element } => {
                    if let Some(builder) = &mut rows_builder {
                        builder.push(Box::new(
                            element
                                .elements
                                .iter()
                                .filter_map(|x| x.container_element()),
                        )
                            as Box<dyn Iterator<Item = &'b ContainerElement>>);
                    } else {
                        rows_builder.replace(vec![Box::new(
                            element
                                .elements
                                .iter()
                                .filter_map(|x| x.container_element()),
                        )
                            as Box<dyn Iterator<Item = &'b ContainerElement>>]);
                    }
                }
                _ => {
                    panic!("Invalid table element: {element}");
                }
            }
        }

        if let Some(rows_builder) = rows_builder {
            rows = Box::new(rows_builder.into_iter());
        }

        TableIter { headings, rows }
    }

    /// # Panics
    ///
    /// Will panic if `Element` is not a table
    #[must_use]
    pub fn table_iter_mut<'a, 'b>(&'a mut self) -> TableIterMut<'b>
    where
        'a: 'b,
    {
        self.table_iter_mut_with_observer(None::<fn(&mut Self)>)
    }

    /// # Panics
    ///
    /// Will panic if `Element` is not a table
    #[must_use]
    pub fn table_iter_mut_with_observer<'a, 'b>(
        &'a mut self,
        mut observer: Option<impl FnMut(&mut Self)>,
    ) -> TableIterMut<'b>
    where
        'a: 'b,
    {
        let Self::Table { element: container } = self else {
            panic!("Not a table");
        };

        let mut rows_builder: Option<Vec<Box<dyn Iterator<Item = &'b mut ContainerElement>>>> =
            None;
        let mut headings: Option<
            Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut ContainerElement>>> + 'b>,
        > = None;
        let mut rows: Box<
            dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut ContainerElement>>> + 'b,
        > = Box::new(std::iter::empty());

        for element in &mut container.elements {
            if let Some(observer) = &mut observer {
                match element {
                    Self::THead { .. } | Self::TBody { .. } | Self::TR { .. } => {
                        observer(element);
                    }
                    _ => {}
                }
            }
            match element {
                Self::THead { element } => {
                    headings = Some(Box::new(
                        element
                            .elements
                            .iter_mut()
                            .filter_map(|x| x.container_element_mut())
                            .map(|x| {
                                Box::new(
                                    x.elements
                                        .iter_mut()
                                        .filter_map(|x| x.container_element_mut()),
                                )
                                    as Box<dyn Iterator<Item = &mut ContainerElement>>
                            }),
                    )
                        as Box<
                            dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut ContainerElement>>>
                                + 'b,
                        >);
                }
                Self::TBody { element } => {
                    rows = Box::new(
                        element
                            .elements
                            .iter_mut()
                            .filter_map(|x| x.container_element_mut())
                            .map(|x| {
                                Box::new(
                                    x.elements
                                        .iter_mut()
                                        .filter_map(|x| x.container_element_mut()),
                                )
                                    as Box<dyn Iterator<Item = &mut ContainerElement>>
                            }),
                    )
                        as Box<
                            dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut ContainerElement>>>,
                        >;
                }
                Self::TR { element } => {
                    if let Some(builder) = &mut rows_builder {
                        builder.push(Box::new(
                            element
                                .elements
                                .iter_mut()
                                .filter_map(|x| x.container_element_mut()),
                        )
                            as Box<dyn Iterator<Item = &'b mut ContainerElement>>);
                    } else {
                        rows_builder.replace(vec![Box::new(
                            element
                                .elements
                                .iter_mut()
                                .filter_map(|x| x.container_element_mut()),
                        )
                            as Box<dyn Iterator<Item = &'b mut ContainerElement>>]);
                    }
                }
                _ => {
                    panic!("Invalid table element: {element}");
                }
            }
        }

        if let Some(rows_builder) = rows_builder {
            rows = Box::new(rows_builder.into_iter());
        }

        TableIterMut { headings, rows }
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
