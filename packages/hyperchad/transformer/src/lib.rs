#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{any::Any, collections::BTreeMap, io::Write};

use switchy_env::var;

use hyperchad_actions::Action;
use hyperchad_color::Color;
use hyperchad_transformer_models::{
    AlignItems, Cursor, FontWeight, ImageFit, ImageLoading, JustifyContent, LayoutDirection,
    LayoutOverflow, LinkTarget, OverflowWrap, Position, Route, TextAlign, TextDecorationLine,
    TextDecorationStyle, TextOverflow, UserSelect, Visibility, WhiteSpace,
};
use parse::parse_number;
use serde::{Deserialize, Serialize, de::Error};
use serde_json::Value;

pub use hyperchad_actions as actions;
pub use hyperchad_transformer_models as models;
use strum::{EnumDiscriminants, EnumIter};

#[cfg(test)]
pub mod arb;
#[cfg(any(test, feature = "html"))]
pub mod html;
#[cfg(feature = "layout")]
pub mod layout;
pub mod parse;

#[derive(Clone, Debug, PartialEq, EnumDiscriminants, Serialize, Deserialize)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(CalculationType))]
pub enum Calculation {
    Number(Box<Number>),
    Add(Box<Self>, Box<Self>),
    Subtract(Box<Self>, Box<Self>),
    Multiply(Box<Self>, Box<Self>),
    Divide(Box<Self>, Box<Self>),
    Grouping(Box<Self>),
    Min(Box<Self>, Box<Self>),
    Max(Box<Self>, Box<Self>),
}

impl Calculation {
    fn calc(&self, container: f32, view_width: f32, view_height: f32) -> f32 {
        match self {
            Self::Number(number) => number.calc(container, view_width, view_height),
            Self::Add(left, right) => {
                left.calc(container, view_width, view_height)
                    + right.calc(container, view_width, view_height)
            }
            Self::Subtract(left, right) => {
                left.calc(container, view_width, view_height)
                    - right.calc(container, view_width, view_height)
            }
            Self::Multiply(left, right) => {
                left.calc(container, view_width, view_height)
                    * right.calc(container, view_width, view_height)
            }
            Self::Divide(left, right) => {
                left.calc(container, view_width, view_height)
                    / right.calc(container, view_width, view_height)
            }
            Self::Grouping(value) => value.calc(container, view_width, view_height),
            Self::Min(left, right) => {
                let a = left.calc(container, view_width, view_height);
                let b = right.calc(container, view_width, view_height);
                if a > b { b } else { a }
            }
            Self::Max(left, right) => {
                let a = left.calc(container, view_width, view_height);
                let b = right.calc(container, view_width, view_height);
                if a > b { a } else { b }
            }
        }
    }

    #[must_use]
    pub fn as_dynamic(&self) -> Option<&Self> {
        match self {
            Self::Number(x) => {
                if x.is_dynamic() {
                    Some(self)
                } else {
                    None
                }
            }
            Self::Add(a, b)
            | Self::Subtract(a, b)
            | Self::Multiply(a, b)
            | Self::Divide(a, b)
            | Self::Min(a, b)
            | Self::Max(a, b) => {
                if a.is_dynamic() || b.is_dynamic() {
                    Some(self)
                } else {
                    None
                }
            }
            Self::Grouping(x) => {
                if x.is_dynamic() {
                    Some(self)
                } else {
                    None
                }
            }
        }
    }

    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.as_dynamic().is_some()
    }

    #[must_use]
    pub fn as_fixed(&self) -> Option<&Self> {
        match self {
            Self::Number(x) => {
                if x.is_fixed() {
                    Some(self)
                } else {
                    None
                }
            }
            Self::Add(a, b)
            | Self::Subtract(a, b)
            | Self::Multiply(a, b)
            | Self::Divide(a, b)
            | Self::Min(a, b)
            | Self::Max(a, b) => {
                if a.is_fixed() && b.is_fixed() {
                    Some(self)
                } else {
                    None
                }
            }
            Self::Grouping(x) => {
                if x.is_fixed() {
                    Some(self)
                } else {
                    None
                }
            }
        }
    }

    #[must_use]
    pub fn is_fixed(&self) -> bool {
        self.as_fixed().is_some()
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
            Self::Grouping(value) => f.write_fmt(format_args!("({value})")),
            Self::Min(left, right) => f.write_fmt(format_args!("min({left}, {right})")),
            Self::Max(left, right) => f.write_fmt(format_args!("max({left}, {right})")),
        }
    }
}

#[derive(Clone, Debug, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(NumberType))]
pub enum Number {
    Real(f32),
    Integer(i64),
    RealPercent(f32),
    IntegerPercent(i64),
    RealDvw(f32),
    IntegerDvw(i64),
    RealDvh(f32),
    IntegerDvh(i64),
    RealVw(f32),
    IntegerVw(i64),
    RealVh(f32),
    IntegerVh(i64),
    Calc(Calculation),
}

impl Serialize for Number {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Real(x) => x.serialize(serializer),
            Self::Integer(x) => x.serialize(serializer),
            Self::RealPercent(x) => format!("{x}%").serialize(serializer),
            Self::IntegerPercent(x) => format!("{x}%").serialize(serializer),
            Self::RealDvw(x) => format!("{x}dvw").serialize(serializer),
            Self::IntegerDvw(x) => format!("{x}dvw").serialize(serializer),
            Self::RealDvh(x) => format!("{x}dvh").serialize(serializer),
            Self::IntegerDvh(x) => format!("{x}dvh").serialize(serializer),
            Self::RealVw(x) => format!("{x}vw").serialize(serializer),
            Self::IntegerVw(x) => format!("{x}vw").serialize(serializer),
            Self::RealVh(x) => format!("{x}vh").serialize(serializer),
            Self::IntegerVh(x) => format!("{x}vh").serialize(serializer),
            Self::Calc(calculation) => format!("calc({calculation})").serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename = "Number")]
        enum NumberInner {
            Real(f32),
            Integer(i64),
            RealPercent(f32),
            IntegerPercent(i64),
            RealDvw(f32),
            IntegerDvw(i64),
            RealDvh(f32),
            IntegerDvh(i64),
            RealVw(f32),
            IntegerVw(i64),
            RealVh(f32),
            IntegerVh(i64),
            Calc(Calculation),
        }

        impl From<NumberInner> for Number {
            fn from(value: NumberInner) -> Self {
                match value {
                    NumberInner::Real(x) => Self::Real(x),
                    NumberInner::Integer(x) => Self::Integer(x),
                    NumberInner::RealPercent(x) => Self::RealPercent(x),
                    NumberInner::IntegerPercent(x) => Self::IntegerPercent(x),
                    NumberInner::RealDvw(x) => Self::RealDvw(x),
                    NumberInner::IntegerDvw(x) => Self::IntegerDvw(x),
                    NumberInner::RealDvh(x) => Self::RealDvh(x),
                    NumberInner::IntegerDvh(x) => Self::IntegerDvh(x),
                    NumberInner::RealVw(x) => Self::RealVw(x),
                    NumberInner::IntegerVw(x) => Self::IntegerVw(x),
                    NumberInner::RealVh(x) => Self::RealVh(x),
                    NumberInner::IntegerVh(x) => Self::IntegerVh(x),
                    NumberInner::Calc(calculation) => Self::Calc(calculation),
                }
            }
        }

        log::trace!("attempting to deserialize Number");
        let value: Value = Value::deserialize(deserializer)?;
        log::trace!("deserialized Number to {value:?}");

        Ok(if value.is_i64() {
            #[allow(clippy::cast_possible_wrap)]
            Self::Integer(value.as_i64().unwrap())
        } else if value.is_u64() {
            #[allow(clippy::cast_possible_wrap)]
            Self::Integer(value.as_u64().unwrap() as i64)
        } else if value.is_f64() {
            #[allow(clippy::cast_possible_truncation)]
            Self::Real(value.as_f64().unwrap() as f32)
        } else if value.is_string() {
            parse_number(value.as_str().unwrap()).map_err(D::Error::custom)?
        } else {
            serde_json::from_value::<NumberInner>(value)
                .map_err(D::Error::custom)?
                .into()
        })
    }
}

impl Number {
    #[must_use]
    pub fn calc(&self, container: f32, view_width: f32, view_height: f32) -> f32 {
        match self {
            Self::Real(x) => *x,
            #[allow(clippy::cast_precision_loss)]
            Self::Integer(x) => *x as f32,
            Self::RealPercent(x) => container * (*x / 100.0),
            #[allow(clippy::cast_precision_loss)]
            Self::IntegerPercent(x) => container * (*x as f32 / 100.0),
            Self::RealVw(x) | Self::RealDvw(x) => view_width * (*x / 100.0),
            #[allow(clippy::cast_precision_loss)]
            Self::IntegerVw(x) | Self::IntegerDvw(x) => view_width * (*x as f32 / 100.0),
            Self::RealVh(x) | Self::RealDvh(x) => view_height * (*x / 100.0),
            #[allow(clippy::cast_precision_loss)]
            Self::IntegerVh(x) | Self::IntegerDvh(x) => view_height * (*x as f32 / 100.0),
            Self::Calc(x) => x.calc(container, view_width, view_height),
        }
    }

    #[must_use]
    pub fn as_dynamic(&self) -> Option<&Self> {
        match self {
            Self::RealPercent(_) | Self::IntegerPercent(_) => Some(self),
            Self::Real(_)
            | Self::Integer(_)
            | Self::RealDvw(_)
            | Self::IntegerDvw(_)
            | Self::RealDvh(_)
            | Self::IntegerDvh(_)
            | Self::RealVw(_)
            | Self::IntegerVw(_)
            | Self::RealVh(_)
            | Self::IntegerVh(_) => None,
            Self::Calc(x) => {
                if x.is_dynamic() {
                    Some(self)
                } else {
                    None
                }
            }
        }
    }

    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.as_dynamic().is_some()
    }

    #[must_use]
    pub fn as_fixed(&self) -> Option<&Self> {
        match self {
            Self::RealPercent(_) | Self::IntegerPercent(_) => None,
            Self::Real(_)
            | Self::Integer(_)
            | Self::RealDvw(_)
            | Self::IntegerDvw(_)
            | Self::RealDvh(_)
            | Self::IntegerDvh(_)
            | Self::RealVw(_)
            | Self::IntegerVw(_)
            | Self::RealVh(_)
            | Self::IntegerVh(_) => Some(self),
            Self::Calc(x) => {
                if x.is_fixed() {
                    Some(self)
                } else {
                    None
                }
            }
        }
    }

    #[must_use]
    pub fn is_fixed(&self) -> bool {
        self.as_fixed().is_some()
    }
}

#[cfg(test)]
mod test_number_deserialize {
    use pretty_assertions::assert_eq;
    use quickcheck_macros::quickcheck;

    use crate::Number;

    #[quickcheck]
    #[allow(clippy::needless_pass_by_value)]
    fn can_serialize_then_deserialize(number: Number) {
        log::trace!("number={number:?}");
        let serialized = serde_json::to_string(&number).unwrap();
        log::trace!("serialized={serialized}");
        let deserialized = serde_json::from_str(&serialized).unwrap();
        log::trace!("deserialized={deserialized:?}");

        assert_eq!(number, deserialized);
    }
}

