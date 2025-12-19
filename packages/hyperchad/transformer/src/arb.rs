//! Arbitrary value generation for property-based testing.
//!
//! This module implements the [`Arbitrary`](proptest::arbitrary::Arbitrary) trait for transformer types,
//! enabling property-based testing with proptest. Only available in test builds.

use std::collections::BTreeMap;

use moosicbox_arb::{
    css::CssIdentifierString,
    serde::{JsonF32, JsonValue},
    xml::{XmlAttrNameString, XmlString},
};
use proptest::prelude::*;

use crate::{
    Calculation, ConfigOverride, Container, Element, Flex, HeaderSize, Input, Number,
    OverrideCondition, OverrideItem, OverrideItemType, TextDecoration,
};

/// Strategy for generating non-Calc Number variants only.
///
/// This is used to break the mutual recursion between Number and Calculation.
/// These are the "leaf" number types that don't contain calculations.
fn non_calc_number_strategy() -> BoxedStrategy<Number> {
    prop_oneof![
        any::<JsonF32>().prop_map(|f| Number::Real(f.0)),
        any::<i64>().prop_map(Number::Integer),
        any::<JsonF32>().prop_map(|f| Number::RealPercent(f.0)),
        any::<i64>().prop_map(Number::IntegerPercent),
        any::<JsonF32>().prop_map(|f| Number::RealVw(f.0)),
        any::<i64>().prop_map(Number::IntegerVw),
        any::<JsonF32>().prop_map(|f| Number::RealVh(f.0)),
        any::<i64>().prop_map(Number::IntegerVh),
        any::<JsonF32>().prop_map(|f| Number::RealDvw(f.0)),
        any::<i64>().prop_map(Number::IntegerDvw),
        any::<JsonF32>().prop_map(|f| Number::RealDvh(f.0)),
        any::<i64>().prop_map(Number::IntegerDvh),
    ]
    .boxed()
}

/// Strategy for generating Calculation values with proper recursion.
///
/// This matches the original quickcheck behavior:
/// - Binary operations (Add, Subtract, Multiply, Divide) have Number-only operands
/// - Grouping, Min, Max can have full recursive Calculation operands
/// - Weighted toward simpler structures (leaves are more likely)
///
/// Uses `prop_recursive` to handle depth control and ensure good shrinking.
fn calculation_strategy() -> BoxedStrategy<Calculation> {
    // Leaf strategy: just a Number wrapped in Calculation::Number
    let leaf = non_calc_number_strategy().prop_map(|n| Calculation::Number(Box::new(n)));

    leaf.prop_recursive(
        4,  // depth - supports deep nesting but weighted toward shallow
        32, // desired_size - target complexity
        8,  // expected_branch_size
        |inner| {
            prop_oneof![
                // Weight leaves heavily (4x) for "weighted toward smaller"
                4 => non_calc_number_strategy().prop_map(|n| Calculation::Number(Box::new(n))),

                // Binary operations: operands are Number-only (matching original)
                // This prevents Add(Add(...), Add(...)) directly
                1 => (non_calc_number_strategy(), non_calc_number_strategy())
                    .prop_map(|(a, b)| Calculation::Add(
                        Box::new(Calculation::Number(Box::new(a))),
                        Box::new(Calculation::Number(Box::new(b)))
                    )),
                1 => (non_calc_number_strategy(), non_calc_number_strategy())
                    .prop_map(|(a, b)| Calculation::Subtract(
                        Box::new(Calculation::Number(Box::new(a))),
                        Box::new(Calculation::Number(Box::new(b)))
                    )),
                1 => (non_calc_number_strategy(), non_calc_number_strategy())
                    .prop_map(|(a, b)| Calculation::Multiply(
                        Box::new(Calculation::Number(Box::new(a))),
                        Box::new(Calculation::Number(Box::new(b)))
                    )),
                1 => (non_calc_number_strategy(), non_calc_number_strategy())
                    .prop_map(|(a, b)| Calculation::Divide(
                        Box::new(Calculation::Number(Box::new(a))),
                        Box::new(Calculation::Number(Box::new(b)))
                    )),

                // Grouping, Min, Max: allow full recursive operands (matching original)
                1 => inner.clone().prop_map(|c| Calculation::Grouping(Box::new(c))),
                1 => (inner.clone(), inner.clone())
                    .prop_map(|(a, b)| Calculation::Min(Box::new(a), Box::new(b))),
                1 => (inner.clone(), inner)
                    .prop_map(|(a, b)| Calculation::Max(Box::new(a), Box::new(b))),
            ]
        },
    )
    .boxed()
}

