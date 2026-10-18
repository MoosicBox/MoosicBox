//! Terminal-native layout of immutable `HyperChad` containers.
//!
//! Document lengths are resolved through `Number::calc` in logical document
//! units, then converted with axis-specific cell metrics. All returned geometry
//! is in terminal cells. Painting and interaction must use this tree rather than
//! recalculating geometry. Conditional overrides must be resolved by the caller.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use bmux_tui::component::{Constraints, LogicalSize};
use bmux_tui::geometry::{Rect, Size};
use bmux_tui::style::{Color, Modifier, Style};
use hyperchad_transformer::models::{
    AlignItems, FontWeight, JustifyContent, LayoutDirection, LayoutOverflow, OverflowWrap,
    Position, TextAlign, TextDecorationLine, TextOverflow, Visibility, WhiteSpace,
};
use hyperchad_transformer::{Container, Element, Input, Number};

mod inline;
mod table;

pub use inline::{InlineFragment, SourceRange, TextStyle};

/// Maximum logical extent, keeping signed translations and arithmetic bounded.
const MAX_EXTENT: u64 = i32::MAX as u64;

/// Logical document units occupied by one terminal cell on each axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
}

impl Default for TerminalMetrics {
    fn default() -> Self {
        Self {
            cell_width: 8.0,
            cell_height: 16.0,
        }
    }
}

/// Caller-owned scroll position, in logical cells, not terminal coordinates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScrollOffset {
    pub x: u64,
    pub y: u64,
}

/// Inputs affecting geometry. An embedded renderer supplies its allocated viewport.
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub viewport: Size,
    pub metrics: TerminalMetrics,
    pub scroll_offsets: BTreeMap<String, ScrollOffset>,
    pub open_details: BTreeMap<String, bool>,
    pub tab_width: u16,
    /// Recursion limit for untrusted documents.
    pub max_depth: usize,
    /// Limit on source containers and table grid slots.
    pub max_nodes: usize,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            viewport: Size::new(80, 24),
            metrics: TerminalMetrics::default(),
            scroll_offsets: BTreeMap::new(),
            open_details: BTreeMap::new(),
            tab_width: 8,
            max_depth: 256,
            max_nodes: 100_000,
        }
    }
}

/// Signed origin and large logical extent. Rectangles are half-open.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    pub x: i64,
    pub y: i64,
    pub width: u64,
    pub height: u64,
}

impl CellRect {
    #[must_use]
    pub const fn new(x: i64, y: i64, width: u64, height: u64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[must_use]
    pub fn right(self) -> i64 {
        self.x.saturating_add(signed(self.width))
    }

    #[must_use]
    pub fn bottom(self) -> i64 {
        self.y.saturating_add(signed(self.height))
    }

    #[must_use]
    pub fn contains(self, x: i64, y: i64) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self::new(
            x,
            y,
            positive(self.right().min(other.right()) - x),
            positive(self.bottom().min(other.bottom()) - y),
        )
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.width == 0 || self.height == 0 {
            return other;
        }
        if other.width == 0 || other.height == 0 {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            positive(self.right().max(other.right()) - x),
            positive(self.bottom().max(other.bottom()) - y),
        )
    }

    #[must_use]
    pub const fn translated(self, x: i64, y: i64) -> Self {
        Self {
            x: self.x.saturating_add(x),
            y: self.y.saturating_add(y),
            ..self
        }
    }

    /// Clip to terminal coordinates only at the presentation boundary.
    #[must_use]
    pub fn terminal(self) -> Rect {
        let clipped = self.intersection(Self::new(0, 0, u64::from(u16::MAX), u64::from(u16::MAX)));
        Rect::new(
            u16::try_from(clipped.x.clamp(0, i64::from(u16::MAX))).unwrap_or(0),
            u16::try_from(clipped.y.clamp(0, i64::from(u16::MAX))).unwrap_or(0),
            u16::try_from(clipped.width).unwrap_or(u16::MAX),
            u16::try_from(clipped.height).unwrap_or(u16::MAX),
        )
    }
}

/// Authoritative source-node geometry, in viewport coordinates.
#[derive(Debug, Clone)]
pub struct ResolvedNode {
    pub key: String,
    pub path: Vec<usize>,
    pub parent_key: Option<String>,
    pub children: Vec<String>,
    /// Border box, excluding margins. Inline span bounds are the union of
    /// fragments; use `hit_regions` rather than filling this rectangle for text.
    pub bounds: CellRect,
    /// Ancestor/viewport clip. Overflowing descendants may exceed `bounds`.
    pub clip: CellRect,
    /// Content viewport before scrolling; excludes border and padding.
    pub content_bounds: CellRect,
    /// Clip for this node's inline content, including its own overflow policy.
    pub text_clip: CellRect,
    /// Unscrolled content extent, including overflowing descendants.
    pub content_extents: LogicalSize,
    pub scroll_offset: ScrollOffset,
    /// Viewport origin of unscrolled inline/content coordinates.
    pub text_origin: (i64, i64),
    pub paint_order: usize,
    pub visible: bool,
    /// Exact inline hit regions; a multiline span's bounding box includes gaps.
    pub hit_regions: Vec<CellRect>,
    /// Only the owning formatting context paints these fragments.
    pub text: Vec<InlineFragment>,
    pub style: TextStyle,
}

/// Retained scene shared by painting, selection, hit testing and image placement.
#[derive(Debug, Clone)]
pub struct LayoutTree {
    pub root_key: String,
    pub viewport: Size,
    pub nodes: Vec<ResolvedNode>,
    indices: BTreeMap<String, usize>,
}

impl LayoutTree {
    // Option already carries must_use; avoid a redundant function attribute.
    #[allow(clippy::must_use_candidate)]
    pub fn get(&self, key: &str) -> Option<&ResolvedNode> {
        self.indices.get(key).map(|&index| &self.nodes[index])
    }

    // Option already carries must_use; avoid a redundant function attribute.
    #[allow(clippy::must_use_candidate)]
    pub fn at_path(&self, path: &[usize]) -> Option<&ResolvedNode> {
        self.nodes.iter().find(|node| node.path == path)
    }

    /// Topmost visible source node under a viewport cell.
    // Option already carries must_use; avoid a redundant function attribute.
    #[allow(clippy::must_use_candidate)]
    pub fn hit_test(&self, x: i64, y: i64) -> Option<&ResolvedNode> {
        self.nodes.iter().rev().find(|node| {
            node.visible
                && node.clip.contains(x, y)
                && if node.hit_regions.is_empty() {
                    node.bounds.contains(x, y)
                } else {
                    node.hit_regions.iter().any(|rect| rect.contains(x, y))
                }
        })
    }
}

/// Invalid source geometry or an exhausted document resource bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    InvalidMetrics,
    InvalidLength { key: String },
    DuplicateKey(String),
    DepthLimit,
    NodeLimit,
    InvalidTableSpan { key: String },
    InvalidTableStructure { key: String },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMetrics => {
                f.write_str("cell metrics must be positive and finite; tab width must be nonzero")
            }
            Self::InvalidLength { key } => {
                write!(f, "nonfinite or excessive layout length at {key}")
            }
            Self::DuplicateKey(key) => write!(f, "duplicate layout node key: {key}"),
            Self::DepthLimit => f.write_str("layout recursion limit exceeded"),
            Self::NodeLimit => f.write_str("layout node/grid limit exceeded"),
            Self::InvalidTableSpan { key } => {
                write!(f, "table span must be a positive integer at {key}")
            }
            Self::InvalidTableStructure { key } => {
                write!(f, "unexpected element in table structure at {key}")
            }
        }
    }
}

impl std::error::Error for LayoutError {}

/// Stable identity: string ID, nonzero numeric ID, then structural path.
#[must_use]
pub fn node_key(container: &Container, path: &[usize]) -> String {
    if let Some(id) = &container.str_id {
        return id.clone();
    }
    if container.id != 0 {
        return container.id.to_string();
    }
    format!(
        "@{}",
        path.iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("/")
    )
}

/// Native controls and decoded media supply their real intrinsic measurements.
/// Implementations must be deterministic for a given source and constraints.
pub trait LeafMeasure {
    fn measure(&self, container: &Container, constraints: Constraints) -> LogicalSize;
}

impl<F: Fn(&Container, Constraints) -> LogicalSize> LeafMeasure for F {
    fn measure(&self, container: &Container, constraints: Constraints) -> LogicalSize {
        self(container, constraints)
    }
}

/// Unicode-aware default measurements, including terminal-native control chrome.
///
/// Image/canvas intrinsic dimensions should be supplied by the media measurer;
/// without decoded metadata an image reserves its alternative text's geometry.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultLeafMeasure;

impl LeafMeasure for DefaultLeafMeasure {
    fn measure(&self, container: &Container, constraints: Constraints) -> LogicalSize {
        let label = plain_text(container);
        let (text, chrome, minimum, rows) = match &container.element {
            Element::Input {
                input: Input::Hidden { .. },
                ..
            } => return LogicalSize::new(0, 0),
            Element::Input {
                input: Input::Checkbox { .. },
                ..
            } => (String::new(), 3, 3, 1),
            Element::Input {
                input: Input::Text { value, placeholder },
                ..
            } => (
                value
                    .as_ref()
                    .or(placeholder.as_ref())
                    .cloned()
                    .unwrap_or_default(),
                2,
                12,
                1,
            ),
            Element::Input {
                input: Input::Password { value, placeholder },
                ..
            } => {
                use unicode_segmentation::UnicodeSegmentation;
                (
                    value.as_ref().map_or_else(
                        || placeholder.clone().unwrap_or_default(),
                        |value| "•".repeat(value.graphemes(true).count()),
                    ),
                    2,
                    12,
                    1,
                )
            }
            Element::Input {
                input: Input::File, ..
            } => ("Choose file".into(), 4, 0, 1),
            Element::Button { .. } | Element::Select { .. } => (label, 4, 4, 1),
            Element::Image { alt, .. } => (alt.clone().unwrap_or_default(), 0, 1, 1),
            Element::Textarea {
                value, placeholder, ..
            } => (
                if value.is_empty() {
                    placeholder.clone().unwrap_or_default()
                } else {
                    value.clone()
                },
                0,
                12,
                3,
            ),
            _ => (label, 0, 0, 0),
        };
        let width = constraints.max_width().saturating_sub(chrome);
        let size = inline::plain_size(
            &text,
            width,
            container.white_space.unwrap_or_default(),
            container.overflow_wrap.unwrap_or_default(),
            8,
        );
        constraints.constrain(LogicalSize::new(
            (size.width + chrome).max(minimum),
            size.height.max(rows),
        ))
    }
}

