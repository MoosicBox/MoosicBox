//! Arbitrary value generation for property-based testing.
//!
//! This module implements the [`Arbitrary`](quickcheck::Arbitrary) trait for transformer types,
//! enabling property-based testing with quickcheck. Only available in test builds.

use std::collections::BTreeMap;

use moosicbox_arb::{
    css::CssIdentifierString,
    serde::{JsonF32, JsonValue},
    xml::{XmlAttrNameString, XmlString},
};
use quickcheck::{Arbitrary, Gen};
use strum::IntoEnumIterator;

use crate::{
    Calculation, CalculationType, ConfigOverride, Container, Element, Flex, HeaderSize, Input,
    Number, NumberType, OverrideCondition, OverrideItem, OverrideItemType, TextDecoration,
};

/// Generates a calculation of one of the specified types.
fn one_of_calc(g: &mut Gen, types: &[CalculationType]) -> Calculation {
    match *g.choose(types).unwrap() {
        CalculationType::Number => Calculation::Number(Box::new(Number::arbitrary(g))),
        CalculationType::Add => Calculation::Add(
            Box::new(one_of_calc(g, &[CalculationType::Number])),
            Box::new(one_of_calc(g, &[CalculationType::Number])),
        ),
        CalculationType::Subtract => Calculation::Subtract(
            Box::new(one_of_calc(g, &[CalculationType::Number])),
            Box::new(one_of_calc(g, &[CalculationType::Number])),
        ),
        CalculationType::Multiply => Calculation::Multiply(
            Box::new(one_of_calc(g, &[CalculationType::Number])),
            Box::new(one_of_calc(g, &[CalculationType::Number])),
        ),
        CalculationType::Divide => Calculation::Divide(
            Box::new(one_of_calc(g, &[CalculationType::Number])),
            Box::new(one_of_calc(g, &[CalculationType::Number])),
        ),
        CalculationType::Grouping => Calculation::Grouping(Box::new(Arbitrary::arbitrary(g))),
        CalculationType::Min => Calculation::Min(
            Box::new(Arbitrary::arbitrary(g)),
            Box::new(Arbitrary::arbitrary(g)),
        ),
        CalculationType::Max => Calculation::Max(
            Box::new(Arbitrary::arbitrary(g)),
            Box::new(Arbitrary::arbitrary(g)),
        ),
    }
}

impl Arbitrary for Calculation {
    fn arbitrary(g: &mut Gen) -> Self {
        if g.size() <= 3 {
            return Self::Number(Box::new(Number::arbitrary(g)));
        }

        one_of_calc(
            &mut half_g_max(g, 10),
            &CalculationType::iter().collect::<Vec<_>>(),
        )
    }
}

/// Generates a number of one of the specified types.
fn one_of_number(g: &mut Gen, types: &[NumberType]) -> Number {
    match *g.choose(types).unwrap() {
        NumberType::Real => Number::Real(JsonF32::arbitrary(g).0),
        NumberType::Integer => Number::Integer(Arbitrary::arbitrary(g)),
        NumberType::RealPercent => Number::RealPercent(JsonF32::arbitrary(g).0),
        NumberType::IntegerPercent => Number::IntegerPercent(Arbitrary::arbitrary(g)),
        NumberType::RealVw => Number::RealVw(JsonF32::arbitrary(g).0),
        NumberType::IntegerVw => Number::IntegerVw(Arbitrary::arbitrary(g)),
        NumberType::RealVh => Number::RealVh(JsonF32::arbitrary(g).0),
        NumberType::IntegerVh => Number::IntegerVh(Arbitrary::arbitrary(g)),
        NumberType::RealDvw => Number::RealDvw(JsonF32::arbitrary(g).0),
        NumberType::IntegerDvw => Number::IntegerDvw(Arbitrary::arbitrary(g)),
        NumberType::RealDvh => Number::RealDvh(JsonF32::arbitrary(g).0),
        NumberType::IntegerDvh => Number::IntegerDvh(Arbitrary::arbitrary(g)),
        NumberType::Calc => Number::Calc(Arbitrary::arbitrary(g)),
    }
}

impl Arbitrary for Number {
    fn arbitrary(g: &mut Gen) -> Self {
        one_of_number(g, &NumberType::iter().collect::<Vec<_>>())
    }
}

