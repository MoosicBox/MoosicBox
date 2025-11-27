//! Arbitrary value generation for property-based testing
//!
//! This module provides [`proptest::arbitrary::Arbitrary`] implementations for action types,
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
//! use proptest::prelude::*;
//! use hyperchad_actions::Action;
//!
//! proptest! {
//!     #[test]
//!     fn prop_action_roundtrip(action: Action) {
//!         let json = serde_json::to_string(&action).unwrap();
//!         let deserialized: Action = serde_json::from_str(&json).unwrap();
//!         prop_assert_eq!(action, deserialized);
//!     }
//! }
//! ```

use hyperchad_transformer_models::Visibility;
use moosicbox_arb::xml::XmlString;
use proptest::prelude::*;

use crate::{
    Action, ActionEffect, ActionTrigger, ActionType, ElementTarget, LogLevel, StyleAction,
};

impl Arbitrary for ActionTrigger {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `ActionTrigger` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(Self::Click),
            Just(Self::ClickOutside),
            Just(Self::Hover),
            Just(Self::Change),
            Just(Self::Immediate),
            Just(Self::HttpBeforeRequest),
            Just(Self::HttpAfterRequest),
            Just(Self::HttpRequestSuccess),
            Just(Self::HttpRequestError),
            Just(Self::HttpRequestAbort),
            Just(Self::HttpRequestTimeout),
            any::<XmlString>().prop_map(|s| Self::Event(s.0)),
        ]
        .boxed()
    }
}

impl Arbitrary for StyleAction {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `StyleAction` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            any::<Visibility>().prop_map(Self::SetVisibility),
            any::<bool>().prop_map(Self::SetDisplay),
        ]
        .boxed()
    }
}

impl Arbitrary for LogLevel {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `LogLevel` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(Self::Error),
            Just(Self::Warn),
            Just(Self::Info),
            Just(Self::Debug),
            Just(Self::Trace),
        ]
        .boxed()
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::CalcValue {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `CalcValue` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        any::<ElementTarget>()
            .prop_map(|target| Self::Visibility { target })
            .boxed()
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::Value {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `Value` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            any::<crate::logic::CalcValue>().prop_map(Self::Calc),
            any::<Visibility>().prop_map(Self::Visibility),
        ]
        .boxed()
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::Condition {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `Condition` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (any::<crate::logic::Value>(), any::<crate::logic::Value>())
            .prop_map(|(a, b)| Self::Eq(a, b))
            .boxed()
    }
}

/// Strategy for generating a simple (non-recursive) `ActionType`
#[cfg(feature = "logic")]
fn simple_action_type_strategy() -> BoxedStrategy<ActionType> {
    prop_oneof![
        (any::<ElementTarget>(), any::<StyleAction>())
            .prop_map(|(target, action)| ActionType::Style { target, action }),
        any::<XmlString>().prop_map(|s| ActionType::Navigate { url: s.0 }),
        (any::<XmlString>(), any::<LogLevel>()).prop_map(|(s, level)| ActionType::Log {
            message: s.0,
            level
        }),
        any::<XmlString>().prop_map(|s| ActionType::Custom { action: s.0 }),
    ]
    .boxed()
}

/// Strategy for generating a simple (non-recursive) `ActionEffect`
#[cfg(feature = "logic")]
fn simple_action_effect_strategy() -> BoxedStrategy<ActionEffect> {
    (
        simple_action_type_strategy(),
        any::<Option<u64>>(),
        any::<Option<u64>>(),
        any::<Option<bool>>(),
    )
        .prop_map(|(action, delay_off, throttle, unique)| ActionEffect {
            action,
            delay_off,
            throttle,
            unique,
        })
        .boxed()
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::If {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `If` conditional for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (
            any::<crate::logic::Condition>(),
            prop::collection::vec(simple_action_effect_strategy(), 0..3),
            prop::collection::vec(simple_action_effect_strategy(), 0..3),
        )
            .prop_map(|(condition, actions, else_actions)| Self {
                condition,
                actions,
                else_actions,
            })
            .boxed()
    }
}

impl Arbitrary for ActionType {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `ActionType` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        #[cfg(feature = "logic")]
        {
            prop_oneof![
                (any::<ElementTarget>(), any::<StyleAction>())
                    .prop_map(|(target, action)| Self::Style { target, action }),
                any::<XmlString>().prop_map(|s| Self::Navigate { url: s.0 }),
                (any::<XmlString>(), any::<LogLevel>()).prop_map(|(s, level)| Self::Log {
                    message: s.0,
                    level
                }),
                any::<XmlString>().prop_map(|s| Self::Custom { action: s.0 }),
                any::<crate::logic::If>().prop_map(Self::Logic),
            ]
            .boxed()
        }

        #[cfg(not(feature = "logic"))]
        {
            prop_oneof![
                (any::<ElementTarget>(), any::<StyleAction>())
                    .prop_map(|(target, action)| Self::Style { target, action }),
                any::<XmlString>().prop_map(|s| Self::Navigate { url: s.0 }),
                (any::<XmlString>(), any::<LogLevel>()).prop_map(|(s, level)| Self::Log {
                    message: s.0,
                    level
                }),
                any::<XmlString>().prop_map(|s| Self::Custom { action: s.0 }),
            ]
            .boxed()
        }
    }
}

impl Arbitrary for Action {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `Action` for property-based testing.
    /// Uses `prop_map` instead of `prop_flat_map` to preserve shrinking behavior.
    ///
    /// When the trigger is `ActionTrigger::Event(name)`, the action must be wrapped
    /// in `ActionType::Event { name, action }` with matching name.
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (
            any::<ActionTrigger>(),
            any::<ActionType>(),
            any::<Option<u64>>(),
            any::<Option<u64>>(),
            any::<Option<bool>>(),
        )
            .prop_map(|(trigger, action_type, delay_off, throttle, unique)| {
                let action = match &trigger {
                    ActionTrigger::Event(name) => ActionType::Event {
                        name: name.clone(),
                        action: Box::new(action_type),
                    },
                    _ => action_type,
                };
                Self {
                    trigger,
                    effect: ActionEffect {
                        action,
                        delay_off,
                        throttle,
                        unique,
                    },
                }
            })
            .boxed()
    }
}

impl Arbitrary for ActionEffect {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Generates an arbitrary `ActionEffect` for property-based testing
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (
            any::<ActionType>(),
            any::<Option<u64>>(),
            any::<Option<u64>>(),
            any::<Option<bool>>(),
        )
            .prop_map(|(action, delay_off, throttle, unique)| Self {
                action,
                delay_off,
                throttle,
                unique,
            })
            .boxed()
    }
}