/// Resolve a canonical container without changing source or calculated fields.
///
/// # Errors
/// * Invalid/nonfinite metrics or lengths, duplicate identities, invalid table
///   spans, or configured recursion/node limits return [`LayoutError`].
pub fn layout(
    root: &Container,
    options: &LayoutOptions,
    measurer: &impl LeafMeasure,
) -> Result<LayoutTree, LayoutError> {
    if !options.metrics.cell_width.is_finite()
        || options.metrics.cell_width <= 0.0
        || !options.metrics.cell_height.is_finite()
        || options.metrics.cell_height <= 0.0
        || options.tab_width == 0
    {
        return Err(LayoutError::InvalidMetrics);
    }
    validate(root, &mut Vec::new(), &mut BTreeSet::new(), &mut 0, options)?;
    let mut engine = Engine {
        options,
        measurer,
        extents: BTreeMap::new(),
        unscrolled: true,
    };
    let viewport = LogicalSize::new(
        u64::from(options.viewport.width),
        u64::from(options.viewport.height),
    );
    let draft = engine.node(
        root,
        &[],
        viewport,
        Some(viewport.width),
        Some(viewport.height),
        &TextStyle::default(),
        0,
    )?;
    let root_key = node_key(root, &[]);
    let mut tree = LayoutTree {
        root_key,
        viewport: options.viewport,
        nodes: Vec::new(),
        indices: BTreeMap::new(),
    };
    if let Some(draft) = draft {
        let rect = CellRect::new(0, 0, viewport.width, viewport.height);
        engine.resolve(draft.clone(), (0, 0), rect, rect, rect, None, &mut tree)?;
        engine.extents = scroll_extents(root, &tree);
        engine.unscrolled = false;
        tree.nodes.clear();
        tree.indices.clear();
        engine.resolve(draft, (0, 0), rect, rect, rect, None, &mut tree)?;
    }
    Ok(tree)
}

/// Reproject one changed scrollport using committed geometry. Fixed/sticky and
/// unproven absolute descendants require normal layout; source/sizing must be unchanged.
pub(crate) fn reproject_scroll(
    root: &Container,
    tree: &LayoutTree,
    offsets: &BTreeMap<String, ScrollOffset>,
) -> Option<LayoutTree> {
    let mut changed = None;
    for node in &tree.nodes {
        let requested = offsets.get(&node.key).copied().unwrap_or_default();
        let source = source_at(root, &node.path)?;
        let next = ScrollOffset {
            x: if scrollable(source.overflow_x) {
                requested.x.min(
                    node.content_extents
                        .width
                        .saturating_sub(node.content_bounds.width),
                )
            } else {
                0
            },
            y: if scrollable(source.overflow_y) {
                requested.y.min(
                    node.content_extents
                        .height
                        .saturating_sub(node.content_bounds.height),
                )
            } else {
                0
            },
        };
        if next != node.scroll_offset {
            if changed.is_some() {
                log::debug!(
                    "bmux scroll rejection reason=multiple-scrollports key={:.128}",
                    node.key
                );
                return None;
            }
            changed = Some((node.path.clone(), next, node.scroll_offset));
        }
    }
    let (path, next, previous) = changed?;
    for node in &tree.nodes {
        if !node.path.starts_with(&path) || node.path == path {
            continue;
        }
        let source = source_at(root, &node.path)?;
        match source.position.unwrap_or_default() {
            Position::Fixed | Position::Sticky => {
                log::debug!(
                    "bmux scroll rejection reason=fixed-or-sticky key={:.128}",
                    node.key
                );
                return None;
            }
            Position::Absolute if !scrolls_absolute(root, &node.path, &path)? => {
                log::debug!(
                    "bmux scroll rejection reason=absolute-dependency key={:.128} fixed_width={} fixed_height={}",
                    node.key,
                    matches!(source.width, Some(Number::Integer(_))),
                    matches!(source.height, Some(Number::Integer(_)))
                );
                return None;
            }
            _ => {}
        }
    }
    let dx = signed(previous.x) - signed(next.x);
    let dy = signed(previous.y) - signed(next.y);
    let mut result = tree.clone();
    for index in 0..result.nodes.len() {
        if !result.nodes[index].path.starts_with(&path) {
            continue;
        }
        let is_scrollport = result.nodes[index].path == path;
        let inherited = result.nodes[index]
            .parent_key
            .as_ref()
            .and_then(|key| result.get(key))
            .map(|parent| parent.text_clip);
        let node = &mut result.nodes[index];
        if is_scrollport {
            node.scroll_offset = next;
        } else {
            node.bounds = node.bounds.translated(dx, dy);
            node.content_bounds = node.content_bounds.translated(dx, dy);
            for region in &mut node.hit_regions {
                *region = region.translated(dx, dy);
            }
            node.clip = inherited?;
        }
        node.text_origin.0 += dx;
        node.text_origin.1 += dy;
        for fragment in &mut node.text {
            fragment.bounds = fragment.bounds.translated(dx, dy);
        }
        let source = source_at(root, &node.path)?;
        node.text_clip = node.clip;
        if clips(source.overflow_x) {
            node.text_clip = clip_axis(node.text_clip, node.content_bounds, true);
        }
        if clips(source.overflow_y) {
            node.text_clip = clip_axis(node.text_clip, node.content_bounds, false);
        }
    }
    Some(result)
}

// A fixed-size overlay can translate only when its positioned containing block
// moves inside the changed scrollport. Dynamic lengths retain the full reflow path.
fn scrolls_absolute(root: &Container, node: &[usize], scrollport: &[usize]) -> Option<bool> {
    let source = source_at(root, node)?;
    let intrinsic_height = source.height.is_none()
        && source.bottom.is_none()
        && intrinsic_scroll_subtree(source, true);
    if !matches!(source.width, Some(Number::Integer(_)))
        || !(matches!(source.height, Some(Number::Integer(_))) || intrinsic_height)
        || [&source.left, &source.right, &source.top, &source.bottom]
            .into_iter()
            .any(|length| match length {
                None | Some(Number::Integer(_) | Number::IntegerPercent(_)) => false,
                Some(Number::RealPercent(value)) => !value.is_finite(),
                _ => true,
            })
        || [
            &source.min_width,
            &source.max_width,
            &source.min_height,
            &source.max_height,
            &source.margin_left,
            &source.margin_right,
            &source.margin_top,
            &source.margin_bottom,
            &source.padding_left,
            &source.padding_right,
            &source.padding_top,
            &source.padding_bottom,
        ]
        .into_iter()
        .any(|length| !matches!(length, None | Some(Number::Integer(_))))
    {
        return Some(false);
    }
    let anchor = (0..node.len()).rev().find(|&length| {
        source_at(root, &node[..length])
            .is_some_and(|ancestor| ancestor.position.unwrap_or_default() != Position::Static)
    })?;
    if anchor <= scrollport.len() {
        return Some(false);
    }
    if intrinsic_height
        || [&source.left, &source.right, &source.top, &source.bottom]
            .into_iter()
            .any(|length| {
                matches!(
                    length,
                    Some(Number::IntegerPercent(_) | Number::RealPercent(_))
                )
            })
    {
        let containing = source_at(root, &node[..anchor])?;
        // Percentage insets use the positioned content-box dimensions. Prove
        // those bases fixed, not merely the overlay's own border-box size.
        if ![&containing.width, &containing.height]
            .into_iter()
            .all(|length| matches!(length, Some(Number::Integer(_))))
            || [
                &containing.min_width,
                &containing.max_width,
                &containing.min_height,
                &containing.max_height,
                &containing.padding_left,
                &containing.padding_right,
                &containing.padding_top,
                &containing.padding_bottom,
            ]
            .into_iter()
            .any(|length| !matches!(length, None | Some(Number::Integer(_))))
        {
            return Some(false);
        }
    }
    Some(true)
}

// Intrinsic height is translation invariant only for a fixed-width, nonflex
// flow subtree with fixed local lengths and no positioned/scrolling descendants.
fn intrinsic_scroll_subtree(node: &Container, overlay: bool) -> bool {
    let fixed = |value: Option<&Number>| match value {
        None | Some(Number::Integer(_)) => true,
        Some(Number::Real(value)) => value.is_finite(),
        _ => false,
    };
    (overlay || node.position.unwrap_or_default() == Position::Static)
        && node.flex.is_none()
        && !scrollable(node.overflow_x)
        && !scrollable(node.overflow_y)
        && [
            node.width.as_ref(),
            node.height.as_ref(),
            node.min_width.as_ref(),
            node.max_width.as_ref(),
            node.min_height.as_ref(),
            node.max_height.as_ref(),
            node.margin_left.as_ref(),
            node.margin_right.as_ref(),
            node.margin_top.as_ref(),
            node.margin_bottom.as_ref(),
            node.padding_left.as_ref(),
            node.padding_right.as_ref(),
            node.padding_top.as_ref(),
            node.padding_bottom.as_ref(),
            node.column_gap.as_ref(),
            node.row_gap.as_ref(),
            node.font_size.as_ref(),
            node.border_left.as_ref().map(|(_, width)| width),
            node.border_right.as_ref().map(|(_, width)| width),
            node.border_top.as_ref().map(|(_, width)| width),
            node.border_bottom.as_ref().map(|(_, width)| width),
        ]
        .into_iter()
        .all(fixed)
        && node
            .children
            .iter()
            .all(|child| intrinsic_scroll_subtree(child, false))
}

fn source_at<'a>(root: &'a Container, path: &[usize]) -> Option<&'a Container> {
    path.iter()
        .try_fold(root, |node, index| node.children.get(*index))
}

/// Collect scrollable overflow from authoritative, unscrolled placement. This
/// includes positioned descendants through static ancestors, stops each axis at
/// clipping boundaries, and never propagates viewport-fixed subtrees.
fn scroll_extents(root: &Container, tree: &LayoutTree) -> BTreeMap<String, LogicalSize> {
    let mut extents = BTreeMap::new();
    let mut overflow = BTreeMap::<String, CellRect>::new();
    for node in tree.nodes.iter().rev() {
        let mut source = root;
        for &index in &node.path {
            source = &source.children[index];
        }
        let mut content = CellRect::new(
            node.content_bounds.x,
            node.content_bounds.y,
            node.content_extents.width,
            node.content_extents.height,
        );
        for child_key in &node.children {
            let Some(child) = tree.get(child_key) else {
                continue;
            };
            let mut child_source = root;
            for &index in &child.path {
                child_source = &child_source.children[index];
            }
            if child_source.position == Some(Position::Fixed) {
                continue;
            }
            if let Some(rect) = overflow.get(child_key) {
                content = content.union(*rect);
            }
        }
        let size = LogicalSize::new(
            node.content_extents
                .width
                .max(positive(content.right() - node.content_bounds.x)),
            node.content_extents
                .height
                .max(positive(content.bottom() - node.content_bounds.y)),
        );
        extents.insert(node.key.clone(), size);
        // The parent's extent includes this border box even when this node
        // clips its own content. Negative overflow never creates negative scroll.
        if clips(source.overflow_x) {
            content = clip_axis(content, node.content_bounds, true);
        }
        if clips(source.overflow_y) {
            content = clip_axis(content, node.content_bounds, false);
        }
        overflow.insert(node.key.clone(), node.bounds.union(content));
    }
    extents
}