impl Arbitrary for Calculation {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        calculation_strategy()
    }
}

/// Strategy for generating Number values including the Calc variant.
///
/// Uses `prop_oneof!` with weights to favor simpler variants while still
/// allowing Calc variants with full calculation trees.
fn number_strategy() -> BoxedStrategy<Number> {
    prop_oneof![
        // Non-Calc variants (higher weight - 12 total)
        1 => any::<JsonF32>().prop_map(|f| Number::Real(f.0)),
        1 => any::<i64>().prop_map(Number::Integer),
        1 => any::<JsonF32>().prop_map(|f| Number::RealPercent(f.0)),
        1 => any::<i64>().prop_map(Number::IntegerPercent),
        1 => any::<JsonF32>().prop_map(|f| Number::RealVw(f.0)),
        1 => any::<i64>().prop_map(Number::IntegerVw),
        1 => any::<JsonF32>().prop_map(|f| Number::RealVh(f.0)),
        1 => any::<i64>().prop_map(Number::IntegerVh),
        1 => any::<JsonF32>().prop_map(|f| Number::RealDvw(f.0)),
        1 => any::<i64>().prop_map(Number::IntegerDvw),
        1 => any::<JsonF32>().prop_map(|f| Number::RealDvh(f.0)),
        1 => any::<i64>().prop_map(Number::IntegerDvh),
        // Calc variant (weight 1 - less frequent but still tested)
        1 => calculation_strategy().prop_map(Number::Calc),
    ]
    .boxed()
}

impl Arbitrary for Number {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        number_strategy()
    }
}

impl Arbitrary for HeaderSize {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(Self::H1),
            Just(Self::H2),
            Just(Self::H3),
            Just(Self::H4),
            Just(Self::H5),
            Just(Self::H6),
        ]
        .boxed()
    }
}

impl Arbitrary for TextDecoration {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (
            any::<Option<hyperchad_color::Color>>(),
            prop::collection::vec(
                any::<hyperchad_transformer_models::TextDecorationLine>(),
                0..3,
            ),
            any::<Option<hyperchad_transformer_models::TextDecorationStyle>>(),
            any::<Option<Number>>(),
        )
            .prop_map(|(color, line, style, thickness)| Self {
                color,
                line,
                style,
                thickness,
            })
            .boxed()
    }
}

impl Arbitrary for Flex {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (any::<Number>(), any::<Number>(), any::<Number>())
            .prop_map(|(grow, shrink, basis)| Self {
                grow,
                shrink,
                basis,
            })
            .boxed()
    }
}

impl Arbitrary for Input {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            (any::<Option<XmlString>>(), any::<Option<XmlString>>()).prop_map(
                |(value, placeholder)| Self::Text {
                    value: value.map(|x| x.0),
                    placeholder: placeholder.map(|x| x.0),
                }
            ),
            (any::<Option<XmlString>>(), any::<Option<XmlString>>()).prop_map(
                |(value, placeholder)| Self::Password {
                    value: value.map(|x| x.0),
                    placeholder: placeholder.map(|x| x.0),
                }
            ),
            any::<Option<bool>>().prop_map(|checked| Self::Checkbox { checked }),
        ]
        .boxed()
    }
}

impl Arbitrary for Element {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        #[cfg(feature = "canvas")]
        let canvas_strategy = prop_oneof![Just(Self::Canvas)].boxed();

        #[cfg(not(feature = "canvas"))]
        let canvas_strategy = Just(Self::Div).boxed();

