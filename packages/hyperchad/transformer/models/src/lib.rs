#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "arb")]
pub mod arb;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum LayoutDirection {
    Row,
    #[default]
    Column,
}

impl std::fmt::Display for LayoutDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Row => f.write_str("row"),
            Self::Column => f.write_str("col"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum LayoutOverflow {
    Auto,
    Scroll,
    #[default]
    Expand,
    Squash,
    Wrap {
        grid: bool,
    },
    Hidden,
}

impl std::fmt::Display for LayoutOverflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Scroll => f.write_str("scroll"),
            Self::Expand => f.write_str("expand"),
            Self::Squash => f.write_str("squash"),
            Self::Wrap { grid } => f.write_str(if *grid { "wrap-grid" } else { "wrap" }),
            Self::Hidden => f.write_str("hidden"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum JustifyContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceEvenly,
}

impl std::fmt::Display for JustifyContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Center => f.write_str("center"),
            Self::End => f.write_str("end"),
            Self::SpaceBetween => f.write_str("space-between"),
            Self::SpaceEvenly => f.write_str("space-evenly"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum AlignItems {
    #[default]
    Start,
    Center,
    End,
}

impl std::fmt::Display for AlignItems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Center => f.write_str("center"),
            Self::End => f.write_str("end"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

impl std::fmt::Display for TextAlign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start => f.write_str("start"),
            Self::Center => f.write_str("center"),
            Self::End => f.write_str("end"),
            Self::Justify => f.write_str("justify"),
        }
    }
}

#[cfg(feature = "layout")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum LayoutPosition {
    Wrap {
        row: u32,
        col: u32,
    },
    #[default]
    Default,
}

#[cfg(feature = "layout")]
impl LayoutPosition {
    #[must_use]
    pub const fn row(&self) -> Option<u32> {
        match self {
            Self::Wrap { row, .. } => Some(*row),
            Self::Default => None,
        }
    }