/// Resolve either sticky edge; start wins if both constraints cannot fit.
// Both axes use the same explicit viewport, parent, and inset constraints.
#[allow(clippy::too_many_arguments)]
fn sticky_axis(
    normal: i64,
    size: u64,
    viewport_start: i64,
    viewport_size: u64,
    parent_start: i64,
    parent_size: u64,
    start: Option<i64>,
    end: Option<i64>,
) -> i64 {
    if start.is_none() && end.is_none() {
        return normal;
    }
    let mut position = normal;
    if let Some(end) = end {
        position = position.min(viewport_start + signed(viewport_size) - end - signed(size));
    }
    if let Some(start) = start {
        position = position.max(viewport_start + start);
    }
    let parent_end = parent_start
        .saturating_add(signed(parent_size))
        .saturating_sub(signed(size));
    position.clamp(parent_start, parent_end.max(parent_start))
}

fn validate(
    node: &Container,
    path: &mut Vec<usize>,
    keys: &mut BTreeSet<String>,
    count: &mut usize,
    options: &LayoutOptions,
) -> Result<(), LayoutError> {
    if path.len() > options.max_depth {
        return Err(LayoutError::DepthLimit);
    }
    *count += 1;
    if *count > options.max_nodes {
        return Err(LayoutError::NodeLimit);
    }
    let key = node_key(node, path);
    if !keys.insert(key.clone()) {
        return Err(LayoutError::DuplicateKey(key));
    }
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        validate(child, path, keys, count, options)?;
        path.pop();
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
struct Edges {
    top: i64,
    right: i64,
    bottom: i64,
    left: i64,
}
impl Edges {
    fn horizontal(self) -> u64 {
        positive(self.left + self.right)
    }
    fn vertical(self) -> u64 {
        positive(self.top + self.bottom)
    }
}

#[derive(Clone)]
struct Draft<'a> {
    source: &'a Container,
    path: Vec<usize>,
    key: String,
    rect: CellRect,
    content: CellRect,
    extents: LogicalSize,
    margin: Edges,
    children: Vec<Self>,
    text: Vec<InlineFragment>,
    regions: Vec<CellRect>,
    style: TextStyle,
}
impl Draft<'_> {
    fn outer_width(&self) -> u64 {
        positive(signed(self.rect.width) + self.margin.left + self.margin.right)
    }
    fn outer_height(&self) -> u64 {
        positive(signed(self.rect.height) + self.margin.top + self.margin.bottom)
    }
}

struct Engine<'a, M> {
    options: &'a LayoutOptions,
    measurer: &'a M,
    extents: BTreeMap<String, LogicalSize>,
    unscrolled: bool,
}