        prop_oneof![
            Just(Self::Div),
            // Skip Raw to avoid issues in property testing
            Just(Self::Aside),
            Just(Self::Main),
            Just(Self::Header),
            Just(Self::Footer),
            Just(Self::Section),
            Just(Self::Form {
                action: None,
                method: None,
            }),
            Just(Self::Span),
            (
                any::<Option<XmlString>>(),
                any::<Option<bool>>(),
                any::<Input>()
            )
                .prop_map(|(name, autofocus, input)| Self::Input {
                    name: name.map(|x| x.0),
                    autofocus,
                    input,
                }),
            any::<Option<XmlString>>().prop_map(|r#type| Self::Button {
                r#type: r#type.map(|x| x.0)
            }),
            (
                any::<Option<XmlString>>(),
                any::<Option<XmlString>>(),
                any::<Option<hyperchad_transformer_models::ImageFit>>(),
                any::<Option<hyperchad_transformer_models::ImageLoading>>(),
                any::<Option<XmlString>>(),
                any::<Option<Number>>(),
            )
                .prop_map(|(source, alt, fit, loading, source_set, sizes)| {
                    Self::Image {
                        source: source.map(|x| x.0),
                        alt: alt.map(|x| x.0),
                        fit,
                        loading,
                        source_set: source_set.map(|x| x.0),
                        sizes,
                    }
                }),
            (
                any::<Option<hyperchad_transformer_models::LinkTarget>>(),
                any::<Option<XmlString>>()
            )
                .prop_map(|(target, href)| Self::Anchor {
                    target,
                    href: href.map(|x| x.0),
                }),
            any::<HeaderSize>().prop_map(|size| Self::Heading { size }),
            Just(Self::UnorderedList),
            Just(Self::OrderedList),
            Just(Self::ListItem),
            Just(Self::Table),
            Just(Self::THead),
            (any::<Option<Number>>(), any::<Option<Number>>())
                .prop_map(|(rows, columns)| Self::TH { rows, columns }),
            Just(Self::TBody),
            Just(Self::TR),
            (any::<Option<Number>>(), any::<Option<Number>>())
                .prop_map(|(rows, columns)| Self::TD { rows, columns }),
            (
                any::<XmlString>(),
                any::<Option<XmlString>>(),
                any::<Option<XmlString>>(),
                any::<Option<Number>>(),
                any::<Option<Number>>(),
            )
                .prop_map(|(value, placeholder, name, rows, cols)| Self::Textarea {
                    value: value.0,
                    placeholder: placeholder.map(|x| x.0),
                    name: name.map(|x| x.0),
                    rows,
                    cols,
                }),
            canvas_strategy,
        ]
        .boxed()
    }
}

impl Arbitrary for ConfigOverride {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Uses `prop_map` instead of `prop_flat_map` to preserve shrinking behavior.
    /// Generates the override and an optional default of the same type.
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        (
            any::<OverrideCondition>(),
            any::<OverrideItem>(),
            any::<Option<OverrideItem>>(),
        )
            .prop_map(|(condition, override_item, potential_default)| {
                // Get the type of the override
                let override_type: OverrideItemType = (&override_item).into();

                // Only use default if it's the same type as the override
                let default = potential_default.filter(|d| {
                    let default_type: OverrideItemType = d.into();
                    default_type == override_type
                });

                Self {
                    condition,
                    overrides: vec![override_item],
                    default,
                }
            })
            .boxed()
    }
}

impl Arbitrary for OverrideCondition {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        any::<CssIdentifierString>()
            .prop_map(|s| Self::ResponsiveTarget { name: s.0 })
            .boxed()
    }
}

