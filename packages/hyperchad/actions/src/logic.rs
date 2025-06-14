use hyperchad_transformer_models::{LayoutDirection, Visibility};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{Action, ActionType, ElementTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CalcValue {
    Visibility { target: ElementTarget },
    Display { target: ElementTarget },
    Id { target: ElementTarget },
    DataAttrValue { attr: String, target: ElementTarget },
    EventValue,
    WidthPx { target: ElementTarget },
    HeightPx { target: ElementTarget },
    PositionX { target: ElementTarget },
    PositionY { target: ElementTarget },
    MouseX { target: Option<ElementTarget> },
    MouseY { target: Option<ElementTarget> },
}

impl CalcValue {
    #[must_use]
    pub fn eq(self, other: impl Into<Value>) -> Condition {
        Condition::Eq(self.into(), other.into())
    }

    #[must_use]
    pub fn plus(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Plus(self.into(), other.into())
    }

    #[must_use]
    pub fn minus(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Minus(self.into(), other.into())
    }

    #[must_use]
    pub fn multiply(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Multiply(self.into(), other.into())
    }

    #[must_use]
    pub fn divide(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Divide(self.into(), other.into())
    }

    #[must_use]
    pub fn min(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Min(self.into(), other.into())
    }

    #[must_use]
    pub fn max(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Max(self.into(), other.into())
    }

    #[must_use]
    pub fn clamp(self, min: impl Into<Value>, max: impl Into<Value>) -> Arithmetic {
        Arithmetic::Min(max.into(), Arithmetic::Max(self.into(), min.into()).into())
    }

    #[must_use]
    pub fn then_pass_to(self, other: impl Into<ActionType>) -> ActionType {
        ActionType::Parameterized {
            action: Box::new(other.into()),
            value: Value::Calc(self),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum Value {
    // IF YOU ADD A NEW VALUE TYPE, ADD IT TO THE DESERIALIZE IMPL BELOW
    Calc(CalcValue),
    Arithmetic(Box<Arithmetic>),
    Real(f32),
    Visibility(Visibility),
    Display(bool),
    LayoutDirection(LayoutDirection),
    String(String),
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        #[derive(Deserialize)]
        #[serde(rename = "Value")]
        enum ValueInner {
            Calc(CalcValue),
            Arithmetic(Box<Arithmetic>),
            Real(f32),
            Visibility(Visibility),
            Display(bool),
            LayoutDirection(LayoutDirection),
            String(String),
        }

        impl From<ValueInner> for Value {
            fn from(value: ValueInner) -> Self {
                match value {
                    ValueInner::Calc(x) => Self::Calc(x),
                    ValueInner::Arithmetic(x) => Self::Arithmetic(x),
                    ValueInner::Real(x) => Self::Real(x),
                    ValueInner::Visibility(x) => Self::Visibility(x),
                    ValueInner::Display(x) => Self::Display(x),
                    ValueInner::LayoutDirection(x) => Self::LayoutDirection(x),
                    ValueInner::String(x) => Self::String(x),
                }
            }
        }

        log::trace!("attempting to deserialize Value");
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        log::trace!("deserialized Value to {value:?}");

        Ok(if value.is_i64() {
            #[allow(clippy::cast_precision_loss)]
            Self::Real(value.as_i64().unwrap() as f32)
        } else if value.is_u64() {
            #[allow(clippy::cast_precision_loss)]
            Self::Real(value.as_u64().unwrap() as f32)
        } else if value.is_f64() {
            #[allow(clippy::cast_possible_truncation)]
            Self::Real(value.as_f64().unwrap() as f32)
        } else {
            serde_json::from_value::<ValueInner>(value)
                .map_err(D::Error::custom)?
                .into()
        })
    }
}

impl Value {
    #[must_use]
    pub fn eq(self, other: impl Into<Self>) -> Condition {
        Condition::Eq(self, other.into())
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(str) => Some(str),
            Self::Arithmetic(..)
            | Self::Calc(..)
            | Self::Real(..)
            | Self::Visibility(..)
            | Self::Display(..)
            | Self::LayoutDirection(..) => None,
        }
    }

    #[must_use]
    pub fn as_f32(&self, calc_func: Option<&impl Fn(&CalcValue) -> Option<Self>>) -> Option<f32> {
        match self {
            Self::Arithmetic(x) => x.as_f32(calc_func),
            Self::Calc(x) => calc_func
                .and_then(|func| func(x))
                .and_then(|x| x.as_f32(calc_func)),
            Self::Real(x) => Some(*x),
            Self::Visibility(..)
            | Self::Display(..)
            | Self::String(..)
            | Self::LayoutDirection(..) => None,
        }
    }
}

impl From<CalcValue> for Value {
    fn from(value: CalcValue) -> Self {
        Self::Calc(value)
    }
}

impl From<Visibility> for Value {
    fn from(value: Visibility) -> Self {
        Self::Visibility(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Self::Real(value)
    }
}

impl From<LayoutDirection> for Value {
    fn from(value: LayoutDirection) -> Self {
        Self::LayoutDirection(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Condition {
    Eq(Value, Value),
}

impl Condition {
    #[must_use]
    pub fn then(self, action: impl Into<Action>) -> If {
        If {
            condition: self,
            actions: vec![action.into()],
            else_actions: vec![],
        }
    }

    #[must_use]
    pub fn or_else(self, action: impl Into<Action>) -> If {
        If {
            condition: self,
            actions: vec![],
            else_actions: vec![action.into()],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ConditionExpression {
    Eq(Value, Value),
}

impl ConditionExpression {
    #[must_use]
    pub fn then<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: None,
            default: Some(value.into()),
        }
    }

    #[must_use]
    pub fn or_else<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: None,
            default: Some(value.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IfExpression<T, C> {
    pub condition: C,
    pub value: Option<T>,
    pub default: Option<T>,
}

impl<T, C> IfExpression<T, C> {
    #[must_use]
    pub fn then(mut self, value: impl Into<T>) -> Self {
        self.value.replace(value.into());
        self
    }

    #[must_use]
    pub fn or_else(mut self, value: impl Into<T>) -> Self {
        self.default.replace(value.into());
        self
    }
}

#[cfg(feature = "serde")]
impl<T: Serialize, C: Serialize> std::fmt::Display for IfExpression<T, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[cfg(feature = "serde")]
impl<'a, T: for<'de> Deserialize<'de>, C: for<'de> Deserialize<'de>> TryFrom<&'a str>
    for IfExpression<T, C>
{
    type Error = serde_json::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ParameterizedAction {
    action: Action,
    value: Value,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Arithmetic {
    Plus(Value, Value),
    Minus(Value, Value),
    Multiply(Value, Value),
    Divide(Value, Value),
    Min(Value, Value),
    Max(Value, Value),
    Grouping(Box<Arithmetic>),
}

impl Arithmetic {
    #[must_use]
    pub fn as_f32(&self, calc_func: Option<&impl Fn(&CalcValue) -> Option<Value>>) -> Option<f32> {
        match self {
            Self::Plus(a, b) => a
                .as_f32(calc_func)
                .and_then(|a| b.as_f32(calc_func).map(|b| a + b)),
            Self::Minus(a, b) => a
                .as_f32(calc_func)
                .and_then(|a| b.as_f32(calc_func).map(|b| a - b)),
            Self::Multiply(a, b) => a
                .as_f32(calc_func)
                .and_then(|a| b.as_f32(calc_func).map(|b| a * b)),
            Self::Divide(a, b) => a
                .as_f32(calc_func)
                .and_then(|a| b.as_f32(calc_func).map(|b| a / b)),
            Self::Min(a, b) => a
                .as_f32(calc_func)
                .and_then(|a| b.as_f32(calc_func).map(|b| if b < a { b } else { a })),
            Self::Max(a, b) => a
                .as_f32(calc_func)
                .and_then(|a| b.as_f32(calc_func).map(|b| if b > a { b } else { a })),
            Self::Grouping(x) => x.as_f32(calc_func),
        }
    }

    #[must_use]
    pub fn eq(self, other: impl Into<Value>) -> Condition {
        Condition::Eq(self.into(), other.into())
    }

    #[must_use]
    pub fn then_pass_to(self, other: impl Into<ActionType>) -> ActionType {
        ActionType::Parameterized {
            action: Box::new(other.into()),
            value: self.into(),
        }
    }

    #[must_use]
    pub fn plus(self, other: impl Into<Value>) -> Self {
        Self::Plus(self.into(), other.into())
    }

    #[must_use]
    pub fn minus(self, other: impl Into<Value>) -> Self {
        Self::Minus(self.into(), other.into())
    }

    #[must_use]
    pub fn multiply(self, other: impl Into<Value>) -> Self {
        Self::Multiply(self.into(), other.into())
    }

    #[must_use]
    pub fn divide(self, other: impl Into<Value>) -> Self {
        Self::Divide(self.into(), other.into())
    }

    #[must_use]
    pub fn min(self, other: impl Into<Value>) -> Self {
        Self::Min(self.into(), other.into())
    }

    #[must_use]
    pub fn max(self, other: impl Into<Value>) -> Self {
        Self::Max(self.into(), other.into())
    }

    #[must_use]
    pub fn group(value: impl Into<Self>) -> Self {
        Self::Grouping(Box::new(value.into()))
    }

    #[must_use]
    pub fn clamp(self, min: impl Into<Value>, max: impl Into<Value>) -> Self {
        Self::Min(max.into(), Self::Max(self.into(), min.into()).into())
    }
}

impl From<Arithmetic> for Value {
    fn from(value: Arithmetic) -> Self {
        Self::Arithmetic(Box::new(value))
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct If {
    pub condition: Condition,
    pub actions: Vec<Action>,
    pub else_actions: Vec<Action>,
}

impl If {
    #[must_use]
    pub fn then(mut self, action: impl Into<Action>) -> Self {
        self.actions.push(action.into());
        self
    }

    #[must_use]
    pub fn or_else(mut self, action: impl Into<Action>) -> Self {
        self.else_actions.push(action.into());
        self
    }
}

#[cfg(feature = "serde")]
impl std::fmt::Display for If {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[cfg(feature = "serde")]
impl<'a> TryFrom<&'a str> for If {
    type Error = serde_json::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

#[must_use]
pub fn if_stmt(condition: Condition, action: impl Into<Action>) -> If {
    If {
        condition,
        actions: vec![action.into()],
        else_actions: vec![],
    }
}

pub fn value(value: impl Into<Value>) -> Value {
    value.into()
}

#[must_use]
pub const fn hidden() -> Value {
    Value::Visibility(Visibility::Hidden)
}

#[must_use]
pub const fn visible() -> Value {
    Value::Visibility(Visibility::Visible)
}

pub fn eq(a: impl Into<Value>, b: impl Into<Value>) -> Condition {
    Condition::Eq(a.into(), b.into())
}

#[must_use]
pub fn get_visibility_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::Visibility {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[must_use]
pub const fn get_visibility_id(id: usize) -> CalcValue {
    CalcValue::Visibility {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_visibility_self() -> CalcValue {
    CalcValue::Visibility {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_display_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[must_use]
pub const fn get_display_id(id: usize) -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_display_self() -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub const fn get_event_value() -> CalcValue {
    CalcValue::EventValue
}

#[must_use]
pub const fn get_id_self() -> CalcValue {
    CalcValue::Id {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_data_attr_value_self(attr: impl Into<String>) -> CalcValue {
    CalcValue::DataAttrValue {
        attr: attr.into(),
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_width_px_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::WidthPx {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[must_use]
pub const fn get_width_px_id(id: usize) -> CalcValue {
    CalcValue::WidthPx {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_width_px_self() -> CalcValue {
    CalcValue::WidthPx {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_height_px_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::HeightPx {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[must_use]
pub const fn get_height_px_id(id: usize) -> CalcValue {
    CalcValue::HeightPx {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_height_px_self() -> CalcValue {
    CalcValue::HeightPx {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_position_x_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::PositionX {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[must_use]
pub const fn get_position_x_id(id: usize) -> CalcValue {
    CalcValue::PositionX {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_position_x_self() -> CalcValue {
    CalcValue::PositionX {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_position_y_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::PositionY {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[must_use]
pub const fn get_position_y_id(id: usize) -> CalcValue {
    CalcValue::PositionY {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_position_y_self() -> CalcValue {
    CalcValue::PositionY {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub const fn get_mouse_x() -> CalcValue {
    CalcValue::MouseX { target: None }
}

#[must_use]
pub fn get_mouse_x_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::MouseX {
        target: Some(ElementTarget::StrId(str_id.into())),
    }
}

#[must_use]
pub const fn get_mouse_x_id(id: usize) -> CalcValue {
    CalcValue::MouseX {
        target: Some(ElementTarget::Id(id)),
    }
}

#[must_use]
pub const fn get_mouse_x_self() -> CalcValue {
    CalcValue::MouseX {
        target: Some(ElementTarget::SelfTarget),
    }
}

#[must_use]
pub const fn get_mouse_y() -> CalcValue {
    CalcValue::MouseY { target: None }
}

#[must_use]
pub fn get_mouse_y_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::MouseY {
        target: Some(ElementTarget::StrId(str_id.into())),
    }
}

#[must_use]
pub const fn get_mouse_y_id(id: usize) -> CalcValue {
    CalcValue::MouseY {
        target: Some(ElementTarget::Id(id)),
    }
}

#[must_use]
pub const fn get_mouse_y_self() -> CalcValue {
    CalcValue::MouseY {
        target: Some(ElementTarget::SelfTarget),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Responsive {
    Target(String),
    Targets(Vec<String>),
}

impl Responsive {
    #[must_use]
    pub fn then<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: Some(value.into()),
            default: None,
        }
    }

    #[must_use]
    pub fn or_else<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: None,
            default: Some(value.into()),
        }
    }
}

#[must_use]
pub fn if_responsive(target: impl Into<String>) -> Responsive {
    let target = target.into();
    Responsive::Target(target)
}

#[must_use]
pub fn if_responsive_any<T: Into<String>>(targets: impl Into<Vec<T>>) -> Responsive {
    Responsive::Targets(targets.into().into_iter().map(Into::into).collect())
}

// Add From implementations for specific enum types to handle responsive expressions
impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::LayoutDirection {
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::TextAlign {
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::AlignItems {
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::JustifyContent {
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::Position {
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::LayoutOverflow {
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}
