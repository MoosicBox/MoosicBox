#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::io::Write;

use gigachad_actions::Action;
use gigachad_color::Color;
use gigachad_transformer_models::{
    AlignItems, Cursor, JustifyContent, LayoutDirection, LayoutOverflow, Position, Route,
    Visibility,
};
use serde_json::Value;

pub use gigachad_transformer_models as models;

#[cfg(feature = "calc")]
pub mod calc;
#[cfg(any(test, feature = "html"))]
pub mod html;
pub mod parse;

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
    Grouping(Box<Calculation>),
    Min(Box<Calculation>, Box<Calculation>),
    Max(Box<Calculation>, Box<Calculation>),
}

impl Calculation {
    fn calc(&self, container: f32) -> f32 {
        match self {
            Self::Number(number) => calc_number(number, container),
            Self::Add(left, right) => left.calc(container) + right.calc(container),
            Self::Subtract(left, right) => left.calc(container) - right.calc(container),
            Self::Multiply(left, right) => left.calc(container) * right.calc(container),
            Self::Divide(left, right) => left.calc(container) / right.calc(container),
            Self::Grouping(value) => value.calc(container),
            Self::Min(left, right) => {
                let a = left.calc(container);
                let b = right.calc(container);
                if a > b {
                    b
                } else {
                    a
                }
            }
            Self::Max(left, right) => {
                let a = left.calc(container);
                let b = right.calc(container);
                if a > b {
                    a
                } else {
                    b
                }
            }
        }
    }
}

impl std::fmt::Display for Calculation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(number) => f.write_str(&number.to_string()),
            Self::Add(left, right) => f.write_fmt(format_args!("{left} + {right}")),
            Self::Subtract(left, right) => f.write_fmt(format_args!("{left} - {right}")),
            Self::Multiply(left, right) => f.write_fmt(format_args!("{left} * {right}")),
            Self::Divide(left, right) => f.write_fmt(format_args!("{left} / {right}")),
            Self::Grouping(value) => f.write_str(&value.to_string()),
            Self::Min(left, right) => f.write_fmt(format_args!("min({left}, {right})")),
            Self::Max(left, right) => f.write_fmt(format_args!("max({left}, {right})")),
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

impl std::fmt::Display for Number {
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    #[cfg(feature = "id")]
    pub id: usize,
    pub str_id: Option<String>,
    pub element: Element,
    pub children: Vec<Container>,
    pub direction: LayoutDirection,
    pub overflow_x: LayoutOverflow,
    pub overflow_y: LayoutOverflow,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub width: Option<Number>,
    pub height: Option<Number>,
    pub gap: Option<Number>,
    pub opacity: Option<Number>,
    pub left: Option<Number>,
    pub right: Option<Number>,
    pub top: Option<Number>,
    pub bottom: Option<Number>,
    pub cursor: Option<Cursor>,
    pub position: Option<Position>,
    pub background: Option<Color>,
    pub border_top: Option<(Color, Number)>,
    pub border_right: Option<(Color, Number)>,
    pub border_bottom: Option<(Color, Number)>,
    pub border_left: Option<(Color, Number)>,
    pub border_top_left_radius: Option<Number>,
    pub border_top_right_radius: Option<Number>,
    pub border_bottom_left_radius: Option<Number>,
    pub border_bottom_right_radius: Option<Number>,
    pub margin_left: Option<Number>,
    pub margin_right: Option<Number>,
    pub margin_top: Option<Number>,
    pub margin_bottom: Option<Number>,
    pub padding_left: Option<Number>,
    pub padding_right: Option<Number>,
    pub padding_top: Option<Number>,
    pub padding_bottom: Option<Number>,
    pub state: Option<Value>,
    pub hidden: Option<bool>,
    pub debug: Option<bool>,
    pub visibility: Option<Visibility>,
    pub route: Option<Route>,
    pub actions: Vec<Action>,
    #[cfg(feature = "calc")]
    pub internal_margin_left: Option<f32>,
    #[cfg(feature = "calc")]
    pub internal_margin_right: Option<f32>,
    #[cfg(feature = "calc")]
    pub internal_margin_top: Option<f32>,
    #[cfg(feature = "calc")]
    pub internal_margin_bottom: Option<f32>,
    #[cfg(feature = "calc")]
    pub internal_padding_left: Option<f32>,
    #[cfg(feature = "calc")]
    pub internal_padding_right: Option<f32>,
    #[cfg(feature = "calc")]
    pub internal_padding_top: Option<f32>,
    #[cfg(feature = "calc")]
    pub internal_padding_bottom: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_margin_left: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_margin_right: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_margin_top: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_margin_bottom: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_padding_left: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_padding_right: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_padding_top: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_padding_bottom: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_width: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_height: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_x: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_y: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_position: Option<gigachad_transformer_models::LayoutPosition>,
    #[cfg(feature = "calc")]
    pub calculated_border_top: Option<(Color, f32)>,
    #[cfg(feature = "calc")]
    pub calculated_border_right: Option<(Color, f32)>,
    #[cfg(feature = "calc")]
    pub calculated_border_bottom: Option<(Color, f32)>,
    #[cfg(feature = "calc")]
    pub calculated_border_left: Option<(Color, f32)>,
    #[cfg(feature = "calc")]
    pub calculated_border_top_left_radius: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_border_top_right_radius: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_border_bottom_left_radius: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_border_bottom_right_radius: Option<f32>,
    #[cfg(feature = "calc")]
    pub calculated_opacity: Option<f32>,
    #[cfg(feature = "calc")]
    pub scrollbar_right: Option<f32>,
    #[cfg(feature = "calc")]
    pub scrollbar_bottom: Option<f32>,
}