impl Arbitrary for HeaderSize {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::H1, Self::H2, Self::H3, Self::H4, Self::H5, Self::H6])
            .unwrap()
    }
}

impl Arbitrary for TextDecoration {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            color: Arbitrary::arbitrary(g),
            line: Arbitrary::arbitrary(g),
            style: Arbitrary::arbitrary(g),
            thickness: Arbitrary::arbitrary(g),
        }
    }
}

impl Arbitrary for Flex {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            grow: Arbitrary::arbitrary(g),
            shrink: Arbitrary::arbitrary(g),
            basis: Arbitrary::arbitrary(g),
        }
    }
}

impl Arbitrary for Input {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=2).collect::<Vec<_>>()).unwrap() {
            0 => Self::Text {
                value: Option::arbitrary(g).map(|x: XmlString| x.0),
                placeholder: Option::arbitrary(g).map(|x: XmlString| x.0),
            },
            1 => Self::Password {
                value: Option::arbitrary(g).map(|x: XmlString| x.0),
                placeholder: Option::arbitrary(g).map(|x: XmlString| x.0),
            },
            2 => Self::Checkbox {
                checked: Option::arbitrary(g),
            },
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for Element {
    fn arbitrary(g: &mut Gen) -> Self {
        #[cfg(feature = "canvas")]
        let max = 24;
        #[cfg(not(feature = "canvas"))]
        let max = 23;
        match *g.choose(&(0..=max).collect::<Vec<_>>()).unwrap() {
            0 => Self::Div,
            1 => Self::Raw {
                value: XmlString::arbitrary(g).0,
            },
            2 => Self::Aside,
            3 => Self::Main,
            4 => Self::Header,
            5 => Self::Footer,
            6 => Self::Section,
            7 => Self::Form,
            8 => Self::Span,
            9 => Self::Input {
                name: Option::arbitrary(g).map(|x: XmlString| x.0),
                autofocus: Option::arbitrary(g),
                input: Input::arbitrary(g),
            },
            10 => Self::Button {
                r#type: Option::arbitrary(g).map(|x: XmlString| x.0),
            },
            11 => Self::Image {
                source: Option::arbitrary(g).map(|x: XmlString| x.0),
                alt: Option::arbitrary(g).map(|x: XmlString| x.0),
                fit: Option::arbitrary(g),
                loading: Option::arbitrary(g),
                source_set: Option::arbitrary(g).map(|x: XmlString| x.0),
                sizes: Option::arbitrary(g),
            },
            12 => Self::Anchor {
                target: Option::arbitrary(g),
                href: Option::arbitrary(g).map(|x: XmlString| x.0),
            },
            13 => Self::Heading {
                size: HeaderSize::arbitrary(g),
            },
            14 => Self::UnorderedList,
            15 => Self::OrderedList,
            16 => Self::ListItem,
            17 => Self::Table,
            18 => Self::THead,
            19 => Self::TH {
                rows: Option::arbitrary(g),
                columns: Option::arbitrary(g),
            },
            20 => Self::TBody,
            21 => Self::TR,
            22 => Self::TD {
                rows: Option::arbitrary(g),
                columns: Option::arbitrary(g),
            },
            23 => Self::Textarea {
                value: XmlString::arbitrary(g).0,
                placeholder: Option::arbitrary(g).map(|x: XmlString| x.0),
                name: Option::arbitrary(g).map(|x: XmlString| x.0),
                rows: Option::arbitrary(g),
                cols: Option::arbitrary(g),
            },
            #[cfg(feature = "canvas")]
            24 => Self::Canvas,
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for ConfigOverride {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut overrides = loop {
            let overrides: Vec<OverrideItem> = Arbitrary::arbitrary(g);
            if overrides.is_empty() {
                continue;
            }
            break overrides;
        };
        let first: OverrideItemType = overrides.first().unwrap().into();

        overrides.retain_mut(|x| {
            let item_type: OverrideItemType = x.clone().into();
            item_type == first
        });
        let len = overrides.len();
        let overrides = overrides.into_iter().take(std::cmp::min(len, 1)).collect();

        let default: Option<OverrideItem> =
            Option::arbitrary(g).map(|_: bool| override_item_of_type(g, first));

        Self {
            condition: Arbitrary::arbitrary(g),
            overrides,
            default,
        }
    }
}

impl Arbitrary for OverrideCondition {
    fn arbitrary(g: &mut Gen) -> Self {
        Self::ResponsiveTarget {
            name: CssIdentifierString::arbitrary(g).0,
        }
    }
}

/// Generates an override item of the specified type.
fn override_item_of_type(g: &mut Gen, value: OverrideItemType) -> OverrideItem {
    match value {
        OverrideItemType::StrId => OverrideItem::StrId(XmlString::arbitrary(g).0),
        OverrideItemType::Classes => OverrideItem::Classes(
            Vec::arbitrary(g)
                .into_iter()
                .map(|x: XmlString| x.0)
                .filter(|x| !x.is_empty())
                .filter(|x| !x.chars().any(char::is_whitespace))
                .collect(),
        ),
        OverrideItemType::Direction => OverrideItem::Direction(Arbitrary::arbitrary(g)),
        OverrideItemType::OverflowX => OverrideItem::OverflowX(Arbitrary::arbitrary(g)),
        OverrideItemType::OverflowY => OverrideItem::OverflowY(Arbitrary::arbitrary(g)),
        OverrideItemType::GridCellSize => OverrideItem::GridCellSize(Arbitrary::arbitrary(g)),
        OverrideItemType::JustifyContent => OverrideItem::JustifyContent(Arbitrary::arbitrary(g)),
        OverrideItemType::AlignItems => OverrideItem::AlignItems(Arbitrary::arbitrary(g)),
        OverrideItemType::TextAlign => OverrideItem::TextAlign(Arbitrary::arbitrary(g)),
        OverrideItemType::WhiteSpace => OverrideItem::WhiteSpace(Arbitrary::arbitrary(g)),
        OverrideItemType::TextDecoration => OverrideItem::TextDecoration(Arbitrary::arbitrary(g)),
        OverrideItemType::FontFamily => OverrideItem::FontFamily(
            Vec::<XmlString>::arbitrary(g)
                .into_iter()
                .map(|x| x.0.trim().to_string())
                .filter(|x| !x.is_empty())
                .filter(|x| !x.chars().any(|x| matches!(x, ',')))
                .collect(),
        ),
        OverrideItemType::FontWeight => OverrideItem::FontWeight(Arbitrary::arbitrary(g)),
        OverrideItemType::Width => OverrideItem::Width(Arbitrary::arbitrary(g)),
        OverrideItemType::MinWidth => OverrideItem::MinWidth(Arbitrary::arbitrary(g)),
        OverrideItemType::MaxWidth => OverrideItem::MaxWidth(Arbitrary::arbitrary(g)),
        OverrideItemType::Height => OverrideItem::Height(Arbitrary::arbitrary(g)),
        OverrideItemType::MinHeight => OverrideItem::MinHeight(Arbitrary::arbitrary(g)),
        OverrideItemType::MaxHeight => OverrideItem::MaxHeight(Arbitrary::arbitrary(g)),
        OverrideItemType::Flex => OverrideItem::Flex(Arbitrary::arbitrary(g)),
        OverrideItemType::ColumnGap => OverrideItem::ColumnGap(Arbitrary::arbitrary(g)),
        OverrideItemType::RowGap => OverrideItem::RowGap(Arbitrary::arbitrary(g)),
        OverrideItemType::Opacity => OverrideItem::Opacity(Arbitrary::arbitrary(g)),
        OverrideItemType::Left => OverrideItem::Left(Arbitrary::arbitrary(g)),
        OverrideItemType::Right => OverrideItem::Right(Arbitrary::arbitrary(g)),
        OverrideItemType::Top => OverrideItem::Top(Arbitrary::arbitrary(g)),
        OverrideItemType::Bottom => OverrideItem::Bottom(Arbitrary::arbitrary(g)),
        OverrideItemType::TranslateX => OverrideItem::TranslateX(Arbitrary::arbitrary(g)),
        OverrideItemType::TranslateY => OverrideItem::TranslateY(Arbitrary::arbitrary(g)),
        OverrideItemType::Cursor => OverrideItem::Cursor(Arbitrary::arbitrary(g)),
        OverrideItemType::UserSelect => OverrideItem::UserSelect(Arbitrary::arbitrary(g)),
        OverrideItemType::OverflowWrap => OverrideItem::OverflowWrap(Arbitrary::arbitrary(g)),
        OverrideItemType::TextOverflow => OverrideItem::TextOverflow(Arbitrary::arbitrary(g)),
        OverrideItemType::Position => OverrideItem::Position(Arbitrary::arbitrary(g)),
        OverrideItemType::Background => OverrideItem::Background(Arbitrary::arbitrary(g)),
        OverrideItemType::BorderTop => OverrideItem::BorderTop(Arbitrary::arbitrary(g)),
        OverrideItemType::BorderRight => OverrideItem::BorderRight(Arbitrary::arbitrary(g)),
        OverrideItemType::BorderBottom => OverrideItem::BorderBottom(Arbitrary::arbitrary(g)),
        OverrideItemType::BorderLeft => OverrideItem::BorderLeft(Arbitrary::arbitrary(g)),
        OverrideItemType::BorderTopLeftRadius => {
            OverrideItem::BorderTopLeftRadius(Arbitrary::arbitrary(g))
        }
        OverrideItemType::BorderTopRightRadius => {
            OverrideItem::BorderTopRightRadius(Arbitrary::arbitrary(g))
        }
        OverrideItemType::BorderBottomLeftRadius => {
            OverrideItem::BorderBottomLeftRadius(Arbitrary::arbitrary(g))
        }
        OverrideItemType::BorderBottomRightRadius => {
            OverrideItem::BorderBottomRightRadius(Arbitrary::arbitrary(g))
        }
        OverrideItemType::MarginLeft => OverrideItem::MarginLeft(Arbitrary::arbitrary(g)),
        OverrideItemType::MarginRight => OverrideItem::MarginRight(Arbitrary::arbitrary(g)),
        OverrideItemType::MarginTop => OverrideItem::MarginTop(Arbitrary::arbitrary(g)),
        OverrideItemType::MarginBottom => OverrideItem::MarginBottom(Arbitrary::arbitrary(g)),
        OverrideItemType::PaddingLeft => OverrideItem::PaddingLeft(Arbitrary::arbitrary(g)),
        OverrideItemType::PaddingRight => OverrideItem::PaddingRight(Arbitrary::arbitrary(g)),
        OverrideItemType::PaddingTop => OverrideItem::PaddingTop(Arbitrary::arbitrary(g)),
        OverrideItemType::PaddingBottom => OverrideItem::PaddingBottom(Arbitrary::arbitrary(g)),
        OverrideItemType::FontSize => OverrideItem::FontSize(Arbitrary::arbitrary(g)),
        OverrideItemType::Color => OverrideItem::Color(Arbitrary::arbitrary(g)),
        OverrideItemType::Hidden => OverrideItem::Hidden(Arbitrary::arbitrary(g)),
        OverrideItemType::Visibility => OverrideItem::Visibility(Arbitrary::arbitrary(g)),
    }
}

impl Arbitrary for OverrideItem {
    fn arbitrary(g: &mut Gen) -> Self {
        let types = OverrideItemType::iter().collect::<Vec<_>>();
        let value = *g.choose(&types).unwrap();
        override_item_of_type(g, value)
    }
}

/// Creates a new generator with half the size, capped at `max`.
fn half_g_max(g: &Gen, max: usize) -> Gen {
    Gen::new(std::cmp::min(max, g.size() / 2))
}

/// Generates a `BTreeMap` with XML-safe string keys and values.
fn xml_btreemap(g: &mut Gen) -> BTreeMap<String, String> {
    let map: BTreeMap<XmlAttrNameString, XmlString> = Arbitrary::arbitrary(g);

    map.into_iter().map(|(k, v)| (k.0, v.0)).collect()
}

/// Returns the default value from overrides if present, otherwise generates an arbitrary value.
fn default_value_or_arbitrary<T: Arbitrary + Default>(
    g: &mut Gen,
    overrides: &[ConfigOverride],
    to_value: impl Fn(OverrideItem) -> Option<T>,
) -> T {
    default_value(overrides, to_value).map_or_else(|| T::arbitrary(g), Option::unwrap_or_default)
}

/// Extracts the default value for a specific override type from the list of overrides.
///
/// Returns `None` if no matching override is found, `Some(None)` if found but no default,
/// or `Some(Some(value))` if a default value exists.
#[allow(clippy::option_option)]
fn default_value<T: Arbitrary>(
    overrides: &[ConfigOverride],
    to_value: impl Fn(OverrideItem) -> Option<T>,
) -> Option<Option<T>> {
    overrides.iter().find_map(|x| {
        if x.overrides.iter().any(|x| to_value(x.clone()).is_some()) {
            Some(x.default.clone().and_then(&to_value))
        } else {
            None
        }
    })
}

/// Returns the default value from overrides if present, otherwise generates an optional arbitrary value.
fn opt_default_value_or_arbitrary<T: Arbitrary + Clone>(
    g: &mut Gen,
    overrides: &[ConfigOverride],
    to_value: impl Fn(OverrideItem) -> Option<T>,
) -> Option<T> {
    default_value(overrides, to_value).unwrap_or_else(|| Option::arbitrary(g))
}

impl Arbitrary for Container {
    #[allow(clippy::too_many_lines)]
    fn arbitrary(g: &mut Gen) -> Self {
        let smaller_g = &mut half_g_max(g, 10);
        let element = loop {
            let element = Element::arbitrary(g);
            if !matches!(element, Element::Raw { .. }) {
                break element;
            }
        };

        let children = if element.allows_children() {
            Vec::arbitrary(smaller_g)
        } else {
            vec![]
        };

        let mut i = 0;
        let mut overrides: Vec<ConfigOverride> = if cfg!(feature = "logic") {
            Arbitrary::arbitrary(smaller_g)
        } else {
            vec![]
        };
        let overrides2 = overrides.clone();
        overrides.retain(|x| {
            i += 1;
            overrides2.iter().take(i - 1).all(|prev| {
                let Some(current_type) = x
                    .overrides
                    .iter()
                    .map(|x| {
                        let item: OverrideItemType = x.clone().into();
                        item
                    })
                    .next()
                    .or_else(|| {
                        x.default.as_ref().map(|x| {
                            let item: OverrideItemType = x.clone().into();
                            item
                        })
                    })
                else {
                    return false;
                };
                prev.condition != x.condition
                    && prev.overrides.iter().all(|x| {
                        let item_type: OverrideItemType = x.clone().into();
                        item_type != current_type
                    })
            })
        });

        overrides.sort_by(|a, b| format!("{:?}", a.condition).cmp(&format!("{:?}", b.condition)));

        Self {
            id: usize::arbitrary(g),
            str_id: default_value(&overrides, |x| {
                if let OverrideItem::StrId(x) = x {
                    Some(x)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Option::arbitrary(g).map(|x: XmlString| x.0)),
            font_family: default_value(&overrides, |x| {
                if let OverrideItem::FontFamily(x) = x {
                    Some(x)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                Option::arbitrary(g).map(|x: Vec<XmlString>| {
                    x.into_iter()
                        .map(|x| x.0.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .filter(|x| !x.chars().any(|x| matches!(x, ',')))
                        .collect()
                })
            }),
            font_weight: default_value(&overrides, |x| {
                if let OverrideItem::FontWeight(x) = x {
                    Some(x)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Option::arbitrary(g)),
            classes: default_value(&overrides, |x| {
                if let OverrideItem::Classes(x) = x {
                    Some(x)
                } else {
                    None
                }
            })
            .map_or_else(
                || {
                    Vec::arbitrary(g)
                        .into_iter()
                        .map(|x: XmlString| x.0)
                        .filter(|x| !x.is_empty())
                        .filter(|x| !x.chars().any(char::is_whitespace))
                        .collect()
                },
                Option::unwrap_or_default,
            ),
            data: xml_btreemap(g),
            element,
            children,
            direction: default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Direction(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            overflow_x: default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::OverflowX(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            grid_cell_size: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::GridCellSize(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            overflow_y: default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::OverflowY(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            justify_content: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::JustifyContent(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            align_items: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::AlignItems(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            text_align: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::TextAlign(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            white_space: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::WhiteSpace(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            text_decoration: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::TextDecoration(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            width: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Width(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            min_width: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MinWidth(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            max_width: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MaxWidth(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            height: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Height(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            min_height: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MinHeight(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            max_height: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MaxHeight(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            flex: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Flex(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            column_gap: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::ColumnGap(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            row_gap: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::RowGap(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            opacity: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Opacity(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            left: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Left(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            right: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Right(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            top: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Top(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            bottom: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Bottom(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            translate_x: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::TranslateX(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            translate_y: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::TranslateY(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            cursor: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Cursor(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            user_select: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::UserSelect(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            overflow_wrap: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::OverflowWrap(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            text_overflow: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::TextOverflow(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            position: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Position(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            background: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Background(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_top: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderTop(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_right: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderRight(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_bottom: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderBottom(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_left: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderLeft(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_top_left_radius: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderTopLeftRadius(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_top_right_radius: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderTopRightRadius(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_bottom_left_radius: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderBottomLeftRadius(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            border_bottom_right_radius: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::BorderBottomRightRadius(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            margin_left: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MarginLeft(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            margin_right: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MarginRight(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            margin_top: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MarginTop(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            margin_bottom: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::MarginBottom(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            padding_left: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::PaddingLeft(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            padding_right: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::PaddingRight(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            padding_top: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::PaddingTop(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            padding_bottom: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::PaddingBottom(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            font_size: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::FontSize(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            color: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Color(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            state: Option::arbitrary(g).map(|x: JsonValue| x.0),
            hidden: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Hidden(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            debug: Option::arbitrary(g),
            visibility: opt_default_value_or_arbitrary(g, &overrides, |x| {
                if let OverrideItem::Visibility(x) = x {
                    Some(x)
                } else {
                    None
                }
            }),
            route: Option::arbitrary(g),
            actions: Vec::arbitrary(smaller_g),
            overrides,
            #[cfg(feature = "layout")]
            calculated_margin_left: None,
            #[cfg(feature = "layout")]
            calculated_margin_right: None,
            #[cfg(feature = "layout")]
            calculated_margin_top: None,
            #[cfg(feature = "layout")]
            calculated_margin_bottom: None,
            #[cfg(feature = "layout")]
            calculated_padding_left: None,
            #[cfg(feature = "layout")]
            calculated_padding_right: None,
            #[cfg(feature = "layout")]
            calculated_padding_top: None,
            #[cfg(feature = "layout")]
            calculated_padding_bottom: None,
            #[cfg(feature = "layout")]
            calculated_min_width: None,
            #[cfg(feature = "layout")]
            calculated_child_min_width: None,
            #[cfg(feature = "layout")]
            calculated_max_width: None,
            #[cfg(feature = "layout")]
            calculated_preferred_width: None,
            #[cfg(feature = "layout")]
            calculated_width: None,
            #[cfg(feature = "layout")]
            calculated_min_height: None,
            #[cfg(feature = "layout")]
            calculated_child_min_height: None,
            #[cfg(feature = "layout")]
            calculated_max_height: None,
            #[cfg(feature = "layout")]
            calculated_preferred_height: None,
            #[cfg(feature = "layout")]
            calculated_height: None,
            #[cfg(feature = "layout")]
            calculated_x: None,
            #[cfg(feature = "layout")]
            calculated_y: None,
            #[cfg(feature = "layout")]
            calculated_position: None,
            #[cfg(feature = "layout")]
            calculated_border_top: None,
            #[cfg(feature = "layout")]
            calculated_border_right: None,
            #[cfg(feature = "layout")]
            calculated_border_bottom: None,
            #[cfg(feature = "layout")]
            calculated_border_left: None,
            #[cfg(feature = "layout")]
            calculated_border_top_left_radius: None,
            #[cfg(feature = "layout")]
            calculated_border_top_right_radius: None,
            #[cfg(feature = "layout")]
            calculated_border_bottom_left_radius: None,
            #[cfg(feature = "layout")]
            calculated_border_bottom_right_radius: None,
            #[cfg(feature = "layout")]
            calculated_column_gap: None,
            #[cfg(feature = "layout")]
            calculated_row_gap: None,
            #[cfg(feature = "layout")]
            calculated_opacity: None,
            #[cfg(feature = "layout")]
            calculated_font_size: None,
            #[cfg(feature = "layout")]
            scrollbar_right: None,
            #[cfg(feature = "layout")]
            scrollbar_bottom: None,
            #[cfg(feature = "layout-offset")]
            calculated_offset_x: None,
            #[cfg(feature = "layout-offset")]
            calculated_offset_y: None,
        }
    }
}
