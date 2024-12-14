use quickcheck::{Arbitrary, Gen};

use crate::{
    AlignItems, Cursor, JustifyContent, LayoutDirection, LayoutOverflow, Position, Route,
    SwapTarget, Visibility,
};

impl Arbitrary for LayoutDirection {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Row, Self::Column]).unwrap()
    }
}

impl Arbitrary for LayoutOverflow {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Auto,
            Self::Scroll,
            Self::Show,
            Self::Wrap,
            Self::Squash,
        ])
        .unwrap()
    }
}

impl Arbitrary for JustifyContent {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Start,
            Self::Center,
            Self::End,
            Self::SpaceBetween,
            Self::SpaceEvenly,
            Self::Default,
        ])
        .unwrap()
    }
}

impl Arbitrary for AlignItems {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Start, Self::Center, Self::End, Self::Default])
            .unwrap()
    }
}

impl Arbitrary for Cursor {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Auto,
            Self::Pointer,
            Self::Text,
            Self::Crosshair,
            Self::Move,
            Self::NotAllowed,
            Self::NoDrop,
            Self::Grab,
            Self::Grabbing,
            Self::AllScroll,
            Self::ColResize,
            Self::RowResize,
            Self::NResize,
            Self::EResize,
            Self::SResize,
            Self::WResize,
            Self::NeResize,
            Self::NwResize,
            Self::SeResize,
            Self::SwResize,
            Self::EwResize,
            Self::NsResize,
            Self::NeswResize,
            Self::ZoomIn,
            Self::ZoomOut,
        ])
        .unwrap()
    }
}

impl Arbitrary for Position {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Static, Self::Relative, Self::Absolute])
            .unwrap()
    }
}

impl Arbitrary for Visibility {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Visible, Self::Hidden]).unwrap()
    }
}

impl Arbitrary for SwapTarget {
    fn arbitrary(g: &mut Gen) -> Self {
        g.choose(&[Self::This, Self::Children]).unwrap().clone()
    }
}

impl Arbitrary for Route {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=1).collect::<Vec<_>>()).unwrap() {
            0 => Self::Get {
                route: String::arbitrary(g),
                trigger: Option::arbitrary(g),
                swap: SwapTarget::arbitrary(g),
            },
            1 => Self::Post {
                route: String::arbitrary(g),
                trigger: Option::arbitrary(g),
                swap: SwapTarget::arbitrary(g),
            },
            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "calc")]
impl Arbitrary for crate::LayoutPosition {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=1).collect::<Vec<_>>()).unwrap() {
            0 => Self::Default,
            1 => Self::Wrap {
                row: u32::arbitrary(g),
                col: u32::arbitrary(g),
            },
            _ => unreachable!(),
        }
    }
}
