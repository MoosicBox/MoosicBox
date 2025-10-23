use moosicbox_arb::xml::XmlString;
use quickcheck::{Arbitrary, Gen};

use crate::{
    AlignItems, Cursor, FontWeight, ImageFit, ImageLoading, JustifyContent, LayoutDirection,
    LayoutOverflow, LinkTarget, OverflowWrap, Position, Route, SwapTarget, TextAlign,
    TextDecorationLine, TextDecorationStyle, TextOverflow, UserSelect, Visibility, WhiteSpace,
};

impl Arbitrary for LayoutDirection {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Row, Self::Column]).unwrap()
    }
}

impl Arbitrary for LayoutOverflow {
    fn arbitrary(g: &mut Gen) -> Self {
        let grid = *g.choose(&[true, false]).unwrap();
        *g.choose(&[
            Self::Auto,
            Self::Scroll,
            Self::Expand,
            Self::Wrap { grid },
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
        ])
        .unwrap()
    }
}

impl Arbitrary for AlignItems {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Start, Self::Center, Self::End]).unwrap()
    }
}

impl Arbitrary for TextAlign {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Start, Self::Center, Self::End, Self::Justify])
            .unwrap()
    }
}

impl Arbitrary for WhiteSpace {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Normal, Self::Preserve, Self::PreserveWrap])
            .unwrap()
    }
}

impl Arbitrary for UserSelect {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Auto, Self::None, Self::Text, Self::All])
            .unwrap()
    }
}

impl Arbitrary for OverflowWrap {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Normal, Self::BreakWord, Self::Anywhere])
            .unwrap()
    }
}

impl Arbitrary for TextOverflow {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Clip, Self::Ellipsis]).unwrap()
    }
}

impl Arbitrary for FontWeight {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Thin,
            Self::ExtraLight,
            Self::Light,
            Self::Normal,
            Self::Medium,
            Self::SemiBold,
            Self::Bold,
            Self::ExtraBold,
            Self::Black,
            Self::Lighter,
            Self::Bolder,
            Self::Weight100,
            Self::Weight200,
            Self::Weight300,
            Self::Weight400,
            Self::Weight500,
            Self::Weight600,
            Self::Weight700,
            Self::Weight800,
            Self::Weight900,
        ])
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
        *g.choose(&[Self::Static, Self::Relative, Self::Absolute, Self::Fixed])
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
        let id = Arbitrary::arbitrary(g);
        g.choose(&[Self::This, Self::Id(id)]).unwrap().clone()
    }
}

impl Arbitrary for crate::SwapStrategy {
    fn arbitrary(g: &mut Gen) -> Self {
        g.choose(&[
            Self::This,
            Self::Children,
            Self::BeforeBegin,
            Self::AfterBegin,
            Self::BeforeEnd,
            Self::AfterEnd,
            Self::Delete,
            Self::None,
        ])
        .unwrap()
        .clone()
    }
}

impl Arbitrary for Route {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=4).collect::<Vec<_>>()).unwrap() {
            0 => Self::Get {
                route: XmlString::arbitrary(g).0,
                trigger: Option::arbitrary(g).map(|x: XmlString| x.0),
                target: SwapTarget::arbitrary(g),
                strategy: crate::SwapStrategy::arbitrary(g),
            },
            1 => Self::Post {
                route: XmlString::arbitrary(g).0,
                trigger: Option::arbitrary(g).map(|x: XmlString| x.0),
                target: SwapTarget::arbitrary(g),
                strategy: crate::SwapStrategy::arbitrary(g),
            },
            2 => Self::Put {
                route: XmlString::arbitrary(g).0,
                trigger: Option::arbitrary(g).map(|x: XmlString| x.0),
                target: SwapTarget::arbitrary(g),
                strategy: crate::SwapStrategy::arbitrary(g),
            },
            3 => Self::Delete {
                route: XmlString::arbitrary(g).0,
                trigger: Option::arbitrary(g).map(|x: XmlString| x.0),
                target: SwapTarget::arbitrary(g),
                strategy: crate::SwapStrategy::arbitrary(g),
            },
            4 => Self::Patch {
                route: XmlString::arbitrary(g).0,
                trigger: Option::arbitrary(g).map(|x: XmlString| x.0),
                target: SwapTarget::arbitrary(g),
                strategy: crate::SwapStrategy::arbitrary(g),
            },
            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "layout")]
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

impl Arbitrary for ImageFit {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Default,
            Self::Contain,
            Self::Cover,
            Self::Fill,
            Self::None,
        ])
        .unwrap()
    }
}

impl Arbitrary for ImageLoading {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[Self::Eager, Self::Lazy]).unwrap()
    }
}

impl Arbitrary for TextDecorationLine {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Inherit,
            Self::None,
            Self::Underline,
            Self::Overline,
            Self::LineThrough,
        ])
        .unwrap()
    }
}

impl Arbitrary for TextDecorationStyle {
    fn arbitrary(g: &mut Gen) -> Self {
        *g.choose(&[
            Self::Inherit,
            Self::Solid,
            Self::Double,
            Self::Dotted,
            Self::Dashed,
            Self::Wavy,
        ])
        .unwrap()
    }
}

impl Arbitrary for LinkTarget {
    fn arbitrary(g: &mut Gen) -> Self {
        match *g.choose(&(0..=4).collect::<Vec<_>>()).unwrap() {
            0 => Self::SelfTarget,
            1 => Self::Blank,
            2 => Self::Parent,
            3 => Self::Top,
            4 => Self::Custom(XmlString::arbitrary(g).0),
            _ => unreachable!(),
        }
    }
}