static EPSILON: f32 = 0.00001;

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            #[allow(clippy::cast_precision_loss)]
            (Self::Real(float), Self::Integer(int))
            | (Self::RealPercent(float), Self::IntegerPercent(int))
            | (Self::RealVw(float), Self::IntegerVw(int))
            | (Self::RealVh(float), Self::IntegerVh(int))
            | (Self::RealDvw(float), Self::IntegerDvw(int))
            | (Self::RealDvh(float), Self::IntegerDvh(int))
            | (Self::Integer(int), Self::Real(float))
            | (Self::IntegerPercent(int), Self::RealPercent(float))
            | (Self::IntegerVw(int), Self::RealVw(float))
            | (Self::IntegerVh(int), Self::RealVh(float))
            | (Self::IntegerDvw(int), Self::RealDvw(float))
            | (Self::IntegerDvh(int), Self::RealDvh(float)) => {
                (*int as f32 - *float).abs() < EPSILON
            }
            (Self::Real(l), Self::Real(r))
            | (Self::RealPercent(l), Self::RealPercent(r))
            | (Self::RealVw(l), Self::RealVw(r))
            | (Self::RealVh(l), Self::RealVh(r))
            | (Self::RealDvw(l), Self::RealDvw(r))
            | (Self::RealDvh(l), Self::RealDvh(r)) => {
                l.is_infinite() && r.is_infinite()
                    || l.is_nan() && r.is_nan()
                    || (l - r).abs() < EPSILON
            }
            (Self::Integer(l), Self::Integer(r))
            | (Self::IntegerPercent(l), Self::IntegerPercent(r))
            | (Self::IntegerVw(l), Self::IntegerVw(r))
            | (Self::IntegerVh(l), Self::IntegerVh(r))
            | (Self::IntegerDvw(l), Self::IntegerDvw(r))
            | (Self::IntegerDvh(l), Self::IntegerDvh(r)) => l == r,
            (Self::Calc(l), Self::Calc(r)) => l == r,
            _ => false,
        }
    }
}

impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Real(x) => {
                if x.abs() < EPSILON {
                    return f.write_fmt(format_args!("0"));
                }
                f.write_fmt(format_args!("{x}"))
            }
            Self::Integer(x) => {
                if *x == 0 {
                    return f.write_fmt(format_args!("0"));
                }
                f.write_fmt(format_args!("{x}"))
            }
            Self::RealPercent(x) => {
                if x.abs() < EPSILON {
                    return f.write_fmt(format_args!("0%"));
                }
                f.write_fmt(format_args!("{x}%"))
            }
            Self::IntegerPercent(x) => {
                if *x == 0 {
                    return f.write_fmt(format_args!("0%"));
                }
                f.write_fmt(format_args!("{x}%"))
            }
            Self::RealVw(x) => {
                if x.abs() < EPSILON {
                    return f.write_fmt(format_args!("0vw"));
                }
                f.write_fmt(format_args!("{x}vw"))
            }
            Self::IntegerVw(x) => {
                if *x == 0 {
                    return f.write_fmt(format_args!("0vw"));
                }
                f.write_fmt(format_args!("{x}vw"))
            }
            Self::RealVh(x) => {
                if x.abs() < EPSILON {
                    return f.write_fmt(format_args!("0vh"));
                }
                f.write_fmt(format_args!("{x}vh"))
            }
            Self::IntegerVh(x) => {
                if *x == 0 {
                    return f.write_fmt(format_args!("0vh"));
                }
                f.write_fmt(format_args!("{x}vh"))
            }
            Self::RealDvw(x) => {
                if x.abs() < EPSILON {
                    return f.write_fmt(format_args!("0dvw"));
                }
                f.write_fmt(format_args!("{x}dvw"))
            }
            Self::IntegerDvw(x) => {
                if *x == 0 {
                    return f.write_fmt(format_args!("0dvw"));
                }
                f.write_fmt(format_args!("{x}dvw"))
            }
            Self::RealDvh(x) => {
                if x.abs() < EPSILON {
                    return f.write_fmt(format_args!("0dvh"));
                }
                f.write_fmt(format_args!("{x}dvh"))
            }
            Self::IntegerDvh(x) => {
                if *x == 0 {
                    return f.write_fmt(format_args!("0dvh"));
                }
                f.write_fmt(format_args!("{x}dvh"))
            }
            Self::Calc(x) => f.write_fmt(format_args!("calc({x})")),
        }
    }
}

impl From<f32> for Number {
    fn from(x: f32) -> Self {
        Self::Real(x)
    }
}

impl From<f64> for Number {
    fn from(x: f64) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self::Real(x as f32)
    }
}

impl From<i8> for Number {
    fn from(x: i8) -> Self {
        Self::Integer(x.into())
    }
}

impl From<i16> for Number {
    fn from(x: i16) -> Self {
        Self::Integer(x.into())
    }
}

impl From<i32> for Number {
    fn from(x: i32) -> Self {
        Self::Integer(x.into())
    }
}

impl From<i64> for Number {
    fn from(x: i64) -> Self {
        Self::Integer(x)
    }
}

impl From<u8> for Number {
    fn from(x: u8) -> Self {
        Self::Integer(x.into())
    }
}

impl From<u16> for Number {
    fn from(x: u16) -> Self {
        Self::Integer(x.into())
    }
}

