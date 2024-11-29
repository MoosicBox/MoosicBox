#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use gigachad_actions::{ElementTarget, StyleAction};
use gigachad_transformer_models::Visibility;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CalcValue {
    GetVisibility { target: ElementTarget },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Value {
    Calc(CalcValue),
    Visibility(Visibility),
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Condition {
    Eq(Value, Value),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct If {
    pub condition: Condition,
    pub action: StyleAction,
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
pub const fn if_stmt(condition: Condition, action: StyleAction) -> If {
    If { condition, action }
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
pub const fn get_visibility_self() -> CalcValue {
    CalcValue::GetVisibility {
        target: ElementTarget::SelfTarget,
    }
}
