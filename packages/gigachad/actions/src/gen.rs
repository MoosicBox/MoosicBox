use gigachad_transformer_models::Visibility;
use moosicbox_gen::xml::XmlString;
use quickcheck::{Arbitrary, Gen};

use crate::{
    Action, ActionEffect, ActionTrigger, ActionType, ElementTarget, LogLevel, StyleAction,
};

fn half_g_max(g: &Gen, max: usize) -> Gen {
    Gen::new(std::cmp::min(max, g.size() / 2))
}

impl Arbitrary for ActionTrigger {
    fn arbitrary(g: &mut Gen) -> Self {
        let half_g = &mut half_g_max(g, 10);
        g.choose(&[
            Self::Click,
            Self::ClickOutside,
            Self::Hover,
            Self::Change,
            Self::Immediate,
            Self::Event(XmlString::arbitrary(half_g).0),
        ])
        .unwrap()
        .clone()
    }
}

impl Arbitrary for ElementTarget {
    fn arbitrary(g: &mut Gen) -> Self {
        #[cfg(feature = "id")]
        let max = 3;
        #[cfg(not(feature = "id"))]
        let max = 2;
        match *g.choose(&(0..=max).collect::<Vec<_>>()).unwrap() {
            0 => Self::StrId(XmlString::arbitrary(g).0),
            1 => Self::SelfTarget,
            2 => Self::LastChild,
            #[cfg(feature = "id")]
            3 => Self::Id(usize::arbitrary(g)),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for StyleAction {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=1).collect::<Vec<_>>()).unwrap() {
            0 => Self::SetVisibility(Visibility::arbitrary(g)),
            1 => Self::SetDisplay(bool::arbitrary(g)),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for LogLevel {
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
    fn arbitrary(g: &mut Gen) -> Self {
        Self::Visibility {
            target: ElementTarget::arbitrary(g),
        }
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::Value {
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
    fn arbitrary(g: &mut Gen) -> Self {
        Self::Eq(Arbitrary::arbitrary(g), Arbitrary::arbitrary(g))
    }
}

#[cfg(feature = "logic")]
impl Arbitrary for crate::logic::If {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            condition: Arbitrary::arbitrary(g),
            actions: Arbitrary::arbitrary(g),
            else_actions: Arbitrary::arbitrary(g),
        }
    }
}

impl Arbitrary for ActionType {
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
    fn arbitrary(g: &mut Gen) -> Self {
        let trigger = ActionTrigger::arbitrary(g);

        if let ActionTrigger::Event(name) = &trigger {
            Self {
                trigger: trigger.clone(),
                action: ActionEffect {
                    action: ActionType::Event {
                        name: name.to_string(),
                        action: Box::new(ActionType::arbitrary(g)),
                    },
                    delay_off: Option::arbitrary(g),
                    throttle: Option::arbitrary(g),
                },
            }
        } else {
            Self {
                trigger,
                action: ActionEffect {
                    action: ActionType::arbitrary(g),
                    delay_off: Option::arbitrary(g),
                    throttle: Option::arbitrary(g),
                },
            }
        }
    }
}