    #[must_use]
    pub const fn column(&self) -> Option<u32> {
        match self {
            Self::Wrap { col, .. } => Some(*col),
            Self::Default => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Target {
    Literal(String),
    Ref(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Selector {
    Id(String),
    Class(String),
    ChildClass(String),
    #[default]
    SelfTarget,
}

impl Selector {
    #[must_use]
    pub fn id(id: impl Into<String>) -> Self {
        Self::Id(id.into())
    }

    #[must_use]
    pub fn class(class: impl Into<String>) -> Self {
        Self::Class(class.into())
    }

    #[must_use]
    pub fn child_class(class: impl Into<String>) -> Self {
        Self::ChildClass(class.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub struct ParseSelectorError;

impl std::fmt::Display for ParseSelectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid selector")
    }
}

impl TryFrom<String> for Selector {
    type Error = ParseSelectorError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&String> for Selector {
    type Error = ParseSelectorError;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for Selector {
    type Error = ParseSelectorError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value {
            "self" => Self::SelfTarget,
            value => {
                if let Some(id) = value.strip_prefix('#') {
                    Self::Id(id.to_string())
                } else if let Some(class) = value.strip_prefix('.') {
                    Self::Class(class.to_string())
                } else if let Some(class) = value.strip_prefix("> .") {
                    Self::ChildClass(class.to_string())
                } else {
                    return Err(ParseSelectorError);
                }
            }
        })
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(id) => write!(f, "#{id}"),
            Self::Class(class) => write!(f, ".{class}"),
            Self::ChildClass(class) => write!(f, "> .{class}"),
            Self::SelfTarget => f.write_str("this"),
        }
    }
}

impl From<String> for Target {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}

impl From<&str> for Target {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_string())
    }
}

impl From<&String> for Target {
    fn from(value: &String) -> Self {
        Self::Literal(value.clone())
    }
}

impl From<&Self> for Target {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl Target {
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Literal(x) | Self::Ref(x) => Some(x),
        }
    }

    #[must_use]
    pub fn literal(str: impl Into<String>) -> Self {
        Self::Literal(str.into())
    }

    #[must_use]
    pub fn reference(str: impl Into<String>) -> Self {
        Self::Ref(str.into())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ElementTarget {
    StrId(Target),
    Class(Target),
    ChildClass(Target),
    Id(usize),
    #[default]
    SelfTarget,
    LastChild,
}

impl ElementTarget {
    #[must_use]
    pub fn str_id(target: impl Into<Target>) -> Self {
        Self::StrId(target.into())
    }

    #[must_use]
    pub fn class(target: impl Into<Target>) -> Self {
        Self::Class(target.into())
    }

    #[must_use]
    pub fn child_class(target: impl Into<Target>) -> Self {
        Self::ChildClass(target.into())
    }
}

impl std::fmt::Display for ElementTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StrId(target) => {
                if let Some(s) = target.as_str() {
                    write!(f, "#{s}")
                } else {
                    f.write_str("#")
                }
            }
            Self::Class(target) => {
                if let Some(s) = target.as_str() {
                    write!(f, ".{s}")
                } else {
                    f.write_str(".")
                }
            }
            Self::ChildClass(target) => {
                if let Some(s) = target.as_str() {
                    write!(f, "> .{s}")
                } else {
                    f.write_str("> .")
                }
            }
            Self::Id(_) => f.write_str("[internal-id]"),
            Self::SelfTarget => f.write_str("this"),
            Self::LastChild => f.write_str(":last-child"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum SwapStrategy {
    #[default]
    This,
    Children,
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
    Delete,
    None,
}

impl std::fmt::Display for SwapStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::This => f.write_str("this"),
            Self::Children => f.write_str("children"),
            Self::BeforeBegin => f.write_str("beforebegin"),
            Self::AfterBegin => f.write_str("afterbegin"),
            Self::BeforeEnd => f.write_str("beforeend"),
            Self::AfterEnd => f.write_str("afterend"),
            Self::Delete => f.write_str("delete"),
            Self::None => f.write_str("none"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum Route {
    Get {
        route: String,
        trigger: Option<String>,
        target: Selector,
        strategy: SwapStrategy,
    },
    Post {
        route: String,
        trigger: Option<String>,
        target: Selector,
        strategy: SwapStrategy,
    },
    Put {
        route: String,
        trigger: Option<String>,
        target: Selector,
        strategy: SwapStrategy,
    },
    Delete {
        route: String,
        trigger: Option<String>,
        target: Selector,
        strategy: SwapStrategy,
    },
    Patch {
        route: String,
        trigger: Option<String>,
        target: Selector,
        strategy: SwapStrategy,
    },
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum Cursor {
    #[default]
    Auto,
    Pointer,
    Text,
    Crosshair,
    Move,
    NotAllowed,
    NoDrop,
    Grab,
    Grabbing,
    AllScroll,
    ColResize,
    RowResize,
    NResize,
    EResize,
    SResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
    EwResize,
    NsResize,
    NeswResize,
    ZoomIn,
    ZoomOut,
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Pointer => f.write_str("pointer"),
            Self::Text => f.write_str("text"),
            Self::Crosshair => f.write_str("crosshair"),
            Self::Move => f.write_str("move"),
            Self::NotAllowed => f.write_str("not-allowed"),
            Self::NoDrop => f.write_str("no-drop"),
            Self::Grab => f.write_str("grab"),
            Self::Grabbing => f.write_str("grabbing"),
            Self::AllScroll => f.write_str("all-scroll"),
            Self::ColResize => f.write_str("col-resize"),
            Self::RowResize => f.write_str("row-resize"),
            Self::NResize => f.write_str("n-resize"),
            Self::EResize => f.write_str("e-resize"),
            Self::SResize => f.write_str("s-resize"),
            Self::WResize => f.write_str("w-resize"),
            Self::NeResize => f.write_str("ne-resize"),
            Self::NwResize => f.write_str("nw-resize"),
            Self::SeResize => f.write_str("se-resize"),
            Self::SwResize => f.write_str("sw-resize"),
            Self::EwResize => f.write_str("ew-resize"),
            Self::NsResize => f.write_str("ns-resize"),
            Self::NeswResize => f.write_str("nesw-resize"),
            Self::ZoomIn => f.write_str("zoom-in"),
            Self::ZoomOut => f.write_str("zoom-out"),
        }
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Sticky,
    Fixed,
}

impl Position {
    #[must_use]
    pub const fn is_relative(self) -> bool {
        match self {
            Self::Static | Self::Relative | Self::Sticky => true,
            Self::Absolute | Self::Fixed => false,
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static => f.write_str("static"),
            Self::Relative => f.write_str("relative"),
            Self::Absolute => f.write_str("absolute"),
            Self::Sticky => f.write_str("sticky"),
            Self::Fixed => f.write_str("fixed"),
        }
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Visible => f.write_str("visible"),
            Self::Hidden => f.write_str("hidden"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum ImageLoading {
    #[default]
    Eager,
    Lazy,
}

impl std::fmt::Display for ImageLoading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eager => f.write_str("eager"),
            Self::Lazy => f.write_str("lazy"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum ImageFit {
    #[default]
    Default,
    Contain,
    Cover,
    Fill,
    None,
}

impl std::fmt::Display for ImageFit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => f.write_str("default"),
            Self::Contain => f.write_str("contain"),
            Self::Cover => f.write_str("cover"),
            Self::Fill => f.write_str("fill"),
            Self::None => f.write_str("none"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum TextDecorationLine {
    #[default]
    Inherit,
    None,
    Underline,
    Overline,
    LineThrough,
}

impl std::fmt::Display for TextDecorationLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inherit => f.write_str("inherit"),
            Self::None => f.write_str("none"),
            Self::Underline => f.write_str("underline"),
            Self::Overline => f.write_str("overline"),
            Self::LineThrough => f.write_str("line-through"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum TextDecorationStyle {
    #[default]
    Inherit,
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

impl std::fmt::Display for TextDecorationStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inherit => f.write_str("inherit"),
            Self::Solid => f.write_str("solid"),
            Self::Double => f.write_str("double"),
            Self::Dotted => f.write_str("dotted"),
            Self::Dashed => f.write_str("dashed"),
            Self::Wavy => f.write_str("wavy"),
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum LinkTarget {
    #[default]
    SelfTarget,
    Blank,
    Parent,
    Top,
    Custom(String),
}

impl std::fmt::Display for LinkTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelfTarget => f.write_str("_self"),
            Self::Blank => f.write_str("_blank"),
            Self::Parent => f.write_str("_parent"),
            Self::Top => f.write_str("_top"),
            Self::Custom(target) => f.write_str(target),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    #[default]
    Normal,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
    Lighter,
    Bolder,
    Weight100,
    Weight200,
    Weight300,
    Weight400,
    Weight500,
    Weight600,
    Weight700,
    Weight800,
    Weight900,
}

impl std::fmt::Display for FontWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Named variants output their semantic names
            Self::Thin => f.write_str("thin"),
            Self::ExtraLight => f.write_str("extra-light"),
            Self::Light => f.write_str("light"),
            Self::Normal => f.write_str("normal"),
            Self::Medium => f.write_str("medium"),
            Self::SemiBold => f.write_str("semi-bold"),
            Self::Bold => f.write_str("bold"),
            Self::ExtraBold => f.write_str("extra-bold"),
            Self::Black => f.write_str("black"),
            Self::Lighter => f.write_str("lighter"),
            Self::Bolder => f.write_str("bolder"),
            // Numeric variants output their numbers
            Self::Weight100 => f.write_str("100"),
            Self::Weight200 => f.write_str("200"),
            Self::Weight300 => f.write_str("300"),
            Self::Weight400 => f.write_str("400"),
            Self::Weight500 => f.write_str("500"),
            Self::Weight600 => f.write_str("600"),
            Self::Weight700 => f.write_str("700"),
            Self::Weight800 => f.write_str("800"),
            Self::Weight900 => f.write_str("900"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum WhiteSpace {
    #[default]
    Normal,
    Preserve,
    PreserveWrap,
}

impl std::fmt::Display for WhiteSpace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => f.write_str("normal"),
            Self::Preserve => f.write_str("preserve"),
            Self::PreserveWrap => f.write_str("preserve-wrap"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum UserSelect {
    #[default]
    Auto,
    None,
    Text,
    All,
}

impl std::fmt::Display for UserSelect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::None => f.write_str("none"),
            Self::Text => f.write_str("text"),
            Self::All => f.write_str("all"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum OverflowWrap {
    #[default]
    Normal,
    BreakWord,
    Anywhere,
}

impl std::fmt::Display for OverflowWrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => f.write_str("normal"),
            Self::BreakWord => f.write_str("break-word"),
            Self::Anywhere => f.write_str("anywhere"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

impl std::fmt::Display for TextOverflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clip => f.write_str("clip"),
            Self::Ellipsis => f.write_str("ellipsis"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_weight_display() {
        // Named variants output semantic names
        assert_eq!(FontWeight::Normal.to_string(), "normal");
        assert_eq!(FontWeight::Bold.to_string(), "bold");
        assert_eq!(FontWeight::Thin.to_string(), "thin");
        assert_eq!(FontWeight::Black.to_string(), "black");
        assert_eq!(FontWeight::Lighter.to_string(), "lighter");
        assert_eq!(FontWeight::Bolder.to_string(), "bolder");

        // Numeric variants output numbers
        assert_eq!(FontWeight::Weight100.to_string(), "100");
        assert_eq!(FontWeight::Weight400.to_string(), "400");
        assert_eq!(FontWeight::Weight700.to_string(), "700");
        assert_eq!(FontWeight::Weight900.to_string(), "900");
    }

    #[test]
    fn test_font_weight_default() {
        assert_eq!(FontWeight::default(), FontWeight::Normal);
    }

    #[test]
    fn test_white_space_display() {
        assert_eq!(WhiteSpace::Normal.to_string(), "normal");
        assert_eq!(WhiteSpace::Preserve.to_string(), "preserve");
    }

    #[test]
    fn test_white_space_default() {
        assert_eq!(WhiteSpace::default(), WhiteSpace::Normal);
    }

    #[test]
    fn test_user_select_display() {
        assert_eq!(UserSelect::Auto.to_string(), "auto");
        assert_eq!(UserSelect::None.to_string(), "none");
        assert_eq!(UserSelect::Text.to_string(), "text");
        assert_eq!(UserSelect::All.to_string(), "all");
    }

    #[test]
    fn test_user_select_default() {
        assert_eq!(UserSelect::default(), UserSelect::Auto);
    }

    #[test]
    fn test_overflow_wrap_display() {
        assert_eq!(OverflowWrap::Normal.to_string(), "normal");
        assert_eq!(OverflowWrap::BreakWord.to_string(), "break-word");
        assert_eq!(OverflowWrap::Anywhere.to_string(), "anywhere");
    }

    #[test]
    fn test_overflow_wrap_default() {
        assert_eq!(OverflowWrap::default(), OverflowWrap::Normal);
    }

    #[test]
    fn test_text_overflow_display() {
        assert_eq!(TextOverflow::Clip.to_string(), "clip");
        assert_eq!(TextOverflow::Ellipsis.to_string(), "ellipsis");
    }

    #[test]
    fn test_text_overflow_default() {
        assert_eq!(TextOverflow::default(), TextOverflow::Clip);
    }
}
