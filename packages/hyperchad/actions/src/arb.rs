//! Arbitrary value generation for property-based testing
//!
//! This module provides [`quickcheck::Arbitrary`] implementations for action types,
//! enabling property-based testing of action serialization, deserialization, and processing.
//!
//! # Usage
//!
//! Enable the `arb` feature to use this module:
//!
//! ```toml
//! [dependencies]
//! hyperchad_actions = { version = "...", features = ["arb"] }
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use quickcheck::{quickcheck, TestResult};
//! use hyperchad_actions::Action;
//!
//! fn prop_action_roundtrip(action: Action) -> TestResult {
//!     let json = serde_json::to_string(&action).unwrap();
//!     let deserialized: Action = serde_json::from_str(&json).unwrap();
//!     TestResult::from_bool(action == deserialized)
//! }
//!
//! quickcheck(prop_action_roundtrip as fn(Action) -> TestResult);
//! ```

use hyperchad_transformer_models::Visibility;
use moosicbox_arb::xml::XmlString;
use quickcheck::{Arbitrary, Gen};

use crate::{
    Action, ActionEffect, ActionTrigger, ActionType, ElementTarget, LogLevel, StyleAction,
};

/// Helper function to create a generator with half the size, capped at max
fn half_g_max(g: &Gen, max: usize) -> Gen {
    Gen::new(std::cmp::min(max, g.size() / 2))
}

impl Arbitrary for ActionTrigger {
    /// Generates an arbitrary `ActionTrigger` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        let half_g = &mut half_g_max(g, 10);
        g.choose(&[
            Self::Click,
            Self::ClickOutside,
            Self::Hover,
            Self::Change,
            Self::Immediate,
            Self::HttpBeforeRequest,
            Self::HttpAfterRequest,
            Self::HttpRequestSuccess,
            Self::HttpRequestError,
            Self::HttpRequestAbort,
            Self::HttpRequestTimeout,
            Self::Event(XmlString::arbitrary(half_g).0),
        ])
        .unwrap()
        .clone()
    }
}

impl Arbitrary for StyleAction {
    /// Generates an arbitrary `StyleAction` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=1).collect::<Vec<_>>()).unwrap() {
            0 => Self::SetVisibility(Visibility::arbitrary(g)),
            1 => Self::SetDisplay(bool::arbitrary(g)),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for LogLevel {
    /// Generates an arbitrary `LogLevel` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Error,
            Self::Warn,
            Self::Info,
            Self::Debug,
            Self::Trace,
        ])
        .unwrap()
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::CalcValue {
    /// Generates an arbitrary `CalcValue` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        Self::Visibility {
            target: ElementTarget::arbitrary(g),
        }
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::Value {
    /// Generates an arbitrary `Value` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=1).collect::<Vec<_>>()).unwrap() {
            0 => Self::Calc(Arbitrary::arbitrary(g)),
            1 => Self::Visibility(Arbitrary::arbitrary(g)),
            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::Condition {
    /// Generates an arbitrary `Condition` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        Self::Eq(Arbitrary::arbitrary(g), Arbitrary::arbitrary(g))
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::If {
    /// Generates an arbitrary `If` conditional for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            condition: Arbitrary::arbitrary(g),
            actions: Arbitrary::arbitrary(g),
            else_actions: Arbitrary::arbitrary(g),
        }
    }
}

impl Arbitrary for ActionType {
    /// Generates an arbitrary `ActionType` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        let g = &mut half_g_max(g, 10);

        #[cfg(feature = "logic")]
        let max = 4;
        #[cfg(not(feature = "logic"))]
        let max = 3;
        match *g.choose(&(0..=max).collect::<Vec<_>>()).unwrap() {
            0 => Self::Style {
                target: ElementTarget::arbitrary(g),
                action: StyleAction::arbitrary(g),
            },
            1 => Self::Navigate {
                url: XmlString::arbitrary(g).0,
            },
            2 => Self::Log {
                message: XmlString::arbitrary(g).0,
                level: LogLevel::arbitrary(g),
            },
            3 => Self::Custom {
                action: XmlString::arbitrary(g).0,
            },
            #[cfg(feature = "logic")]
            4 => Self::Logic(crate::logic::If::arbitrary(g)),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for Action {
    /// Generates an arbitrary `Action` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        let trigger = ActionTrigger::arbitrary(g);

        if let ActionTrigger::Event(name) = &trigger {
            Self {
                trigger: trigger.clone(),
                effect: ActionEffect {
                    action: ActionType::Event {
                        name: name.clone(),
                        action: Box::new(ActionType::arbitrary(g)),
                    },
                    delay_off: Option::arbitrary(g),
                    throttle: Option::arbitrary(g),
                    unique: Option::arbitrary(g),
                },
            }
        } else {
            Self {
                trigger,
                effect: Arbitrary::arbitrary(g),
            }
        }
    }
}

impl Arbitrary for ActionEffect {
    /// Generates an arbitrary `ActionEffect` for property-based testing
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            action: Arbitrary::arbitrary(g),
            delay_off: Option::arbitrary(g),
            throttle: Option::arbitrary(g),
            unique: Option::arbitrary(g),
        }
    }
}
