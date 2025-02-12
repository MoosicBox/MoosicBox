use std::collections::HashMap;

use gigachad_transformer_models::{LayoutDirection, LayoutOverflow};
use moosicbox_gen::{
    serde::JsonValue,
    xml::{XmlAttrNameString, XmlString},
};
use quickcheck::{Arbitrary, Gen};
use strum::IntoEnumIterator;

use crate::{
    Calculation, CalculationType, ConfigOverride, Container, Element, Flex, HeaderSize, Input,
    Number, NumberType, OverrideCondition, OverrideItem, TextDecoration,
};

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

fn one_of_number(g: &mut Gen, types: &[NumberType]) -> Number {
    match *g.choose(types).unwrap() {
        NumberType::Real => Number::Real(Arbitrary::arbitrary(g)),
        NumberType::Integer => Number::Integer(Arbitrary::arbitrary(g)),
        NumberType::RealPercent => Number::RealPercent(Arbitrary::arbitrary(g)),
        NumberType::IntegerPercent => Number::IntegerPercent(Arbitrary::arbitrary(g)),
        NumberType::RealVw => Number::RealVw(Arbitrary::arbitrary(g)),
        NumberType::IntegerVw => Number::IntegerVw(Arbitrary::arbitrary(g)),
        NumberType::RealVh => Number::RealVh(Arbitrary::arbitrary(g)),
        NumberType::IntegerVh => Number::IntegerVh(Arbitrary::arbitrary(g)),
        NumberType::RealDvw => Number::RealDvw(Arbitrary::arbitrary(g)),
        NumberType::IntegerDvw => Number::IntegerDvw(Arbitrary::arbitrary(g)),
        NumberType::RealDvh => Number::RealDvh(Arbitrary::arbitrary(g)),
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
        let max = 23;
        #[cfg(not(feature = "canvas"))]
        let max = 22;
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
                input: Input::arbitrary(g),
            },
            10 => Self::Button,
            11 => Self::Image {
                source: Option::arbitrary(g).map(|x: XmlString| x.0),
                alt: Option::arbitrary(g).map(|x: XmlString| x.0),
                fit: Option::arbitrary(g),
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
            19 => Self::TH,
            20 => Self::TBody,
            21 => Self::TR,
            22 => Self::TD,
            #[cfg(feature = "canvas")]
            23 => Self::Canvas,
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for ConfigOverride {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            condition: Arbitrary::arbitrary(g),
            overrides: Arbitrary::arbitrary(g),
        }
    }
}

impl Arbitrary for OverrideCondition {
    fn arbitrary(g: &mut Gen) -> Self {
        Self::ResponsiveTarget {
            name: XmlString::arbitrary(g).0,
        }
    }
}

