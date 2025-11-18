//! Conditional logic and dynamic value evaluation for actions
//!
//! This module provides types and functions for creating conditional actions that respond to
//! runtime state, such as element visibility, display properties, mouse position, and more.
//!
//! # Core Types
//!
//! * [`crate::logic::Value`] - Dynamic values that can be calculated at runtime
//! * [`crate::logic::CalcValue`] - Computed values from element state (visibility, dimensions, position, etc.)
//! * [`crate::logic::Condition`] - Conditional expressions for if-then-else logic
//! * [`crate::logic::If`] - Conditional action execution based on conditions
//! * [`crate::logic::Arithmetic`] - Mathematical operations on dynamic values
//!
//! # Example
//!
//! ```rust
//! use hyperchad_actions::logic::{get_visibility_str_id, visible, If};
//! use hyperchad_actions::ActionType;
//!
//! // Create a conditional action that shows an element if another is visible
//! let condition = get_visibility_str_id("other-element").eq(visible());
//! let if_action = condition.then(ActionType::show_str_id("my-element"));
//! ```

use hyperchad_transformer_models::{LayoutDirection, Visibility};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{Action, ActionEffect, ActionType, ElementTarget, Key, Target};

/// Computed value from element or event state
///
/// Represents a value that is calculated at runtime based on element properties,
/// mouse position, or event data. These values are used in conditional logic and
/// parameterized actions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CalcValue {
    /// Get visibility state of an element
    Visibility {
        /// Target element
        target: ElementTarget,
    },
    /// Get display state of an element
    Display {
        /// Target element
        target: ElementTarget,
    },
    /// Get string ID of an element
    Id {
        /// Target element
        target: ElementTarget,
    },
    /// Get data attribute value from an element
    DataAttrValue {
        /// Attribute name
        attr: String,
        /// Target element
        target: ElementTarget,
    },
    /// Get current event value (e.g., input field value)
    EventValue,
    /// Get width in pixels of an element
    WidthPx {
        /// Target element
        target: ElementTarget,
    },
    /// Get height in pixels of an element
    HeightPx {
        /// Target element
        target: ElementTarget,
    },
    /// Get X position of an element
    PositionX {
        /// Target element
        target: ElementTarget,
    },
    /// Get Y position of an element
    PositionY {
        /// Target element
        target: ElementTarget,
    },
    /// Get mouse X coordinate (optionally relative to element)
    MouseX {
        /// Optional target element for relative coordinates
        target: Option<ElementTarget>,
    },
    /// Get mouse Y coordinate (optionally relative to element)
    MouseY {
        /// Optional target element for relative coordinates
        target: Option<ElementTarget>,
    },
    /// Get keyboard key value
    Key {
        /// Keyboard key
        key: Key,
    },
}

impl CalcValue {
    /// Creates an equality condition comparing this value to another
    #[must_use]
    pub fn eq(self, other: impl Into<Value>) -> Condition {
        Condition::Eq(self.into(), other.into())
    }

    /// Adds another value to this value
    #[must_use]
    pub fn plus(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Plus(self.into(), other.into())
    }

    /// Subtracts another value from this value
    #[must_use]
    pub fn minus(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Minus(self.into(), other.into())
    }

    /// Multiplies this value by another value
    #[must_use]
    pub fn multiply(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Multiply(self.into(), other.into())
    }

    /// Divides this value by another value
    #[must_use]
    pub fn divide(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Divide(self.into(), other.into())
    }

    /// Returns the minimum of this value and another
    #[must_use]
    pub fn min(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Min(self.into(), other.into())
    }

    /// Returns the maximum of this value and another
    #[must_use]
    pub fn max(self, other: impl Into<Value>) -> Arithmetic {
        Arithmetic::Max(self.into(), other.into())
    }

    /// Clamps this value between min and max bounds
    #[must_use]
    pub fn clamp(self, min: impl Into<Value>, max: impl Into<Value>) -> Arithmetic {
        Arithmetic::Min(max.into(), Arithmetic::Max(self.into(), min.into()).into())
    }