impl<M: LeafMeasure> Engine<'_, M> {
    fn length(
        &self,
        value: &Number,
        base: u64,
        horizontal: bool,
        key: &str,
    ) -> Result<f64, LayoutError> {
        let metrics = self.options.metrics;
        let scale = if horizontal {
            metrics.cell_width
        } else {
            metrics.cell_height
        };
        let value = value.calc(
            document_float(base) * scale,
            f32::from(self.options.viewport.width) * metrics.cell_width,
            f32::from(self.options.viewport.height) * metrics.cell_height,
        ) / scale;
        if !value.is_finite() || value.abs() > document_float(MAX_EXTENT) {
            return Err(LayoutError::InvalidLength { key: key.into() });
        }
        Ok(f64::from(value))
    }

    fn dimension(
        &self,
        value: Option<&Number>,
        base: u64,
        horizontal: bool,
        key: &str,
    ) -> Result<Option<u64>, LayoutError> {
        value
            .map(|value| self.length(value, base, horizontal, key).map(rounded_cells))
            .transpose()
    }

    fn edges(
        &self,
        node: &Container,
        parent: LogicalSize,
        border: bool,
        margin: bool,
        key: &str,
    ) -> Result<Edges, LayoutError> {
        let values = if border {
            [
                node.border_top.as_ref().map(|x| &x.1),
                node.border_right.as_ref().map(|x| &x.1),
                node.border_bottom.as_ref().map(|x| &x.1),
                node.border_left.as_ref().map(|x| &x.1),
            ]
        } else if margin {
            [
                node.margin_top.as_ref(),
                node.margin_right.as_ref(),
                node.margin_bottom.as_ref(),
                node.margin_left.as_ref(),
            ]
        } else {
            [
                node.padding_top.as_ref(),
                node.padding_right.as_ref(),
                node.padding_bottom.as_ref(),
                node.padding_left.as_ref(),
            ]
        };
        let mut resolved = [0_i64; 4];
        for (index, value) in values.into_iter().enumerate() {
            if let Some(value) = value {
                let horizontal = index % 2 == 1;
                let size = self.length(
                    value,
                    if horizontal {
                        parent.width
                    } else {
                        parent.height
                    },
                    horizontal,
                    key,
                )?;
                resolved[index] = if border {
                    signed_cells(size.max(0.0).ceil())
                } else if margin {
                    signed_cells(size.round())
                } else {
                    signed_cells(size.max(0.0).round())
                };
            }
        }
        Ok(Edges {
            top: resolved[0],
            right: resolved[1],
            bottom: resolved[2],
            left: resolved[3],
        })
    }

    fn limits(
        &self,
        node: &Container,
        parent: LogicalSize,
        horizontal: bool,
        key: &str,
    ) -> Result<(u64, u64), LayoutError> {
        let (min, max, base) = if horizontal {
            (
                node.min_width.as_ref(),
                node.max_width.as_ref(),
                parent.width,
            )
        } else {
            (
                node.min_height.as_ref(),
                node.max_height.as_ref(),
                parent.height,
            )
        };
        let min = self.dimension(min, base, horizontal, key)?.unwrap_or(0);
        let max = self
            .dimension(max, base, horizontal, key)?
            .unwrap_or(MAX_EXTENT)
            .max(min);
        Ok((min, max))
    }

    // Keep box resolution and its mutually exclusive content algorithms in one pass.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn node<'a>(
        &self,
        node: &'a Container,
        path: &[usize],
        available: LogicalSize,
        forced_width: Option<u64>,
        forced_height: Option<u64>,
        inherited: &TextStyle,
        depth: usize,
    ) -> Result<Option<Draft<'a>>, LayoutError> {
        if depth > self.options.max_depth {
            return Err(LayoutError::DepthLimit);
        }
        if hidden(node) {
            return Ok(None);
        }
        let key = node_key(node, path);
        let style = inherited.inherit(node);
        let margin = self.edges(node, available, false, true, &key)?;
        let padding = self.edges(node, available, false, false, &key)?;
        let border = self.edges(node, available, true, false, &key)?;
        let inset = Edges {
            top: padding.top + border.top,
            right: padding.right + border.right,
            bottom: padding.bottom + border.bottom,
            left: padding.left + border.left,
        };
        let width = match (node.width.as_ref(), forced_width) {
            (Some(value), Some(forced)) if node.flex.is_none() && value.is_dynamic() => {
                Some(forced.saturating_sub(inset.horizontal()))
            }
            _ => self.dimension(node.width.as_ref(), available.width, true, &key)?,
        };
        let height = self.dimension(node.height.as_ref(), available.height, false, &key)?;
        let (min_width, max_width) = self.limits(node, available, true, &key)?;
        let (min_height, max_height) = self.limits(node, available, false, &key)?;
        let content_width = (if node.flex.is_some() {
            forced_width
                .map(|size| size.saturating_sub(inset.horizontal()))
                .or(width)
        } else {
            width.or_else(|| forced_width.map(|size| size.saturating_sub(inset.horizontal())))
        })
        .unwrap_or_else(|| {
            available
                .width
                .saturating_sub(margin.horizontal() + inset.horizontal())
        })
        .clamp(min_width, max_width);
        let requested_height = if node.flex.is_some() {
            forced_height
                .map(|size| size.saturating_sub(inset.vertical()))
                .or(height)
        } else {
            height.or_else(|| forced_height.map(|size| size.saturating_sub(inset.vertical())))
        };
        let content_height = requested_height
            .unwrap_or_else(|| available.height.saturating_sub(inset.vertical()))
            .clamp(min_height, max_height);
        let content_size = LogicalSize::new(content_width, content_height);
        let mut draft = Draft {
            source: node,
            key,
            path: path.into(),
            rect: CellRect::default(),
            content: CellRect::new(inset.left, inset.top, content_width, content_height),
            extents: LogicalSize::default(),
            margin,
            children: Vec::new(),
            text: Vec::new(),
            regions: Vec::new(),
            style,
        };
        if native_leaf(node) {
            let measured = self
                .measurer
                .measure(node, Constraints::new(0, content_width, 0, None));
            if measured.width > MAX_EXTENT || measured.height > MAX_EXTENT {
                return Err(LayoutError::InvalidLength { key: draft.key });
            }
            let measured = if matches!(node.element, Element::Image { .. } | Element::Canvas) {
                // Decoded pixels are intrinsic input only for auto-sized axes.
                // Explicit image boxes contain their payload; pixel dimensions
                // must not become descendant overflow or change ancestor flow.
                LogicalSize::new(
                    if width.is_some() {
                        content_width
                    } else {
                        measured.width
                    },
                    if requested_height.is_some() {
                        content_height
                    } else {
                        measured.height
                    },
                )
            } else {
                measured
            };
            draft.extents = measured;
            if width.is_none() && node.flex.is_none() {
                draft.content.width = measured
                    .width
                    .min(content_width)
                    .clamp(min_width, max_width);
            }
            if let Element::Textarea { rows, cols, .. } = &node.element {
                if width.is_none() {
                    draft.content.width = self
                        .dimension(cols.as_ref(), available.width, true, &draft.key)?
                        .unwrap_or(draft.content.width)
                        .clamp(min_width, max_width);
                }
                if requested_height.is_none() {
                    draft.content.height = self
                        .dimension(rows.as_ref(), available.height, false, &draft.key)?
                        .unwrap_or(measured.height)
                        .clamp(min_height, max_height);
                }
            }
        } else if matches!(node.element, Element::Table) {
            self.table(&mut draft, content_size, depth)?;
        } else if is_inline(node)
            || matches!(node.element, Element::Raw { .. } | Element::Text { .. })
        {
            let result = inline::format(
                &[(node, path.to_vec())],
                content_width,
                &draft.style,
                self.options,
            );
            draft.extents = result.size;
            draft.text = result.fragments;
            // The formatting-context node itself is already represented here.
            draft.children = Self::inline_nodes(&result.sources, &mut draft.text, Some(path));
        } else {
            self.flow(&mut draft, content_size, requested_height.is_some(), depth)?;
        }
        if width.is_none() && forced_width.is_none() && !native_leaf(node) {
            draft.content.width = draft.extents.width.clamp(min_width, max_width);
        }
        if requested_height.is_none() && !matches!(node.element, Element::Textarea { .. }) {
            draft.content.height = draft.extents.height.clamp(min_height, max_height);
        }
        draft.rect.width = draft.content.width + inset.horizontal();
        draft.rect.height = draft.content.height + inset.vertical();
        if draft.rect.width > MAX_EXTENT || draft.rect.height > MAX_EXTENT {
            return Err(LayoutError::InvalidLength { key: draft.key });
        }
        Ok(Some(draft))
    }

    fn intrinsic(
        &self,
        node: &Container,
        path: &[usize],
        available: LogicalSize,
        style: &TextStyle,
        depth: usize,
    ) -> Result<u64, LayoutError> {
        if hidden(node) {
            return Ok(0);
        }
        let key = node_key(node, path);
        let margin = self.edges(node, available, false, true, &key)?;
        let padding = self.edges(node, available, false, false, &key)?;
        let border = self.edges(node, available, true, false, &key)?;
        let decoration = padding.horizontal() + border.horizontal();
        if let Some(width) = self.dimension(node.width.as_ref(), available.width, true, &key)? {
            let (min, max) = self.limits(node, available, true, &key)?;
            return Ok(width.clamp(min, max) + decoration + margin.horizontal());
        }
        if depth > self.options.max_depth {
            return Err(LayoutError::DepthLimit);
        }
        let width = if native_leaf(node) {
            self.measurer
                .measure(node, Constraints::new(0, MAX_EXTENT, 0, None))
                .width
        } else if is_inline(node) {
            inline::format(
                &[(node, path.into())],
                MAX_EXTENT,
                &style.inherit(node),
                self.options,
            )
            .size
            .width
        } else {
            let mut sizes = Vec::new();
            let mut inline_group = Vec::new();
            for (index, child) in self.active_children(node, path) {
                let child_path = child_path(path, index);
                if out_of_flow(child) {
                    continue;
                }
                if is_inline(child) {
                    inline_group.push((child, child_path));
                } else {
                    if !inline_group.is_empty() {
                        sizes.push(
                            inline::format(
                                &inline_group,
                                MAX_EXTENT,
                                &style.inherit(node),
                                self.options,
                            )
                            .size
                            .width,
                        );
                        inline_group.clear();
                    }
                    sizes.push(self.intrinsic(
                        child,
                        &child_path,
                        available,
                        &style.inherit(node),
                        depth + 1,
                    )?);
                }
            }
            if !inline_group.is_empty() {
                sizes.push(
                    inline::format(
                        &inline_group,
                        MAX_EXTENT,
                        &style.inherit(node),
                        self.options,
                    )
                    .size
                    .width,
                );
            }
            if node.direction == LayoutDirection::Row || matches!(node.element, Element::TR) {
                let gap = self
                    .dimension(node.column_gap.as_ref(), available.width, true, &key)?
                    .unwrap_or(0);
                sizes.iter().sum::<u64>() + gap * sizes.len().saturating_sub(1) as u64
            } else {
                sizes.into_iter().max().unwrap_or(0)
            }
        };
        let (min, max) = self.limits(node, available, true, &key)?;
        Ok(width
            .clamp(min, max)
            .saturating_add(decoration + margin.horizontal())
            .min(MAX_EXTENT))
    }

    fn active_children<'a>(
        &self,
        node: &'a Container,
        path: &[usize],
    ) -> Vec<(usize, &'a Container)> {
        let collapsed = if let Element::Details { open } = node.element {
            !self
                .options
                .open_details
                .get(&node_key(node, path))
                .copied()
                .unwrap_or_else(|| open.unwrap_or(false))
        } else {
            false
        };
        node.children
            .iter()
            .enumerate()
            .filter(|(_, child)| !hidden(child))
            .filter(|(_, child)| !collapsed || matches!(child.element, Element::Summary))
            .collect()
    }

    // Flow placement shares measured items across flex, wrapping, and alignment phases.
    #[allow(clippy::too_many_lines)]
    fn flow<'a>(
        &self,
        draft: &mut Draft<'a>,
        available: LogicalSize,
        definite_height: bool,
        depth: usize,
    ) -> Result<(), LayoutError> {
        let source = draft.source;
        let row = source.direction == LayoutDirection::Row;
        let gap_x = self
            .dimension(
                source.column_gap.as_ref(),
                available.width,
                true,
                &draft.key,
            )?
            .unwrap_or(0);
        let gap_y = self
            .dimension(source.row_gap.as_ref(), available.height, false, &draft.key)?
            .unwrap_or(0);
        let wrap = matches!(source.overflow_x, LayoutOverflow::Wrap { .. });
        let vertical_wrap =
            !row && !wrap && matches!(source.overflow_y, LayoutOverflow::Wrap { .. });
        let grid = matches!(source.overflow_x, LayoutOverflow::Wrap { grid: true });
        let grid_width = self.dimension(
            source.grid_cell_size.as_ref(),
            available.width,
            true,
            &draft.key,
        )?;
        let active = self.active_children(source, &draft.path);
        let normal: Vec<_> = active
            .iter()
            .copied()
            .filter(|(_, child)| !out_of_flow(child))
            .collect();
        let mut items = Vec::<FlowItem<'a>>::new();
        let mut index = 0;
        while index < normal.len() {
            let (child_index, child) = normal[index];
            if is_inline(child) {
                let mut group = Vec::new();
                while index < normal.len() && is_inline(normal[index].1) {
                    let (child_index, child) = normal[index];
                    let mut path = draft.path.clone();
                    path.push(child_index);
                    group.push((child, path));
                    index += 1;
                }
                let natural = inline::format(&group, MAX_EXTENT, &draft.style, self.options)
                    .size
                    .width;
                items.push(FlowItem::Inline { group, natural });
            } else {
                let mut path = draft.path.clone();
                path.push(child_index);
                let natural = self.intrinsic(child, &path, available, &draft.style, depth + 1)?;
                items.push(FlowItem::Block {
                    source: child,
                    path,
                    natural,
                });
                index += 1;
            }
        }
        let mut widths: Vec<u64> = items.iter().map(FlowItem::natural).collect();
        if row || wrap {
            let mut boundary = 0.0_f64;
            let mut previous = 0_u64;
            for (index, item) in items.iter().enumerate() {
                let exact = if let FlowItem::Block { source, path, .. } = item {
                    if let Some(value) = source.width.as_ref().filter(|value| value.is_dynamic()) {
                        let key = node_key(source, path);
                        let rounded = self
                            .dimension(Some(value), available.width, true, &key)?
                            .unwrap_or(0);
                        allocation_float(widths[index]) - allocation_float(rounded)
                            + self.length(value, available.width, true, &key)?.max(0.0)
                    } else {
                        allocation_float(widths[index])
                    }
                } else {
                    allocation_float(widths[index])
                };
                boundary += exact;
                let next = rounded_cells(boundary);
                widths[index] = next.saturating_sub(previous);
                previous = next;
            }
        }
        let can_shrink = source.overflow_x == LayoutOverflow::Squash
            || source.children.iter().any(|child| child.flex.is_some());
        if row && !wrap {
            self.flex_widths(
                &items,
                &mut widths,
                available,
                gap_x,
                &draft.key,
                can_shrink,
            )?;
        }
        if grid {
            let cell = grid_width
                .unwrap_or_else(|| widths.iter().copied().max().unwrap_or(1))
                .max(1);
            widths.fill(cell);
        }
        let mut x = 0_u64;
        let mut y = 0_u64;
        let mut line_height = 0_u64;
        let mut lines: Vec<Vec<(usize, u64, u64)>> = vec![Vec::new()];
        let mut all = Vec::<Placed<'a>>::new();
        for (index, item) in items.into_iter().enumerate() {
            let item_width = if row || wrap {
                widths[index]
            } else {
                available.width
            };
            if wrap && x > 0 && x.saturating_add(item_width) > available.width {
                y += line_height + gap_y;
                x = 0;
                line_height = 0;
                lines.push(Vec::new());
            }
            let placed = match item {
                FlowItem::Block { source, path, .. } => {
                    let margin =
                        self.edges(source, available, false, true, &node_key(source, &path))?;
                    let force = item_width.saturating_sub(margin.horizontal());
                    let child = self
                        .node(
                            source,
                            &path,
                            available,
                            if !(row || wrap) && source.width.is_some() {
                                None
                            } else if row || grid || draft.source.align_items.is_none() {
                                Some(force)
                            } else {
                                Some(force.min(widths[index].saturating_sub(margin.horizontal())))
                            },
                            // Only a definite, single-line row establishes a cross-axis
                            // constraint here. An auto-height row must first measure its
                            // contents; otherwise nested rows inherit viewport height.
                            if row
                                && !wrap
                                && definite_height
                                && draft.source.align_items.is_none()
                                && source.height.is_none()
                            {
                                Some(available.height.saturating_sub(margin.vertical()))
                            } else {
                                None
                            },
                            &draft.style,
                            depth + 1,
                        )?
                        .expect("active child");
                    Placed::Block(child)
                }
                FlowItem::Inline { group, .. } => {
                    // Intrinsic row measurement is not an inline line-box
                    // allocation. Constrain normal text to the remaining row
                    // space so word wrapping occurs before clipping/painting.
                    // Scroll containers retain their horizontal content extent;
                    // whitespace/ellipsis policy remains the formatter's job.
                    let inline_width = if row && !wrap && !scrollable(source.overflow_x) {
                        item_width.min(available.width.saturating_sub(x))
                    } else {
                        item_width
                    };
                    let mut result =
                        inline::format(&group, inline_width, &draft.style, self.options);
                    let children = Self::inline_nodes(&result.sources, &mut result.fragments, None);
                    Placed::Inline {
                        size: result.size,
                        text: result.fragments,
                        children,
                    }
                }
            };
            let size = placed.size();
            let position = all.len();
            lines
                .last_mut()
                .expect("initial line")
                .push((position, x, y));
            all.push(placed);
            if row || wrap {
                x += size.width + gap_x;
                line_height = line_height.max(size.height);
            } else {
                y += size.height + gap_y;
            }
        }
        // Auto-height rows stretch to their measured line, not the enclosing
        // viewport. Explicit alignment opts out, as do explicit child heights.
        if row && !wrap && !definite_height && source.align_items.is_none() {
            for item in &mut all {
                if let Placed::Block(child) = item
                    && child.source.height.is_none()
                    && child.outer_height() != line_height
                {
                    *child = self
                        .node(
                            child.source,
                            &child.path,
                            available,
                            Some(child.rect.width),
                            Some(line_height.saturating_sub(child.margin.vertical())),
                            &draft.style,
                            depth + 1,
                        )?
                        .expect("visible row child");
                }
            }
        }
        if !row && !wrap && !vertical_wrap {
            self.flex_heights(
                &mut all,
                available,
                gap_y,
                &draft.style,
                depth + 1,
                source.overflow_y == LayoutOverflow::Squash,
            )?;
            y = 0;
            for (index, _, py) in &mut lines[0] {
                *py = y;
                y += all[*index].size().height + gap_y;
            }
        }
        if vertical_wrap {
            lines = vec![Vec::new()];
            x = 0;
            y = 0;
            let mut column_width = 0;
            for (index, item) in all.iter().enumerate() {
                let size = item.size();
                if y > 0 && y + size.height > available.height {
                    x += column_width + gap_x;
                    y = 0;
                    column_width = 0;
                    lines.push(Vec::new());
                }
                lines
                    .last_mut()
                    .expect("initial column")
                    .push((index, x, y));
                y += size.height + gap_y;
                column_width = column_width.max(size.width);
            }
            for column in &mut lines {
                let used = column
                    .iter()
                    .map(|(index, _, _)| all[*index].size().height)
                    .sum::<u64>()
                    + gap_y * column.len().saturating_sub(1) as u64;
                let width = column
                    .iter()
                    .map(|(index, _, _)| all[*index].size().width)
                    .max()
                    .unwrap_or(0);
                let (start, spacing) = distribution(
                    source.justify_content,
                    available.height.saturating_sub(used),
                    column.len(),
                );
                let count = column.len();
                for (ordinal, (index, px, py)) in column.iter_mut().enumerate() {
                    *py += start + distributed(spacing, ordinal, count.saturating_sub(1));
                    *px += cross_offset(source.align_items, width, all[*index].size().width);
                }
            }
        } else if !row && !wrap {
            let total = y.saturating_sub(if all.is_empty() { 0 } else { gap_y });
            let spare = available.height.saturating_sub(total);
            let (start, spacing) = distribution(source.justify_content, spare, all.len());
            for (ordinal, (index, px, py)) in lines[0].iter_mut().enumerate() {
                *py += start + distributed(spacing, ordinal, all.len().saturating_sub(1));
                let size = all[*index].size();
                *px = cross_offset(source.align_items, available.width, size.width);
            }
        } else {
            // Grid tracks are shared across rows, including an incomplete last
            // row. Ordinary wrapping still justifies each line independently.
            let grid_columns = lines.iter().map(Vec::len).max().unwrap_or(0);
            let grid_used = lines
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|(index, _, _)| all[*index].size().width)
                        .sum::<u64>()
                        + gap_x * line.len().saturating_sub(1) as u64
                })
                .max()
                .unwrap_or(0);
            for line in &mut lines {
                let used = line
                    .iter()
                    .map(|(index, _, _)| all[*index].size().width)
                    .sum::<u64>()
                    + gap_x * line.len().saturating_sub(1) as u64;
                let height = if row && !wrap && definite_height {
                    available.height
                } else {
                    line.iter()
                        .map(|(index, _, _)| all[*index].size().height)
                        .max()
                        .unwrap_or(0)
                };
                let count = if grid { grid_columns } else { line.len() };
                let (start, spacing) = distribution(
                    source.justify_content,
                    available
                        .width
                        .saturating_sub(if grid { grid_used } else { used }),
                    count,
                );
                for (ordinal, (index, px, py)) in line.iter_mut().enumerate() {
                    *px += start + distributed(spacing, ordinal, count.saturating_sub(1));
                    *py += cross_offset(source.align_items, height, all[*index].size().height);
                }
            }
        }
        let positions: BTreeMap<_, _> = lines
            .into_iter()
            .flatten()
            .map(|(index, x, y)| (index, (x, y)))
            .collect();
        for (index, placed) in all.into_iter().enumerate() {
            let (x, y) = positions[&index];
            let size = placed.size();
            draft.extents.width = draft.extents.width.max(x + size.width);
            draft.extents.height = draft.extents.height.max(y + size.height);
            match placed {
                Placed::Block(mut child) => {
                    child.rect.x = signed(x) + child.margin.left;
                    child.rect.y = signed(y) + child.margin.top;
                    if !clips(child.source.overflow_x) {
                        draft.extents.width = draft
                            .extents
                            .width
                            .max(positive(child.rect.x + child.content.x) + child.extents.width);
                    }
                    if !clips(child.source.overflow_y) {
                        draft.extents.height = draft
                            .extents
                            .height
                            .max(positive(child.rect.y + child.content.y) + child.extents.height);
                    }
                    draft.children.push(child);
                }
                Placed::Inline {
                    mut text,
                    mut children,
                    ..
                } => {
                    for fragment in &mut text {
                        fragment.bounds = fragment.bounds.translated(signed(x), signed(y));
                    }
                    for child in &mut children {
                        child.rect.x += signed(x);
                        child.rect.y += signed(y);
                    }
                    draft.text.extend(text);
                    draft.children.extend(children);
                }
            }
        }
        for (index, child) in active.into_iter().filter(|(_, child)| out_of_flow(child)) {
            let mut path = draft.path.clone();
            path.push(index);
            let key = node_key(child, &path);
            let containing = if child.position == Some(Position::Fixed) {
                LogicalSize::new(
                    u64::from(self.options.viewport.width),
                    u64::from(self.options.viewport.height),
                )
            } else {
                available
            };
            let natural = self.intrinsic(child, &path, containing, &draft.style, depth + 1)?;
            let left = self.dimension(child.left.as_ref(), containing.width, true, &key)?;
            let right = self.dimension(child.right.as_ref(), containing.width, true, &key)?;
            let top = self.dimension(child.top.as_ref(), containing.height, false, &key)?;
            let bottom = self.dimension(child.bottom.as_ref(), containing.height, false, &key)?;
            let width = if child.width.is_none() && left.is_some() && right.is_some() {
                containing
                    .width
                    .saturating_sub(left.unwrap_or(0) + right.unwrap_or(0))
            } else {
                natural
            };
            let height = if child.height.is_none() && top.is_some() && bottom.is_some() {
                Some(
                    containing
                        .height
                        .saturating_sub(top.unwrap_or(0) + bottom.unwrap_or(0)),
                )
            } else {
                None
            };
            if let Some(mut child) = self.node(
                child,
                &path,
                containing,
                Some(width),
                height,
                &draft.style,
                depth + 1,
            )? {
                child.rect.x = child.margin.left;
                child.rect.y = child.margin.top;
                draft.children.push(child);
            }
        }
        Ok(())
    }

    fn flex_heights(
        &self,
        items: &mut [Placed<'_>],
        available: LogicalSize,
        gap: u64,
        style: &TextStyle,
        depth: usize,
        squash: bool,
    ) -> Result<(), LayoutError> {
        let mut heights: Vec<_> = items.iter().map(|item| item.size().height).collect();
        let mut minimums = heights.clone();
        let mut maximums = heights.clone();
        let mut grow = vec![0.0; items.len()];
        let mut shrink = vec![0.0; items.len()];
        let mut flexible = false;
        for (index, item) in items.iter().enumerate() {
            if let Placed::Block(child) = item {
                let (min, max) = self.limits(child.source, available, false, &child.key)?;
                let decoration = child.rect.height.saturating_sub(child.content.height)
                    + child.margin.vertical();
                minimums[index] = min + decoration;
                maximums[index] = max.saturating_add(decoration).min(MAX_EXTENT);
                if let Some(flex) = &child.source.flex {
                    flexible = true;
                    heights[index] = self
                        .dimension(Some(&flex.basis), available.height, false, &child.key)?
                        .unwrap_or(0)
                        + decoration;
                    grow[index] = f64::from(flex.grow.calc(1.0, 1.0, 1.0)).max(0.0);
                    shrink[index] = f64::from(flex.shrink.calc(1.0, 1.0, 1.0)).max(0.0)
                        * allocation_float(heights[index]);
                } else if squash {
                    shrink[index] = allocation_float(heights[index]);
                }
                heights[index] = heights[index].clamp(minimums[index], maximums[index]);
            }
        }
        if grow.iter().chain(&shrink).any(|weight| !weight.is_finite()) {
            return Err(LayoutError::InvalidLength {
                key: "flex-height".into(),
            });
        }
        let target = available
            .height
            .saturating_sub(gap * items.len().saturating_sub(1) as u64);
        let total: u64 = heights.iter().sum();
        if total < target && flexible {
            redistribute(&mut heights, &maximums, &grow, target - total, true);
        } else if total > target && (flexible || squash) {
            redistribute(&mut heights, &minimums, &shrink, total - target, false);
        }
        for (index, item) in items.iter_mut().enumerate() {
            if let Placed::Block(child) = item
                && heights[index] != child.outer_height()
            {
                let replacement = self
                    .node(
                        child.source,
                        &child.path,
                        available,
                        Some(child.rect.width),
                        Some(heights[index].saturating_sub(child.margin.vertical())),
                        style,
                        depth,
                    )?
                    .expect("visible flex child");
                *child = replacement;
            }
        }
        Ok(())
    }

    fn flex_widths(
        &self,
        items: &[FlowItem<'_>],
        widths: &mut [u64],
        available: LogicalSize,
        gap: u64,
        key: &str,
        can_shrink: bool,
    ) -> Result<(), LayoutError> {
        let target = available
            .width
            .saturating_sub(gap.saturating_mul(items.len().saturating_sub(1) as u64));
        let mut grow = Vec::new();
        let mut shrink = Vec::new();
        let mut mins = Vec::new();
        let mut maxs = Vec::new();
        for (index, item) in items.iter().enumerate() {
            if let FlowItem::Block { source, path, .. } = item {
                let child_key = node_key(source, path);
                let (min, max) = self.limits(source, available, true, &child_key)?;
                let margin = self
                    .edges(source, available, false, true, &child_key)?
                    .horizontal();
                let decoration = self
                    .edges(source, available, true, false, &child_key)?
                    .horizontal()
                    + self
                        .edges(source, available, false, false, &child_key)?
                        .horizontal();
                mins.push(min + decoration + margin);
                maxs.push(max.saturating_add(decoration + margin).min(MAX_EXTENT));
                if let Some(flex) = &source.flex {
                    widths[index] = self
                        .dimension(Some(&flex.basis), available.width, true, &child_key)?
                        .unwrap_or(0)
                        + decoration
                        + margin;
                    grow.push(f64::from(flex.grow.calc(1.0, 1.0, 1.0)).max(0.0));
                    shrink.push(
                        f64::from(flex.shrink.calc(1.0, 1.0, 1.0)).max(0.0)
                            * allocation_float(widths[index]),
                    );
                } else {
                    grow.push(0.0);
                    shrink.push(if source.width.is_some() {
                        0.0
                    } else {
                        allocation_float(widths[index])
                    });
                }
                widths[index] = widths[index].clamp(mins[index], maxs[index]);
            } else {
                mins.push(0);
                maxs.push(MAX_EXTENT);
                grow.push(0.0);
                shrink.push(allocation_float(widths[index]));
            }
        }
        if grow.iter().chain(&shrink).any(|value| !value.is_finite()) {
            return Err(LayoutError::InvalidLength { key: key.into() });
        }
        let total: u64 = widths.iter().sum();
        if total < target {
            redistribute(widths, &maxs, &grow, target - total, true);
        } else if total > target && can_shrink {
            redistribute(widths, &mins, &shrink, total - target, false);
        }
        Ok(())
    }

    // This traversal applies one authoritative transform/clip chain to all scene data.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn resolve(
        &self,
        draft: Draft<'_>,
        origin: (i64, i64),
        inherited_clip: CellRect,
        containing: CellRect,
        scrollport: CellRect,
        parent_key: Option<String>,
        tree: &mut LayoutTree,
    ) -> Result<(), LayoutError> {
        let position = draft.source.position.unwrap_or_default();
        let viewport = CellRect::new(
            0,
            0,
            u64::from(self.options.viewport.width),
            u64::from(self.options.viewport.height),
        );
        let anchor = if position == Position::Fixed {
            viewport
        } else {
            containing
        };
        let mut draft = draft;
        if matches!(position, Position::Absolute | Position::Fixed) {
            let available = LogicalSize::new(anchor.width, anchor.height);
            let margin = self.edges(draft.source, available, false, true, &draft.key)?;
            let left =
                self.dimension_signed(draft.source.left.as_ref(), anchor.width, true, &draft.key)?;
            let right =
                self.dimension_signed(draft.source.right.as_ref(), anchor.width, true, &draft.key)?;
            let top =
                self.dimension_signed(draft.source.top.as_ref(), anchor.height, false, &draft.key)?;
            let bottom = self.dimension_signed(
                draft.source.bottom.as_ref(),
                anchor.height,
                false,
                &draft.key,
            )?;
            let width = if draft.source.width.is_none()
                && draft.source.left.is_some()
                && draft.source.right.is_some()
            {
                positive(signed(anchor.width) - left - right).saturating_sub(margin.horizontal())
            } else {
                self.intrinsic(
                    draft.source,
                    &draft.path,
                    available,
                    &draft.style,
                    draft.path.len(),
                )?
                .saturating_sub(margin.horizontal())
            };
            let height = if draft.source.height.is_none()
                && draft.source.top.is_some()
                && draft.source.bottom.is_some()
            {
                Some(
                    positive(signed(anchor.height) - top - bottom)
                        .saturating_sub(margin.vertical()),
                )
            } else {
                None
            };
            draft = self
                .node(
                    draft.source,
                    &draft.path,
                    available,
                    Some(width),
                    height,
                    &draft.style,
                    draft.path.len(),
                )?
                .expect("visible positioned node");
        }
        let mut bounds = draft.rect.translated(origin.0, origin.1);
        if matches!(position, Position::Absolute | Position::Fixed) {
            bounds.x = anchor.x
                + self.offset(
                    draft.source.left.as_ref(),
                    draft.source.right.as_ref(),
                    anchor.width,
                    positive(signed(bounds.width) + draft.margin.left + draft.margin.right),
                    true,
                    &draft.key,
                )?
                + draft.margin.left;
            bounds.y = anchor.y
                + self.offset(
                    draft.source.top.as_ref(),
                    draft.source.bottom.as_ref(),
                    anchor.height,
                    positive(signed(bounds.height) + draft.margin.top + draft.margin.bottom),
                    false,
                    &draft.key,
                )?
                + draft.margin.top;
        } else if position == Position::Relative {
            bounds.x += self.relative_offset(
                draft.source.left.as_ref(),
                draft.source.right.as_ref(),
                containing.width,
                true,
                &draft.key,
            )?;
            bounds.y += self.relative_offset(
                draft.source.top.as_ref(),
                draft.source.bottom.as_ref(),
                containing.height,
                false,
                &draft.key,
            )?;
        } else if position == Position::Sticky && !self.unscrolled {
            // Sticky uses its normal-flow parent as the movement boundary, not
            // the nearest positioned ancestor used by absolute positioning.
            let parent = parent_key.as_ref().and_then(|key| tree.get(key));
            let parent_is_scrollport = parent.is_some_and(|node| node.content_bounds == scrollport);
            let boundary = parent.map_or(containing, |node| {
                CellRect::new(
                    node.text_origin.0,
                    node.text_origin.1,
                    if parent_is_scrollport {
                        node.content_bounds.width.max(node.content_extents.width)
                    } else {
                        node.content_bounds.width
                    },
                    if parent_is_scrollport {
                        node.content_bounds.height.max(node.content_extents.height)
                    } else {
                        node.content_bounds.height
                    },
                )
            });
            bounds.x = sticky_axis(
                bounds.x,
                bounds.width,
                scrollport.x,
                scrollport.width,
                boundary.x,
                boundary.width,
                self.optional_offset(
                    draft.source.left.as_ref(),
                    scrollport.width,
                    true,
                    &draft.key,
                )?,
                self.optional_offset(
                    draft.source.right.as_ref(),
                    scrollport.width,
                    true,
                    &draft.key,
                )?,
            );
            bounds.y = sticky_axis(
                bounds.y,
                bounds.height,
                scrollport.y,
                scrollport.height,
                boundary.y,
                boundary.height,
                self.optional_offset(
                    draft.source.top.as_ref(),
                    scrollport.height,
                    false,
                    &draft.key,
                )?,
                self.optional_offset(
                    draft.source.bottom.as_ref(),
                    scrollport.height,
                    false,
                    &draft.key,
                )?,
            );
        }
        bounds.x += self.dimension_signed(
            draft.source.translate_x.as_ref(),
            bounds.width,
            true,
            &draft.key,
        )?;
        bounds.y += self.dimension_signed(
            draft.source.translate_y.as_ref(),
            bounds.height,
            false,
            &draft.key,
        )?;
        let clip = if position == Position::Fixed {
            viewport
        } else {
            inherited_clip
        };
        let content = draft.content.translated(bounds.x, bounds.y);
        if let Some(extents) = self.extents.get(&draft.key) {
            draft.extents = *extents;
        }
        let requested = if self.unscrolled {
            ScrollOffset::default()
        } else {
            self.options
                .scroll_offsets
                .get(&draft.key)
                .copied()
                .unwrap_or_default()
        };
        let scroll = ScrollOffset {
            x: if scrollable(draft.source.overflow_x) {
                requested
                    .x
                    .min(draft.extents.width.saturating_sub(content.width))
            } else {
                0
            },
            y: if scrollable(draft.source.overflow_y) {
                requested
                    .y
                    .min(draft.extents.height.saturating_sub(content.height))
            } else {
                0
            },
        };
        let text_origin = (content.x - signed(scroll.x), content.y - signed(scroll.y));
        let mut text = draft.text;
        for fragment in &mut text {
            fragment.bounds = fragment.bounds.translated(text_origin.0, text_origin.1);
        }
        let mut child_clip = clip;
        if clips(draft.source.overflow_x) {
            child_clip = clip_axis(child_clip, content, true);
        }
        if clips(draft.source.overflow_y) {
            child_clip = clip_axis(child_clip, content, false);
        }
        let visible = draft.style.visible;
        let order = tree.nodes.len();
        tree.indices.insert(draft.key.clone(), order);
        let children_keys = draft
            .children
            .iter()
            .map(|child| child.key.clone())
            .collect();
        tree.nodes.push(ResolvedNode {
            key: draft.key.clone(),
            path: draft.path,
            parent_key,
            children: children_keys,
            bounds,
            clip,
            content_bounds: content,
            content_extents: draft.extents,
            scroll_offset: scroll,
            text_clip: child_clip,
            paint_order: order,
            visible,
            text_origin: (content.x - signed(scroll.x), content.y - signed(scroll.y)),
            hit_regions: draft
                .regions
                .into_iter()
                .map(|region| region.translated(bounds.x, bounds.y))
                .collect(),
            text,
            style: draft.style,
        });
        let next_containing = (if position != Position::Static || order == 0 {
            content
        } else {
            containing
        })
        .translated(-signed(scroll.x), -signed(scroll.y));
        let next_scrollport =
            if scrollable(draft.source.overflow_x) || scrollable(draft.source.overflow_y) {
                content
            } else {
                scrollport
            };
        for child in draft.children {
            self.resolve(
                child,
                text_origin,
                child_clip,
                next_containing,
                next_scrollport,
                Some(draft.key.clone()),
                tree,
            )?;
        }
        Ok(())
    }

    fn optional_offset(
        &self,
        value: Option<&Number>,
        base: u64,
        horizontal: bool,
        key: &str,
    ) -> Result<Option<i64>, LayoutError> {
        value
            .map(|value| {
                self.length(value, base, horizontal, key)
                    .map(|value| signed_cells(value.round()))
            })
            .transpose()
    }

    fn dimension_signed(
        &self,
        value: Option<&Number>,
        base: u64,
        horizontal: bool,
        key: &str,
    ) -> Result<i64, LayoutError> {
        value.map_or(Ok(0), |value| {
            self.length(value, base, horizontal, key)
                .map(|value| signed_cells(value.round()))
        })
    }

    fn relative_offset(
        &self,
        start: Option<&Number>,
        end: Option<&Number>,
        base: u64,
        horizontal: bool,
        key: &str,
    ) -> Result<i64, LayoutError> {
        if start.is_some() {
            self.dimension_signed(start, base, horizontal, key)
        } else {
            self.dimension_signed(end, base, horizontal, key)
                .map(|value| -value)
        }
    }

    fn offset(
        &self,
        start: Option<&Number>,
        end: Option<&Number>,
        base: u64,
        size: u64,
        horizontal: bool,
        key: &str,
    ) -> Result<i64, LayoutError> {
        if start.is_some() {
            self.dimension_signed(start, base, horizontal, key)
        } else if end.is_some() {
            self.dimension_signed(end, base, horizontal, key)
                .map(|end| signed(base) - signed(size) - end)
        } else {
            Ok(0)
        }
    }
}