impl Arbitrary for OverrideItem {
    fn arbitrary(g: &mut Gen) -> Self {
        let max = 51;
        match *g.choose(&(0..=max).collect::<Vec<_>>()).unwrap() {
            0 => Self::StrId(XmlString::arbitrary(g).0),
            1 => Self::FontFamily(
                Vec::arbitrary(g)
                    .into_iter()
                    .map(|x: XmlString| x.0.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .filter(|x| !x.chars().any(|x| matches!(x, ',')))
                    .collect(),
            ),
            2 => Self::Classes(
                Vec::arbitrary(g)
                    .into_iter()
                    .map(|x: XmlString| x.0)
                    .filter(|x| !x.is_empty())
                    .filter(|x| !x.chars().any(char::is_whitespace))
                    .collect(),
            ),
            3 => Self::Data(xml_hashmap(g)),
            4 => Self::Direction(LayoutDirection::arbitrary(g)),
            5 => Self::OverflowX(LayoutOverflow::arbitrary(g)),
            6 => Self::OverflowY(LayoutOverflow::arbitrary(g)),
            7 => Self::JustifyContent(Arbitrary::arbitrary(g)),
            8 => Self::AlignItems(Arbitrary::arbitrary(g)),
            9 => Self::TextAlign(Arbitrary::arbitrary(g)),
            10 => Self::TextDecoration(Arbitrary::arbitrary(g)),
            11 => Self::Width(Arbitrary::arbitrary(g)),
            12 => Self::MinWidth(Arbitrary::arbitrary(g)),
            13 => Self::MaxWidth(Arbitrary::arbitrary(g)),
            14 => Self::Height(Arbitrary::arbitrary(g)),
            15 => Self::MinHeight(Arbitrary::arbitrary(g)),
            16 => Self::MaxHeight(Arbitrary::arbitrary(g)),
            17 => Self::Flex(Arbitrary::arbitrary(g)),
            18 => Self::ColumnGap(Arbitrary::arbitrary(g)),
            19 => Self::Opacity(Arbitrary::arbitrary(g)),
            20 => Self::Left(Arbitrary::arbitrary(g)),
            21 => Self::Right(Arbitrary::arbitrary(g)),
            22 => Self::Top(Arbitrary::arbitrary(g)),
            23 => Self::Bottom(Arbitrary::arbitrary(g)),
            24 => Self::TranslateX(Arbitrary::arbitrary(g)),
            25 => Self::TranslateY(Arbitrary::arbitrary(g)),
            26 => Self::Cursor(Arbitrary::arbitrary(g)),
            27 => Self::Position(Arbitrary::arbitrary(g)),
            28 => Self::Background(Arbitrary::arbitrary(g)),
            29 => Self::BorderTop(Arbitrary::arbitrary(g)),
            30 => Self::BorderRight(Arbitrary::arbitrary(g)),
            31 => Self::BorderBottom(Arbitrary::arbitrary(g)),
            32 => Self::BorderLeft(Arbitrary::arbitrary(g)),
            33 => Self::BorderTopLeftRadius(Arbitrary::arbitrary(g)),
            34 => Self::BorderTopRightRadius(Arbitrary::arbitrary(g)),
            35 => Self::BorderBottomLeftRadius(Arbitrary::arbitrary(g)),
            36 => Self::BorderBottomRightRadius(Arbitrary::arbitrary(g)),
            37 => Self::MarginLeft(Arbitrary::arbitrary(g)),
            38 => Self::MarginRight(Arbitrary::arbitrary(g)),
            39 => Self::MarginTop(Arbitrary::arbitrary(g)),
            40 => Self::MarginBottom(Arbitrary::arbitrary(g)),
            41 => Self::PaddingLeft(Arbitrary::arbitrary(g)),
            42 => Self::PaddingRight(Arbitrary::arbitrary(g)),
            43 => Self::PaddingTop(Arbitrary::arbitrary(g)),
            44 => Self::PaddingBottom(Arbitrary::arbitrary(g)),
            45 => Self::FontSize(Arbitrary::arbitrary(g)),
            46 => Self::Color(Arbitrary::arbitrary(g)),
            47 => Self::Hidden(Arbitrary::arbitrary(g)),
            48 => Self::Debug(Arbitrary::arbitrary(g)),
            49 => Self::Visibility(Arbitrary::arbitrary(g)),
            50 => Self::Route(Arbitrary::arbitrary(g)),
            51 => Self::RowGap(Arbitrary::arbitrary(g)),
            _ => unreachable!(),
        }
    }
}

fn half_g_max(g: &Gen, max: usize) -> Gen {
    Gen::new(std::cmp::min(max, g.size() / 2))
}

fn xml_hashmap(g: &mut Gen) -> HashMap<String, String> {
    let map: HashMap<XmlAttrNameString, XmlString> = Arbitrary::arbitrary(g);

    map.into_iter().map(|(k, v)| (k.0, v.0)).collect()
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

        Self {
            #[cfg(feature = "id")]
            id: usize::arbitrary(g),
            str_id: Option::arbitrary(g).map(|x: XmlString| x.0),
            font_family: Option::arbitrary(g).map(|x: Vec<XmlString>| {
                x.into_iter()
                    .map(|x| x.0.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .filter(|x| !x.chars().any(|x| matches!(x, ',')))
                    .collect()
            }),
            classes: Vec::arbitrary(g)
                .into_iter()
                .map(|x: XmlString| x.0)
                .filter(|x| !x.is_empty())
                .filter(|x| !x.chars().any(char::is_whitespace))
                .collect(),
            data: xml_hashmap(g),
            element,
            children,
            direction: LayoutDirection::arbitrary(g),
            overflow_x: LayoutOverflow::arbitrary(g),
            overflow_y: LayoutOverflow::arbitrary(g),
            justify_content: Arbitrary::arbitrary(g),
            align_items: Arbitrary::arbitrary(g),
            text_align: Arbitrary::arbitrary(g),
            text_decoration: Arbitrary::arbitrary(g),
            width: Option::arbitrary(g),
            min_width: Option::arbitrary(g),
            max_width: Option::arbitrary(g),
            height: Option::arbitrary(g),
            min_height: Option::arbitrary(g),
            max_height: Option::arbitrary(g),
            flex: Option::arbitrary(g),
            column_gap: Option::arbitrary(g),
            row_gap: Option::arbitrary(g),
            opacity: Option::arbitrary(g),
            left: Option::arbitrary(g),
            right: Option::arbitrary(g),
            top: Option::arbitrary(g),
            bottom: Option::arbitrary(g),
            translate_x: Option::arbitrary(g),
            translate_y: Option::arbitrary(g),
            cursor: Option::arbitrary(g),
            position: Option::arbitrary(g),
            background: Option::arbitrary(g),
            border_top: Option::arbitrary(g),
            border_right: Option::arbitrary(g),
            border_bottom: Option::arbitrary(g),
            border_left: Option::arbitrary(g),
            border_top_left_radius: Option::arbitrary(g),
            border_top_right_radius: Option::arbitrary(g),
            border_bottom_left_radius: Option::arbitrary(g),
            border_bottom_right_radius: Option::arbitrary(g),
            margin_left: Option::arbitrary(g),
            margin_right: Option::arbitrary(g),
            margin_top: Option::arbitrary(g),
            margin_bottom: Option::arbitrary(g),
            padding_left: Option::arbitrary(g),
            padding_right: Option::arbitrary(g),
            padding_top: Option::arbitrary(g),
            padding_bottom: Option::arbitrary(g),
            font_size: Option::arbitrary(g),
            color: Option::arbitrary(g),
            state: Option::arbitrary(g).map(|x: JsonValue| x.0),
            hidden: Option::arbitrary(g),
            debug: Option::arbitrary(g),
            visibility: Option::arbitrary(g),
            route: Option::arbitrary(g),
            actions: Vec::arbitrary(smaller_g),
            overrides: vec![],
            // overrides: Arbitrary::arbitrary(smaller_g),
            #[cfg(feature = "calc")]
            internal_margin_left: None,
            #[cfg(feature = "calc")]
            internal_margin_right: None,
            #[cfg(feature = "calc")]
            internal_margin_top: None,
            #[cfg(feature = "calc")]
            internal_margin_bottom: None,
            #[cfg(feature = "calc")]
            internal_padding_left: None,
            #[cfg(feature = "calc")]
            internal_padding_right: None,
            #[cfg(feature = "calc")]
            internal_padding_top: None,
            #[cfg(feature = "calc")]
            internal_padding_bottom: None,
            #[cfg(feature = "calc")]
            calculated_margin_left: None,
            #[cfg(feature = "calc")]
            calculated_margin_right: None,
            #[cfg(feature = "calc")]
            calculated_margin_top: None,
            #[cfg(feature = "calc")]
            calculated_margin_bottom: None,
            #[cfg(feature = "calc")]
            calculated_padding_left: None,
            #[cfg(feature = "calc")]
            calculated_padding_right: None,
            #[cfg(feature = "calc")]
            calculated_padding_top: None,
            #[cfg(feature = "calc")]
            calculated_padding_bottom: None,
            #[cfg(feature = "calc")]
            calculated_width: None,
            #[cfg(feature = "calc")]
            calculated_height: None,
            #[cfg(feature = "calc")]
            calculated_x: None,
            #[cfg(feature = "calc")]
            calculated_y: None,
            #[cfg(feature = "calc")]
            calculated_position: None,
            #[cfg(feature = "calc")]
            calculated_border_top: None,
            #[cfg(feature = "calc")]
            calculated_border_right: None,
            #[cfg(feature = "calc")]
            calculated_border_bottom: None,
            #[cfg(feature = "calc")]
            calculated_border_left: None,
            #[cfg(feature = "calc")]
            calculated_border_top_left_radius: None,
            #[cfg(feature = "calc")]
            calculated_border_top_right_radius: None,
            #[cfg(feature = "calc")]
            calculated_border_bottom_left_radius: None,
            #[cfg(feature = "calc")]
            calculated_border_bottom_right_radius: None,
            #[cfg(feature = "calc")]
            calculated_opacity: None,
            #[cfg(feature = "calc")]
            scrollbar_right: None,
            #[cfg(feature = "calc")]
            scrollbar_bottom: None,
        }
    }
}
