//! Type definitions for `HyperChad` UI transformer models.
//!
//! This crate provides the core data models used by the `HyperChad` transformer framework,
//! including layout primitives, styling enums, routing definitions, and DOM manipulation types.
//!
//! # Core Types
//!
//! * [`LayoutDirection`], [`LayoutOverflow`], [`JustifyContent`], [`AlignItems`] - Flexbox-style layout controls
//! * [`Selector`], [`ElementTarget`] - CSS-style element targeting
//! * [`Route`], [`SwapStrategy`] - HTTP routing and DOM content swapping (htmx-inspired)
//! * [`Position`], [`Cursor`], [`Visibility`] - Element positioning and styling
//! * [`TextAlign`], [`FontWeight`], [`WhiteSpace`] - Text styling and formatting
//! * [`ImageFit`], [`ImageLoading`] - Image display controls
//!
//! # Features
//!
//! * `serde` - Enables serialization/deserialization for all public types
//! * `arb` - Provides `Arbitrary` implementations for property-based testing
//! * `layout` - Enables the [`LayoutPosition`] type for grid-based layouts

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Property-based testing support via `proptest::arbitrary::Arbitrary` implementations.
///
/// This module is only available when the `arb` feature is enabled.
#[cfg(feature = "arb")]
pub mod arb;

/// Layout direction for flexbox-style layouts.
///
/// Determines whether child elements are laid out horizontally or vertically.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum LayoutDirection {
    /// Horizontal layout (left-to-right or right-to-left).
    Row,
    /// Vertical layout (top-to-bottom). This is the default.
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

/// Overflow behavior for layouts when content exceeds available space.
///
/// Controls how a layout container handles child elements that don't fit.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum LayoutOverflow {
    /// Automatically determine overflow behavior.
    Auto,
    /// Enable scrolling for overflow content.
    Scroll,
    /// Expand the container to fit all content. This is the default.
    #[default]
    Expand,
    /// Compress content to fit within the container.
    Squash,
    /// Wrap content to the next line or column.
    Wrap {
        /// Whether to use grid layout for wrapped content.
        grid: bool,
    },
    /// Hide content that overflows the container.
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

/// Content justification along the main axis in flexbox-style layouts.
///
/// Controls how space is distributed between and around child elements along the main axis.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum JustifyContent {
    /// Align items to the start of the container. This is the default.
    #[default]
    Start,
    /// Center items along the main axis.
    Center,
    /// Align items to the end of the container.
    End,
    /// Distribute items with equal space between them.
    SpaceBetween,
    /// Distribute items with equal space around them.
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

/// Item alignment along the cross axis in flexbox-style layouts.
///
/// Controls how child elements are positioned along the cross axis (perpendicular to the main axis).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum AlignItems {
    /// Align items to the start of the cross axis. This is the default.
    #[default]
    Start,
    /// Center items along the cross axis.
    Center,
    /// Align items to the end of the cross axis.
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

/// Text alignment within a container.
///
/// Controls horizontal alignment of text content.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum TextAlign {
    /// Align text to the start edge. This is the default.
    #[default]
    Start,
    /// Center text horizontally.
    Center,
    /// Align text to the end edge.
    End,
    /// Justify text to fill the full width.
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

/// Position information for elements in wrapped layouts.
///
/// Specifies the row and column position when layout overflow is set to wrap with grid.
#[cfg(feature = "layout")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum LayoutPosition {
    /// Explicit grid position with row and column indices.
    Wrap {
        /// The row index in the wrapped grid layout.
        row: u32,
        /// The column index in the wrapped grid layout.
        col: u32,
    },
    /// Use default positioning (no explicit grid position). This is the default.
    #[default]
    Default,
}

#[cfg(feature = "layout")]
impl LayoutPosition {
    /// Returns the row index if this is a `Wrap` position.
    #[must_use]
    pub const fn row(&self) -> Option<u32> {
        match self {
            Self::Wrap { row, .. } => Some(*row),
            Self::Default => None,
        }
    }

    /// Returns the column index if this is a `Wrap` position.
    #[must_use]
    pub const fn column(&self) -> Option<u32> {
        match self {
            Self::Wrap { col, .. } => Some(*col),
            Self::Default => None,
        }
    }
}

/// A target reference that can be either a literal string or a reference to a value.
///
/// Used to specify targets in routes and other contexts where values might be literals or references.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum Target {
    /// A literal string value.
    Literal(String),
    /// A reference to a string value.
    Ref(String),
}