enum FlowItem<'a> {
    Block {
        source: &'a Container,
        path: Vec<usize>,
        natural: u64,
    },
    Inline {
        group: Vec<(&'a Container, Vec<usize>)>,
        natural: u64,
    },
}
impl FlowItem<'_> {
    const fn natural(&self) -> u64 {
        match self {
            Self::Block { natural, .. } | Self::Inline { natural, .. } => *natural,
        }
    }
}
enum Placed<'a> {
    Block(Draft<'a>),
    Inline {
        size: LogicalSize,
        text: Vec<InlineFragment>,
        children: Vec<Draft<'a>>,
    },
}
impl Placed<'_> {
    fn size(&self) -> LogicalSize {
        match self {
            Self::Block(child) => LogicalSize::new(child.outer_width(), child.outer_height()),
            Self::Inline { size, .. } => *size,
        }
    }
}

const fn hidden(node: &Container) -> bool {
    matches!(node.hidden, Some(true))
        || matches!(
            node.element,
            Element::Input {
                input: Input::Hidden { .. },
                ..
            }
        )
}
const fn out_of_flow(node: &Container) -> bool {
    matches!(node.position, Some(Position::Absolute | Position::Fixed))
}
pub(crate) fn composite_button(node: &Container) -> bool {
    fn has_image(node: &Container) -> bool {
        matches!(node.element, Element::Image { .. }) || node.children.iter().any(has_image)
    }
    matches!(node.element, Element::Button { .. })
        && (node.children.iter().any(has_image)
            // Multi-row terminal buttons can render their complete source label
            // with normal inline wrapping instead of one-line native [] chrome.
            || (!node.children.is_empty()
                && matches!(node.height, Some(Number::Integer(height)) if height >= 32)))
}

