#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;

use hyperchad_transformer_models::Visibility;

/// Logic and conditional evaluation for actions
#[cfg(feature = "logic")]
pub mod logic;

/// Arbitrary value generation for property-based testing
#[cfg(feature = "arb")]
pub mod arb;

/// Action handler implementation for processing and executing actions
#[cfg(feature = "handler")]
pub mod handler;

pub mod dsl;

pub use hyperchad_transformer_models::{ElementTarget, Target};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Keyboard key representation for key events
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
    /// Returns the string representation of the key
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

/// Context information for HTTP-related action events
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HttpEventContext {
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub headers: Option<BTreeMap<String, String>>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// Event trigger that determines when an action should execute
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
    HttpBeforeRequest,
    HttpAfterRequest,
    HttpRequestSuccess,
    HttpRequestError,
    HttpRequestAbort,
    HttpRequestTimeout,
}

impl ActionTrigger {
    /// Returns the trigger type name as a string
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
            Self::HttpBeforeRequest => "HttpBeforeRequest",
            Self::HttpAfterRequest => "HttpAfterRequest",
            Self::HttpRequestSuccess => "HttpRequestSuccess",
            Self::HttpRequestError => "HttpRequestError",
            Self::HttpRequestAbort => "HttpRequestAbort",
            Self::HttpRequestTimeout => "HttpRequestTimeout",
        }
    }
}

/// Action that combines a trigger event with an effect
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub struct Action {
    /// Event that triggers this action
    pub trigger: ActionTrigger,
    /// Effect to execute when triggered
    pub effect: ActionEffect,
}

#[cfg(feature = "serde")]
impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

/// Effect that wraps an action with timing and execution modifiers
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub struct ActionEffect {
    /// The action to execute
    pub action: ActionType,
    /// Milliseconds to delay before turning off the effect
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub delay_off: Option<u64>,
    /// Milliseconds to throttle repeated executions
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub throttle: Option<u64>,
    /// Whether this effect should be unique
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub unique: Option<bool>,
}

impl ActionEffect {
    /// Converts this effect into an `ActionEffect` (identity function for chaining)
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
    /// Sets the `delay_off` duration in milliseconds
    #[must_use]
    pub const fn delay_off(mut self, millis: u64) -> Self {
        self.delay_off = Some(millis);
        self
    }

    /// Sets the throttle duration in milliseconds
    #[must_use]
    pub const fn throttle(mut self, millis: u64) -> Self {
        self.throttle = Some(millis);
        self
    }

    /// Marks this effect as unique
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

/// Log severity level for logging actions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Input-related action types
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum InputActionType {
    /// Select an input element
    Select {
        /// Target element to select
        target: ElementTarget,
    },
}

/// Types of actions that can be performed on UI elements
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ActionType {
    /// No operation action
    #[default]
    NoOp,
    /// Variable assignment
    Let {
        /// Variable name
        name: String,
        /// Value expression
        value: dsl::Expression,
    },
    /// Style modification action
    Style {
        /// Target element
        target: ElementTarget,
        /// Style action to apply
        action: StyleAction,
    },
    /// Input-related action
    Input(InputActionType),
    /// Navigate to a URL
    Navigate {
        /// URL to navigate to
        url: String,
    },
    /// Log a message
    Log {
        /// Message to log
        message: String,
        /// Log level
        level: LogLevel,
    },
    /// Custom action
    Custom {
        /// Custom action name
        action: String,
    },
    /// Event-triggered action
    Event {
        /// Event name
        name: String,
        /// Action to execute on event
        action: Box<Self>,
    },
    /// Multiple actions executed sequentially
    Multi(Vec<Self>),
    /// Multiple effects executed sequentially
    MultiEffect(Vec<ActionEffect>),
    /// Parameterized action with dynamic value
    #[cfg(feature = "logic")]
    Parameterized {
        /// Action to execute
        action: Box<Self>,
        /// Dynamic value parameter
        value: logic::Value,
    },
    /// Conditional logic action
    #[cfg(feature = "logic")]
    Logic(logic::If),
}

impl ActionType {
    /// Converts this action into an `ActionEffect`
    #[must_use]
    pub fn into_action_effect(self) -> ActionEffect {
        self.into()
    }
}

impl ActionType {
    /// Creates an event-triggered action
    #[must_use]
    pub fn on_event(event: impl Into<String>, action: impl Into<Self>) -> Self {
        Self::Event {
            name: event.into(),
            action: Box::new(action.into()),
        }
    }

