use gigachad_transformer_models::{AlignItems, JustifyContent, LayoutDirection, LayoutOverflow};
use quickcheck::{Arbitrary, Gen};

use crate::{Calculation, Container, Element, HeaderSize, Input, Number};

#[derive(Clone, Debug)]
struct JsonValue(serde_json::Value);

impl Arbitrary for JsonValue {
    fn arbitrary(g: &mut Gen) -> Self {
        Self(serde_json::Value::String(String::arbitrary(g)))
    }
}

impl Arbitrary for Calculation {
    fn arbitrary(g: &mut Gen) -> Self {
        if g.size() <= 3 {
            return Self::Number(Box::new(Number::arbitrary(g)));
        }

        let g = &mut half_g_max(g, 10);

        match *g.choose(&(0..=7).collect::<Vec<_>>()).unwrap() {
            0 => Self::Number(Box::new(Number::arbitrary(g))),
            1 => Self::Add(
                Box::new(Arbitrary::arbitrary(g)),
                Box::new(Arbitrary::arbitrary(g)),
            ),
            2 => Self::Subtract(
                Box::new(Arbitrary::arbitrary(g)),
                Box::new(Arbitrary::arbitrary(g)),
            ),
            3 => Self::Multiply(
                Box::new(Arbitrary::arbitrary(g)),
                Box::new(Arbitrary::arbitrary(g)),
            ),
            4 => Self::Divide(
                Box::new(Arbitrary::arbitrary(g)),
                Box::new(Arbitrary::arbitrary(g)),
            ),
            5 => Self::Grouping(Box::new(Arbitrary::arbitrary(g))),
            6 => Self::Min(
                Box::new(Arbitrary::arbitrary(g)),
                Box::new(Arbitrary::arbitrary(g)),
            ),
            7 => Self::Max(
                Box::new(Arbitrary::arbitrary(g)),
                Box::new(Arbitrary::arbitrary(g)),
            ),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for Number {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=3).collect::<Vec<_>>()).unwrap() {
            0 => Self::Real(f32::arbitrary(g)),
            1 => Self::Integer(i64::arbitrary(g)),
            2 => Self::RealPercent(f32::arbitrary(g)),
            3 => Self::IntegerPercent(i64::arbitrary(g)),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for HeaderSize {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::H1, Self::H2, Self::H3, Self::H4, Self::H5, Self::H6])
            .unwrap()
    }
}

impl Arbitrary for Input {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=2).collect::<Vec<_>>()).unwrap() {
            0 => Self::Text {
                value: Option::arbitrary(g),
                placeholder: Option::arbitrary(g),
            },
            1 => Self::Password {
                value: Option::arbitrary(g),
                placeholder: Option::arbitrary(g),
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
                value: String::arbitrary(g),
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
                source: Option::arbitrary(g),
            },
            12 => Self::Anchor {
                href: Option::arbitrary(g),
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

fn half_g_max(g: &Gen, max: usize) -> Gen {
    Gen::new(std::cmp::min(max, g.size() / 2))
}

impl Arbitrary for Container {
    #[allow(clippy::too_many_lines)]
    fn arbitrary(g: &mut Gen) -> Self {
        let smaller_g = &mut half_g_max(g, 10);
        let element = loop {
            let element = Element::arbitrary(g);
            if matches!(element, Element::Raw { .. }) {
                continue;
            }
            break element;
        };
        Self {
            #[cfg(feature = "id")]
            id: usize::arbitrary(g),
            str_id: Option::arbitrary(g),
            element,
            direction: LayoutDirection::arbitrary(g),
            overflow_x: LayoutOverflow::arbitrary(g),
            overflow_y: LayoutOverflow::arbitrary(g),
            justify_content: JustifyContent::arbitrary(g),
            align_items: AlignItems::arbitrary(g),
            width: Option::arbitrary(g),
            height: Option::arbitrary(g),
            gap: Option::arbitrary(g),
            opacity: Option::arbitrary(g),
            left: Option::arbitrary(g),
            right: Option::arbitrary(g),
            top: Option::arbitrary(g),
            bottom: Option::arbitrary(g),
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
            state: Option::arbitrary(g).map(|x: JsonValue| x.0),
            hidden: Option::arbitrary(g),
            debug: Option::arbitrary(g),
            visibility: Option::arbitrary(g),
            route: Option::arbitrary(g),
            actions: Vec::arbitrary(smaller_g),
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
            ..Default::default()
        }
    }
}