fn native_leaf(node: &Container) -> bool {
    matches!(
        node.element,
        Element::Input { .. }
            | Element::Select { .. }
            | Element::Textarea { .. }
            | Element::Image { .. }
    ) || (matches!(node.element, Element::Button { .. }) && !composite_button(node))
        || (node.children.is_empty()
            && !matches!(
                node.element,
                Element::Text { .. } | Element::Raw { .. } | Element::Span | Element::Anchor { .. }
            ))
}
fn is_inline(node: &Container) -> bool {
    matches!(
        node.element,
        Element::Text { .. } | Element::Raw { .. } | Element::Span | Element::Anchor { .. }
    ) && node.width.is_none()
        && node.height.is_none()
        && !out_of_flow(node)
        && node.children.iter().all(is_inline)
}
const fn scrollable(overflow: LayoutOverflow) -> bool {
    matches!(overflow, LayoutOverflow::Auto | LayoutOverflow::Scroll)
}
const fn clips(overflow: LayoutOverflow) -> bool {
    matches!(
        overflow,
        LayoutOverflow::Hidden
            | LayoutOverflow::Auto
            | LayoutOverflow::Scroll
            | LayoutOverflow::Squash
    )
}
/// Convert cell counts for proportional allocation. Layout dimensions are
/// bounded by `MAX_EXTENT`; accumulated totals may be larger and intentionally
/// use floating-point weights before deterministic integer boundary rounding.
#[allow(clippy::cast_precision_loss)]
const fn allocation_float(value: u64) -> f64 {
    value as f64
}