    /// Creates a parameterized action that passes this value to the action
    #[must_use]
    pub fn then_pass_to(self, other: impl Into<ActionType>) -> ActionType {
        ActionType::Parameterized {
            action: Box::new(other.into()),
            value: Value::Calc(self),
        }
    }
}

/// Dynamic value type that can be calculated or provided at runtime
///
/// Values can be computed from element state, arithmetic operations, or literal values.
/// They are evaluated during action execution to enable conditional and data-driven behaviors.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum Value {
    // IF YOU ADD A NEW VALUE TYPE, ADD IT TO THE DESERIALIZE IMPL BELOW
    /// Computed value from element or event state
    Calc(CalcValue),
    /// Arithmetic operation result
    Arithmetic(Box<Arithmetic>),
    /// Numeric value
    Real(f32),
    /// Visibility state value
    Visibility(Visibility),
    /// Display state value
    Display(bool),
    /// Layout direction value
    LayoutDirection(LayoutDirection),
    /// String value
    String(String),
    /// Keyboard key value
    Key(Key),
}

impl From<Key> for Value {
    /// Converts a keyboard `Key` into a `Value`
    fn from(value: Key) -> Self {
        Self::Key(value)
    }
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
    /// Creates an equality condition comparing this value to another
    #[must_use]
    pub fn eq(self, other: impl Into<Self>) -> Condition {
        Condition::Eq(self, other.into())
    }

    /// Converts this value to a string slice if possible
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(str) => Some(str),
            Self::Key(key) => Some(key.as_str()),
            Self::Arithmetic(..)
            | Self::Calc(..)
            | Self::Real(..)
            | Self::Visibility(..)
            | Self::Display(..)
            | Self::LayoutDirection(..) => None,
        }
    }

    /// Converts this value to an `f32` if possible, optionally evaluating calculated values
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
            | Self::Key(..)
            | Self::LayoutDirection(..) => None,
        }
    }
}

impl From<CalcValue> for Value {
    /// Converts a `CalcValue` into a `Value`
    fn from(value: CalcValue) -> Self {
        Self::Calc(value)
    }
}

impl From<Visibility> for Value {
    /// Converts a `Visibility` into a `Value`
    fn from(value: Visibility) -> Self {
        Self::Visibility(value)
    }
}

impl From<f32> for Value {
    /// Converts an `f32` into a `Value`
    fn from(value: f32) -> Self {
        Self::Real(value)
    }
}

impl From<f64> for Value {
    /// Converts an `f64` into a `Value` (with precision loss)
    fn from(value: f64) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self::Real(value as f32)
    }
}

impl From<LayoutDirection> for Value {
    /// Converts a `LayoutDirection` into a `Value`
    fn from(value: LayoutDirection) -> Self {
        Self::LayoutDirection(value)
    }
}

/// Conditional expression for if-then-else logic
///
/// Conditions are evaluated to determine which branch of actions to execute.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Condition {
    /// Boolean literal condition
    Bool(bool),
    /// Equality comparison between two values
    Eq(Value, Value),
}

impl Condition {
    /// Creates an if-then action with this condition
    #[must_use]
    pub fn then(self, action: impl Into<ActionEffect>) -> If {
        If {
            condition: self,
            actions: vec![action.into()],
            else_actions: vec![],
        }
    }

    /// Creates an if-else action with this condition
    #[must_use]
    pub fn or_else(self, action: impl Into<ActionEffect>) -> If {
        If {
            condition: self,
            actions: vec![],
            else_actions: vec![action.into()],
        }
    }
}

/// Conditional expression for value selection
///
/// Similar to [`Condition`] but used for selecting between values rather than actions.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ConditionExpression {
    /// Equality comparison between two values
    Eq(Value, Value),
}