#[cfg(any(test, feature = "maud"))]
impl TryFrom<maud::Markup> for Container {
    type Error = tl::ParseError;

    fn try_from(value: maud::Markup) -> Result<Self, Self::Error> {
        value.into_string().try_into()
    }
}

fn visible_elements(elements: &[Container]) -> impl Iterator<Item = &Container> {
    elements.iter().filter(|x| x.hidden != Some(true))
}

fn visible_elements_mut(elements: &mut [Container]) -> impl Iterator<Item = &mut Container> {
    elements.iter_mut().filter(|x| x.hidden != Some(true))
}

fn relative_positioned_elements(elements: &[Container]) -> impl Iterator<Item = &Container> {
    visible_elements(elements).filter(|x| x.position != Some(Position::Absolute))
}

fn relative_positioned_elements_mut(
    elements: &mut [Container],
) -> impl Iterator<Item = &mut Container> {
    visible_elements_mut(elements).filter(|x| x.position != Some(Position::Absolute))
}

fn absolute_positioned_elements(elements: &[Container]) -> impl Iterator<Item = &Container> {
    visible_elements(elements).filter(|x| x.position == Some(Position::Absolute))
}

fn absolute_positioned_elements_mut(
    elements: &mut [Container],
) -> impl Iterator<Item = &mut Container> {
    visible_elements_mut(elements).filter(|x| x.position == Some(Position::Absolute))
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Container {
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.hidden != Some(true)
    }

    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.hidden == Some(true)
    }

    pub fn visible_elements(&self) -> impl Iterator<Item = &Self> {
        visible_elements(&self.children)
    }

    pub fn visible_elements_mut(&mut self) -> impl Iterator<Item = &mut Self> {
        visible_elements_mut(&mut self.children)
    }

    pub fn relative_positioned_elements(&self) -> impl Iterator<Item = &Self> {
        relative_positioned_elements(&self.children)
    }

    pub fn relative_positioned_elements_mut(&mut self) -> impl Iterator<Item = &mut Self> {
        relative_positioned_elements_mut(&mut self.children)
    }

    pub fn absolute_positioned_elements(&self) -> impl Iterator<Item = &Self> {
        absolute_positioned_elements(&self.children)
    }

    pub fn absolute_positioned_elements_mut(&mut self) -> impl Iterator<Item = &mut Self> {
        absolute_positioned_elements_mut(&mut self.children)
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_element_by_id(&self, id: usize) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|x| x.find_element_by_id(id))
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_element_by_id_mut(&mut self, id: usize) -> Option<&mut Self> {
        if self.id == id {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|x| x.find_element_by_id_mut(id))
    }

    #[must_use]
    pub fn find_element_by_str_id(&self, str_id: &str) -> Option<&Self> {
        if self.str_id.as_ref().is_some_and(|x| x == str_id) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|x| x.find_element_by_str_id(str_id))
    }

    #[must_use]
    pub fn find_element_by_str_id_mut(&mut self, str_id: &str) -> Option<&mut Self> {
        if self.str_id.as_ref().is_some_and(|x| x == str_id) {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|x| x.find_element_by_str_id_mut(str_id))
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_parent<'a>(&self, root: &'a mut Self) -> Option<&'a Self> {
        if root.children.iter().any(|x| x.id == self.id) {
            Some(root)
        } else {
            root.children
                .iter()
                .find(|x| x.children.iter().any(|x| x.id == self.id))
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_parent_by_id(&self, id: usize) -> Option<&Self> {
        if self.children.iter().any(|x| x.id == id) {
            Some(self)
        } else {
            self.children.iter().find_map(|x| x.find_parent_by_id(id))
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub fn find_parent_by_id_mut(&mut self, id: usize) -> Option<&mut Self> {
        if self.children.iter().any(|x| x.id == id) {
            Some(self)
        } else {
            self.children
                .iter_mut()
                .find_map(|x| x.find_parent_by_id_mut(id))
        }
    }

    #[must_use]
    pub fn find_parent_by_str_id_mut(&mut self, id: &str) -> Option<&mut Self> {
        if self
            .children
            .iter()
            .filter_map(|x| x.str_id.as_ref())
            .map(String::as_str)
            .any(|x| x == id)
        {
            Some(self)
        } else {
            self.children
                .iter_mut()
                .find_map(|x| x.find_parent_by_str_id_mut(id))
        }
    }

    pub fn replace_with(&mut self, replacement: Self) {
        *self = replacement;
    }

    /// # Panics
    ///
    /// * If the `Container` is the root node
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "id")]
    pub fn replace_with_elements(&mut self, replacement: Vec<Self>, root: &mut Self) {
        let Some(parent) = &mut root.find_parent_by_id_mut(self.id) else {
            panic!("Cannot replace the root node with multiple elements");
        };

        let index = parent
            .children
            .iter()
            .enumerate()
            .find_map(|(i, x)| if x.id == self.id { Some(i) } else { None })
            .unwrap_or_else(|| panic!("Container is not attached properly to tree"));

        parent.children.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.children.insert(index + i, element);
        }
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "id")]
    pub fn replace_id_with_elements(&mut self, replacement: Vec<Self>, id: usize) -> bool {
        let Some(parent) = &mut self.find_parent_by_id_mut(id) else {
            return false;
        };

        let index = parent
            .children
            .iter()
            .enumerate()
            .find_map(|(i, x)| if x.id == id { Some(i) } else { None })
            .unwrap_or_else(|| panic!("Container is not attached properly to tree"));

        parent.children.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.children.insert(index + i, element);
        }

        true
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "id")]
    pub fn replace_str_id_with_elements(
        &mut self,
        replacement: Vec<Self>,
        id: &str,
    ) -> Option<Self> {
        let Some(parent) = &mut self.find_parent_by_str_id_mut(id) else {
            return None;
        };

        let index = parent
            .children
            .iter()
            .enumerate()
            .find_map(|(i, x)| {
                if x.str_id.as_ref().is_some_and(|x| x.as_str() == id) {
                    Some(i)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Container is not attached properly to tree"));

        let element = parent.children.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.children.insert(index + i, element);
        }

        Some(element)
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "id")]
    pub fn replace_ids_with_elements(&mut self, replacement: Vec<Self>, ids: &[usize]) -> bool {
        let Some(parent) = &mut self.find_parent_by_id_mut(ids[0]) else {
            return false;
        };

        let index = parent
            .children
            .iter()
            .enumerate()
            .find_map(|(i, x)| if x.id == ids[0] { Some(i) } else { None })
            .unwrap_or_else(|| panic!("Container is not attached properly to tree"));

        for _ in 0..ids.len() {
            parent.children.remove(index);
        }

        for (i, element) in replacement.into_iter().enumerate() {
            parent.children.insert(index + i, element);
        }

        true
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub enum Element {
    #[default]
    Div,
    Raw {
        value: String,
    },
    Aside,
    Main,
    Header,
    Footer,
    Section,
    Form,
    Span,
    Input {
        input: Input,
    },
    Button,
    Image {
        source: Option<String>,
    },
    Anchor {
        href: Option<String>,
    },
    Heading {
        size: HeaderSize,
    },
    UnorderedList,
    OrderedList,
    ListItem,
    Table,
    THead,
    TH,
    TBody,
    TR,
    TD,
    #[cfg(feature = "canvas")]
    Canvas,
}

#[derive(Default)]
struct Attrs {
    values: Vec<(String, String)>,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Attrs {
    fn new() -> Self {
        Self::default()
    }

    #[allow(unused)]
    fn with_attr<K: Into<String>, V: std::fmt::Display + 'static>(
        mut self,
        name: K,
        value: V,
    ) -> Self {
        self.add(name, value);
        self
    }

    fn with_attr_opt<K: Into<String>, V: std::fmt::Display + 'static>(
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

    fn add<K: Into<String>, V: std::fmt::Display>(&mut self, name: K, value: V) {
        self.values.push((name.into(), value.to_string()));
    }

    fn add_opt<K: Into<String>, V: std::fmt::Display>(&mut self, name: K, value: Option<V>) {
        if let Some(value) = value {
            self.values.push((name.into(), value.to_string()));
        }
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Container {
    #[allow(clippy::too_many_lines)]
    fn attrs(&self, with_debug_attrs: bool) -> Attrs {
        let mut attrs = Attrs { values: vec![] };

        if let Some(route) = &self.route {
            match route {
                Route::Get {
                    route,
                    trigger,
                    swap,
                } => {
                    attrs.add("hx-get", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                    attrs.add("hx-swap", swap);
                }
                Route::Post {
                    route,
                    trigger,
                    swap,
                } => {
                    attrs.add("hx-post", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                    attrs.add("hx-swap", swap);
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

        if let Some(padding) = &self.padding_left {
            attrs.add("sx-padding-left", padding);
        }
        if let Some(padding) = &self.padding_right {
            attrs.add("sx-padding-right", padding);
        }
        if let Some(padding) = &self.padding_top {
            attrs.add("sx-padding-top", padding);
        }
        if let Some(padding) = &self.padding_bottom {
            attrs.add("sx-padding-bottom", padding);
        }

        if let Some(margin) = &self.margin_left {
            attrs.add("sx-margin-left", margin);
        }
        if let Some(margin) = &self.margin_right {
            attrs.add("sx-margin-right", margin);
        }
        if let Some(margin) = &self.margin_top {
            attrs.add("sx-margin-top", margin);
        }
        if let Some(margin) = &self.margin_bottom {
            attrs.add("sx-margin-bottom", margin);
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
                attrs.add_opt("dbg-margin-left", self.calculated_margin_left);
                attrs.add_opt("dbg-margin-right", self.calculated_margin_right);
                attrs.add_opt("dbg-margin-top", self.calculated_margin_top);
                attrs.add_opt("dbg-margin-bottom", self.calculated_margin_bottom);
                attrs.add_opt("dbg-padding-left", self.calculated_padding_left);
                attrs.add_opt("dbg-padding-right", self.calculated_padding_right);
                attrs.add_opt("dbg-padding-top", self.calculated_padding_top);
                attrs.add_opt("dbg-padding-bottom", self.calculated_padding_bottom);
                attrs.add_opt("dbg-internal-margin-left", self.internal_margin_left);
                attrs.add_opt("dbg-internal-margin-right", self.internal_margin_right);
                attrs.add_opt("dbg-internal-margin-top", self.internal_margin_top);
                attrs.add_opt("dbg-internal-margin-bottom", self.internal_margin_bottom);
                attrs.add_opt("dbg-internal-padding-left", self.internal_padding_left);
                attrs.add_opt("dbg-internal-padding-right", self.internal_padding_right);
                attrs.add_opt("dbg-internal-padding-top", self.internal_padding_top);
                attrs.add_opt("dbg-internal-padding-bottom", self.internal_padding_bottom);

                if let Some(gigachad_transformer_models::LayoutPosition::Wrap { row, col }) =
                    &self.calculated_position
                {
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

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_lines)]
    fn display(&self, f: &mut dyn Write, with_debug_attrs: bool) -> Result<(), std::io::Error> {
        match &self.element {
            Element::Raw { value } => {
                f.write_fmt(format_args!("{value}"))?;
            }
            Element::Div => {
                f.write_fmt(format_args!(
                    "<div{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</div>"))?;
            }
            Element::Aside => {
                f.write_fmt(format_args!(
                    "<aside{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</aside>"))?;
            }

            Element::Main => {
                f.write_fmt(format_args!(
                    "<main{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</main>"))?;
            }
            Element::Header => {
                f.write_fmt(format_args!(
                    "<header{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</header>"))?;
            }
            Element::Footer => {
                f.write_fmt(format_args!(
                    "<footer{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</footer>"))?;
            }
            Element::Section => {
                f.write_fmt(format_args!(
                    "<section{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</section>"))?;
            }
            Element::Form => {
                f.write_fmt(format_args!(
                    "<form{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</from>"))?;
            }
            Element::Span => {
                f.write_fmt(format_args!(
                    "<span{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</span>"))?;
            }
            Element::Input { input, .. } => {
                input.display(f, with_debug_attrs)?;
            }
            Element::Button => {
                f.write_fmt(format_args!(
                    "<button{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</button>"))?;
            }
            Element::Image { source } => {
                f.write_fmt(format_args!(
                    "<img{src_attr}{attrs} />",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs),
                    src_attr = Attrs::new()
                        .with_attr_opt("src", source.to_owned())
                        .to_string_pad_left()
                ))?;
            }
            Element::Anchor { href } => {
                f.write_fmt(format_args!(
                    "<a{href_attr}{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs),
                    href_attr = Attrs::new()
                        .with_attr_opt("href", href.to_owned())
                        .to_string_pad_left(),
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</a>"))?;
            }
            Element::Heading { size } => {
                f.write_fmt(format_args!(
                    "<{size}{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</{size}>"))?;
            }
            Element::UnorderedList => {
                f.write_fmt(format_args!(
                    "<ul{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</ul>"))?;
            }
            Element::OrderedList => {
                f.write_fmt(format_args!(
                    "<ol{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</ol>"))?;
            }
            Element::ListItem => {
                f.write_fmt(format_args!(
                    "<li{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</li>"))?;
            }
            Element::Table => {
                f.write_fmt(format_args!(
                    "<table{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</table>"))?;
            }
            Element::THead => {
                f.write_fmt(format_args!(
                    "<thead{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</thead>"))?;
            }
            Element::TH => {
                f.write_fmt(format_args!(
                    "<th{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</th>"))?;
            }
            Element::TBody => {
                f.write_fmt(format_args!(
                    "<tbody{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</tbody>"))?;
            }
            Element::TR => {
                f.write_fmt(format_args!(
                    "<tr{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</tr>"))?;
            }
            Element::TD => {
                f.write_fmt(format_args!(
                    "<td{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</td>"))?;
            }
            #[cfg(feature = "canvas")]
            Element::Canvas => {
                f.write_fmt(format_args!(
                    "<canvas{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs)?;
                f.write_fmt(format_args!("</canvas>"))?;
            }
        }

        Ok(())
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
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
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl std::fmt::Display for Container {
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
    elements: &[Container],
    f: &mut dyn Write,
    with_debug_attrs: bool,
) -> Result<(), std::io::Error> {
    for element in elements {
        element.display(f, with_debug_attrs)?;
    }

    Ok(())
}

impl Element {
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
            Self::Input { .. } => "Input",
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
            #[cfg(feature = "canvas")]
            Self::Canvas { .. } => "Canvas",
        }
    }
}

pub struct TableIter<'a> {
    pub headings:
        Option<Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a Container> + 'a>> + 'a>>,
    pub rows: Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a Container> + 'a>> + 'a>,
}

pub struct TableIterMut<'a> {
    pub headings:
        Option<Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a mut Container> + 'a>> + 'a>>,
    pub rows: Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'a mut Container> + 'a>> + 'a>,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Container {
    /// # Panics
    ///
    /// Will panic if `Element` is not a table
    #[must_use]
    pub fn table_iter<'a, 'b>(&'a self) -> TableIter<'b>
    where
        'a: 'b,
    {
        moosicbox_assert::assert_or_panic!(self.element == Element::Table, "Not a table");

        let mut rows_builder: Option<Vec<Box<dyn Iterator<Item = &'b Self>>>> = None;
        let mut headings: Option<Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b Self>>>>> =
            None;
        let mut rows: Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b Self>>> + 'b> =
            Box::new(std::iter::empty());

        for element in &self.children {
            match &element.element {
                Element::THead => {
                    headings =
                        Some(Box::new(element.children.iter().map(|x| {
                            Box::new(x.children.iter()) as Box<dyn Iterator<Item = &Self>>
                        }))
                            as Box<
                                dyn Iterator<Item = Box<dyn Iterator<Item = &'b Self>>> + 'b,
                            >);
                }
                Element::TBody => {
                    rows =
                        Box::new(element.children.iter().map(|x| {
                            Box::new(x.children.iter()) as Box<dyn Iterator<Item = &Self>>
                        }))
                            as Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b Self>>>>;
                }
                Element::TR => {
                    if let Some(builder) = &mut rows_builder {
                        builder
                            .push(Box::new(element.children.iter())
                                as Box<dyn Iterator<Item = &'b Self>>);
                    } else {
                        rows_builder
                            .replace(vec![Box::new(element.children.iter())
                                as Box<dyn Iterator<Item = &'b Self>>]);
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
        moosicbox_assert::assert_or_panic!(self.element == Element::Table, "Not a table");

        let mut rows_builder: Option<Vec<Box<dyn Iterator<Item = &'b mut Self>>>> = None;
        let mut headings: Option<
            Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut Self>>> + 'b>,
        > = None;
        let mut rows: Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut Self>>> + 'b> =
            Box::new(std::iter::empty());

        for container in &mut self.children {
            if let Some(observer) = &mut observer {
                match container.element {
                    Element::THead | Element::TBody | Element::TR => {
                        observer(container);
                    }
                    _ => {}
                }
            }
            match container.element {
                Element::THead => {
                    headings = Some(Box::new(container.children.iter_mut().map(|x| {
                        Box::new(x.children.iter_mut()) as Box<dyn Iterator<Item = &mut Self>>
                    }))
                        as Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut Self>>> + 'b>);
                }
                Element::TBody => {
                    rows = Box::new(container.children.iter_mut().map(|x| {
                        Box::new(x.children.iter_mut()) as Box<dyn Iterator<Item = &mut Self>>
                    }))
                        as Box<dyn Iterator<Item = Box<dyn Iterator<Item = &'b mut Self>>>>;
                }
                Element::TR => {
                    if let Some(builder) = &mut rows_builder {
                        builder.push(Box::new(container.children.iter_mut())
                            as Box<dyn Iterator<Item = &'b mut Self>>);
                    } else {
                        rows_builder.replace(vec![Box::new(container.children.iter_mut())
                            as Box<dyn Iterator<Item = &'b mut Self>>]);
                    }
                }
                _ => {
                    panic!("Invalid table container: {container}");
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

impl std::fmt::Display for HeaderSize {
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
    Checkbox {
        checked: Option<bool>,
    },
    Text {
        value: Option<String>,
        placeholder: Option<String>,
    },
    Password {
        value: Option<String>,
        placeholder: Option<String>,
    },
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Input {
    fn display(&self, f: &mut dyn Write, _with_debug_attrs: bool) -> Result<(), std::io::Error> {
        match self {
            Self::Checkbox { checked } => {
                let attrs = Attrs::new().with_attr_opt("checked", checked.map(|x| x.to_string()));
                f.write_fmt(format_args!(
                    "<input type=\"checkbox\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))?;
            }
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

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl std::fmt::Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Checkbox { checked } => {
                let attrs = Attrs::new().with_attr_opt("checked", checked.map(|x| x.to_string()));
                f.write_fmt(format_args!(
                    "<input type=\"checkbox\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))
            }
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
