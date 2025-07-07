#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad_transformer_models::Visibility;

#[cfg(feature = "logic")]
pub mod logic;

#[cfg(feature = "arb")]
pub mod arb;

pub mod dsl;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Target {
    Literal(String),
    Ref(String),
}

impl From<String> for Target {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}

impl From<&str> for Target {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_string())
    }
}

impl From<&String> for Target {
    fn from(value: &String) -> Self {
        Self::Literal(value.clone())
    }
}

impl From<&Self> for Target {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl Target {
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Literal(x) | Self::Ref(x) => Some(x),
        }
    }

    pub fn literal(str: impl Into<String>) -> Self {
        Self::Literal(str.into())
    }

    pub fn reference(str: impl Into<String>) -> Self {
        Self::Ref(str.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ElementTarget {
    StrId(Target),
    Class(Target),
    ChildClass(Target),
    Id(usize),
    SelfTarget,
    LastChild,
}

impl ElementTarget {
    #[must_use]
    pub fn str_id(target: impl Into<Target>) -> Self {
        Self::StrId(target.into())
    }

    #[must_use]
    pub fn class(target: impl Into<Target>) -> Self {
        Self::Class(target.into())
    }

    #[must_use]
    pub fn child_class(target: impl Into<Target>) -> Self {
        Self::ChildClass(target.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Key {
    Escape,
    Tab,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Backspace,
    Enter,
    Insert,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    NumLock,
    ScrollLock,
    CapsLock,
    Shift,
    Control,
    Alt,
    Meta,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
}

impl Key {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Escape => "Escape",
            Self::Tab => "Tab",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::Delete => "Delete",
            Self::Backspace => "Backspace",
            Self::Enter => "Enter",
            Self::Insert => "Insert",
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
            Self::NumLock => "NumLock",
            Self::ScrollLock => "ScrollLock",
            Self::CapsLock => "CapsLock",
            Self::Shift => "Shift",
            Self::Control => "Control",
            Self::Alt => "Alt",
            Self::Meta => "Meta",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::H => "H",
            Self::I => "I",
            Self::J => "J",
            Self::K => "K",
            Self::L => "L",
            Self::M => "M",
            Self::N => "N",
            Self::O => "O",
            Self::P => "P",
            Self::Q => "Q",
            Self::R => "R",
            Self::S => "S",
            Self::T => "T",
            Self::U => "U",
            Self::V => "V",
            Self::W => "W",
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::Key0 => "Key0",
            Self::Key1 => "Key1",
            Self::Key2 => "Key2",
            Self::Key3 => "Key3",
            Self::Key4 => "Key4",
            Self::Key5 => "Key5",
            Self::Key6 => "Key6",
            Self::Key7 => "Key7",
            Self::Key8 => "Key8",
            Self::Key9 => "Key9",
            Self::Numpad0 => "Numpad0",
            Self::Numpad1 => "Numpad1",
            Self::Numpad2 => "Numpad2",
            Self::Numpad3 => "Numpad3",
            Self::Numpad4 => "Numpad4",
            Self::Numpad5 => "Numpad5",
            Self::Numpad6 => "Numpad6",
            Self::Numpad7 => "Numpad7",
            Self::Numpad8 => "Numpad8",
            Self::Numpad9 => "Numpad9",
        }
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ActionTrigger {
    Click,
    ClickOutside,
    MouseDown,
    KeyDown,
    Hover,
    Change,
    Resize,
    Event(String),
    #[default]
    Immediate,
}

impl ActionTrigger {
    #[must_use]
    pub const fn trigger_type(&self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::ClickOutside => "ClickOutside",
            Self::MouseDown => "MouseDown",
            Self::KeyDown => "KeyDown",
            Self::Hover => "Hover",
            Self::Change => "Change",
            Self::Resize => "Resize",
            Self::Event(_) => "Event",
            Self::Immediate => "Immediate",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub struct Action {
    pub trigger: ActionTrigger,
    pub effect: ActionEffect,
}

#[cfg(feature = "serde")]
impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub struct ActionEffect {
    pub action: ActionType,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub delay_off: Option<u64>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub throttle: Option<u64>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub unique: Option<bool>,
}

impl ActionEffect {
    #[must_use]
    pub const fn into_action_effect(self) -> Self {
        self
    }
}

impl From<Vec<Self>> for ActionEffect {
    fn from(value: Vec<Self>) -> Self {
        Self {
            action: ActionType::MultiEffect(value),
            ..Default::default()
        }
    }
}

impl From<Vec<ActionType>> for ActionEffect {
    fn from(value: Vec<ActionType>) -> Self {
        Self {
            action: ActionType::Multi(value),
            ..Default::default()
        }
    }
}

impl ActionEffect {
    #[must_use]
    pub const fn delay_off(mut self, millis: u64) -> Self {
        self.delay_off = Some(millis);
        self
    }

    #[must_use]
    pub const fn throttle(mut self, millis: u64) -> Self {
        self.throttle = Some(millis);
        self
    }

    #[must_use]
    pub const fn unique(mut self) -> Self {
        self.unique = Some(true);
        self
    }
}

impl From<ActionType> for ActionEffect {
    fn from(value: ActionType) -> Self {
        Self {
            action: value,
            ..Default::default()
        }
    }
}

impl From<Box<ActionType>> for ActionEffect {
    fn from(value: Box<ActionType>) -> Self {
        (*value).into()
    }
}

#[cfg(feature = "serde")]
impl std::fmt::Display for ActionEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
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
pub enum InputActionType {
    Select { target: ElementTarget },
}

#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ActionType {
    #[default]
    NoOp,
    Let {
        name: String,
        value: dsl::Expression,
    },
    Style {
        target: ElementTarget,
        action: StyleAction,
    },
    Input(InputActionType),
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
    Event {
        name: String,
        action: Box<ActionType>,
    },
    Multi(Vec<ActionType>),
    MultiEffect(Vec<ActionEffect>),
    #[cfg(feature = "logic")]
    Parameterized {
        action: Box<ActionType>,
        value: logic::Value,
    },
    #[cfg(feature = "logic")]
    Logic(logic::If),
}

impl ActionType {
    #[must_use]
    pub fn into_action_effect(self) -> ActionEffect {
        self.into()
    }
}

impl ActionType {
    #[must_use]
    pub fn on_event(event: impl Into<String>, action: impl Into<Self>) -> Self {
        Self::Event {
            name: event.into(),
            action: Box::new(action.into()),
        }
    }

    #[must_use]
    pub fn set_focus_str_id(focus: bool, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetFocus(focus),
        }
    }

    #[must_use]
    pub fn set_focus_class(focus: bool, class: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class.into()),
            action: StyleAction::SetFocus(focus),
        }
    }

    #[must_use]
    pub fn set_focus_child_class(focus: bool, class: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::ChildClass(class.into()),
            action: StyleAction::SetFocus(focus),
        }
    }

    #[must_use]
    pub fn select_str_id(target: impl Into<Target>) -> Self {
        Self::Input(InputActionType::Select {
            target: ElementTarget::StrId(target.into()),
        })
    }

    #[must_use]
    pub fn select_class(class: impl Into<Target>) -> Self {
        Self::Input(InputActionType::Select {
            target: ElementTarget::Class(class.into()),
        })
    }

    #[must_use]
    pub fn select_child_class(class: impl Into<Target>) -> Self {
        Self::Input(InputActionType::Select {
            target: ElementTarget::ChildClass(class.into()),
        })
    }

    #[must_use]
    pub fn set_visibility_str_id(visibility: Visibility, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[must_use]
    pub fn set_visibility_child_class(visibility: Visibility, class: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::ChildClass(class.into()),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[must_use]
    pub fn hide_str_id(target: impl Into<Target>) -> Self {
        Self::set_visibility_str_id(Visibility::Hidden, target)
    }

    #[must_use]
    pub fn show_str_id(target: impl Into<Target>) -> Self {
        Self::set_visibility_str_id(Visibility::Visible, target)
    }

    #[must_use]
    pub fn hide_class(class_name: impl Into<Target>) -> Self {
        Self::set_visibility_class(Visibility::Hidden, class_name)
    }

    #[must_use]
    pub fn show_class(class_name: impl Into<Target>) -> Self {
        Self::set_visibility_class(Visibility::Visible, class_name)
    }

    #[must_use]
    pub fn focus_str_id(target: impl Into<Target>) -> Self {
        Self::set_focus_str_id(true, target)
    }

    #[must_use]
    pub fn focus_class(class_name: impl Into<Target>) -> Self {
        Self::set_focus_class(true, class_name)
    }

    #[must_use]
    pub fn set_visibility_class(visibility: Visibility, class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class_name.into()),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[must_use]
    pub const fn set_visibility_id(visibility: Visibility, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    #[must_use]
    pub const fn hide_id(target: usize) -> Self {
        Self::set_visibility_id(Visibility::Hidden, target)
    }

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

    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_visibility_str_id(target: impl Into<Target>) -> Self {
        let target = target.into();
        Self::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(
                crate::logic::get_visibility_str_id(&target).into(),
                crate::logic::Value::Visibility(hyperchad_transformer_models::Visibility::Visible),
            ),
            actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::str_id(&target),
                    action: crate::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Hidden,
                    ),
                },
                ..Default::default()
            }],
            else_actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::str_id(&target),
                    action: crate::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Visible,
                    ),
                },
                ..Default::default()
            }],
        })
    }

    #[must_use]
    pub fn set_display_str_id(display: bool, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetDisplay(display),
        }
    }

    #[must_use]
    pub fn set_display_class(display: bool, class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class_name.into()),
            action: StyleAction::SetDisplay(display),
        }
    }

    #[must_use]
    pub fn no_display_str_id(target: impl Into<Target>) -> Self {
        Self::set_display_str_id(false, target)
    }

    #[must_use]
    pub fn display_str_id(target: impl Into<Target>) -> Self {
        Self::set_display_str_id(true, target)
    }

    #[must_use]
    pub fn no_display_class(class_name: impl Into<Target>) -> Self {
        Self::set_display_class(false, class_name)
    }

    #[must_use]
    pub fn display_class(class_name: impl Into<Target>) -> Self {
        Self::set_display_class(true, class_name)
    }

    #[must_use]
    pub const fn set_display_id(display: bool, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetDisplay(display),
        }
    }

    #[must_use]
    pub const fn no_display_id(target: usize) -> Self {
        Self::set_display_id(false, target)
    }

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

    #[must_use]
    pub fn set_background_str_id(background: impl Into<String>, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    #[must_use]
    pub fn set_background_id(background: impl Into<String>, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    #[must_use]
    pub fn set_background_self(background: impl Into<String>) -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    #[must_use]
    pub const fn remove_background_self() -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetBackground(None),
        }
    }

    #[must_use]
    pub fn remove_background_str_id(target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetBackground(None),
        }
    }

    #[must_use]
    pub const fn remove_background_id(target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetBackground(None),
        }
    }

    #[must_use]
    pub fn remove_background_class(class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class_name.into()),
            action: StyleAction::SetBackground(None),
        }
    }

    #[must_use]
    pub fn remove_background_child_class(class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::ChildClass(class_name.into()),
            action: StyleAction::SetBackground(None),
        }
    }

    #[must_use]
    pub const fn remove_background_last_child() -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetBackground(None),
        }
    }

    #[must_use]
    pub fn set_background_last_child(background: impl Into<String>) -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    #[must_use]
    pub fn and(self, action: impl Into<Self>) -> Self {
        if let Self::Multi(mut actions) = self {
            actions.push(action.into());
            Self::Multi(actions)
        } else {
            Self::Multi(vec![self, action.into()])
        }
    }

    #[must_use]
    pub const fn throttle(self, millis: u64) -> ActionEffect {
        ActionEffect {
            action: self,
            throttle: Some(millis),
            delay_off: None,
            unique: None,
        }
    }

    #[must_use]
    pub const fn delay_off(self, millis: u64) -> ActionEffect {
        ActionEffect {
            action: self,
            throttle: None,
            delay_off: Some(millis),
            unique: None,
        }
    }

    #[must_use]
    pub const fn unique(self) -> ActionEffect {
        ActionEffect {
            action: self,
            throttle: None,
            delay_off: None,
            unique: Some(true),
        }
    }
}

#[cfg(feature = "logic")]
impl From<logic::If> for ActionType {
    fn from(value: logic::If) -> Self {
        Self::Logic(value)
    }
}

impl From<ActionType> for Action {
    fn from(value: ActionType) -> Self {
        Self {
            trigger: ActionTrigger::default(),
            effect: ActionEffect {
                action: value,
                delay_off: None,
                throttle: None,
                unique: None,
            },
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
    SetFocus(bool),
    SetDisplay(bool),
    SetBackground(Option<String>),
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

/// Prelude module that re-exports commonly used items.
pub mod prelude {
    pub use crate::{
        Action, ActionEffect, ActionTrigger, ActionType, ElementTarget, LogLevel, StyleAction,
        dsl::*,
    };

    #[cfg(feature = "logic")]
    pub use crate::logic::*;
}