impl ConditionExpression {
    /// Sets the value to use when the condition is true
    #[must_use]
    pub fn then<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: None,
            default: Some(value.into()),
        }
    }

    /// Sets the value to use when the condition is false
    #[must_use]
    pub fn or_else<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: None,
            default: Some(value.into()),
        }
    }
}

/// Conditional value expression (if-then-else for values)
///
/// Selects a value based on a condition, similar to a ternary operator.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IfExpression<T, C> {
    /// The condition to evaluate
    pub condition: C,
    /// Value to use when condition is true
    pub value: Option<T>,
    /// Value to use when condition is false
    pub default: Option<T>,
}

impl<T, C> IfExpression<T, C> {
    /// Sets the value to use when the condition is true
    #[must_use]
    pub fn then(mut self, value: impl Into<T>) -> Self {
        self.value.replace(value.into());
        self
    }

    /// Sets the value to use when the condition is false
    #[must_use]
    pub fn or_else(mut self, value: impl Into<T>) -> Self {
        self.default.replace(value.into());
        self
    }
}

#[cfg(feature = "serde")]
impl<T: Serialize, C: Serialize> std::fmt::Display for IfExpression<T, C> {
    /// Formats the conditional expression as JSON
    ///
    /// # Panics
    ///
    /// * If serialization fails (should not happen for valid conditional expressions)
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[cfg(feature = "serde")]
impl<'a, T: for<'de> Deserialize<'de>, C: for<'de> Deserialize<'de>> TryFrom<&'a str>
    for IfExpression<T, C>
{
    type Error = serde_json::Error;

    /// Parses an `IfExpression` from a JSON string
    ///
    /// # Errors
    ///
    /// * If the string is not valid JSON
    /// * If the JSON structure doesn't match `IfExpression`
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

/// Action with a dynamic parameter value
///
/// Associates an action with a dynamically computed value that is passed to the action
/// during execution.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ParameterizedAction {
    /// The action to execute
    action: Action,
    /// The dynamic value parameter
    value: Value,
}

/// Arithmetic operation on dynamic values
///
/// Supports basic mathematical operations and aggregations on values that can be
/// computed at runtime.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Arithmetic {
    /// Addition of two values
    Plus(Value, Value),
    /// Subtraction of two values
    Minus(Value, Value),
    /// Multiplication of two values
    Multiply(Value, Value),
    /// Division of two values
    Divide(Value, Value),
    /// Minimum of two values
    Min(Value, Value),
    /// Maximum of two values
    Max(Value, Value),
    /// Grouped arithmetic expression
    Grouping(Box<Self>),
}

impl Arithmetic {
    /// Evaluates this arithmetic expression to an `f32` value
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

    /// Creates an equality condition comparing this arithmetic result to another value
    #[must_use]
    pub fn eq(self, other: impl Into<Value>) -> Condition {
        Condition::Eq(self.into(), other.into())
    }

    /// Creates a parameterized action that passes this arithmetic result to the action
    #[must_use]
    pub fn then_pass_to(self, other: impl Into<ActionType>) -> ActionType {
        ActionType::Parameterized {
            action: Box::new(other.into()),
            value: self.into(),
        }
    }

    /// Adds another value to this arithmetic expression
    #[must_use]
    pub fn plus(self, other: impl Into<Value>) -> Self {
        Self::Plus(self.into(), other.into())
    }

    /// Subtracts another value from this arithmetic expression
    #[must_use]
    pub fn minus(self, other: impl Into<Value>) -> Self {
        Self::Minus(self.into(), other.into())
    }

    /// Multiplies this arithmetic expression by another value
    #[must_use]
    pub fn multiply(self, other: impl Into<Value>) -> Self {
        Self::Multiply(self.into(), other.into())
    }

    /// Divides this arithmetic expression by another value
    #[must_use]
    pub fn divide(self, other: impl Into<Value>) -> Self {
        Self::Divide(self.into(), other.into())
    }

    /// Returns the minimum of this arithmetic expression and another value
    #[must_use]
    pub fn min(self, other: impl Into<Value>) -> Self {
        Self::Min(self.into(), other.into())
    }