/// CSS-style selector for targeting elements.
///
/// Supports ID selectors, class selectors, child class selectors, and self-targeting.
/// Can be parsed from strings like `#id`, `.class`, `> .child-class`, or `self`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum Selector {
    /// Select by element ID (e.g., `#my-id`).
    Id(String),
    /// Select by element class (e.g., `.my-class`).
    Class(String),
    /// Select direct children with a class (e.g., `> .my-class`).
    ChildClass(String),
    /// Target the current element. This is the default.
    #[default]
    SelfTarget,
}

impl Selector {
    /// Creates an ID selector.
    #[must_use]
    pub fn id(id: impl Into<String>) -> Self {
        Self::Id(id.into())
    }

    /// Creates a class selector.
    #[must_use]
    pub fn class(class: impl Into<String>) -> Self {
        Self::Class(class.into())
    }

    /// Creates a child class selector.
    #[must_use]
    pub fn child_class(class: impl Into<String>) -> Self {
        Self::ChildClass(class.into())
    }
}

/// Error type for selector parsing failures.
///
/// Returned when a string cannot be parsed into a valid [`Selector`].
#[derive(Debug, thiserror::Error)]
pub struct ParseSelectorError;

impl std::fmt::Display for ParseSelectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid selector")
    }
}

impl TryFrom<String> for Selector {
    type Error = ParseSelectorError;

    /// Attempts to parse a selector from a string.
    ///
    /// # Errors
    ///
    /// * Returns `ParseSelectorError` if the string is not a valid selector format
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&String> for Selector {
    type Error = ParseSelectorError;

    /// Attempts to parse a selector from a string reference.
    ///
    /// # Errors
    ///
    /// * Returns `ParseSelectorError` if the string is not a valid selector format
    fn try_from(value: &String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for Selector {
    type Error = ParseSelectorError;

    /// Attempts to parse a selector from a string slice.
    ///
    /// Supports the following formats:
    /// * `"self"` - Self target
    /// * `"#id"` - ID selector
    /// * `".class"` - Class selector
    /// * `"> .class"` - Child class selector
    ///
    /// # Errors
    ///
    /// * Returns `ParseSelectorError` if the string does not match any valid selector format
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
    /// Returns the string value of the target, regardless of whether it's literal or a reference.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Literal(x) | Self::Ref(x) => Some(x),
        }
    }

    /// Creates a literal target.
    #[must_use]
    pub fn literal(str: impl Into<String>) -> Self {
        Self::Literal(str.into())
    }

    /// Creates a reference target.
    #[must_use]
    pub fn reference(str: impl Into<String>) -> Self {
        Self::Ref(str.into())
    }
}

/// Target specification for DOM element selection.
///
/// Provides various ways to target elements including by ID, class, child class,
/// CSS selector, internal numeric ID, self-reference, or last child.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum ElementTarget {
    /// Target by element ID only (uses `getElementById`, no `#` prefix needed).
    ById(Target),
    /// Target by CSS selector (uses `querySelectorAll`, works for any selector).
    Selector(Target),
    /// Target by CSS class.
    Class(Target),
    /// Target direct children with a specific class.
    ChildClass(Target),
    /// Target by internal numeric ID.
    Id(usize),
    /// Target the current element. This is the default.
    #[default]
    SelfTarget,
    /// Target the last child element.
    LastChild,
}

impl ElementTarget {
    /// Creates a by-ID target (uses `getElementById`).
    #[must_use]
    pub fn by_id(target: impl Into<Target>) -> Self {
        Self::ById(target.into())
    }

    /// Creates a CSS selector target (uses `querySelectorAll`).
    #[must_use]
    pub fn selector(target: impl Into<Target>) -> Self {
        Self::Selector(target.into())
    }

    /// Creates a class target.
    #[must_use]
    pub fn class(target: impl Into<Target>) -> Self {
        Self::Class(target.into())
    }

    /// Creates a child class target.
    #[must_use]
    pub fn child_class(target: impl Into<Target>) -> Self {
        Self::ChildClass(target.into())
    }
}