    /// Sets focus on an element identified by string ID
    #[must_use]
    pub fn set_focus_str_id(focus: bool, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetFocus(focus),
        }
    }

    /// Sets focus on an element identified by class name
    #[must_use]
    pub fn set_focus_class(focus: bool, class: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class.into()),
            action: StyleAction::SetFocus(focus),
        }
    }

    /// Sets focus on a child element identified by class name
    #[must_use]
    pub fn set_focus_child_class(focus: bool, class: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::ChildClass(class.into()),
            action: StyleAction::SetFocus(focus),
        }
    }

    /// Selects an input element identified by string ID
    #[must_use]
    pub fn select_str_id(target: impl Into<Target>) -> Self {
        Self::Input(InputActionType::Select {
            target: ElementTarget::StrId(target.into()),
        })
    }

    /// Selects an input element identified by class name
    #[must_use]
    pub fn select_class(class: impl Into<Target>) -> Self {
        Self::Input(InputActionType::Select {
            target: ElementTarget::Class(class.into()),
        })
    }

    /// Selects a child input element identified by class name
    #[must_use]
    pub fn select_child_class(class: impl Into<Target>) -> Self {
        Self::Input(InputActionType::Select {
            target: ElementTarget::ChildClass(class.into()),
        })
    }

    /// Sets visibility on an element identified by string ID
    #[must_use]
    pub fn set_visibility_str_id(visibility: Visibility, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    /// Sets visibility on a child element identified by class name
    #[must_use]
    pub fn set_visibility_child_class(visibility: Visibility, class: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::ChildClass(class.into()),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    /// Hides an element identified by string ID
    #[must_use]
    pub fn hide_str_id(target: impl Into<Target>) -> Self {
        Self::set_visibility_str_id(Visibility::Hidden, target)
    }

    /// Shows an element identified by string ID
    #[must_use]
    pub fn show_str_id(target: impl Into<Target>) -> Self {
        Self::set_visibility_str_id(Visibility::Visible, target)
    }

    /// Hides an element identified by class name
    #[must_use]
    pub fn hide_class(class_name: impl Into<Target>) -> Self {
        Self::set_visibility_class(Visibility::Hidden, class_name)
    }

    /// Shows an element identified by class name
    #[must_use]
    pub fn show_class(class_name: impl Into<Target>) -> Self {
        Self::set_visibility_class(Visibility::Visible, class_name)
    }

    /// Focuses an element identified by string ID
    #[must_use]
    pub fn focus_str_id(target: impl Into<Target>) -> Self {
        Self::set_focus_str_id(true, target)
    }

    /// Focuses an element identified by class name
    #[must_use]
    pub fn focus_class(class_name: impl Into<Target>) -> Self {
        Self::set_focus_class(true, class_name)
    }

    /// Sets visibility on an element identified by class name
    #[must_use]
    pub fn set_visibility_class(visibility: Visibility, class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class_name.into()),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    /// Sets visibility on an element identified by numeric ID
    #[must_use]
    pub const fn set_visibility_id(visibility: Visibility, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetVisibility(visibility),
        }
    }

    /// Hides an element identified by numeric ID
    #[must_use]
    pub const fn hide_id(target: usize) -> Self {
        Self::set_visibility_id(Visibility::Hidden, target)
    }

    /// Shows an element identified by numeric ID
    #[must_use]
    pub const fn show_id(target: usize) -> Self {
        Self::set_visibility_id(Visibility::Visible, target)
    }

    /// Sets visibility on the element itself
    #[must_use]
    pub const fn set_visibility_self(visibility: Visibility) -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetVisibility(visibility),
        }
    }

    /// Hides the element itself
    #[must_use]
    pub const fn hide_self() -> Self {
        Self::set_visibility_self(Visibility::Hidden)
    }

    /// Shows the element itself
    #[must_use]
    pub const fn show_self() -> Self {
        Self::set_visibility_self(Visibility::Visible)
    }

    /// Sets visibility on the last child element
    #[must_use]
    pub const fn set_visibility_last_child(visibility: Visibility) -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetVisibility(visibility),
        }
    }

    /// Hides the last child element
    #[must_use]
    pub const fn hide_last_child() -> Self {
        Self::set_visibility_last_child(Visibility::Hidden)
    }

    /// Shows the last child element
    #[must_use]
    pub const fn show_last_child() -> Self {
        Self::set_visibility_last_child(Visibility::Visible)
    }

    /// Toggles visibility on an element identified by string ID
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

    /// Sets display property on an element identified by string ID
    #[must_use]
    pub fn set_display_str_id(display: bool, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetDisplay(display),
        }
    }

    /// Sets display property on an element identified by class name
    #[must_use]
    pub fn set_display_class(display: bool, class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class_name.into()),
            action: StyleAction::SetDisplay(display),
        }
    }

    /// Disables display on an element identified by string ID
    #[must_use]
    pub fn no_display_str_id(target: impl Into<Target>) -> Self {
        Self::set_display_str_id(false, target)
    }

    /// Enables display on an element identified by string ID
    #[must_use]
    pub fn display_str_id(target: impl Into<Target>) -> Self {
        Self::set_display_str_id(true, target)
    }

    /// Disables display on an element identified by class name
    #[must_use]
    pub fn no_display_class(class_name: impl Into<Target>) -> Self {
        Self::set_display_class(false, class_name)
    }

    /// Enables display on an element identified by class name
    #[must_use]
    pub fn display_class(class_name: impl Into<Target>) -> Self {
        Self::set_display_class(true, class_name)
    }

    /// Sets display property on an element identified by numeric ID
    #[must_use]
    pub const fn set_display_id(display: bool, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetDisplay(display),
        }
    }

    /// Disables display on an element identified by numeric ID
    #[must_use]
    pub const fn no_display_id(target: usize) -> Self {
        Self::set_display_id(false, target)
    }

    /// Enables display on an element identified by numeric ID
    #[must_use]
    pub const fn display_id(target: usize) -> Self {
        Self::set_display_id(true, target)
    }

    /// Sets display property on the element itself
    #[must_use]
    pub const fn set_display_self(display: bool) -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetDisplay(display),
        }
    }

    /// Disables display on the element itself
    #[must_use]
    pub const fn no_display_self() -> Self {
        Self::set_display_self(false)
    }

    /// Enables display on the element itself
    #[must_use]
    pub const fn display_self() -> Self {
        Self::set_display_self(true)
    }

    /// Sets display property on the last child element
    #[must_use]
    pub const fn set_display_last_child(display: bool) -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetDisplay(display),
        }
    }

    /// Disables display on the last child element
    #[must_use]
    pub const fn no_display_last_child() -> Self {
        Self::set_display_last_child(false)
    }

    /// Enables display on the last child element
    #[must_use]
    pub const fn display_last_child() -> Self {
        Self::set_display_last_child(true)
    }

    /// Sets display property on a child element identified by class name
    #[must_use]
    pub fn set_display_child_class(display: bool, class: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::ChildClass(class.into()),
            action: StyleAction::SetDisplay(display),
        }
    }

    /// Disables display on a child element identified by class name
    #[must_use]
    pub fn no_display_child_class(class: impl Into<Target>) -> Self {
        Self::set_display_child_class(false, class)
    }

    /// Enables display on a child element identified by class name
    #[must_use]
    pub fn display_child_class(class: impl Into<Target>) -> Self {
        Self::set_display_child_class(true, class)
    }

    /// Toggles display property on an element identified by string ID
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_display_str_id(target: impl Into<Target>) -> Self {
        let target = target.into();
        Self::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(
                crate::logic::get_display_str_id(&target).into(),
                crate::logic::Value::Display(true),
            ),
            actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::str_id(&target),
                    action: crate::StyleAction::SetDisplay(false),
                },
                ..Default::default()
            }],
            else_actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::str_id(&target),
                    action: crate::StyleAction::SetDisplay(true),
                },
                ..Default::default()
            }],
        })
    }

    /// Toggles display property on an element identified by class name
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_display_str_class(target: impl Into<Target>) -> Self {
        let target = target.into();
        Self::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(
                crate::logic::get_display_class(&target).into(),
                crate::logic::Value::Display(true),
            ),
            actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::class(&target),
                    action: crate::StyleAction::SetDisplay(false),
                },
                ..Default::default()
            }],
            else_actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::class(&target),
                    action: crate::StyleAction::SetDisplay(true),
                },
                ..Default::default()
            }],
        })
    }

    /// Toggles display property on a child element identified by class name
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_display_child_class(target: impl Into<Target>) -> Self {
        let target = target.into();
        Self::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(
                crate::logic::get_display_child_class(&target).into(),
                crate::logic::Value::Display(true),
            ),
            actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::child_class(&target),
                    action: crate::StyleAction::SetDisplay(false),
                },
                ..Default::default()
            }],
            else_actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::child_class(&target),
                    action: crate::StyleAction::SetDisplay(true),
                },
                ..Default::default()
            }],
        })
    }

    /// Toggles display property on an element identified by numeric ID
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_display_id(target: usize) -> Self {
        Self::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(
                crate::logic::Value::Calc(crate::logic::get_display_id(target)),
                crate::logic::Value::Display(true),
            ),
            actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::Id(target),
                    action: crate::StyleAction::SetDisplay(false),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
            else_actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::Id(target),
                    action: crate::StyleAction::SetDisplay(true),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
        })
    }

    /// Toggles display property on the element itself
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_display_self() -> Self {
        Self::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(
                crate::logic::Value::Calc(crate::logic::get_display_self()),
                crate::logic::Value::Display(true),
            ),
            actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::SelfTarget,
                    action: crate::StyleAction::SetDisplay(false),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
            else_actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::SelfTarget,
                    action: crate::StyleAction::SetDisplay(true),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
        })
    }

    /// Toggles display property on the last child element
    #[cfg(feature = "logic")]
    #[must_use]
    pub fn toggle_display_last_child() -> Self {
        Self::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(
                crate::logic::Value::Calc(crate::logic::get_display_last_child()),
                crate::logic::Value::Display(true),
            ),
            actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::LastChild,
                    action: crate::StyleAction::SetDisplay(false),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
            else_actions: vec![crate::ActionEffect {
                action: Self::Style {
                    target: crate::ElementTarget::LastChild,
                    action: crate::StyleAction::SetDisplay(true),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
        })
    }

    /// Sets background on an element identified by string ID
    #[must_use]
    pub fn set_background_str_id(background: impl Into<String>, target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    /// Sets background on an element identified by numeric ID
    #[must_use]
    pub fn set_background_id(background: impl Into<String>, target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    /// Sets background on the element itself
    #[must_use]
    pub fn set_background_self(background: impl Into<String>) -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    /// Removes background from the element itself
    #[must_use]
    pub const fn remove_background_self() -> Self {
        Self::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetBackground(None),
        }
    }

    /// Removes background from an element identified by string ID
    #[must_use]
    pub fn remove_background_str_id(target: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::StrId(target.into()),
            action: StyleAction::SetBackground(None),
        }
    }

    /// Removes background from an element identified by numeric ID
    #[must_use]
    pub const fn remove_background_id(target: usize) -> Self {
        Self::Style {
            target: ElementTarget::Id(target),
            action: StyleAction::SetBackground(None),
        }
    }

    /// Removes background from an element identified by class name
    #[must_use]
    pub fn remove_background_class(class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::Class(class_name.into()),
            action: StyleAction::SetBackground(None),
        }
    }

    /// Removes background from a child element identified by class name
    #[must_use]
    pub fn remove_background_child_class(class_name: impl Into<Target>) -> Self {
        Self::Style {
            target: ElementTarget::ChildClass(class_name.into()),
            action: StyleAction::SetBackground(None),
        }
    }

    /// Removes background from the last child element
    #[must_use]
    pub const fn remove_background_last_child() -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetBackground(None),
        }
    }

    /// Sets background on the last child element
    #[must_use]
    pub fn set_background_last_child(background: impl Into<String>) -> Self {
        Self::Style {
            target: ElementTarget::LastChild,
            action: StyleAction::SetBackground(Some(background.into())),
        }
    }

    /// Chains this action with another action
    #[must_use]
    pub fn and(self, action: impl Into<Self>) -> Self {
        if let Self::Multi(mut actions) = self {
            actions.push(action.into());
            Self::Multi(actions)
        } else {
            Self::Multi(vec![self, action.into()])
        }
    }

    /// Wraps this action with a throttle delay
    #[must_use]
    pub const fn throttle(self, millis: u64) -> ActionEffect {
        ActionEffect {
            action: self,
            throttle: Some(millis),
            delay_off: None,
            unique: None,
        }
    }

    /// Wraps this action with a `delay_off` duration
    #[must_use]
    pub const fn delay_off(self, millis: u64) -> ActionEffect {
        ActionEffect {
            action: self,
            throttle: None,
            delay_off: Some(millis),
            unique: None,
        }
    }

    /// Marks this action as unique
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

/// Style modification actions for UI elements
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