    /// Returns the maximum of this arithmetic expression and another value
    #[must_use]
    pub fn max(self, other: impl Into<Value>) -> Self {
        Self::Max(self.into(), other.into())
    }

    /// Groups an arithmetic expression for explicit precedence
    #[must_use]
    pub fn group(value: impl Into<Self>) -> Self {
        Self::Grouping(Box::new(value.into()))
    }

    /// Clamps this arithmetic expression between min and max bounds
    #[must_use]
    pub fn clamp(self, min: impl Into<Value>, max: impl Into<Value>) -> Self {
        Self::Min(max.into(), Self::Max(self.into(), min.into()).into())
    }
}

impl From<Arithmetic> for Value {
    /// Converts an `Arithmetic` operation into a `Value`
    fn from(value: Arithmetic) -> Self {
        Self::Arithmetic(Box::new(value))
    }
}

/// Conditional action execution (if-then-else for actions)
///
/// Executes different sets of actions based on a condition evaluated at runtime.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct If {
    /// The condition to evaluate
    pub condition: Condition,
    /// Actions to execute when condition is true
    pub actions: Vec<ActionEffect>,
    /// Actions to execute when condition is false
    pub else_actions: Vec<ActionEffect>,
}

impl If {
    /// Adds an action to execute when the condition is true
    #[must_use]
    pub fn then(mut self, action: impl Into<ActionEffect>) -> Self {
        self.actions.push(action.into());
        self
    }

    /// Adds an action to execute when the condition is false
    #[must_use]
    pub fn or_else(mut self, action: impl Into<ActionEffect>) -> Self {
        self.else_actions.push(action.into());
        self
    }
}

#[cfg(feature = "serde")]
impl std::fmt::Display for If {
    /// Formats the conditional as JSON
    ///
    /// # Panics
    ///
    /// * If serialization fails (should not happen for valid conditionals)
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[cfg(feature = "serde")]
impl<'a> TryFrom<&'a str> for If {
    type Error = serde_json::Error;

    /// Parses an `If` conditional from a JSON string
    ///
    /// # Errors
    ///
    /// * If the string is not valid JSON
    /// * If the JSON structure doesn't match `If`
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

/// Creates a conditional if statement with a single action
#[must_use]
pub fn if_stmt(condition: Condition, action: impl Into<ActionEffect>) -> If {
    If {
        condition,
        actions: vec![action.into()],
        else_actions: vec![],
    }
}

/// Converts a value into a [`Value`] type
pub fn value(value: impl Into<Value>) -> Value {
    value.into()
}

/// Returns a hidden visibility value
#[must_use]
pub const fn hidden() -> Value {
    Value::Visibility(Visibility::Hidden)
}

/// Returns a visible visibility value
#[must_use]
pub const fn visible() -> Value {
    Value::Visibility(Visibility::Visible)
}

/// Returns a displayed value (display = true)
#[must_use]
pub const fn displayed() -> Value {
    Value::Display(true)
}

/// Returns a not displayed value (display = false)
#[must_use]
pub const fn not_displayed() -> Value {
    Value::Display(false)
}

/// Creates an equality condition between two values
pub fn eq(a: impl Into<Value>, b: impl Into<Value>) -> Condition {
    Condition::Eq(a.into(), b.into())
}

/// Gets the visibility value of an element by string ID
#[must_use]
pub fn get_visibility_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::Visibility {
        target: ElementTarget::StrId(str_id.into()),
    }
}

/// Gets the visibility value of an element by numeric ID
#[must_use]
pub const fn get_visibility_id(id: usize) -> CalcValue {
    CalcValue::Visibility {
        target: ElementTarget::Id(id),
    }
}

/// Gets the visibility value of the element itself
#[must_use]
pub const fn get_visibility_self() -> CalcValue {
    CalcValue::Visibility {
        target: ElementTarget::SelfTarget,
    }
}

/// Gets the visibility value of an element by class name
#[must_use]
pub fn get_visibility_class(class_name: impl Into<Target>) -> CalcValue {
    CalcValue::Visibility {
        target: ElementTarget::Class(class_name.into()),
    }
}

/// Gets the display value of an element by string ID
#[must_use]
pub fn get_display_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::StrId(str_id.into()),
    }
}

