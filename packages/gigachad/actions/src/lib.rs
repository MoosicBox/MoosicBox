#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use gigachad_transformer_models::Visibility;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ElementTarget {
    StrId(String),
    #[cfg(feature = "id")]
    Id(usize),
    SelfTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionType {
    Click { action: String },
    ClickOutside { action: String },
    Hover { action: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleActionType {
    Click(StyleAction),
    ClickOutside(StyleAction),
    Hover(StyleAction),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum StyleAction {
    SetVisibility {
        visibility: Visibility,
        target: ElementTarget,
    },
}

impl StyleAction {
    #[must_use]
    pub fn visibility_str_id(visibility: Visibility, target: &str) -> Self {
        Self::SetVisibility {
            visibility,
            target: ElementTarget::StrId(target.to_string()),
        }
    }

    #[cfg(feature = "id")]
    #[must_use]
    pub const fn visibility_id(visibility: Visibility, target: usize) -> Self {
        Self::SetVisibility {
            visibility,
            target: ElementTarget::Id(target),
        }
    }

    #[must_use]
    pub const fn visibility_self(visibility: Visibility) -> Self {
        Self::SetVisibility {
            visibility,
            target: ElementTarget::SelfTarget,
        }
    }

    #[must_use]
    pub const fn hide_self() -> Self {
        Self::visibility_self(Visibility::Hidden)
    }

    #[must_use]
    pub const fn show_self() -> Self {
        Self::visibility_self(Visibility::Visible)
    }
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