impl std::fmt::Display for ElementTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ById(target) => {
                if let Some(s) = target.as_str() {
                    write!(f, "#{s}")
                } else {
                    f.write_str("#")
                }
            }
            Self::Selector(target) => {
                if let Some(s) = target.as_str() {
                    f.write_str(s)
                } else {
                    f.write_str("[selector]")
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

/// Strategy for how to swap or insert content in the DOM.
///
/// Based on htmx swap strategies, controls where new content is placed relative to the target element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum SwapStrategy {
    /// Replace the target element itself. This is the default.
    #[default]
    This,
    /// Replace the children of the target element.
    Children,
    /// Insert before the target element (as a sibling).
    BeforeBegin,
    /// Insert as the first child of the target element.
    AfterBegin,
    /// Insert as the last child of the target element.
    BeforeEnd,
    /// Insert after the target element (as a sibling).
    AfterEnd,
    /// Delete the target element.
    Delete,
    /// Do not swap any content.
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

/// HTTP route definition with target and swap strategy.
///
/// Represents different HTTP methods with associated route paths, event triggers,
/// target selectors, and swap strategies for dynamic content updates.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum Route {
    /// HTTP GET request.
    Get {
        /// The URL path for the request.
        route: String,
        /// Optional event trigger (e.g., `click`, `load`).
        trigger: Option<String>,
        /// Target element selector.
        target: Selector,
        /// How to swap the response content.
        strategy: SwapStrategy,
    },
    /// HTTP POST request.
    Post {
        /// The URL path for the request.
        route: String,
        /// Optional event trigger (e.g., `click`, `load`).
        trigger: Option<String>,
        /// Target element selector.
        target: Selector,
        /// How to swap the response content.
        strategy: SwapStrategy,
    },
    /// HTTP PUT request.
    Put {
        /// The URL path for the request.
        route: String,
        /// Optional event trigger (e.g., `click`, `load`).
        trigger: Option<String>,
        /// Target element selector.
        target: Selector,
        /// How to swap the response content.
        strategy: SwapStrategy,
    },
    /// HTTP DELETE request.
    Delete {
        /// The URL path for the request.
        route: String,
        /// Optional event trigger (e.g., `click`, `load`).
        trigger: Option<String>,
        /// Target element selector.
        target: Selector,
        /// How to swap the response content.
        strategy: SwapStrategy,
    },
    /// HTTP PATCH request.
    Patch {
        /// The URL path for the request.
        route: String,
        /// Optional event trigger (e.g., `click`, `load`).
        trigger: Option<String>,
        /// Target element selector.
        target: Selector,
        /// How to swap the response content.
        strategy: SwapStrategy,
    },
}

/// Mouse cursor style.
///
/// Defines the appearance of the mouse cursor when hovering over an element.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum Cursor {
    /// Default cursor (usually an arrow). This is the default.
    #[default]
    Auto,
    /// Pointing hand cursor (typically for links).
    Pointer,
    /// Text selection cursor (I-beam).
    Text,
    /// Crosshair cursor.
    Crosshair,
    /// Move cursor (four-directional arrows).
    Move,
    /// Not-allowed cursor (circle with a line through it).
    NotAllowed,
    /// No-drop cursor (hand with a no symbol).
    NoDrop,
    /// Grab cursor (open hand).
    Grab,
    /// Grabbing cursor (closed hand).
    Grabbing,
    /// All-scroll cursor (arrows in all directions).
    AllScroll,
    /// Column resize cursor (horizontal arrows).
    ColResize,
    /// Row resize cursor (vertical arrows).
    RowResize,
    /// North resize cursor.
    NResize,
    /// East resize cursor.
    EResize,
    /// South resize cursor.
    SResize,
    /// West resize cursor.
    WResize,
    /// Northeast resize cursor.
    NeResize,
    /// Northwest resize cursor.
    NwResize,
    /// Southeast resize cursor.
    SeResize,
    /// Southwest resize cursor.
    SwResize,
    /// East-west resize cursor.
    EwResize,
    /// North-south resize cursor.
    NsResize,
    /// Northeast-southwest resize cursor.
    NeswResize,
    /// Zoom in cursor (magnifying glass with +).
    ZoomIn,
    /// Zoom out cursor (magnifying glass with -).
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

/// CSS positioning mode for elements.
///
/// Controls how an element is positioned in the layout.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum Position {
    /// Default positioning in the normal document flow. This is the default.
    #[default]
    Static,
    /// Positioned relative to its normal position.
    Relative,
    /// Positioned relative to its nearest positioned ancestor.
    Absolute,
    /// Positioned based on scroll position.
    Sticky,
    /// Positioned relative to the viewport.
    Fixed,
}