/// Gets the display value of an element by numeric ID
#[must_use]
pub const fn get_display_id(id: usize) -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::Id(id),
    }
}

/// Gets the display value of the element itself
#[must_use]
pub const fn get_display_self() -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::SelfTarget,
    }
}

/// Gets the display value of an element by class name
#[must_use]
pub fn get_display_class(class_name: impl Into<Target>) -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::Class(class_name.into()),
    }
}

/// Gets the display value of a child element by class name
#[must_use]
pub fn get_display_child_class(class_name: impl Into<Target>) -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::ChildClass(class_name.into()),
    }
}

/// Gets the display value of the last child element
#[must_use]
pub const fn get_display_last_child() -> CalcValue {
    CalcValue::Display {
        target: ElementTarget::LastChild,
    }
}

/// Gets the current event value (e.g., input field value)
#[must_use]
pub const fn get_event_value() -> CalcValue {
    CalcValue::EventValue
}

/// Gets the string ID of the element itself
#[must_use]
pub const fn get_id_self() -> CalcValue {
    CalcValue::Id {
        target: ElementTarget::SelfTarget,
    }
}

/// Gets a data attribute value from the element itself
#[must_use]
pub fn get_data_attr_value_self(attr: impl Into<String>) -> CalcValue {
    CalcValue::DataAttrValue {
        attr: attr.into(),
        target: ElementTarget::SelfTarget,
    }
}

/// Gets the width in pixels of an element by string ID
#[must_use]
pub fn get_width_px_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::WidthPx {
        target: ElementTarget::StrId(str_id.into()),
    }
}

/// Gets the width in pixels of an element by numeric ID
#[must_use]
pub const fn get_width_px_id(id: usize) -> CalcValue {
    CalcValue::WidthPx {
        target: ElementTarget::Id(id),
    }
}

/// Gets the width in pixels of the element itself
#[must_use]
pub const fn get_width_px_self() -> CalcValue {
    CalcValue::WidthPx {
        target: ElementTarget::SelfTarget,
    }
}

/// Gets the height in pixels of an element by string ID
#[must_use]
pub fn get_height_px_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::HeightPx {
        target: ElementTarget::StrId(str_id.into()),
    }
}

/// Gets the height in pixels of an element by numeric ID
#[must_use]
pub const fn get_height_px_id(id: usize) -> CalcValue {
    CalcValue::HeightPx {
        target: ElementTarget::Id(id),
    }
}

/// Gets the height in pixels of the element itself
#[must_use]
pub const fn get_height_px_self() -> CalcValue {
    CalcValue::HeightPx {
        target: ElementTarget::SelfTarget,
    }
}

/// Gets the X position of an element by string ID
#[must_use]
pub fn get_position_x_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::PositionX {
        target: ElementTarget::StrId(str_id.into()),
    }
}

/// Gets the X position of an element by numeric ID
#[must_use]
pub const fn get_position_x_id(id: usize) -> CalcValue {
    CalcValue::PositionX {
        target: ElementTarget::Id(id),
    }
}

/// Gets the X position of the element itself
#[must_use]
pub const fn get_position_x_self() -> CalcValue {
    CalcValue::PositionX {
        target: ElementTarget::SelfTarget,
    }
}

/// Gets the Y position of an element by string ID
#[must_use]
pub fn get_position_y_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::PositionY {
        target: ElementTarget::StrId(str_id.into()),
    }
}

/// Gets the Y position of an element by numeric ID
#[must_use]
pub const fn get_position_y_id(id: usize) -> CalcValue {
    CalcValue::PositionY {
        target: ElementTarget::Id(id),
    }
}

