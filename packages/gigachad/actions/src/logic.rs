use gigachad_transformer_models::Visibility;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{Action, ElementTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CalcValue {
    GetVisibility { target: ElementTarget },
    GetId { target: ElementTarget },
    GetDataAttrValue { attr: String, target: ElementTarget },
    GetEventValue,
    GetHeightPx { target: ElementTarget },
}

impl CalcValue {
    pub fn eq(self, other: impl Into<Value>) -> Condition {
        Condition::Eq(self.into(), other.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Value {
    Calc(CalcValue),
    Real(f32),
    Visibility(Visibility),
    String(String),
}

impl Value {
    pub fn eq(self, other: impl Into<Self>) -> Condition {
        Condition::Eq(self, other.into())
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

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Condition {
    Eq(Value, Value),
}

impl Condition {
    pub fn then(self, action: impl Into<Action>) -> If {
        If {
            condition: self,
            actions: vec![action.into()],
            else_actions: vec![],
        }
    }

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
    CalcValue::GetVisibility {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[cfg(feature = "id")]
#[must_use]
pub const fn get_visibility_id(id: usize) -> CalcValue {
    CalcValue::GetVisibility {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_visibility_self() -> CalcValue {
    CalcValue::GetVisibility {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub const fn get_event_value() -> CalcValue {
    CalcValue::GetEventValue
}

#[must_use]
pub const fn get_id_self() -> CalcValue {
    CalcValue::GetId {
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_data_attr_value_self(attr: impl Into<String>) -> CalcValue {
    CalcValue::GetDataAttrValue {
        attr: attr.into(),
        target: ElementTarget::SelfTarget,
    }
}

#[must_use]
pub fn get_height_px_str_id(str_id: impl Into<String>) -> CalcValue {
    CalcValue::GetHeightPx {
        target: ElementTarget::StrId(str_id.into()),
    }
}

#[cfg(feature = "id")]
#[must_use]
pub const fn get_height_px_id(id: usize) -> CalcValue {
    CalcValue::GetHeightPx {
        target: ElementTarget::Id(id),
    }
}

#[must_use]
pub const fn get_height_px_self() -> CalcValue {
    CalcValue::GetHeightPx {
        target: ElementTarget::SelfTarget,
    }
}