impl Arbitrary for OverrideItem {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    /// Uses `prop_oneof!` instead of `prop_flat_map` to preserve shrinking behavior.
    #[allow(clippy::too_many_lines)]
    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        prop_oneof![
            // StrId
            any::<XmlString>().prop_map(|s| Self::StrId(s.0)),
            // Classes
            prop::collection::vec(any::<XmlString>(), 0..5).prop_map(|v| {
                Self::Classes(
                    v.into_iter()
                        .map(|x| x.0)
                        .filter(|x| !x.is_empty())
                        .filter(|x| !x.chars().any(char::is_whitespace))
                        .collect(),
                )
            }),
            // Direction
            any::<hyperchad_transformer_models::LayoutDirection>().prop_map(Self::Direction),
            // OverflowX
            any::<hyperchad_transformer_models::LayoutOverflow>().prop_map(Self::OverflowX),
            // OverflowY
            any::<hyperchad_transformer_models::LayoutOverflow>().prop_map(Self::OverflowY),
            // GridCellSize
            any::<Number>().prop_map(Self::GridCellSize),
            // JustifyContent
            any::<hyperchad_transformer_models::JustifyContent>().prop_map(Self::JustifyContent),
            // AlignItems
            any::<hyperchad_transformer_models::AlignItems>().prop_map(Self::AlignItems),
            // TextAlign
            any::<hyperchad_transformer_models::TextAlign>().prop_map(Self::TextAlign),
            // WhiteSpace
            any::<hyperchad_transformer_models::WhiteSpace>().prop_map(Self::WhiteSpace),
            // TextDecoration
            any::<TextDecoration>().prop_map(Self::TextDecoration),
            // FontFamily
            prop::collection::vec(any::<XmlString>(), 0..3).prop_map(|v| {
                Self::FontFamily(
                    v.into_iter()
                        .map(|x| x.0.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .filter(|x| !x.chars().any(|c| c == ','))
                        .collect(),
                )
            }),
            // FontWeight
            any::<hyperchad_transformer_models::FontWeight>().prop_map(Self::FontWeight),
            // Width
            any::<Number>().prop_map(Self::Width),
            // MinWidth
            any::<Number>().prop_map(Self::MinWidth),
            // MaxWidth
            any::<Number>().prop_map(Self::MaxWidth),
            // Height
            any::<Number>().prop_map(Self::Height),
            // MinHeight
            any::<Number>().prop_map(Self::MinHeight),
            // MaxHeight
            any::<Number>().prop_map(Self::MaxHeight),
            // Flex
            any::<Flex>().prop_map(Self::Flex),
            // ColumnGap
            any::<Number>().prop_map(Self::ColumnGap),
            // RowGap
            any::<Number>().prop_map(Self::RowGap),
            // Opacity
            any::<Number>().prop_map(Self::Opacity),
            // Left
            any::<Number>().prop_map(Self::Left),
            // Right
            any::<Number>().prop_map(Self::Right),
            // Top
            any::<Number>().prop_map(Self::Top),
            // Bottom
            any::<Number>().prop_map(Self::Bottom),
            // TranslateX
            any::<Number>().prop_map(Self::TranslateX),
            // TranslateY
            any::<Number>().prop_map(Self::TranslateY),
            // Cursor
            any::<hyperchad_transformer_models::Cursor>().prop_map(Self::Cursor),
            // UserSelect
            any::<hyperchad_transformer_models::UserSelect>().prop_map(Self::UserSelect),
            // OverflowWrap
            any::<hyperchad_transformer_models::OverflowWrap>().prop_map(Self::OverflowWrap),
            // TextOverflow
            any::<hyperchad_transformer_models::TextOverflow>().prop_map(Self::TextOverflow),
            // Position
            any::<hyperchad_transformer_models::Position>().prop_map(Self::Position),
            // Background
            any::<hyperchad_color::Color>().prop_map(Self::Background),
            // BorderTop
            (any::<hyperchad_color::Color>(), any::<Number>()).prop_map(Self::BorderTop),
            // BorderRight
            (any::<hyperchad_color::Color>(), any::<Number>()).prop_map(Self::BorderRight),
            // BorderBottom
            (any::<hyperchad_color::Color>(), any::<Number>()).prop_map(Self::BorderBottom),
            // BorderLeft
            (any::<hyperchad_color::Color>(), any::<Number>()).prop_map(Self::BorderLeft),
            // BorderTopLeftRadius
            any::<Number>().prop_map(Self::BorderTopLeftRadius),
            // BorderTopRightRadius
            any::<Number>().prop_map(Self::BorderTopRightRadius),
            // BorderBottomLeftRadius
            any::<Number>().prop_map(Self::BorderBottomLeftRadius),
            // BorderBottomRightRadius
            any::<Number>().prop_map(Self::BorderBottomRightRadius),
            // MarginLeft
            any::<Number>().prop_map(Self::MarginLeft),
            // MarginRight
            any::<Number>().prop_map(Self::MarginRight),
            // MarginTop
            any::<Number>().prop_map(Self::MarginTop),
            // MarginBottom
            any::<Number>().prop_map(Self::MarginBottom),
            // PaddingLeft
            any::<Number>().prop_map(Self::PaddingLeft),
            // PaddingRight
            any::<Number>().prop_map(Self::PaddingRight),
            // PaddingTop
            any::<Number>().prop_map(Self::PaddingTop),
            // PaddingBottom
            any::<Number>().prop_map(Self::PaddingBottom),
            // FontSize
            any::<Number>().prop_map(Self::FontSize),
            // Color
            any::<hyperchad_color::Color>().prop_map(Self::Color),
            // Hidden
            any::<bool>().prop_map(Self::Hidden),
            // Visibility
            any::<hyperchad_transformer_models::Visibility>().prop_map(Self::Visibility),
        ]
        .boxed()
    }
}