/// Gets the Y position of the element itself
#[must_use]
pub const fn get_position_y_self() -> CalcValue {
    CalcValue::PositionY {
        target: ElementTarget::SelfTarget,
    }
}

/// Gets the global mouse X coordinate
#[must_use]
pub const fn get_mouse_x() -> CalcValue {
    CalcValue::MouseX { target: None }
}

/// Gets the mouse X coordinate relative to an element by string ID
#[must_use]
pub fn get_mouse_x_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::MouseX {
        target: Some(ElementTarget::StrId(str_id.into())),
    }
}

/// Gets the mouse X coordinate relative to an element by numeric ID
#[must_use]
pub const fn get_mouse_x_id(id: usize) -> CalcValue {
    CalcValue::MouseX {
        target: Some(ElementTarget::Id(id)),
    }
}

/// Gets the mouse X coordinate relative to the element itself
#[must_use]
pub const fn get_mouse_x_self() -> CalcValue {
    CalcValue::MouseX {
        target: Some(ElementTarget::SelfTarget),
    }
}

/// Gets the global mouse Y coordinate
#[must_use]
pub const fn get_mouse_y() -> CalcValue {
    CalcValue::MouseY { target: None }
}

/// Gets the mouse Y coordinate relative to an element by string ID
#[must_use]
pub fn get_mouse_y_str_id(str_id: impl Into<Target>) -> CalcValue {
    CalcValue::MouseY {
        target: Some(ElementTarget::StrId(str_id.into())),
    }
}

/// Gets the mouse Y coordinate relative to an element by numeric ID
#[must_use]
pub const fn get_mouse_y_id(id: usize) -> CalcValue {
    CalcValue::MouseY {
        target: Some(ElementTarget::Id(id)),
    }
}

/// Gets the mouse Y coordinate relative to the element itself
#[must_use]
pub const fn get_mouse_y_self() -> CalcValue {
    CalcValue::MouseY {
        target: Some(ElementTarget::SelfTarget),
    }
}

/// Responsive design condition for adaptive styling
///
/// Used to create conditional values based on responsive breakpoints or screen targets.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Responsive {
    /// Single responsive target
    Target(String),
    /// Multiple responsive targets (any match)
    Targets(Vec<String>),
}

impl Responsive {
    /// Sets the value to use when the responsive condition matches
    #[must_use]
    pub fn then<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: Some(value.into()),
            default: None,
        }
    }

    /// Sets the fallback value when the responsive condition doesn't match
    #[must_use]
    pub fn or_else<T>(self, value: impl Into<T>) -> IfExpression<T, Self> {
        IfExpression {
            condition: self,
            value: None,
            default: Some(value.into()),
        }
    }
}

/// Creates a responsive condition for a single target
#[must_use]
pub fn if_responsive(target: impl Into<String>) -> Responsive {
    let target = target.into();
    Responsive::Target(target)
}

/// Creates a responsive condition that matches any of the provided targets
#[must_use]
pub fn if_responsive_any<T: Into<String>>(targets: impl Into<Vec<T>>) -> Responsive {
    Responsive::Targets(targets.into().into_iter().map(Into::into).collect())
}

// Add From implementations for specific enum types to handle responsive expressions
impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::LayoutDirection {
    /// Converts a responsive conditional expression into a `LayoutDirection`
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::TextAlign {
    /// Converts a responsive conditional expression into a `TextAlign`
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::AlignItems {
    /// Converts a responsive conditional expression into an `AlignItems`
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::JustifyContent {
    /// Converts a responsive conditional expression into a `JustifyContent`
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::Position {
    /// Converts a responsive conditional expression into a `Position`
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}

impl From<IfExpression<Self, Responsive>> for hyperchad_transformer_models::LayoutOverflow {
    /// Converts a responsive conditional expression into a `LayoutOverflow`
    fn from(if_expr: IfExpression<Self, Responsive>) -> Self {
        if_expr
            .default
            .unwrap_or_else(|| if_expr.value.unwrap_or_default())
    }
}
