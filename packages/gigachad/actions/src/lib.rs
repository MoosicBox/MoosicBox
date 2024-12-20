#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use gigachad_transformer_models::Visibility;

#[cfg(feature = "logic")]
pub mod logic;

#[cfg(feature = "gen")]
pub mod gen;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ElementTarget {
    StrId(String),
    #[cfg(feature = "id")]
    Id(usize),
    SelfTarget,
    LastChild,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ActionTrigger {
    Click,
    ClickOutside,
    Hover,
    Change,
    #[default]
    Immediate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub struct Action {
    pub trigger: ActionTrigger,
    pub action: ActionType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ActionType {
    Style {
        target: ElementTarget,
        action: StyleAction,
    },
    Navigate {
        url: String,
    },
    Log {
        message: String,
        level: LogLevel,
    },
    Custom {
        action: String,
    },
    #[cfg(feature = "logic")]
    Logic(logic::If),
}

impl ActionType {
    #[must_use]
    pub fn set_visibility_str_id(visibility: Visibility, target: &str) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.to_string()),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[must_use]
    pub fn hide_str_id(target: &str) -> Self {
        Self::set_visibility_str_id(Visibility::Hidden, target)
    }

    #[must_use]
    pub fn show_str_id(target: &str) -> Self {
        Self::set_visibility_str_id(Visibility::Visible, target)
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub const fn set_visibility_id(visibility: Visibility, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub const fn hide_id(target: usize) -> Self {
        Self::set_visibility_id(Visibility::Hidden, target)
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub const fn show_id(target: usize) -> Self {
        Self::set_visibility_id(Visibility::Visible, target)
    }

    #[must_use]
    pub const fn set_visibility_self(visibility: Visibility) -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[must_use]
    pub const fn hide_self() -> Self {
        Self::set_visibility_self(Visibility::Hidden)
    }

    #[must_use]
    pub const fn show_self() -> Self {
        Self::set_visibility_self(Visibility::Visible)
    }

    #[must_use]
    pub const fn set_visibility_last_child(visibility: Visibility) -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[must_use]
    pub const fn hide_last_child() -> Self {
        Self::set_visibility_last_child(Visibility::Hidden)
    }

    #[must_use]
    pub const fn show_last_child() -> Self {
        Self::set_visibility_last_child(Visibility::Visible)
    }

    #[must_use]
    pub fn set_display_str_id(display: bool, target: &str) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.to_string()),
            action: StyleAction::SetDisplay(display),
        }
    }

    #[must_use]
    pub fn no_display_str_id(target: &str) -> Self {
        Self::set_display_str_id(false, target)
    }

    #[must_use]
    pub fn display_str_id(target: &str) -> Self {
        Self::set_display_str_id(true, target)
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub const fn set_display_id(display: bool, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetDisplay(display),
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub const fn no_display_id(target: usize) -> Self {
        Self::set_display_id(false, target)
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub const fn display_id(target: usize) -> Self {
        Self::set_display_id(true, target)
    }

    #[must_use]
    pub const fn set_display_self(display: bool) -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetDisplay(display),
        }
    }

    #[must_use]
    pub const fn no_display_self() -> Self {
        Self::set_display_self(false)
    }

    #[must_use]
    pub const fn display_self() -> Self {
        Self::set_display_self(true)
    }

    #[must_use]
    pub const fn set_display_last_child(display: bool) -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetDisplay(display),
        }
    }

    #[must_use]
    pub const fn no_display_last_child() -> Self {
        Self::set_display_last_child(false)
    }

    #[must_use]
    pub const fn display_last_child() -> Self {
        Self::set_display_last_child(true)
    }
}

impl From<ActionType> for Action {
    fn from(value: ActionType) -> Self {
        Self {
            trigger: ActionTrigger::default(),
            action: value,
        }
    }
}

#[cfg(feature = "serde")]
impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[cfg(feature = "serde")]
impl<'a> TryFrom<&'a str> for ActionType {
    type Error = serde_json::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StyleAction {
    SetVisibility(Visibility),
    SetDisplay(bool),
}

#[cfg(feature = "serde")]
impl std::fmt::Display for StyleAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[cfg(feature = "serde")]
impl<'a> TryFrom<&'a str> for StyleAction {
    type Error = serde_json::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}