/// Generates a `BTreeMap` with XML-safe string keys and values.
fn xml_btreemap_strategy() -> BoxedStrategy<BTreeMap<String, String>> {
    prop::collection::btree_map(any::<XmlAttrNameString>(), any::<XmlString>(), 0..5)
        .prop_map(|map| map.into_iter().map(|(k, v)| (k.0, v.0)).collect())
        .boxed()
}

/// Deduplicates overrides, keeping only one override per condition and ensuring
/// all overrides within each `ConfigOverride` are of the same type.
fn deduplicate_overrides(mut overrides: Vec<ConfigOverride>) -> Vec<ConfigOverride> {
    let overrides2 = overrides.clone();
    let mut i = 0;
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
    overrides
}

// Group 1: Basic identity fields
prop_compose! {
    fn container_group1()(
        id in any::<usize>(),
        str_id in any::<Option<XmlString>>(),
        font_family in any::<Option<Vec<XmlString>>>(),
        font_weight in any::<Option<hyperchad_transformer_models::FontWeight>>(),
        classes in prop::collection::vec(any::<XmlString>(), 0..3),
        data in xml_btreemap_strategy(),
    ) -> (usize, Option<String>, Option<Vec<String>>, Option<hyperchad_transformer_models::FontWeight>, Vec<String>, BTreeMap<String, String>) {
        (
            id,
            str_id.map(|s| s.0),
            font_family.map(|v| {
                v.into_iter()
                    .map(|x| x.0.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .filter(|x| !x.chars().any(|c| c == ','))
                    .collect()
            }),
            font_weight,
            classes
                .into_iter()
                .map(|x| x.0)
                .filter(|x| !x.is_empty())
                .filter(|x| !x.chars().any(char::is_whitespace))
                .collect(),
            data,
        )
    }
}

// Group 2: Element and layout direction
prop_compose! {
    fn container_group2()(
        element in any::<Element>().prop_filter("no raw/text elements", |e| !matches!(e, Element::Raw { .. } | Element::Text { .. })),
        direction in any::<hyperchad_transformer_models::LayoutDirection>(),
        overflow_x in any::<hyperchad_transformer_models::LayoutOverflow>(),
        overflow_y in any::<hyperchad_transformer_models::LayoutOverflow>(),
        grid_cell_size in any::<Option<Number>>(),
        justify_content in any::<Option<hyperchad_transformer_models::JustifyContent>>(),
        align_items in any::<Option<hyperchad_transformer_models::AlignItems>>(),
        text_align in any::<Option<hyperchad_transformer_models::TextAlign>>(),
        white_space in any::<Option<hyperchad_transformer_models::WhiteSpace>>(),
        text_decoration in any::<Option<TextDecoration>>(),
    ) -> (Element, hyperchad_transformer_models::LayoutDirection, hyperchad_transformer_models::LayoutOverflow, hyperchad_transformer_models::LayoutOverflow, Option<Number>, Option<hyperchad_transformer_models::JustifyContent>, Option<hyperchad_transformer_models::AlignItems>, Option<hyperchad_transformer_models::TextAlign>, Option<hyperchad_transformer_models::WhiteSpace>, Option<TextDecoration>) {
        (element, direction, overflow_x, overflow_y, grid_cell_size, justify_content, align_items, text_align, white_space, text_decoration)
    }
}

// Group 3: Size fields
prop_compose! {
    fn container_group3()(
        width in any::<Option<Number>>(),
        min_width in any::<Option<Number>>(),
        max_width in any::<Option<Number>>(),
        height in any::<Option<Number>>(),
        min_height in any::<Option<Number>>(),
        max_height in any::<Option<Number>>(),
        flex in any::<Option<Flex>>(),
        column_gap in any::<Option<Number>>(),
        row_gap in any::<Option<Number>>(),
        opacity in any::<Option<Number>>(),
    ) -> (Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Flex>, Option<Number>, Option<Number>, Option<Number>) {
        (width, min_width, max_width, height, min_height, max_height, flex, column_gap, row_gap, opacity)
    }
}

// Group 4: Position fields
prop_compose! {
    fn container_group4()(
        left in any::<Option<Number>>(),
        right in any::<Option<Number>>(),
        top in any::<Option<Number>>(),
        bottom in any::<Option<Number>>(),
        translate_x in any::<Option<Number>>(),
        translate_y in any::<Option<Number>>(),
        cursor in any::<Option<hyperchad_transformer_models::Cursor>>(),
        user_select in any::<Option<hyperchad_transformer_models::UserSelect>>(),
        overflow_wrap in any::<Option<hyperchad_transformer_models::OverflowWrap>>(),
        text_overflow in any::<Option<hyperchad_transformer_models::TextOverflow>>(),
        position in any::<Option<hyperchad_transformer_models::Position>>(),
    ) -> (Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<hyperchad_transformer_models::Cursor>, Option<hyperchad_transformer_models::UserSelect>, Option<hyperchad_transformer_models::OverflowWrap>, Option<hyperchad_transformer_models::TextOverflow>, Option<hyperchad_transformer_models::Position>) {
        (left, right, top, bottom, translate_x, translate_y, cursor, user_select, overflow_wrap, text_overflow, position)
    }
}

// Group 5: Border and background fields
prop_compose! {
    fn container_group5()(
        background in any::<Option<hyperchad_color::Color>>(),
        border_top in any::<Option<(hyperchad_color::Color, Number)>>(),
        border_right in any::<Option<(hyperchad_color::Color, Number)>>(),
        border_bottom in any::<Option<(hyperchad_color::Color, Number)>>(),
        border_left in any::<Option<(hyperchad_color::Color, Number)>>(),
        border_top_left_radius in any::<Option<Number>>(),
        border_top_right_radius in any::<Option<Number>>(),
        border_bottom_left_radius in any::<Option<Number>>(),
        border_bottom_right_radius in any::<Option<Number>>(),
    ) -> (Option<hyperchad_color::Color>, Option<(hyperchad_color::Color, Number)>, Option<(hyperchad_color::Color, Number)>, Option<(hyperchad_color::Color, Number)>, Option<(hyperchad_color::Color, Number)>, Option<Number>, Option<Number>, Option<Number>, Option<Number>) {
        (background, border_top, border_right, border_bottom, border_left, border_top_left_radius, border_top_right_radius, border_bottom_left_radius, border_bottom_right_radius)
    }
}

// Group 6: Margin and padding fields
prop_compose! {
    fn container_group6()(
        margin_left in any::<Option<Number>>(),
        margin_right in any::<Option<Number>>(),
        margin_top in any::<Option<Number>>(),
        margin_bottom in any::<Option<Number>>(),
        padding_left in any::<Option<Number>>(),
        padding_right in any::<Option<Number>>(),
        padding_top in any::<Option<Number>>(),
        padding_bottom in any::<Option<Number>>(),
        font_size in any::<Option<Number>>(),
        color in any::<Option<hyperchad_color::Color>>(),
    ) -> (Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<Number>, Option<hyperchad_color::Color>) {
        (margin_left, margin_right, margin_top, margin_bottom, padding_left, padding_right, padding_top, padding_bottom, font_size, color)
    }
}

// Group 7: State and behavior fields
prop_compose! {
    fn container_group7()(
        state in any::<Option<JsonValue>>(),
        hidden in any::<Option<bool>>(),
        debug in any::<Option<bool>>(),
        visibility in any::<Option<hyperchad_transformer_models::Visibility>>(),
        route in any::<Option<hyperchad_transformer_models::Route>>(),
        actions in prop::collection::vec(any::<hyperchad_actions::Action>(), 0..2),
        overrides in prop::collection::vec(any::<ConfigOverride>(), 0..2),
    ) -> (Option<serde_json::Value>, Option<bool>, Option<bool>, Option<hyperchad_transformer_models::Visibility>, Option<hyperchad_transformer_models::Route>, Vec<hyperchad_actions::Action>, Vec<ConfigOverride>) {
        (state.map(|x| x.0), hidden, debug, visibility, route, actions, overrides)
    }
}

/// Generates a Container with all fields populated randomly.
/// Uses chained `prop_flat_map` to combine field groups without stack overflow.
#[allow(clippy::too_many_lines)]
fn container_fields_strategy() -> BoxedStrategy<Container> {
    (
        container_group1(),
        container_group2(),
        container_group3(),
        container_group4(),
        container_group5(),
        container_group6(),
        container_group7(),
    )
        .prop_map(|(g1, g2, g3, g4, g5, g6, g7)| {
            let (id, str_id, font_family, font_weight, classes, data) = g1;
            let (
                element,
                direction,
                overflow_x,
                overflow_y,
                grid_cell_size,
                justify_content,
                align_items,
                text_align,
                white_space,
                text_decoration,
            ) = g2;
            let (
                width,
                min_width,
                max_width,
                height,
                min_height,
                max_height,
                flex,
                column_gap,
                row_gap,
                opacity,
            ) = g3;
            let (
                left,
                right,
                top,
                bottom,
                translate_x,
                translate_y,
                cursor,
                user_select,
                overflow_wrap,
                text_overflow,
                position,
            ) = g4;
            let (
                background,
                border_top,
                border_right,
                border_bottom,
                border_left,
                border_top_left_radius,
                border_top_right_radius,
                border_bottom_left_radius,
                border_bottom_right_radius,
            ) = g5;
            let (
                margin_left,
                margin_right,
                margin_top,
                margin_bottom,
                padding_left,
                padding_right,
                padding_top,
                padding_bottom,
                font_size,
                color,
            ) = g6;
            let (state, hidden, debug, visibility, route, actions, overrides) = g7;

            Container {
                id,
                str_id,
                font_family,
                font_weight,
                classes,
                data,
                element,
                children: vec![], // Children added separately to control recursion
                direction,
                overflow_x,
                overflow_y,
                grid_cell_size,
                justify_content,
                align_items,
                text_align,
                white_space,
                text_decoration,
                width,
                min_width,
                max_width,
                height,
                min_height,
                max_height,
                flex,
                column_gap,
                row_gap,
                opacity,
                left,
                right,
                top,
                bottom,
                translate_x,
                translate_y,
                cursor,
                user_select,
                overflow_wrap,
                text_overflow,
                position,
                background,
                border_top,
                border_right,
                border_bottom,
                border_left,
                border_top_left_radius,
                border_top_right_radius,
                border_bottom_left_radius,
                border_bottom_right_radius,
                margin_left,
                margin_right,
                margin_top,
                margin_bottom,
                padding_left,
                padding_right,
                padding_top,
                padding_bottom,
                font_size,
                color,
                state,
                hidden,
                debug,
                visibility,
                route,
                actions,
                overrides: deduplicate_overrides(overrides),
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
        })
        .boxed()
}

/// Strategy for generating a leaf container (no children).
fn leaf_container_strategy() -> BoxedStrategy<Container> {
    container_fields_strategy()
}

impl Arbitrary for Container {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with((): Self::Parameters) -> Self::Strategy {
        // Use prop_recursive to handle children with proper depth limiting.
        // This avoids stack overflow by lazily constructing the recursive strategy.
        //
        // IMPORTANT: We use prop_map (not prop_flat_map) to preserve shrinking.
        // The container and children are generated independently, then combined.
        // If the element doesn't allow children, we clear them in post-processing.
        leaf_container_strategy()
            .prop_recursive(
                2,  // depth - max recursion levels
                16, // desired_size - target number of elements
                4,  // expected_branch_size - expected children per container
                |inner| {
                    // Generate container fields and children independently, then combine.
                    // This preserves shrinking because we use prop_map, not prop_flat_map.
                    (
                        container_fields_strategy(),
                        prop::collection::vec(inner, 0..3),
                    )
                        .prop_map(|(mut container, children)| {
                            // Only add children if the element allows them
                            if container.element.allows_children() {
                                container.children = children;
                            }
                            // Otherwise children stays empty (from container_fields_strategy)
                            container
                        })
                        .boxed()
                },
            )
            .boxed()
    }
}