impl From<u32> for Number {
    fn from(x: u32) -> Self {
        Self::Integer(x.into())
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<u64> for Number {
    fn from(x: u64) -> Self {
        Self::Integer(x.try_into().unwrap())
    }
}

impl Default for Number {
    fn default() -> Self {
        Self::Integer(0)
    }
}

impl From<String> for Number {
    fn from(x: String) -> Self {
        x.as_str().into()
    }
}

impl From<&String> for Number {
    fn from(x: &String) -> Self {
        x.as_str().into()
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<&str> for Number {
    fn from(x: &str) -> Self {
        parse_number(x).unwrap()
    }
}

#[cfg(feature = "logic")]
impl From<hyperchad_actions::logic::IfExpression<i32, hyperchad_actions::logic::Responsive>>
    for Number
{
    fn from(
        if_expr: hyperchad_actions::logic::IfExpression<i32, hyperchad_actions::logic::Responsive>,
    ) -> Self {
        if let Some(default) = if_expr.default {
            Self::Integer(i64::from(default))
        } else if let Some(value) = if_expr.value {
            Self::Integer(i64::from(value))
        } else {
            Self::Integer(0)
        }
    }
}

#[cfg(feature = "logic")]
impl From<hyperchad_actions::logic::IfExpression<i64, hyperchad_actions::logic::Responsive>>
    for Number
{
    fn from(
        if_expr: hyperchad_actions::logic::IfExpression<i64, hyperchad_actions::logic::Responsive>,
    ) -> Self {
        if let Some(default) = if_expr.default {
            Self::Integer(default)
        } else if let Some(value) = if_expr.value {
            Self::Integer(value)
        } else {
            Self::Integer(0)
        }
    }
}

#[cfg(feature = "logic")]
impl From<hyperchad_actions::logic::IfExpression<f32, hyperchad_actions::logic::Responsive>>
    for Number
{
    fn from(
        if_expr: hyperchad_actions::logic::IfExpression<f32, hyperchad_actions::logic::Responsive>,
    ) -> Self {
        if let Some(default) = if_expr.default {
            Self::Real(default)
        } else if let Some(value) = if_expr.value {
            Self::Real(value)
        } else {
            Self::Real(0.0)
        }
    }
}

#[cfg(feature = "logic")]
pub struct BoolWrapper(pub bool);

#[cfg(feature = "logic")]
impl From<bool> for BoolWrapper {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

#[cfg(feature = "logic")]
impl From<BoolWrapper> for bool {
    fn from(wrapper: BoolWrapper) -> Self {
        wrapper.0
    }
}

#[cfg(feature = "logic")]
impl From<hyperchad_actions::logic::IfExpression<bool, hyperchad_actions::logic::Responsive>>
    for BoolWrapper
{
    fn from(
        if_expr: hyperchad_actions::logic::IfExpression<bool, hyperchad_actions::logic::Responsive>,
    ) -> Self {
        if let Some(default) = if_expr.default {
            Self(default)
        } else if let Some(value) = if_expr.value {
            Self(value)
        } else {
            Self(false)
        }
    }
}

#[cfg(feature = "logic")]
impl<T> From<hyperchad_actions::logic::IfExpression<T, hyperchad_actions::logic::Responsive>>
    for Flex
where
    T: Into<Self>,
{
    fn from(
        if_expr: hyperchad_actions::logic::IfExpression<T, hyperchad_actions::logic::Responsive>,
    ) -> Self {
        if let Some(default) = if_expr.default {
            default.into()
        } else if let Some(value) = if_expr.value {
            value.into()
        } else {
            Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TextDecoration {
    pub color: Option<Color>,
    pub line: Vec<TextDecorationLine>,
    pub style: Option<TextDecorationStyle>,
    pub thickness: Option<Number>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flex {
    pub grow: Number,
    pub shrink: Number,
    pub basis: Number,
}

impl Default for Flex {
    fn default() -> Self {
        Self {
            grow: Number::Integer(1),
            shrink: Number::Integer(1),
            basis: Number::IntegerPercent(0),
        }
    }
}

impl From<i64> for Flex {
    fn from(i: i64) -> Self {
        Self {
            grow: Number::Integer(i),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
pub enum ResponsiveTrigger {
    MaxWidth(Number),
    MaxHeight(Number),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigOverride {
    pub condition: OverrideCondition,
    pub overrides: Vec<OverrideItem>,
    pub default: Option<OverrideItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverrideCondition {
    ResponsiveTarget { name: String },
}

impl From<String> for OverrideCondition {
    fn from(value: String) -> Self {
        Self::ResponsiveTarget { name: value }
    }
}

impl From<&str> for OverrideCondition {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

#[derive(Clone, Debug, PartialEq, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(OverrideItemType))]
pub enum OverrideItem {
    StrId(String),
    Classes(Vec<String>),
    Direction(LayoutDirection),
    OverflowX(LayoutOverflow),
    OverflowY(LayoutOverflow),
    GridCellSize(Number),
    JustifyContent(JustifyContent),
    AlignItems(AlignItems),
    TextAlign(TextAlign),
    WhiteSpace(WhiteSpace),
    TextDecoration(TextDecoration),
    FontFamily(Vec<String>),
    FontWeight(FontWeight),
    Width(Number),
    MinWidth(Number),
    MaxWidth(Number),
    Height(Number),
    MinHeight(Number),
    MaxHeight(Number),
    Flex(Flex),
    ColumnGap(Number),
    RowGap(Number),
    Opacity(Number),
    Left(Number),
    Right(Number),
    Top(Number),
    Bottom(Number),
    TranslateX(Number),
    TranslateY(Number),
    Cursor(Cursor),
    UserSelect(UserSelect),
    OverflowWrap(OverflowWrap),
    TextOverflow(TextOverflow),
    Position(Position),
    Background(Color),
    BorderTop((Color, Number)),
    BorderRight((Color, Number)),
    BorderBottom((Color, Number)),
    BorderLeft((Color, Number)),
    BorderTopLeftRadius(Number),
    BorderTopRightRadius(Number),
    BorderBottomLeftRadius(Number),
    BorderBottomRightRadius(Number),
    MarginLeft(Number),
    MarginRight(Number),
    MarginTop(Number),
    MarginBottom(Number),
    PaddingLeft(Number),
    PaddingRight(Number),
    PaddingTop(Number),
    PaddingBottom(Number),
    FontSize(Number),
    Color(Color),
    Hidden(bool),
    Visibility(Visibility),
}

impl OverrideItem {
    /// # Errors
    ///
    /// * If the serialization fails
    pub fn serialize(&self) -> Result<String, serde_json::Error> {
        match self {
            Self::StrId(x) => serde_json::to_string(x),
            Self::Direction(x) => serde_json::to_string(x),
            Self::OverflowX(x) | Self::OverflowY(x) => serde_json::to_string(x),
            Self::JustifyContent(x) => serde_json::to_string(x),
            Self::AlignItems(x) => serde_json::to_string(x),
            Self::TextAlign(x) => serde_json::to_string(x),
            Self::WhiteSpace(x) => serde_json::to_string(x),
            Self::TextDecoration(x) => serde_json::to_string(x),
            Self::Classes(x) | Self::FontFamily(x) => serde_json::to_string(x),
            Self::FontWeight(x) => serde_json::to_string(x),
            Self::Flex(x) => serde_json::to_string(x),
            Self::Width(x)
            | Self::MinWidth(x)
            | Self::MaxWidth(x)
            | Self::Height(x)
            | Self::MinHeight(x)
            | Self::MaxHeight(x)
            | Self::ColumnGap(x)
            | Self::RowGap(x)
            | Self::Opacity(x)
            | Self::Left(x)
            | Self::Right(x)
            | Self::Top(x)
            | Self::Bottom(x)
            | Self::TranslateX(x)
            | Self::TranslateY(x)
            | Self::BorderTopLeftRadius(x)
            | Self::BorderTopRightRadius(x)
            | Self::BorderBottomLeftRadius(x)
            | Self::BorderBottomRightRadius(x)
            | Self::MarginLeft(x)
            | Self::MarginRight(x)
            | Self::MarginTop(x)
            | Self::MarginBottom(x)
            | Self::PaddingLeft(x)
            | Self::PaddingRight(x)
            | Self::PaddingTop(x)
            | Self::PaddingBottom(x)
            | Self::FontSize(x)
            | Self::GridCellSize(x) => serde_json::to_string(x),
            Self::Cursor(x) => serde_json::to_string(x),
            Self::UserSelect(x) => serde_json::to_string(x),
            Self::OverflowWrap(x) => serde_json::to_string(x),
            Self::TextOverflow(x) => serde_json::to_string(x),
            Self::Position(x) => serde_json::to_string(x),
            Self::BorderTop(x)
            | Self::BorderRight(x)
            | Self::BorderBottom(x)
            | Self::BorderLeft(x) => serde_json::to_string(x),
            Self::Background(x) | Self::Color(x) => serde_json::to_string(x),
            Self::Hidden(x) => serde_json::to_string(x),
            Self::Visibility(x) => serde_json::to_string(x),
        }
    }

    /// # Errors
    ///
    /// * If the serialization fails
    pub fn as_value(&self) -> Result<Value, serde_json::Error> {
        match self {
            Self::StrId(x) => serde_json::to_value(x),
            Self::Direction(x) => serde_json::to_value(x),
            Self::OverflowX(x) | Self::OverflowY(x) => serde_json::to_value(x),
            Self::JustifyContent(x) => serde_json::to_value(x),
            Self::AlignItems(x) => serde_json::to_value(x),
            Self::TextAlign(x) => serde_json::to_value(x),
            Self::WhiteSpace(x) => serde_json::to_value(x),
            Self::TextDecoration(x) => serde_json::to_value(x),
            Self::Classes(x) | Self::FontFamily(x) => serde_json::to_value(x),
            Self::FontWeight(x) => serde_json::to_value(x),
            Self::Flex(x) => serde_json::to_value(x),
            Self::Width(x)
            | Self::MinWidth(x)
            | Self::MaxWidth(x)
            | Self::Height(x)
            | Self::MinHeight(x)
            | Self::MaxHeight(x)
            | Self::ColumnGap(x)
            | Self::RowGap(x)
            | Self::Opacity(x)
            | Self::Left(x)
            | Self::Right(x)
            | Self::Top(x)
            | Self::Bottom(x)
            | Self::TranslateX(x)
            | Self::TranslateY(x)
            | Self::BorderTopLeftRadius(x)
            | Self::BorderTopRightRadius(x)
            | Self::BorderBottomLeftRadius(x)
            | Self::BorderBottomRightRadius(x)
            | Self::MarginLeft(x)
            | Self::MarginRight(x)
            | Self::MarginTop(x)
            | Self::MarginBottom(x)
            | Self::PaddingLeft(x)
            | Self::PaddingRight(x)
            | Self::PaddingTop(x)
            | Self::PaddingBottom(x)
            | Self::FontSize(x)
            | Self::GridCellSize(x) => serde_json::to_value(x),
            Self::Cursor(x) => serde_json::to_value(x),
            Self::UserSelect(x) => serde_json::to_value(x),
            Self::OverflowWrap(x) => serde_json::to_value(x),
            Self::TextOverflow(x) => serde_json::to_value(x),
            Self::Position(x) => serde_json::to_value(x),
            Self::BorderTop(x)
            | Self::BorderRight(x)
            | Self::BorderBottom(x)
            | Self::BorderLeft(x) => serde_json::to_value(x),
            Self::Background(x) | Self::Color(x) => serde_json::to_value(x),
            Self::Hidden(x) => serde_json::to_value(x),
            Self::Visibility(x) => serde_json::to_value(x),
        }
    }

    /// # Errors
    ///
    /// * If the serialization fails
    #[must_use]
    pub fn as_any<'a>(&'a self) -> Box<dyn Any + 'a> {
        match self {
            Self::StrId(x) => Box::new(x),
            Self::Direction(x) => Box::new(x),
            Self::OverflowX(x) | Self::OverflowY(x) => Box::new(x),
            Self::JustifyContent(x) => Box::new(x),
            Self::AlignItems(x) => Box::new(x),
            Self::TextAlign(x) => Box::new(x),
            Self::WhiteSpace(x) => Box::new(x),
            Self::TextDecoration(x) => Box::new(x),
            Self::Classes(x) | Self::FontFamily(x) => Box::new(x),
            Self::FontWeight(x) => Box::new(x),
            Self::Flex(x) => Box::new(x),
            Self::Width(x)
            | Self::MinWidth(x)
            | Self::MaxWidth(x)
            | Self::Height(x)
            | Self::MinHeight(x)
            | Self::MaxHeight(x)
            | Self::ColumnGap(x)
            | Self::RowGap(x)
            | Self::Opacity(x)
            | Self::Left(x)
            | Self::Right(x)
            | Self::Top(x)
            | Self::Bottom(x)
            | Self::TranslateX(x)
            | Self::TranslateY(x)
            | Self::BorderTopLeftRadius(x)
            | Self::BorderTopRightRadius(x)
            | Self::BorderBottomLeftRadius(x)
            | Self::BorderBottomRightRadius(x)
            | Self::MarginLeft(x)
            | Self::MarginRight(x)
            | Self::MarginTop(x)
            | Self::MarginBottom(x)
            | Self::PaddingLeft(x)
            | Self::PaddingRight(x)
            | Self::PaddingTop(x)
            | Self::PaddingBottom(x)
            | Self::FontSize(x)
            | Self::GridCellSize(x) => Box::new(x),
            Self::Cursor(x) => Box::new(x),
            Self::UserSelect(x) => Box::new(x),
            Self::OverflowWrap(x) => Box::new(x),
            Self::TextOverflow(x) => Box::new(x),
            Self::Position(x) => Box::new(x),
            Self::BorderTop(x)
            | Self::BorderRight(x)
            | Self::BorderBottom(x)
            | Self::BorderLeft(x) => Box::new(x),
            Self::Background(x) | Self::Color(x) => Box::new(x),
            Self::Hidden(x) => Box::new(x),
            Self::Visibility(x) => Box::new(x),
        }
    }

    /// # Errors
    ///
    /// * If the serialization fails
    #[cfg(feature = "logic")]
    #[allow(clippy::too_many_lines)]
    fn as_json_if_expression_string(
        &self,
        responsive: hyperchad_actions::logic::Responsive,
        default: Option<&Self>,
    ) -> Result<String, serde_json::Error> {
        match self {
            Self::StrId(x) => {
                let mut expr = responsive.then::<&String>(x);

                if let Some(Self::StrId(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Direction(x) => {
                let mut expr = responsive.then::<&LayoutDirection>(x);

                if let Some(Self::Direction(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::OverflowX(x) | Self::OverflowY(x) => {
                let mut expr = responsive.then::<&LayoutOverflow>(x);

                if let Some(Self::OverflowX(default) | Self::OverflowY(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::JustifyContent(x) => {
                let mut expr = responsive.then::<&JustifyContent>(x);

                if let Some(Self::JustifyContent(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::AlignItems(x) => {
                let mut expr = responsive.then::<&AlignItems>(x);

                if let Some(Self::AlignItems(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::TextAlign(x) => {
                let mut expr = responsive.then::<&TextAlign>(x);

                if let Some(Self::TextAlign(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::WhiteSpace(x) => {
                let mut expr = responsive.then::<&WhiteSpace>(x);

                if let Some(Self::WhiteSpace(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::TextDecoration(x) => {
                let mut expr = responsive.then::<&TextDecoration>(x);

                if let Some(Self::TextDecoration(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Classes(x) | Self::FontFamily(x) => {
                let mut expr = responsive.then::<&Vec<String>>(x);

                if let Some(Self::Classes(default) | Self::FontFamily(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::FontWeight(x) => {
                let mut expr = responsive.then::<&FontWeight>(x);

                if let Some(Self::FontWeight(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Flex(x) => {
                let mut expr = responsive.then::<&Flex>(x);

                if let Some(Self::Flex(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Width(x)
            | Self::MinWidth(x)
            | Self::MaxWidth(x)
            | Self::Height(x)
            | Self::MinHeight(x)
            | Self::MaxHeight(x)
            | Self::ColumnGap(x)
            | Self::RowGap(x)
            | Self::Opacity(x)
            | Self::Left(x)
            | Self::Right(x)
            | Self::Top(x)
            | Self::Bottom(x)
            | Self::TranslateX(x)
            | Self::TranslateY(x)
            | Self::BorderTopLeftRadius(x)
            | Self::BorderTopRightRadius(x)
            | Self::BorderBottomLeftRadius(x)
            | Self::BorderBottomRightRadius(x)
            | Self::MarginLeft(x)
            | Self::MarginRight(x)
            | Self::MarginTop(x)
            | Self::MarginBottom(x)
            | Self::PaddingLeft(x)
            | Self::PaddingRight(x)
            | Self::PaddingTop(x)
            | Self::PaddingBottom(x)
            | Self::FontSize(x)
            | Self::GridCellSize(x) => {
                let mut expr = responsive.then::<&Number>(x);

                if let Some(
                    Self::Width(default)
                    | Self::MinWidth(default)
                    | Self::MaxWidth(default)
                    | Self::Height(default)
                    | Self::MinHeight(default)
                    | Self::MaxHeight(default)
                    | Self::ColumnGap(default)
                    | Self::RowGap(default)
                    | Self::Opacity(default)
                    | Self::Left(default)
                    | Self::Right(default)
                    | Self::Top(default)
                    | Self::Bottom(default)
                    | Self::TranslateX(default)
                    | Self::TranslateY(default)
                    | Self::BorderTopLeftRadius(default)
                    | Self::BorderTopRightRadius(default)
                    | Self::BorderBottomLeftRadius(default)
                    | Self::BorderBottomRightRadius(default)
                    | Self::MarginLeft(default)
                    | Self::MarginRight(default)
                    | Self::MarginTop(default)
                    | Self::MarginBottom(default)
                    | Self::PaddingLeft(default)
                    | Self::PaddingRight(default)
                    | Self::PaddingTop(default)
                    | Self::PaddingBottom(default)
                    | Self::FontSize(default)
                    | Self::GridCellSize(default),
                ) = default
                {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Cursor(x) => {
                let mut expr = responsive.then::<&Cursor>(x);

                if let Some(Self::Cursor(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::UserSelect(x) => {
                let mut expr = responsive.then::<&UserSelect>(x);

                if let Some(Self::UserSelect(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::OverflowWrap(x) => {
                let mut expr = responsive.then::<&OverflowWrap>(x);

                if let Some(Self::OverflowWrap(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::TextOverflow(x) => {
                let mut expr = responsive.then::<&TextOverflow>(x);

                if let Some(Self::TextOverflow(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Position(x) => {
                let mut expr = responsive.then::<&Position>(x);

                if let Some(Self::Position(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::BorderTop(x)
            | Self::BorderRight(x)
            | Self::BorderBottom(x)
            | Self::BorderLeft(x) => {
                let mut expr = responsive.then::<&(Color, Number)>(x);

                if let Some(
                    Self::BorderTop(default)
                    | Self::BorderRight(default)
                    | Self::BorderBottom(default)
                    | Self::BorderLeft(default),
                ) = default
                {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Background(x) | Self::Color(x) => {
                let mut expr = responsive.then::<&Color>(x);

                if let Some(Self::Background(default) | Self::Color(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Hidden(x) => {
                let mut expr = responsive.then::<&bool>(x);

                if let Some(Self::Hidden(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
            Self::Visibility(x) => {
                let mut expr = responsive.then::<&Visibility>(x);

                if let Some(Self::Visibility(default)) = default {
                    expr = expr.or_else(default);
                }

                serde_json::to_string(&expr)
            }
        }
    }
}

#[macro_export]
macro_rules! override_item {
    ($val:expr, $name:ident, $action:expr) => {{
        match $val {
            OverrideItem::StrId($name) => $action,
            OverrideItem::Data($name) => $action,
            OverrideItem::Direction($name) => $action,
            OverrideItem::OverflowX($name) | OverrideItem::OverflowY($name) => $action,
            OverrideItem::JustifyContent($name) => $action,
            OverrideItem::AlignItems($name) => $action,
            OverrideItem::TextAlign($name) => $action,
            OverrideItem::WhiteSpace($name) => $action,
            OverrideItem::TextDecoration($name) => $action,
            OverrideItem::Classes($name) | OverrideItem::FontFamily($name) => $action,
            OverrideItem::FontWeight($name) => $action,
            OverrideItem::Flex($name) => $action,
            OverrideItem::Width($name)
            | OverrideItem::MinWidth($name)
            | OverrideItem::MaxWidth($name)
            | OverrideItem::Height($name)
            | OverrideItem::MinHeight($name)
            | OverrideItem::MaxHeight($name)
            | OverrideItem::ColumnGap($name)
            | OverrideItem::RowGap($name)
            | OverrideItem::Opacity($name)
            | OverrideItem::Left($name)
            | OverrideItem::Right($name)
            | OverrideItem::Top($name)
            | OverrideItem::Bottom($name)
            | OverrideItem::TranslateX($name)
            | OverrideItem::TranslateY($name)
            | OverrideItem::BorderTopLeftRadius($name)
            | OverrideItem::BorderTopRightRadius($name)
            | OverrideItem::BorderBottomLeftRadius($name)
            | OverrideItem::BorderBottomRightRadius($name)
            | OverrideItem::MarginLeft($name)
            | OverrideItem::MarginRight($name)
            | OverrideItem::MarginTop($name)
            | OverrideItem::MarginBottom($name)
            | OverrideItem::PaddingLeft($name)
            | OverrideItem::PaddingRight($name)
            | OverrideItem::PaddingTop($name)
            | OverrideItem::PaddingBottom($name)
            | OverrideItem::FontSize($name)
            | OverrideItem::GridCellSize($name) => $action,
            OverrideItem::Cursor($name) => $action,
            OverrideItem::UserSelect($name) => $action,
            OverrideItem::OverflowWrap($name) => $action,
            OverrideItem::TextOverflow($name) => $action,
            OverrideItem::Position($name) => $action,
            OverrideItem::BorderTop($name)
            | OverrideItem::BorderRight($name)
            | OverrideItem::BorderBottom($name)
            | OverrideItem::BorderLeft($name) => $action,
            OverrideItem::Background($name) | OverrideItem::Color($name) => $action,
            OverrideItem::Hidden($name) | OverrideItem::Debug($name) => $action,
            OverrideItem::Visibility($name) => $action,
        }
    }};
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    pub id: usize,
    pub str_id: Option<String>,
    pub classes: Vec<String>,
    pub data: BTreeMap<String, String>,
    pub element: Element,
    pub children: Vec<Self>,
    pub direction: LayoutDirection,
    pub overflow_x: LayoutOverflow,
    pub overflow_y: LayoutOverflow,
    pub grid_cell_size: Option<Number>,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub text_align: Option<TextAlign>,
    pub white_space: Option<WhiteSpace>,
    pub text_decoration: Option<TextDecoration>,
    pub font_family: Option<Vec<String>>,
    pub font_weight: Option<FontWeight>,
    pub width: Option<Number>,
    pub min_width: Option<Number>,
    pub max_width: Option<Number>,
    pub height: Option<Number>,
    pub min_height: Option<Number>,
    pub max_height: Option<Number>,
    pub flex: Option<Flex>,
    pub column_gap: Option<Number>,
    pub row_gap: Option<Number>,
    pub opacity: Option<Number>,
    pub left: Option<Number>,
    pub right: Option<Number>,
    pub top: Option<Number>,
    pub bottom: Option<Number>,
    pub translate_x: Option<Number>,
    pub translate_y: Option<Number>,
    pub cursor: Option<Cursor>,
    pub user_select: Option<UserSelect>,
    pub overflow_wrap: Option<OverflowWrap>,
    pub text_overflow: Option<TextOverflow>,
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
    pub font_size: Option<Number>,
    pub color: Option<Color>,
    pub state: Option<Value>,
    pub hidden: Option<bool>,
    pub debug: Option<bool>,
    pub visibility: Option<Visibility>,
    pub route: Option<Route>,
    pub actions: Vec<Action>,
    pub overrides: Vec<ConfigOverride>,
    #[cfg(feature = "layout")]
    pub calculated_margin_left: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_margin_right: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_margin_top: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_margin_bottom: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_padding_left: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_padding_right: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_padding_top: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_padding_bottom: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_min_width: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_child_min_width: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_max_width: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_preferred_width: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_width: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_min_height: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_child_min_height: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_max_height: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_preferred_height: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_height: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_x: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_y: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_position: Option<hyperchad_transformer_models::LayoutPosition>,
    #[cfg(feature = "layout")]
    pub calculated_border_top: Option<(Color, f32)>,
    #[cfg(feature = "layout")]
    pub calculated_border_right: Option<(Color, f32)>,
    #[cfg(feature = "layout")]
    pub calculated_border_bottom: Option<(Color, f32)>,
    #[cfg(feature = "layout")]
    pub calculated_border_left: Option<(Color, f32)>,
    #[cfg(feature = "layout")]
    pub calculated_border_top_left_radius: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_border_top_right_radius: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_border_bottom_left_radius: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_border_bottom_right_radius: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_column_gap: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_row_gap: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_opacity: Option<f32>,
    #[cfg(feature = "layout")]
    pub calculated_font_size: Option<f32>,
    #[cfg(feature = "layout")]
    pub scrollbar_right: Option<f32>,
    #[cfg(feature = "layout")]
    pub scrollbar_bottom: Option<f32>,
    #[cfg(feature = "layout-offset")]
    pub calculated_offset_x: Option<f32>,
    #[cfg(feature = "layout-offset")]
    pub calculated_offset_y: Option<f32>,
}

impl AsRef<Self> for Container {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl Container {
    pub fn iter_overrides(&self, recurse: bool) -> impl Iterator<Item = (&Self, &ConfigOverride)> {
        let mut iter: Box<dyn Iterator<Item = (&Self, &ConfigOverride)>> =
            if self.overrides.is_empty() {
                Box::new(std::iter::empty())
            } else {
                Box::new(self.overrides.iter().map(move |x| (self, x)))
            };

        if recurse {
            for child in &self.children {
                iter = Box::new(iter.chain(child.iter_overrides(true)));
            }
        }

        iter
    }

    #[must_use]
    pub fn bfs(&self) -> BfsPaths {
        // Collect nodes in pre-order, recording their path
        fn collect_paths(
            node: &Container,
            path: &[usize],
            paths: &mut Vec<Vec<usize>>,
            levels: &mut Vec<Vec<usize>>,
        ) {
            if !node.children.is_empty() {
                // Store the path to this node
                paths.push(path.to_owned());

                // Add this node's index to the appropriate level
                let level = path.len(); // Path length = level + 1 (root is at index 0)
                if levels.len() <= level {
                    levels.resize(level + 1, Vec::new());
                }
                levels[level].push(paths.len() - 1);
                // Process children
                for (i, child) in node.children.iter().enumerate() {
                    let mut child_path = path.to_owned();
                    child_path.push(i);
                    collect_paths(child, &child_path, paths, levels);
                }
            }
        }

        // Collect nodes by level
        let mut levels: Vec<Vec<usize>> = Vec::new();

        // Start by collecting all paths to nodes
        let mut paths: Vec<Vec<usize>> = Vec::new();
        collect_paths(self, &[], &mut paths, &mut levels);

        BfsPaths { levels, paths }
    }

    #[must_use]
    pub fn bfs_visit(&self, mut visitor: impl FnMut(&Self)) -> BfsPaths {
        // Collect nodes in pre-order, recording their path
        fn collect_paths(
            node: &Container,
            path: &[usize],
            paths: &mut Vec<Vec<usize>>,
            levels: &mut Vec<Vec<usize>>,
            visitor: &mut impl FnMut(&Container),
        ) {
            if !node.children.is_empty() {
                // Store the path to this node
                paths.push(path.to_owned());

                // Add this node's index to the appropriate level
                let level = path.len(); // Path length = level + 1 (root is at index 0)
                if levels.len() <= level {
                    levels.resize(level + 1, Vec::new());
                }
                levels[level].push(paths.len() - 1);
                // Process children
                for (i, child) in node.children.iter().enumerate() {
                    visitor(child);
                    let mut child_path = path.to_owned();
                    child_path.push(i);
                    collect_paths(child, &child_path, paths, levels, visitor);
                }
            }
        }

        // Collect nodes by level
        let mut levels: Vec<Vec<usize>> = Vec::new();

        // Start by collecting all paths to nodes
        let mut paths: Vec<Vec<usize>> = Vec::new();

        visitor(self);
        collect_paths(self, &[], &mut paths, &mut levels, &mut visitor);

        BfsPaths { levels, paths }
    }

    #[must_use]
    pub fn bfs_visit_mut(&mut self, mut visitor: impl FnMut(&mut Self)) -> BfsPaths {
        // Collect nodes in pre-order, recording their path
        fn collect_paths(
            node: &mut Container,
            path: &[usize],
            paths: &mut Vec<Vec<usize>>,
            levels: &mut Vec<Vec<usize>>,
            visitor: &mut impl FnMut(&mut Container),
        ) {
            if !node.children.is_empty() {
                // Store the path to this node
                paths.push(path.to_owned());

                // Add this node's index to the appropriate level
                let level = path.len(); // Path length = level + 1 (root is at index 0)
                if levels.len() <= level {
                    levels.resize(level + 1, Vec::new());
                }
                levels[level].push(paths.len() - 1);
                // Process children
                for (i, child) in node.children.iter_mut().enumerate() {
                    visitor(child);
                    let mut child_path = path.to_owned();
                    child_path.push(i);
                    collect_paths(child, &child_path, paths, levels, visitor);
                }
            }
        }

        // Collect nodes by level
        let mut levels: Vec<Vec<usize>> = Vec::new();

        // Start by collecting all paths to nodes
        let mut paths: Vec<Vec<usize>> = Vec::new();

        visitor(self);
        collect_paths(self, &[], &mut paths, &mut levels, &mut visitor);

        BfsPaths { levels, paths }
    }
}

impl From<&Container> for BfsPaths {
    fn from(root: &Container) -> Self {
        root.bfs()
    }
}

pub struct BfsPaths {
    levels: Vec<Vec<usize>>,
    paths: Vec<Vec<usize>>,
}

impl BfsPaths {
    pub fn traverse(&self, root: &Container, mut visitor: impl FnMut(&Container)) {
        // Follow paths to apply visitor to each node
        for level_nodes in &self.levels {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = root;

                for &child_idx in path {
                    current = &current.children[child_idx];
                }

                visitor(current);
            }
        }
    }

    pub fn traverse_mut(&self, root: &mut Container, mut visitor: impl FnMut(&mut Container)) {
        // Follow paths to apply visitor to each node
        for level_nodes in &self.levels {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = &mut *root;

                for &child_idx in path {
                    current = &mut current.children[child_idx];
                }

                visitor(current);
            }
        }
    }

    pub fn traverse_with_parents<R: Clone>(
        &self,
        inclusive: bool,
        initial: R,
        root: &Container,
        mut parent: impl FnMut(&Container, R) -> R,
        mut visitor: impl FnMut(&Container, R),
    ) {
        // Follow paths to apply visitor to each node
        for level_nodes in &self.levels {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = root;
                let mut data = initial.clone();

                for &child_idx in path {
                    data = parent(current, data);
                    current = &current.children[child_idx];
                }

                if inclusive {
                    data = parent(current, data);
                }

                visitor(current, data);
            }
        }
    }

    pub fn traverse_with_parents_mut<R: Clone>(
        &self,
        inclusive: bool,
        initial: R,
        root: &mut Container,
        mut parent: impl FnMut(&mut Container, R) -> R,
        mut visitor: impl FnMut(&mut Container, R),
    ) {
        // Follow paths to apply visitor to each node
        for level_nodes in &self.levels {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = &mut *root;
                let mut data = initial.clone();

                for &child_idx in path {
                    data = parent(current, data);
                    current = &mut current.children[child_idx];
                }

                if inclusive {
                    data = parent(current, data);
                }

                visitor(current, data);
            }
        }
    }

    pub fn traverse_with_parents_ref<R>(
        &self,
        inclusive: bool,
        data: &mut R,
        root: &Container,
        mut parent: impl FnMut(&Container, &mut R),
        mut visitor: impl FnMut(&Container, &mut R),
    ) {
        // Follow paths to apply visitor to each node
        for level_nodes in &self.levels {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = root;

                for &child_idx in path {
                    parent(current, data);
                    current = &current.children[child_idx];
                }

                if inclusive {
                    parent(current, data);
                }

                visitor(current, data);
            }
        }
    }

    pub fn traverse_with_parents_ref_mut<R>(
        &self,
        inclusive: bool,
        data: &mut R,
        root: &mut Container,
        mut parent: impl FnMut(&mut Container, &mut R),
        mut visitor: impl FnMut(&mut Container, &mut R),
    ) {
        // Follow paths to apply visitor to each node
        for level_nodes in &self.levels {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = &mut *root;

                for &child_idx in path {
                    parent(current, data);
                    current = &mut current.children[child_idx];
                }

                if inclusive {
                    parent(current, data);
                }

                visitor(current, data);
            }
        }
    }

    pub fn traverse_rev(&self, root: &Container, mut visitor: impl FnMut(&Container)) {
        // Follow paths to apply visitor to each node
        for level_nodes in self.levels.iter().rev() {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = root;

                for &child_idx in path {
                    current = &current.children[child_idx];
                }

                visitor(current);
            }
        }
    }

    pub fn traverse_rev_mut(&self, root: &mut Container, mut visitor: impl FnMut(&mut Container)) {
        // Follow paths to apply visitor to each node
        for level_nodes in self.levels.iter().rev() {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = &mut *root;

                for &child_idx in path {
                    current = &mut current.children[child_idx];
                }

                visitor(current);
            }
        }
    }

    pub fn traverse_rev_with_parents<R: Clone>(
        &self,
        inclusive: bool,
        initial: R,
        root: &Container,
        mut parent: impl FnMut(&Container, R) -> R,
        mut visitor: impl FnMut(&Container, R),
    ) {
        // Follow paths to apply visitor to each node
        for level_nodes in self.levels.iter().rev() {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = root;
                let mut data = initial.clone();

                for &child_idx in path {
                    data = parent(current, data);
                    current = &current.children[child_idx];
                }

                if inclusive {
                    data = parent(current, data);
                }

                visitor(current, data);
            }
        }
    }

    pub fn traverse_rev_with_parents_mut<R: Clone>(
        &self,
        inclusive: bool,
        initial: R,
        root: &mut Container,
        mut parent: impl FnMut(&mut Container, R) -> R,
        mut visitor: impl FnMut(&mut Container, R),
    ) {
        // Follow paths to apply visitor to each node
        for level_nodes in self.levels.iter().rev() {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = &mut *root;
                let mut data = initial.clone();

                for &child_idx in path {
                    data = parent(current, data);
                    current = &mut current.children[child_idx];
                }

                if inclusive {
                    data = parent(current, data);
                }

                visitor(current, data);
            }
        }
    }

    pub fn traverse_rev_with_parents_ref<R>(
        &self,
        inclusive: bool,
        initial: R,
        root: &Container,
        mut parent: impl FnMut(&Container, R) -> R,
        mut visitor: impl FnMut(&Container, &R),
    ) {
        let mut data = initial;

        // Follow paths to apply visitor to each node
        for level_nodes in self.levels.iter().rev() {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = root;

                for &child_idx in path {
                    data = parent(current, data);
                    current = &current.children[child_idx];
                }

                if inclusive {
                    data = parent(current, data);
                }

                visitor(current, &data);
            }
        }
    }

    pub fn traverse_rev_with_parents_ref_mut<R>(
        &self,
        inclusive: bool,
        initial: R,
        root: &mut Container,
        mut parent: impl FnMut(&mut Container, R) -> R,
        mut visitor: impl FnMut(&mut Container, &R),
    ) {
        let mut data = initial;

        // Follow paths to apply visitor to each node
        for level_nodes in self.levels.iter().rev() {
            for &node_idx in level_nodes {
                let path = &self.paths[node_idx];

                // Follow the path to find the node
                let mut current = &mut *root;

                for &child_idx in path {
                    data = parent(current, data);
                    current = &mut current.children[child_idx];
                }

                if inclusive {
                    data = parent(current, data);
                }

                visitor(current, &data);
            }
        }
    }
}

impl From<Vec<Self>> for Container {
    fn from(value: Vec<Self>) -> Self {
        Self {
            element: Element::Div,
            children: value,
            ..Default::default()
        }
    }
}

fn visible_elements(elements: &[Container]) -> impl Iterator<Item = &Container> {
    elements.iter().filter(|x| x.hidden != Some(true))
}

fn visible_elements_mut(elements: &mut [Container]) -> impl Iterator<Item = &mut Container> {
    elements.iter_mut().filter(|x| x.hidden != Some(true))
}

fn relative_positioned_elements(elements: &[Container]) -> impl Iterator<Item = &Container> {
    visible_elements(elements).filter(|x| x.position.is_none_or(Position::is_relative))
}

fn relative_positioned_elements_mut(
    elements: &mut [Container],
) -> impl Iterator<Item = &mut Container> {
    visible_elements_mut(elements).filter(|x| x.position.is_none_or(Position::is_relative))
}

fn absolute_positioned_elements(elements: &[Container]) -> impl Iterator<Item = &Container> {
    visible_elements(elements).filter(|x| x.position == Some(Position::Absolute))
}

fn absolute_positioned_elements_mut(
    elements: &mut [Container],
) -> impl Iterator<Item = &mut Container> {
    visible_elements_mut(elements).filter(|x| x.position == Some(Position::Absolute))
}

fn fixed_positioned_elements(elements: &[Container]) -> impl Iterator<Item = &Container> {
    visible_elements(elements).filter(|x| x.is_fixed())
}

fn fixed_positioned_elements_mut(
    elements: &mut [Container],
) -> impl Iterator<Item = &mut Container> {
    visible_elements_mut(elements).filter(|x| x.is_fixed())
}

impl Container {
    #[must_use]
    pub const fn is_fixed(&self) -> bool {
        matches!(self.position, Some(Position::Fixed | Position::Sticky))
    }

    #[must_use]
    pub const fn is_raw(&self) -> bool {
        matches!(self.element, Element::Raw { .. })
    }
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

    #[must_use]
    pub fn is_span(&self) -> bool {
        matches!(
            self.element,
            Element::Raw { .. }
                | Element::Span
                | Element::Anchor { .. }
                | Element::Input { .. }
                | Element::Button { .. }
                | Element::Image { .. }
        ) && self.children.iter().all(Self::is_span)
    }

    #[must_use]
    pub fn is_flex_container(&self) -> bool {
        self.direction == LayoutDirection::Row
            || self.justify_content.is_some()
            || self.align_items.is_some()
            || self.children.iter().any(|x| x.flex.is_some())
            || self.column_gap.is_some()
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

    pub fn fixed_positioned_elements(&self) -> impl Iterator<Item = &Self> {
        fixed_positioned_elements(&self.children)
    }

    pub fn fixed_positioned_elements_mut(&mut self) -> impl Iterator<Item = &mut Self> {
        fixed_positioned_elements_mut(&mut self.children)
    }

    #[must_use]
    pub fn find_element_by_id(&self, id: usize) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|x| x.find_element_by_id(id))
    }

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
    pub fn find_element_by_class(&self, class: &str) -> Option<&Self> {
        if self.classes.iter().any(|x| x == class) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|x| x.find_element_by_class(class))
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

    #[must_use]
    pub fn find_parent_by_id(&self, id: usize) -> Option<&Self> {
        if self.children.iter().any(|x| x.id == id) {
            Some(self)
        } else {
            self.children.iter().find_map(|x| x.find_parent_by_id(id))
        }
    }

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

    /// # Panics
    ///
    /// * If the `Container` is the root node
    /// * If the `Container` is not properly attached to the tree
    #[must_use]
    pub fn replace_with_elements(&mut self, replacement: Vec<Self>, root: &mut Self) -> Self {
        let Some(parent) = root.find_parent_by_id_mut(self.id) else {
            panic!("Cannot replace the root node with multiple elements");
        };

        let index = parent
            .children
            .iter()
            .enumerate()
            .find_map(|(i, x)| if x.id == self.id { Some(i) } else { None })
            .unwrap_or_else(|| panic!("Container is not attached properly to tree"));

        let original = parent.children.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.children.insert(index + i, element);
        }

        original
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    pub fn replace_id_children_with_elements(
        &mut self,
        replacement: Vec<Self>,
        id: usize,
    ) -> Option<Vec<Self>> {
        let parent = self.find_element_by_id_mut(id)?;

        let original = parent.children.drain(..).collect::<Vec<_>>();

        for element in replacement {
            parent.children.push(element);
        }

        Some(original)
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    pub fn replace_str_id_children_with_elements(
        &mut self,
        replacement: Vec<Self>,
        id: &str,
    ) -> Option<Vec<Self>> {
        let parent = self.find_element_by_str_id_mut(id)?;

        let original = parent.children.drain(..).collect::<Vec<_>>();

        for element in replacement {
            parent.children.push(element);
        }

        Some(original)
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "layout")]
    pub fn replace_id_children_with_elements_calc(
        &mut self,
        calculator: &impl layout::Calc,
        replacement: Vec<Self>,
        id: usize,
    ) -> bool {
        let Some(parent_id) = self.find_element_by_id(id).map(|x| x.id) else {
            return false;
        };

        self.replace_id_children_with_elements(replacement, id);

        self.partial_calc(calculator, parent_id);

        true
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "layout")]
    pub fn replace_str_id_children_with_elements_calc(
        &mut self,
        calculator: &impl layout::Calc,
        replacement: Vec<Self>,
        id: &str,
    ) -> bool {
        let Some(parent_id) = self.find_element_by_str_id(id).map(|x| x.id) else {
            return false;
        };

        self.replace_str_id_children_with_elements(replacement, id);

        self.partial_calc(calculator, parent_id);

        true
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    pub fn replace_id_with_elements(&mut self, replacement: Vec<Self>, id: usize) -> Option<Self> {
        let parent = self.find_parent_by_id_mut(id)?;

        let index = parent
            .children
            .iter()
            .enumerate()
            .find_map(|(i, x)| if x.id == id { Some(i) } else { None })?;

        let original = parent.children.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.children.insert(index + i, element);
        }

        Some(original)
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "layout")]
    pub fn replace_id_with_elements_calc(
        &mut self,
        calculator: &impl layout::Calc,
        replacement: Vec<Self>,
        id: usize,
    ) -> bool {
        let Some(parent_id) = self.find_parent_by_id_mut(id).map(|x| x.id) else {
            return false;
        };

        self.replace_id_with_elements(replacement, id);

        self.partial_calc(calculator, parent_id);

        true
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    pub fn replace_str_id_with_elements(
        &mut self,
        replacement: Vec<Self>,
        id: &str,
    ) -> Option<Self> {
        let parent = self.find_parent_by_str_id_mut(id)?;

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

        let original = parent.children.remove(index);

        for (i, element) in replacement.into_iter().enumerate() {
            parent.children.insert(index + i, element);
        }

        Some(original)
    }

    /// # Panics
    ///
    /// * If the `Container` is not properly attached to the tree
    #[cfg(feature = "layout")]
    pub fn replace_str_id_with_elements_calc(
        &mut self,
        calculator: &impl layout::Calc,
        replacement: Vec<Self>,
        id: &str,
    ) -> Option<Self> {
        let parent_id = self.find_parent_by_str_id_mut(id)?.id;

        let element = self.replace_str_id_with_elements(replacement, id);

        self.partial_calc(calculator, parent_id);

        element
    }

    #[cfg(feature = "layout")]
    pub fn partial_calc(&mut self, calculator: &impl layout::Calc, id: usize) {
        let Some(parent) = self.find_parent_by_id_mut(id) else {
            return;
        };

        if calculator.calc(parent) {
            calculator.calc(self);
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
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
        name: Option<String>,
        autofocus: Option<bool>,
    },
    Button {
        r#type: Option<String>,
    },
    Image {
        source: Option<String>,
        alt: Option<String>,
        fit: Option<ImageFit>,
        source_set: Option<String>,
        sizes: Option<Number>,
        loading: Option<ImageLoading>,
    },
    Anchor {
        target: Option<LinkTarget>,
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
    TH {
        rows: Option<Number>,
        columns: Option<Number>,
    },
    TBody,
    TR,
    TD {
        rows: Option<Number>,
        columns: Option<Number>,
    },
    #[cfg(feature = "canvas")]
    Canvas,
    Textarea {
        value: String,
        placeholder: Option<String>,
        name: Option<String>,
        rows: Option<Number>,
        cols: Option<Number>,
    },
}

#[derive(Default)]
struct Attrs {
    values: Vec<(String, String)>,
}

#[derive(Debug)]
pub enum MaybeReplaced<T: std::fmt::Display> {
    NotReplaced(T),
    Replaced(String),
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

    #[allow(unused)]
    fn replace_or_add<V: std::fmt::Display>(&mut self, name: &str, new_value: V) -> Option<String> {
        match self.replace(name, new_value) {
            MaybeReplaced::NotReplaced(x) => {
                self.add(name, x);
                None
            }
            MaybeReplaced::Replaced(x) => Some(x),
        }
    }

    #[allow(unused)]
    fn replace<V: std::fmt::Display>(&mut self, name: &str, new_value: V) -> MaybeReplaced<V> {
        for (key, value) in &mut self.values {
            if key == name {
                let mut encoded =
                    html_escape::encode_double_quoted_attribute(new_value.to_string().as_str())
                        .to_string()
                        .replace('\n', "&#10;");

                std::mem::swap(value, &mut encoded);

                let old_value = encoded;

                return MaybeReplaced::Replaced(old_value);
            }
        }

        MaybeReplaced::NotReplaced(new_value)
    }

    fn add<K: Into<String>, V: std::fmt::Display>(&mut self, name: K, value: V) {
        self.values.push((
            name.into(),
            html_escape::encode_double_quoted_attribute(value.to_string().as_str())
                .to_string()
                .replace('\n', "&#10;"),
        ));
    }

    fn add_opt<K: Into<String>, V: std::fmt::Display>(&mut self, name: K, value: Option<V>) {
        if let Some(value) = value {
            self.values.push((
                name.into(),
                html_escape::encode_double_quoted_attribute(value.to_string().as_str())
                    .to_string()
                    .replace('\n', "&#10;"),
            ));
        }
    }

    #[cfg(feature = "layout")]
    fn add_opt_skip_default<K: Into<String>, V: Default + PartialEq + std::fmt::Display>(
        &mut self,
        name: K,
        value: Option<V>,
        skip_default: bool,
    ) {
        if let Some(value) = value {
            if skip_default && value == Default::default() {
                return;
            }
            self.values.push((
                name.into(),
                html_escape::encode_double_quoted_attribute(value.to_string().as_str())
                    .to_string()
                    .replace('\n', "&#10;"),
            ));
        }
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Container {
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn attrs(&self, #[allow(unused)] with_debug_attrs: bool) -> Attrs {
        let mut attrs = Attrs { values: vec![] };

        if with_debug_attrs {
            attrs.add("dbg-id", self.id);
        }

        attrs.add_opt("id", self.str_id.as_ref());

        match &self.element {
            Element::Image {
                fit,
                source_set,
                sizes,
                alt,
                loading,
                ..
            } => {
                attrs.add_opt("sx-fit", *fit);
                attrs.add_opt("loading", *loading);
                attrs.add_opt("srcset", source_set.as_ref());
                attrs.add_opt("sizes", sizes.as_ref());
                attrs.add_opt("alt", alt.as_ref());
            }
            Element::Anchor { target, .. } => {
                attrs.add_opt("target", target.as_ref());
            }
            Element::Input {
                name, autofocus, ..
            } => {
                attrs.add_opt("name", name.as_ref());
                attrs.add_opt("autofocus", autofocus.as_ref());
            }
            Element::Textarea {
                name,
                placeholder,
                rows,
                cols,
                ..
            } => {
                attrs.add_opt("name", name.as_ref());
                attrs.add_opt("placeholder", placeholder.as_ref());
                attrs.add_opt("rows", rows.as_ref());
                attrs.add_opt("cols", cols.as_ref());
            }
            Element::TH { rows, columns } => {
                attrs.add_opt("rows", rows.as_ref());
                attrs.add_opt("columns", columns.as_ref());
            }
            Element::TD { rows, columns } => {
                attrs.add_opt("rows", rows.as_ref());
                attrs.add_opt("columns", columns.as_ref());
            }
            Element::Button { r#type } => {
                attrs.add_opt("type", r#type.as_ref());
            }
            Element::Div
            | Element::Raw { .. }
            | Element::Aside
            | Element::Main
            | Element::Header
            | Element::Footer
            | Element::Section
            | Element::Form
            | Element::Span
            | Element::Heading { .. }
            | Element::UnorderedList
            | Element::OrderedList
            | Element::ListItem
            | Element::Table
            | Element::THead
            | Element::TBody
            | Element::TR => {}
            #[cfg(feature = "canvas")]
            Element::Canvas => {}
        }

        let mut data = self.data.iter().collect::<Vec<_>>();
        data.sort_by(|(a, _), (b, _)| (*a).cmp(b));

        if !self.classes.is_empty() {
            attrs.add("class", self.classes.join(" "));
        }

        for (name, value) in data {
            attrs.add(format!("data-{name}"), value);
        }

        if let Some(route) = &self.route {
            match route {
                Route::Get {
                    route,
                    trigger,
                    target,
                    strategy,
                } => {
                    attrs.add("hx-get", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                    if !matches!(target, hyperchad_transformer_models::Selector::SelfTarget) {
                        attrs.add("hx-target", target);
                    }
                    attrs.add("hx-swap", strategy);
                }
                Route::Post {
                    route,
                    trigger,
                    target,
                    strategy,
                } => {
                    attrs.add("hx-post", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                    if !matches!(target, hyperchad_transformer_models::Selector::SelfTarget) {
                        attrs.add("hx-target", target);
                    }
                    attrs.add("hx-swap", strategy);
                }
                Route::Put {
                    route,
                    trigger,
                    target,
                    strategy,
                } => {
                    attrs.add("hx-put", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                    if !matches!(target, hyperchad_transformer_models::Selector::SelfTarget) {
                        attrs.add("hx-target", target);
                    }
                    attrs.add("hx-swap", strategy);
                }
                Route::Delete {
                    route,
                    trigger,
                    target,
                    strategy,
                } => {
                    attrs.add("hx-delete", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                    if !matches!(target, hyperchad_transformer_models::Selector::SelfTarget) {
                        attrs.add("hx-target", target);
                    }
                    attrs.add("hx-swap", strategy);
                }
                Route::Patch {
                    route,
                    trigger,
                    target,
                    strategy,
                } => {
                    attrs.add("hx-patch", route);
                    attrs.add_opt("hx-trigger", trigger.clone());
                    if !matches!(target, hyperchad_transformer_models::Selector::SelfTarget) {
                        attrs.add("hx-target", target);
                    }
                    attrs.add("hx-swap", strategy);
                }
            }
        }

        attrs.add_opt("sx-justify-content", self.justify_content.as_ref());
        attrs.add_opt("sx-align-items", self.align_items.as_ref());

        attrs.add_opt("sx-text-align", self.text_align.as_ref());
        attrs.add_opt("sx-white-space", self.white_space.as_ref());

        if let Some(text_decoration) = &self.text_decoration {
            attrs.add_opt("sx-text-decoration-color", text_decoration.color);
            attrs.add(
                "sx-text-decoration-line",
                text_decoration
                    .line
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            attrs.add_opt("sx-text-decoration-style", text_decoration.style);
            attrs.add_opt(
                "sx-text-decoration-thickness",
                text_decoration.thickness.as_ref(),
            );
        }

        if let Some(font_family) = &self.font_family {
            attrs.add("sx-font-family", font_family.join(","));
        }

        attrs.add_opt("sx-font-weight", self.font_weight);

        match self.element {
            Element::TR => {
                if self.direction != LayoutDirection::Row {
                    attrs.add("sx-dir", self.direction);
                }
            }
            _ => {
                if self.direction != LayoutDirection::default() {
                    attrs.add("sx-dir", self.direction);
                }
            }
        }

        attrs.add_opt("sx-position", self.position);

        attrs.add_opt("sx-background", self.background);

        attrs.add_opt("sx-width", self.width.as_ref());
        attrs.add_opt("sx-min-width", self.min_width.as_ref());
        attrs.add_opt("sx-max-width", self.max_width.as_ref());
        attrs.add_opt("sx-height", self.height.as_ref());
        attrs.add_opt("sx-min-height", self.min_height.as_ref());
        attrs.add_opt("sx-max-height", self.max_height.as_ref());

        if let Some(flex) = &self.flex {
            attrs.add("sx-flex-grow", &flex.grow);
            attrs.add("sx-flex-shrink", &flex.shrink);
            attrs.add("sx-flex-basis", &flex.basis);
        }

        attrs.add_opt("sx-col-gap", self.column_gap.as_ref());
        attrs.add_opt("sx-row-gap", self.row_gap.as_ref());
        attrs.add_opt("sx-grid-cell-size", self.grid_cell_size.as_ref());

        attrs.add_opt("sx-opacity", self.opacity.as_ref());

        attrs.add_opt("sx-left", self.left.as_ref());
        attrs.add_opt("sx-right", self.right.as_ref());
        attrs.add_opt("sx-top", self.top.as_ref());
        attrs.add_opt("sx-bottom", self.bottom.as_ref());

        attrs.add_opt("sx-translate-x", self.translate_x.as_ref());
        attrs.add_opt("sx-translate-y", self.translate_y.as_ref());

        attrs.add_opt("sx-cursor", self.cursor.as_ref());
        attrs.add_opt("sx-user-select", self.user_select.as_ref());
        attrs.add_opt("sx-overflow-wrap", self.overflow_wrap.as_ref());
        attrs.add_opt("sx-text-overflow", self.text_overflow.as_ref());

        attrs.add_opt("sx-padding-left", self.padding_left.as_ref());
        attrs.add_opt("sx-padding-right", self.padding_right.as_ref());
        attrs.add_opt("sx-padding-top", self.padding_top.as_ref());
        attrs.add_opt("sx-padding-bottom", self.padding_bottom.as_ref());

        attrs.add_opt("sx-margin-left", self.margin_left.as_ref());
        attrs.add_opt("sx-margin-right", self.margin_right.as_ref());
        attrs.add_opt("sx-margin-top", self.margin_top.as_ref());
        attrs.add_opt("sx-margin-bottom", self.margin_bottom.as_ref());

        attrs.add_opt("sx-hidden", self.hidden.as_ref());
        attrs.add_opt("sx-visibility", self.visibility.as_ref());

        attrs.add_opt("sx-font-size", self.font_size.as_ref());
        attrs.add_opt("sx-color", self.color.as_ref());

        attrs.add_opt("debug", self.debug.as_ref());

        attrs.add_opt(
            "sx-border-left",
            self.border_left
                .as_ref()
                .map(|(color, size)| format!("{size}, {color}")),
        );
        attrs.add_opt(
            "sx-border-right",
            self.border_right
                .as_ref()
                .map(|(color, size)| format!("{size}, {color}")),
        );
        attrs.add_opt(
            "sx-border-top",
            self.border_top
                .as_ref()
                .map(|(color, size)| format!("{size}, {color}")),
        );
        attrs.add_opt(
            "sx-border-bottom",
            self.border_bottom
                .as_ref()
                .map(|(color, size)| format!("{size}, {color}")),
        );
        attrs.add_opt(
            "sx-border-top-left-radius",
            self.border_top_left_radius.as_ref(),
        );
        attrs.add_opt(
            "sx-border-top-right-radius",
            self.border_top_right_radius.as_ref(),
        );
        attrs.add_opt(
            "sx-border-bottom-left-radius",
            self.border_bottom_left_radius.as_ref(),
        );
        attrs.add_opt(
            "sx-border-bottom-right-radius",
            self.border_bottom_right_radius.as_ref(),
        );

        attrs.add_opt("state", self.state.as_ref());

        for action in &self.actions {
            match &action.trigger {
                hyperchad_actions::ActionTrigger::Click => {
                    attrs.add("fx-click", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::ClickOutside => {
                    attrs.add("fx-click-outside", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::MouseDown => {
                    attrs.add("fx-mouse-down", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::KeyDown => {
                    attrs.add("fx-key-down", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::Hover => {
                    attrs.add("fx-hover", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::Change => {
                    attrs.add("fx-change", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::Resize => {
                    attrs.add("fx-resize", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::Immediate => {
                    attrs.add("fx-immediate", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::HttpBeforeRequest => {
                    attrs.add("fx-http-before-request", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::HttpAfterRequest => {
                    attrs.add("fx-http-after-request", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::HttpRequestSuccess => {
                    attrs.add("fx-http-success", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::HttpRequestError => {
                    attrs.add("fx-http-error", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::HttpRequestAbort => {
                    attrs.add("fx-http-abort", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::HttpRequestTimeout => {
                    attrs.add("fx-http-timeout", action.effect.to_string());
                }
                hyperchad_actions::ActionTrigger::Event(..) => {
                    attrs.add("fx-event", action.effect.to_string());
                }
            }
        }

        match self.overflow_x {
            LayoutOverflow::Auto => {
                attrs.add("sx-overflow-x", "auto");
            }
            LayoutOverflow::Scroll => {
                attrs.add("sx-overflow-x", "scroll");
            }
            LayoutOverflow::Expand => {}
            LayoutOverflow::Squash => {
                attrs.add("sx-overflow-x", "squash");
            }
            LayoutOverflow::Wrap { grid } => {
                attrs.add("sx-overflow-x", if grid { "wrap-grid" } else { "wrap" });
            }
            LayoutOverflow::Hidden => {
                attrs.add("sx-overflow-x", "hidden");
            }
        }
        match self.overflow_y {
            LayoutOverflow::Auto => {
                attrs.add("sx-overflow-y", "auto");
            }
            LayoutOverflow::Scroll => {
                attrs.add("sx-overflow-y", "scroll");
            }
            LayoutOverflow::Expand => {}
            LayoutOverflow::Squash => {
                attrs.add("sx-overflow-y", "squash");
            }
            LayoutOverflow::Wrap { grid } => {
                attrs.add("sx-overflow-y", if grid { "wrap-grid" } else { "wrap" });
            }
            LayoutOverflow::Hidden => {
                attrs.add("sx-overflow-y", "hidden");
            }
        }

        #[cfg(feature = "layout")]
        if with_debug_attrs {
            let skip_default =
                matches!(var("SKIP_DEFAULT_DEBUG_ATTRS").as_deref(), Ok("1" | "true"));

            attrs.add_opt_skip_default("calc-x", self.calculated_x, skip_default);
            attrs.add_opt_skip_default("calc-y", self.calculated_y, skip_default);
            attrs.add_opt_skip_default("calc-min-width", self.calculated_min_width, skip_default);
            attrs.add_opt_skip_default(
                "calc-child-min-width",
                self.calculated_child_min_width,
                skip_default,
            );
            attrs.add_opt_skip_default("calc-max-width", self.calculated_max_width, skip_default);
            attrs.add_opt_skip_default(
                "calc-preferred-width",
                self.calculated_preferred_width,
                skip_default,
            );
            attrs.add_opt_skip_default("calc-width", self.calculated_width, skip_default);
            attrs.add_opt_skip_default("calc-min-height", self.calculated_min_height, skip_default);
            attrs.add_opt_skip_default(
                "calc-child-min-height",
                self.calculated_child_min_height,
                skip_default,
            );
            attrs.add_opt_skip_default("calc-max-height", self.calculated_max_height, skip_default);
            attrs.add_opt_skip_default(
                "calc-preferred-height",
                self.calculated_preferred_height,
                skip_default,
            );
            attrs.add_opt_skip_default("calc-height", self.calculated_height, skip_default);
            attrs.add_opt_skip_default(
                "calc-margin-left",
                self.calculated_margin_left,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-margin-right",
                self.calculated_margin_right,
                skip_default,
            );
            attrs.add_opt_skip_default("calc-margin-top", self.calculated_margin_top, skip_default);
            attrs.add_opt_skip_default(
                "calc-margin-bottom",
                self.calculated_margin_bottom,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-padding-left",
                self.calculated_padding_left,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-padding-right",
                self.calculated_padding_right,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-padding-top",
                self.calculated_padding_top,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-padding-bottom",
                self.calculated_padding_bottom,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-left",
                self.calculated_border_left
                    .map(|(color, size)| format!("{size}, {color}")),
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-right",
                self.calculated_border_right
                    .map(|(color, size)| format!("{size}, {color}")),
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-top",
                self.calculated_border_top
                    .map(|(color, size)| format!("{size}, {color}")),
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-bottom",
                self.calculated_border_bottom
                    .map(|(color, size)| format!("{size}, {color}")),
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-top-left-radius",
                self.calculated_border_top_left_radius,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-top-right-radius",
                self.calculated_border_top_right_radius,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-bottom-left-radius",
                self.calculated_border_bottom_left_radius,
                skip_default,
            );
            attrs.add_opt_skip_default(
                "calc-border-bottom-right-radius",
                self.calculated_border_bottom_right_radius,
                skip_default,
            );
            attrs.add_opt_skip_default("calc-col-gap", self.calculated_column_gap, skip_default);
            attrs.add_opt_skip_default("calc-row-gap", self.calculated_row_gap, skip_default);
            attrs.add_opt_skip_default("calc-opacity", self.calculated_opacity, skip_default);
            attrs.add_opt_skip_default("calc-font-size", self.calculated_font_size, skip_default);
            attrs.add_opt_skip_default("calc-scrollbar-right", self.scrollbar_right, skip_default);
            attrs.add_opt_skip_default(
                "calc-scrollbar-bottom",
                self.scrollbar_bottom,
                skip_default,
            );

            if let Some(hyperchad_transformer_models::LayoutPosition::Wrap { row, col }) =
                &self.calculated_position
            {
                attrs.add("calc-row", *row);
                attrs.add("calc-col", *col);
            }
            #[cfg(feature = "layout-offset")]
            {
                attrs.add_opt_skip_default("calc-offset-x", self.calculated_offset_x, skip_default);
                attrs.add_opt_skip_default("calc-offset-y", self.calculated_offset_y, skip_default);
            }
        }

        #[cfg(feature = "logic")]
        for config in &self.overrides {
            for item in &config.overrides {
                let name = override_item_to_attr_name(item);

                match &config.condition {
                    OverrideCondition::ResponsiveTarget { name: target } => {
                        match item {
                            OverrideItem::Flex(..) => {
                                attrs.values.retain(|(x, _)| {
                                    !matches!(
                                        x.as_str(),
                                        "sx-flex-grow" | "sx-flex-basis" | "sx-flex-shrink",
                                    )
                                });
                            }
                            OverrideItem::TextDecoration(..) => {
                                attrs.values.retain(|(x, _)| {
                                    !matches!(
                                        x.as_str(),
                                        "sx-text-decoration-line"
                                            | "sx-text-decoration-style"
                                            | "sx-text-decoration-color"
                                            | "sx-text-decoration-thickness",
                                    )
                                });
                            }
                            _ => {}
                        }

                        attrs.replace_or_add(
                            name,
                            item.as_json_if_expression_string(
                                hyperchad_actions::logic::Responsive::Target(target.clone()),
                                config.default.as_ref(),
                            )
                            .unwrap(),
                        );
                    }
                }
            }
        }

        attrs.values.sort_by(|(a, _), (b, _)| a.cmp(b));

        attrs
    }

    fn attrs_to_string_pad_left(&self, with_debug_attrs: bool) -> String {
        self.attrs(with_debug_attrs).to_string_pad_left()
    }

    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::too_many_lines)]
    fn display(
        &self,
        f: &mut dyn Write,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<(), std::io::Error> {
        match &self.element {
            Element::Raw { value } => {
                if wrap_raw_in_element {
                    f.write_fmt(format_args!(
                        "<raw{attrs}>",
                        attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                    ))?;
                    f.write_fmt(format_args!("{value}</raw>"))?;
                } else {
                    f.write_fmt(format_args!("{value}"))?;
                }
            }
            Element::Div => {
                f.write_fmt(format_args!(
                    "<div{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</div>"))?;
            }
            Element::Aside => {
                f.write_fmt(format_args!(
                    "<aside{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</aside>"))?;
            }

            Element::Main => {
                f.write_fmt(format_args!(
                    "<main{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</main>"))?;
            }
            Element::Header => {
                f.write_fmt(format_args!(
                    "<header{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</header>"))?;
            }
            Element::Footer => {
                f.write_fmt(format_args!(
                    "<footer{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</footer>"))?;
            }
            Element::Section => {
                f.write_fmt(format_args!(
                    "<section{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</section>"))?;
            }
            Element::Form => {
                f.write_fmt(format_args!(
                    "<form{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</form>"))?;
            }
            Element::Span => {
                f.write_fmt(format_args!(
                    "<span{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</span>"))?;
            }
            Element::Input { input, .. } => {
                input.display(f, self.attrs(with_debug_attrs))?;
            }
            Element::Textarea { value, .. } => {
                f.write_fmt(format_args!(
                    "<textarea{attrs}>{value}</textarea>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs),
                    value = html_escape::encode_text(value)
                ))?;
            }
            Element::Button { .. } => {
                f.write_fmt(format_args!(
                    "<button{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</button>"))?;
            }
            Element::Image { source, .. } => {
                f.write_fmt(format_args!(
                    "<img{src_attr}{attrs} />",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs),
                    src_attr = Attrs::new()
                        .with_attr_opt("src", source.to_owned())
                        .to_string_pad_left()
                ))?;
            }
            Element::Anchor { href, .. } => {
                f.write_fmt(format_args!(
                    "<a{href_attr}{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs),
                    href_attr = Attrs::new()
                        .with_attr_opt("href", href.to_owned())
                        .to_string_pad_left(),
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</a>"))?;
            }
            Element::Heading { size } => {
                f.write_fmt(format_args!(
                    "<{size}{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</{size}>"))?;
            }
            Element::UnorderedList => {
                f.write_fmt(format_args!(
                    "<ul{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</ul>"))?;
            }
            Element::OrderedList => {
                f.write_fmt(format_args!(
                    "<ol{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</ol>"))?;
            }
            Element::ListItem => {
                f.write_fmt(format_args!(
                    "<li{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</li>"))?;
            }
            Element::Table => {
                f.write_fmt(format_args!(
                    "<table{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</table>"))?;
            }
            Element::THead => {
                f.write_fmt(format_args!(
                    "<thead{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</thead>"))?;
            }
            Element::TH { .. } => {
                f.write_fmt(format_args!(
                    "<th{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</th>"))?;
            }
            Element::TBody => {
                f.write_fmt(format_args!(
                    "<tbody{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</tbody>"))?;
            }
            Element::TR => {
                f.write_fmt(format_args!(
                    "<tr{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</tr>"))?;
            }
            Element::TD { .. } => {
                f.write_fmt(format_args!(
                    "<td{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</td>"))?;
            }
            #[cfg(feature = "canvas")]
            Element::Canvas => {
                f.write_fmt(format_args!(
                    "<canvas{attrs}>",
                    attrs = self.attrs_to_string_pad_left(with_debug_attrs)
                ))?;
                display_elements(&self.children, f, with_debug_attrs, wrap_raw_in_element)?;
                f.write_fmt(format_args!("</canvas>"))?;
            }
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If fails to write to the writer
    /// * If invalid UTF-8 characters
    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn display_to_string_default(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.display_to_string(
            with_debug_attrs,
            wrap_raw_in_element,
            #[cfg(feature = "format")]
            false,
            #[cfg(feature = "syntax-highlighting")]
            false,
        )
    }

    /// # Errors
    ///
    /// * If fails to write to the writer
    /// * If invalid UTF-8 characters
    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn display_to_string_default_pretty(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.display_to_string(
            with_debug_attrs,
            wrap_raw_in_element,
            #[cfg(feature = "format")]
            true,
            #[cfg(feature = "syntax-highlighting")]
            true,
        )
    }

    /// # Errors
    ///
    /// * If fails to write to the writer
    /// * If invalid UTF-8 characters
    ///
    /// # Panics
    ///
    /// * If syntax highlighting fails
    #[cfg_attr(feature = "profiling", profiling::function)]
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn display_to_string(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
        #[cfg(feature = "format")] format: bool,
        #[cfg(feature = "syntax-highlighting")] highlight: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut data = Vec::new();

        let _ = self.display(&mut data, with_debug_attrs, wrap_raw_in_element);

        #[cfg(feature = "format")]
        let data = if format {
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
        } else {
            data
        };

        let xml = String::from_utf8(data)?;

        // Remove doctype header thing
        let xml = if let Some((_, xml)) = xml.split_once('\n') {
            xml.to_string()
        } else {
            xml
        };

        #[cfg(feature = "syntax-highlighting")]
        if highlight {
            use std::sync::LazyLock;

            use syntect::highlighting::ThemeSet;
            use syntect::parsing::{SyntaxReference, SyntaxSet};

            static PS: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
            static TS: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
            static SYNTAX: LazyLock<SyntaxReference> =
                LazyLock::new(|| PS.find_syntax_by_extension("xml").unwrap().clone());

            let mut h =
                syntect::easy::HighlightLines::new(&SYNTAX, &TS.themes["base16-ocean.dark"]);
            let highlighted = syntect::util::LinesWithEndings::from(&xml)
                .map(|line| {
                    let ranges: Vec<(syntect::highlighting::Style, &str)> =
                        h.highlight_line(line, &PS).unwrap();
                    syntect::util::as_24_bit_terminal_escaped(&ranges[..], false)
                })
                .collect::<String>();

            return Ok(highlighted);
        }

        Ok(xml)
    }
}

#[cfg(feature = "logic")]
const fn override_item_to_attr_name(item: &OverrideItem) -> &'static str {
    match item {
        OverrideItem::StrId(..) => "id",
        OverrideItem::Classes(..) => "class",
        OverrideItem::Direction(..) => "sx-dir",
        OverrideItem::OverflowX(..) => "sx-overflow-x",
        OverrideItem::OverflowY(..) => "sx-overflow-y",
        OverrideItem::GridCellSize(..) => "sx-grid-cell-size",
        OverrideItem::JustifyContent(..) => "sx-justify-content",
        OverrideItem::AlignItems(..) => "sx-align-items",
        OverrideItem::TextAlign(..) => "sx-text-align",
        OverrideItem::WhiteSpace(..) => "sx-white-space",
        OverrideItem::TextDecoration(..) => "sx-text-decoration",
        OverrideItem::FontFamily(..) => "sx-font-family",
        OverrideItem::FontWeight(..) => "sx-font-weight",
        OverrideItem::Width(..) => "sx-width",
        OverrideItem::MinWidth(..) => "sx-min-width",
        OverrideItem::MaxWidth(..) => "sx-max-width",
        OverrideItem::Height(..) => "sx-height",
        OverrideItem::MinHeight(..) => "sx-min-height",
        OverrideItem::MaxHeight(..) => "sx-max-height",
        OverrideItem::Flex(..) => "sx-flex",
        OverrideItem::ColumnGap(..) => "sx-column-gap",
        OverrideItem::RowGap(..) => "sx-row-gap",
        OverrideItem::Opacity(..) => "sx-opacity",
        OverrideItem::Left(..) => "sx-left",
        OverrideItem::Right(..) => "sx-right",
        OverrideItem::Top(..) => "sx-top",
        OverrideItem::Bottom(..) => "sx-bottom",
        OverrideItem::TranslateX(..) => "sx-translate-x",
        OverrideItem::TranslateY(..) => "sx-translate-y",
        OverrideItem::Cursor(..) => "sx-cursor",
        OverrideItem::UserSelect(..) => "sx-user-select",
        OverrideItem::OverflowWrap(..) => "sx-overflow-wrap",
        OverrideItem::TextOverflow(..) => "sx-text-overflow",
        OverrideItem::Position(..) => "sx-position",
        OverrideItem::Background(..) => "sx-background",
        OverrideItem::BorderTop(..) => "sx-border-top",
        OverrideItem::BorderRight(..) => "sx-border-right",
        OverrideItem::BorderBottom(..) => "sx-border-bottom",
        OverrideItem::BorderLeft(..) => "sx-border-left",
        OverrideItem::BorderTopLeftRadius(..) => "sx-border-top-left-radius",
        OverrideItem::BorderTopRightRadius(..) => "sx-border-top-right-radius",
        OverrideItem::BorderBottomLeftRadius(..) => "sx-border-bottom-left-radius",
        OverrideItem::BorderBottomRightRadius(..) => "sx-border-bottom-right-radius",
        OverrideItem::MarginLeft(..) => "sx-margin-left",
        OverrideItem::MarginRight(..) => "sx-margin-right",
        OverrideItem::MarginTop(..) => "sx-margin-top",
        OverrideItem::MarginBottom(..) => "sx-margin-bottom",
        OverrideItem::PaddingLeft(..) => "sx-padding-left",
        OverrideItem::PaddingRight(..) => "sx-padding-right",
        OverrideItem::PaddingTop(..) => "sx-padding-top",
        OverrideItem::PaddingBottom(..) => "sx-padding-bottom",
        OverrideItem::FontSize(..) => "sx-font-size",
        OverrideItem::Color(..) => "sx-color",
        OverrideItem::Hidden(..) => "sx-hidden",
        OverrideItem::Visibility(..) => "sx-visibility",
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl std::fmt::Display for Container {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            &self
                .display_to_string(
                    if cfg!(test) {
                        true
                    } else {
                        matches!(var("DEBUG_ATTRS").as_deref(), Ok("1" | "true"))
                    },
                    if cfg!(test) {
                        true
                    } else {
                        matches!(var("DEBUG_RAW_ATTRS").as_deref(), Ok("1" | "true"))
                    },
                    #[cfg(feature = "format")]
                    true,
                    #[cfg(feature = "syntax-highlighting")]
                    true,
                )
                .unwrap_or_else(|e| panic!("Failed to display container: {e:?} ({self:?})")),
        )?;

        Ok(())
    }
}

fn display_elements(
    elements: &[Container],
    f: &mut dyn Write,
    with_debug_attrs: bool,
    wrap_raw_in_element: bool,
) -> Result<(), std::io::Error> {
    for element in elements {
        element.display(f, with_debug_attrs, wrap_raw_in_element)?;
    }

    Ok(())
}

impl Element {
    #[must_use]
    pub const fn allows_children(&self) -> bool {
        match self {
            Self::Div
            | Self::Aside
            | Self::Main
            | Self::Header
            | Self::Footer
            | Self::Section
            | Self::Form
            | Self::Span
            | Self::Button { .. }
            | Self::Anchor { .. }
            | Self::Heading { .. }
            | Self::UnorderedList
            | Self::OrderedList
            | Self::ListItem
            | Self::Table
            | Self::THead
            | Self::TH { .. }
            | Self::TBody
            | Self::TR
            | Self::TD { .. } => true,
            Self::Input { .. } | Self::Raw { .. } | Self::Image { .. } | Self::Textarea { .. } => {
                false
            }
            #[cfg(feature = "canvas")]
            Self::Canvas => false,
        }
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
            Self::Textarea { .. } => "Textarea",
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

impl From<HeaderSize> for Number {
    fn from(value: HeaderSize) -> Self {
        Self::Integer(match value {
            HeaderSize::H1 => 1,
            HeaderSize::H2 => 2,
            HeaderSize::H3 => 3,
            HeaderSize::H4 => 4,
            HeaderSize::H5 => 5,
            HeaderSize::H6 => 6,
        })
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
    Hidden {
        value: Option<String>,
    },
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Input {
    fn display(&self, f: &mut dyn Write, attrs: Attrs) -> Result<(), std::io::Error> {
        match self {
            Self::Checkbox { checked } => {
                let attrs = attrs.with_attr_opt("checked", checked.map(|x| x.to_string()));
                f.write_fmt(format_args!(
                    "<input type=\"checkbox\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))?;
            }
            Self::Text { value, placeholder } => {
                let attrs = attrs
                    .with_attr_opt("value", value.to_owned())
                    .with_attr_opt("placeholder", placeholder.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"text\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))?;
            }
            Self::Password { value, placeholder } => {
                let attrs = attrs
                    .with_attr_opt("value", value.to_owned())
                    .with_attr_opt("placeholder", placeholder.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"password\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))?;
            }
            Self::Hidden { value } => {
                let attrs = attrs.with_attr_opt("value", value.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"hidden\"{attrs} />",
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
            Self::Hidden { value } => {
                let attrs = Attrs::new().with_attr_opt("value", value.to_owned());
                f.write_fmt(format_args!(
                    "<input type=\"hidden\"{attrs} />",
                    attrs = attrs.to_string_pad_left(),
                ))
            }
        }
    }
}