/// `Number::calc` uses f32 document units; conversion necessarily shares that
/// precision. Geometry is checked for finiteness and extent immediately after it.
#[allow(clippy::cast_precision_loss)]
const fn document_float(value: u64) -> f32 {
    value as f32
}

/// Round a finite boundary to cells. Saturation preserves Rust's float-to-int
/// semantics for accumulated allocation boundaries; callers cap each allocation
/// by its remaining integer capacity.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn rounded_cells(value: f64) -> u64 {
    value.max(0.0).round() as u64
}

/// Signed offsets have already passed `length`'s finite `MAX_EXTENT` check.
#[allow(clippy::cast_possible_truncation)]
const fn signed_cells(value: f64) -> i64 {
    value as i64
}

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
fn positive(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}
fn child_path(path: &[usize], index: usize) -> Vec<usize> {
    let mut result = path.to_vec();
    result.push(index);
    result
}
fn plain_text(node: &Container) -> String {
    let mut value = match &node.element {
        Element::Text { value } | Element::Raw { value } => value.clone(),
        _ => String::new(),
    };
    for child in &node.children {
        if !hidden(child) {
            value.push_str(&plain_text(child));
        }
    }
    value
}
const fn cross_offset(align: Option<AlignItems>, available: u64, used: u64) -> u64 {
    match align {
        Some(AlignItems::Center) => available.saturating_sub(used) / 2,
        Some(AlignItems::End) => available.saturating_sub(used),
        _ => 0,
    }
}
const fn distribution(justify: Option<JustifyContent>, spare: u64, count: usize) -> (u64, u64) {
    match justify {
        Some(JustifyContent::Center) => (spare / 2, 0),
        Some(JustifyContent::End) => (spare, 0),
        Some(JustifyContent::SpaceBetween) if count > 1 => (0, spare),
        Some(JustifyContent::SpaceEvenly) if count > 0 => (
            spare / (count as u64 + 1),
            spare.saturating_sub(2 * (spare / (count as u64 + 1))),
        ),
        _ => (0, 0),
    }
}
fn distributed(total: u64, ordinal: usize, gaps: usize) -> u64 {
    if gaps == 0 {
        0
    } else {
        u64::try_from(u128::from(total) * ordinal as u128 / gaps as u128).unwrap_or(u64::MAX)
    }
}
fn clip_axis(clip: CellRect, bounds: CellRect, horizontal: bool) -> CellRect {
    if horizontal {
        let x = clip.x.max(bounds.x);
        CellRect::new(
            x,
            clip.y,
            positive(clip.right().min(bounds.right()) - x),
            clip.height,
        )
    } else {
        let y = clip.y.max(bounds.y);
        CellRect::new(
            clip.x,
            y,
            clip.width,
            positive(clip.bottom().min(bounds.bottom()) - y),
        )
    }
}

/// Weighted allocation with freezing at limits and deterministic residual cells.
fn redistribute(
    widths: &mut [u64],
    limits: &[u64],
    weights: &[f64],
    mut remaining: u64,
    grow: bool,
) {
    while remaining > 0 {
        let active: Vec<_> = (0..widths.len())
            .filter(|&index| {
                weights[index] > 0.0
                    && if grow {
                        widths[index] < limits[index]
                    } else {
                        widths[index] > limits[index]
                    }
            })
            .collect();
        let total: f64 = active.iter().map(|&index| weights[index]).sum();
        if total <= 0.0 {
            break;
        }
        let mut used = 0;
        let mut boundary = 0.0;
        let mut prior = 0;
        for index in active {
            boundary += allocation_float(remaining) * weights[index] / total;
            let next = rounded_cells(boundary);
            let capacity = if grow {
                limits[index] - widths[index]
            } else {
                widths[index] - limits[index]
            };
            let delta = next
                .saturating_sub(prior)
                .min(capacity)
                .min(remaining - used);
            prior = next;
            if grow {
                widths[index] += delta;
            } else {
                widths[index] -= delta;
            }
            used += delta;
        }
        if used == 0 {
            break;
        }
        remaining -= used;
    }
}

#[cfg(test)]
mod scroll_reprojection_tests {
    use super::*;

    fn album_document() -> Container {
        let albums = (0..100)
            .map(|index| Container {
                str_id: Some(format!("album-{index}")),
                width: Some(Number::Integer(160)),
                height: Some(Number::Integer(160)),
                children: vec![Container {
                    element: Element::Text {
                        value: format!("Album {index}: a title that wraps"),
                    },
                    ..Container::default()
                }],
                ..Container::default()
            })
            .collect();
        Container {
            children: vec![
                Container {
                    str_id: Some("scroll".into()),
                    width: Some(Number::Integer(640)),
                    height: Some(Number::Integer(320)),
                    overflow_y: LayoutOverflow::Scroll,
                    children: albums,
                    ..Container::default()
                },
                Container {
                    str_id: Some("fixed-player".into()),
                    position: Some(Position::Fixed),
                    height: Some(Number::Integer(32)),
                    ..Container::default()
                },
            ],
            ..Container::default()
        }
    }

    #[test]
    fn hundred_album_scroll_matches_complete_reflow() {
        let root = album_document();
        let mut options = LayoutOptions::default();
        options.viewport = Size::new(100, 40);
        let mut retained = layout(&root, &options, &DefaultLeafMeasure).unwrap();
        for offset in [3, 6, 30, 200, 99999, 6, 0] {
            options
                .scroll_offsets
                .insert("scroll".into(), ScrollOffset { x: 0, y: offset });
            let projected = reproject_scroll(&root, &retained, &options.scroll_offsets).unwrap();
            let full = layout(&root, &options, &DefaultLeafMeasure).unwrap();
            // Debug includes all geometry, text fragments, hit regions, styles,
            // source paths, keys, clips and paint order, not just border bounds.
            assert_eq!(format!("{projected:?}"), format!("{full:?}"));
            retained = projected;
        }
    }

    #[test]
    fn nested_scroll_images_and_fixed_sibling_match_reflow() {
        let mut root = album_document();
        let mut nested = root.children[0].clone();
        nested.str_id = Some("nested".into());
        nested.width = Some(Number::Integer(1280));
        nested.height = Some(Number::Integer(640));
        nested.overflow_x = LayoutOverflow::Scroll;
        nested.children[0].children.push(Container {
            element: Element::Image {
                source: Some("/real-cover.png".into()),
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None,
            },
            width: Some(Number::Integer(96)),
            height: Some(Number::Integer(96)),
            ..Container::default()
        });
        root.children[0].overflow_x = LayoutOverflow::Scroll;
        root.children[0].children = vec![nested];
        let mut options = LayoutOptions {
            viewport: Size::new(100, 40),
            ..LayoutOptions::default()
        };
        let mut retained = layout(&root, &options, &DefaultLeafMeasure).unwrap();
        for (key, x, y) in [
            ("nested", 0, 5),
            ("scroll", 3, 4),
            ("nested", 0, 30),
            ("scroll", 99999, 99999),
            ("scroll", 0, 0),
            ("nested", 0, 0),
        ] {
            options
                .scroll_offsets
                .insert(key.into(), ScrollOffset { x, y });
            let projected = reproject_scroll(&root, &retained, &options.scroll_offsets).unwrap();
            let full = layout(&root, &options, &DefaultLeafMeasure).unwrap();
            assert_eq!(format!("{projected:?}"), format!("{full:?}"));
            retained = projected;
        }
    }