impl Position {
    /// Returns `true` if the position is relative to its parent container.
    ///
    /// Returns `true` for `Static`, `Relative`, and `Sticky`, which are positioned relative
    /// to their parent. Returns `false` for `Absolute` and `Fixed`, which break out of
    /// the normal flow.
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

/// Element visibility state.
///
/// Controls whether an element is visible or hidden (but still occupies space).
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum Visibility {
    /// Element is visible. This is the default.
    #[default]
    Visible,
    /// Element is hidden but still occupies layout space.
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

/// Image loading strategy.
///
/// Controls when images are loaded relative to page load.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum ImageLoading {
    /// Load image immediately. This is the default.
    #[default]
    Eager,
    /// Defer loading until the image is near the viewport.
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

/// How an image should fit within its container.
///
/// Controls the sizing behavior of images relative to their container.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum ImageFit {
    /// Use default sizing behavior. This is the default.
    #[default]
    Default,
    /// Scale to fit within container while preserving aspect ratio.
    Contain,
    /// Scale to cover entire container while preserving aspect ratio (may crop).
    Cover,
    /// Stretch to fill container (may distort aspect ratio).
    Fill,
    /// Do not resize the image.
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

/// Text decoration line style.
///
/// Controls the type of line decoration applied to text.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum TextDecorationLine {
    /// Inherit from parent element. This is the default.
    #[default]
    Inherit,
    /// No text decoration.
    None,
    /// Underline the text.
    Underline,
    /// Line above the text.
    Overline,
    /// Strike through the text.
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

/// Text decoration style.
///
/// Controls the visual style of text decoration lines.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum TextDecorationStyle {
    /// Inherit from parent element. This is the default.
    #[default]
    Inherit,
    /// Solid line.
    Solid,
    /// Double line.
    Double,
    /// Dotted line.
    Dotted,
    /// Dashed line.
    Dashed,
    /// Wavy line.
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

/// HTML link target attribute.
///
/// Controls where a linked document should open.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum LinkTarget {
    /// Open in the same frame/tab. This is the default.
    #[default]
    SelfTarget,
    /// Open in a new window or tab.
    Blank,
    /// Open in the parent frame.
    Parent,
    /// Open in the top-level frame.
    Top,
    /// Open in a named frame or window.
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

/// Font weight (thickness) values.
///
/// Supports both named weights and numeric weights from 100-900.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum FontWeight {
    /// Thin weight (typically 100).
    Thin,
    /// Extra light weight (typically 200).
    ExtraLight,
    /// Light weight (typically 300).
    Light,
    /// Normal/regular weight (typically 400). This is the default.
    #[default]
    Normal,
    /// Medium weight (typically 500).
    Medium,
    /// Semi-bold weight (typically 600).
    SemiBold,
    /// Bold weight (typically 700).
    Bold,
    /// Extra bold weight (typically 800).
    ExtraBold,
    /// Black/heavy weight (typically 900).
    Black,
    /// Lighter than parent element.
    Lighter,
    /// Bolder than parent element.
    Bolder,
    /// Numeric weight 100.
    Weight100,
    /// Numeric weight 200.
    Weight200,
    /// Numeric weight 300.
    Weight300,
    /// Numeric weight 400.
    Weight400,
    /// Numeric weight 500.
    Weight500,
    /// Numeric weight 600.
    Weight600,
    /// Numeric weight 700.
    Weight700,
    /// Numeric weight 800.
    Weight800,
    /// Numeric weight 900.
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

/// White space handling in text.
///
/// Controls how white space is collapsed and wrapped in text content.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum WhiteSpace {
    /// Collapse white space and wrap text normally. This is the default.
    #[default]
    Normal,
    /// Preserve white space and prevent wrapping.
    Preserve,
    /// Preserve white space but allow wrapping.
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

/// Text selection behavior.
///
/// Controls whether and how users can select text content.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum UserSelect {
    /// Default selection behavior. This is the default.
    #[default]
    Auto,
    /// Prevent text selection.
    None,
    /// Allow text selection.
    Text,
    /// Select all text on click.
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

/// Word breaking and wrapping behavior.
///
/// Controls how words break when they exceed the container width.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum OverflowWrap {
    /// Break only at normal break points (spaces, hyphens). This is the default.
    #[default]
    Normal,
    /// Break long words at arbitrary points to prevent overflow.
    BreakWord,
    /// Break at any character to prevent overflow.
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

/// Text overflow behavior.
///
/// Controls how overflowing text is displayed when it exceeds its container.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum TextOverflow {
    /// Clip the overflowing text. This is the default.
    #[default]
    Clip,
    /// Display an ellipsis (...) to indicate clipped text.
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