    fn production_album_controls(index: usize) -> Container {
        Container {
            str_id: Some(format!("controls-{index}")),
            position: Some(Position::Absolute),
            visibility: Some(Visibility::Hidden),
            width: Some(Number::Integer(160)),
            height: Some(Number::Integer(160)),
            children: [true, false]
                .into_iter()
                .map(|left| Container {
                    str_id: Some(format!("button-{index}-{left}")),
                    element: Element::Button { r#type: None },
                    position: Some(Position::Absolute),
                    width: Some(Number::Integer(40)),
                    height: Some(Number::Integer(40)),
                    bottom: Some(Number::RealPercent(5.0)),
                    left: left.then_some(Number::RealPercent(5.0)),
                    right: (!left).then_some(Number::RealPercent(5.0)),
                    children: vec![Container {
                        str_id: Some(format!("cover-control-{index}-{left}")),
                        element: Element::Image {
                            source: Some("/play-button.svg".into()),
                            alt: None,
                            fit: None,
                            source_set: None,
                            sizes: None,
                            loading: None,
                        },
                        width: Some(Number::Integer(16)),
                        height: Some(Number::Integer(16)),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn album_absolute_overlays_reproject_with_complete_layout_parity() {
        for (width, height, menu_hidden) in [(80, 24), (120, 40), (140, 60)]
            .into_iter()
            .flat_map(|(width, height)| [false, true].map(|hidden| (width, height, hidden)))
        {
            let mut root = album_document();
            for (index, album) in root.children[0].children.iter_mut().enumerate() {
                album.position = Some(Position::Relative);
                album.children.push(Container {
                    str_id: Some(format!("cover-{index}")),
                    element: Element::Image {
                        source: Some("/cover.png".into()),
                        alt: None,
                        fit: None,
                        source_set: None,
                        sizes: None,
                        loading: None,
                    },
                    width: Some(Number::Integer(160)),
                    height: Some(Number::Integer(160)),
                    ..Default::default()
                });
                album.children.push(production_album_controls(index));
                // Shape of native/ui/src/albums.rs #albums-menu: fixed width,
                // intrinsic height, percentage top, two columns and bordered rows.
                album.children.push(Container {
                    str_id: Some(format!("albums-menu-{index}")),
                    position: Some(Position::Absolute),
                    width: Some(Number::Integer(300)),
                    top: Some(Number::RealPercent(100.0)),
                    direction: LayoutDirection::Row,
                    visibility: Some(if menu_hidden {
                        Visibility::Hidden
                    } else {
                        Visibility::Visible
                    }),
                    children: ["Album Artist", "Album Title", "Release Date"]
                        .into_iter()
                        .map(|label| Container {
                            border_top: Some((hyperchad_color::Color::BLACK, Number::Real(1.0))),
                            children: vec![Container {
                                element: Element::Button { r#type: None },
                                children: vec![Container {
                                    element: Element::Text {
                                        value: label.into(),
                                    },
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }],
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                });
                album.children.push(Container {
                    str_id: Some(format!("overlay-{index}")),
                    position: Some(Position::Absolute),
                    left: Some(Number::Integer(8)),
                    top: Some(Number::Integer(16)),
                    width: Some(Number::Integer(80)),
                    height: Some(Number::Integer(32)),
                    children: vec![Container {
                        element: Element::Text {
                            value: "Play album".into(),
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                });
            }
            let mut options = LayoutOptions {
                viewport: Size::new(width, height),
                ..Default::default()
            };
            let measurements = std::cell::Cell::new(0);
            let measure = |node: &Container, constraints| {
                measurements.set(measurements.get() + 1);
                DefaultLeafMeasure.measure(node, constraints)
            };
            let mut retained = layout(&root, &options, &measure).unwrap();
            assert!(measurements.get() > 0);
            for offset in [3, 6, 30, 200, 99999, 6, 0] {
                options
                    .scroll_offsets
                    .insert("scroll".into(), ScrollOffset { x: 0, y: offset });
                measurements.set(0);
                let projected = reproject_scroll(&root, &retained, &options.scroll_offsets)
                    .expect("relative album owns its absolute overlay");
                assert_eq!(
                    measurements.get(),
                    0,
                    "scroll reuse must not remeasure leaves"
                );
                let full = layout(&root, &options, &measure).unwrap();
                assert!(measurements.get() > 0);
                assert_eq!(
                    format!("{projected:?}"),
                    format!("{full:?}"),
                    "{width}x{height} offset={offset}"
                );
                for y in 0..i64::from(height) {
                    for x in 0..i64::from(width) {
                        assert_eq!(
                            projected.hit_test(x, y).map(|node| &node.key),
                            full.hit_test(x, y).map(|node| &node.key),
                            "hit {x},{y} hidden={menu_hidden} offset={offset}"
                        );
                    }
                }
                let mut graphics =
                    crate::graphics::GraphicsStore::new(crate::graphics::GraphicsLimits::default());
                for node in full
                    .nodes
                    .iter()
                    .filter(|node| node.key.starts_with("cover-"))
                {
                    graphics
                        .register_rgba(&node.key, 2, 2, vec![255; 16])
                        .unwrap();
                    let placement =
                        |tree: &LayoutTree, graphics: &mut crate::graphics::GraphicsStore| {
                            let node = tree.get(&node.key).unwrap();
                            graphics
                                .placement(
                                    &node.key,
                                    &node.key,
                                    node.content_bounds.terminal(),
                                    node.clip.terminal(),
                                    crate::graphics::GraphicsFit::Contain,
                                    (8.0, 16.0),
                                    true,
                                    "",
                                )
                                .unwrap()
                        };
                    assert_eq!(
                        format!("{:?}", placement(&projected, &mut graphics)),
                        format!("{:?}", placement(&full, &mut graphics))
                    );
                }
                retained = projected;
            }
        }
    }

    #[test]
    fn intrinsic_menu_requires_moving_fixed_basis_and_static_fixed_lengths() {
        let mut root = album_document();
        let album = &mut root.children[0].children[0];
        album.position = Some(Position::Relative);
        let mut menu = production_album_controls(0);
        menu.height = None;
        menu.children = vec![Container {
            element: Element::Text {
                value: "Album Artist".into(),
            },
            ..Default::default()
        }];
        menu.top = Some(Number::RealPercent(100.0));
        album.children.push(menu);
        let path = [0, 0, 1];
        assert_eq!(scrolls_absolute(&root, &path, &[0]), Some(true));
        assert_eq!(scrolls_absolute(&root, &path, &[0, 0]), Some(false));
        root.children[0].children[0].width = Some(Number::IntegerPercent(100));
        assert_eq!(scrolls_absolute(&root, &path, &[0]), Some(false));
        root.children[0].children[0].width = Some(Number::Integer(160));
        for dependency in [0, 1, 2, 3] {
            let child = &mut root.children[0].children[0].children[1].children[0];
            match dependency {
                0 => child.height = Some(Number::RealPercent(50.0)),
                1 => child.row_gap = Some(Number::RealPercent(5.0)),
                2 => child.position = Some(Position::Fixed),
                _ => child.width = Some(Number::Real(f32::INFINITY)),
            }
            assert_eq!(scrolls_absolute(&root, &path, &[0]), Some(false));
            root.children[0].children[0].children[1].children[0] = Container::default();
        }
        root.children[0].children[0].children[1].bottom = Some(Number::Integer(0));
        assert_eq!(scrolls_absolute(&root, &path, &[0]), Some(false));
    }

    #[test]
    fn real_percent_insets_require_finite_values_and_preserve_fractional_parity() {
        for value in [5.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut root = album_document();
            let album = &mut root.children[0].children[0];
            album.position = Some(Position::Relative);
            let mut controls = production_album_controls(0);
            controls.children[0].left = Some(Number::RealPercent(value));
            album.children.push(controls);
            let mut options = LayoutOptions::default();
            let path = [0, 0, 1, 0];
            assert_eq!(
                scrolls_absolute(&root, &path, &[0]),
                Some(value.is_finite())
            );
            if value.is_finite() {
                let tree = layout(&root, &options, &DefaultLeafMeasure).unwrap();
                options
                    .scroll_offsets
                    .insert("scroll".into(), ScrollOffset { x: 0, y: 3 });
                let projected = reproject_scroll(&root, &tree, &options.scroll_offsets).unwrap();
                let full = layout(&root, &options, &DefaultLeafMeasure).unwrap();
                assert_eq!(format!("{projected:?}"), format!("{full:?}"));
            }
        }
    }

    #[test]
    fn percent_insets_with_unproven_containing_dimensions_require_reflow() {
        let mut root = album_document();
        let album = &mut root.children[0].children[0];
        album.position = Some(Position::Relative);
        let mut controls = production_album_controls(0);
        controls.width = Some(Number::IntegerPercent(100));
        album.children.push(controls);
        let options = LayoutOptions::default();
        let tree = layout(&root, &options, &DefaultLeafMeasure).unwrap();
        let offsets = BTreeMap::from([("scroll".into(), ScrollOffset { x: 0, y: 3 })]);
        assert!(reproject_scroll(&root, &tree, &offsets).is_none());
    }

    #[test]
    fn absolute_scroll_overlay_with_dynamic_size_requires_reflow() {
        let mut root = album_document();
        let album = &mut root.children[0].children[0];
        album.position = Some(Position::Relative);
        album.children.push(Container {
            position: Some(Position::Absolute),
            width: Some(Number::IntegerPercent(100)),
            height: Some(Number::Integer(32)),
            ..Default::default()
        });
        let options = LayoutOptions::default();
        let tree = layout(&root, &options, &DefaultLeafMeasure).unwrap();
        let offsets = BTreeMap::from([("scroll".into(), ScrollOffset { x: 0, y: 3 })]);
        assert!(reproject_scroll(&root, &tree, &offsets).is_none());
    }

    #[test]
    fn positioned_scroll_descendants_require_reflow() {
        for position in [Position::Fixed, Position::Sticky, Position::Absolute] {
            let mut root = album_document();
            root.children[0].children[0].position = Some(position);
            let options = LayoutOptions::default();
            let tree = layout(&root, &options, &DefaultLeafMeasure).unwrap();
            let offsets = BTreeMap::from([("scroll".into(), ScrollOffset { x: 0, y: 3 })]);
            assert!(reproject_scroll(&root, &tree, &offsets).is_none());
        }
    }
}

#[cfg(test)]
mod tests;
