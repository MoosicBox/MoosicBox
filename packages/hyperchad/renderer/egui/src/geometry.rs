//! Geometry projected from HyperChad's calculated tree, never from egui layout.
//!
//! Coordinates are egui logical points. Borders overlay the padding box, matching
//! the calculator's bounding helpers (which do not reserve border widths).
//! Only `Relative` establishes an absolute containing block in the current
//! calculator. Fixed coordinates are consumed verbatim relative to the viewport;
//! the calculator's incorrect fixed right/bottom resolution must be fixed there.
//! Sticky is rejected: the shared calculator currently positions it twice.

use std::collections::BTreeMap;

use eframe::egui::{Pos2, Rect, Vec2, vec2};
use hyperchad_transformer::{
    Container,
    models::{LayoutOverflow, Position, Visibility},
};

/// Viewport in egui points and document-to-point scaling (not native DPI).
#[derive(Clone, Copy, Debug)]
pub struct GeometryViewport {
    /// Visible viewport, including its screen origin.
    pub rect: Rect,
    /// Positive document zoom. `pixels_per_point` must not be included here.
    pub zoom: f32,
}

impl GeometryViewport {
    /// Root content size in document units, before document zoom.
    ///
    /// # Errors
    /// * Returns `InvalidViewport` for nonfinite, inverted, or nonpositive-scale input.
    pub fn logical_size(self) -> Result<Vec2, GeometryError> {
        if !self.rect.is_finite()
            || self.rect.width() < 0.0
            || self.rect.height() < 0.0
            || !self.zoom.is_finite()
            || self.zoom <= 0.0
            || !(self.rect.size() / self.zoom).is_finite()
        {
            return Err(GeometryError::InvalidViewport);
        }
        Ok(self.rect.size() / self.zoom)
    }
}

/// A reversible content-local to screen transform.
#[derive(Clone, Copy, Debug)]
pub struct GeometryTransform {
    /// Screen location of local `(0, 0)`.
    pub origin: Pos2,
    /// Uniform document zoom.
    pub scale: f32,
}

impl GeometryTransform {
    /// Projects a content-local document point to egui screen points.
    #[must_use]
    pub fn to_screen(self, local: Pos2) -> Pos2 {
        self.origin + local.to_vec2() * self.scale
    }

    /// Unprojects an egui screen point to content-local document coordinates.
    #[must_use]
    pub fn to_local(self, screen: Pos2) -> Pos2 {
        ((screen - self.origin) / self.scale).to_pos2()
    }
}

/// Calculated geometry shared by paint, events, and accessibility.
#[derive(Clone, Debug)]
pub struct NodeGeometry<K> {
    /// Stable caller-owned identity.
    pub key: K,
    /// Transformer identity for diagnostics and lookup in the calculated tree.
    pub container_id: usize,
    /// DOM parent, not necessarily the positioning containing block.
    pub parent: Option<K>,
    /// Absolute containing block; `None` means viewport for a fixed node.
    pub containing_block: Option<K>,
    /// Screen-space outer layout slot, including margins.
    pub margin_box: Rect,
    /// Screen-space padding/background box; borders overlay this box.
    pub padding_box: Rect,
    /// Screen-space content box, excluding padding and reserved scrollbars.
    pub content_box: Rect,
    /// Screen-space box including reserved scrollbars, excluding margins.
    pub border_box: Rect,
    /// Ancestor clip used for this node's own paint and hit testing.
    pub paint_clip: Rect,
    /// Clip inherited by ordinary descendants.
    pub descendant_clip: Rect,
    /// Unscrolled content-local transform. Descendant scrolling is separate.
    pub transform: GeometryTransform,
    /// Content extent in unscrolled local document units, including the origin.
    pub scroll_extent: Rect,
    /// Maximum nonnegative scroll offset in document units.
    pub scroll_max: Vec2,
    /// Clamped scroll offset in document units.
    pub scroll_offset: Vec2,
    /// Whether this node paints and participates in pointer/accessibility queries.
    /// Visibility-hidden nodes retain geometry; hidden/display-none subtrees do not.
    pub visible: bool,
    /// Whether this node is directly viewport-fixed.
    pub fixed: bool,
}

impl<K> NodeGeometry<K> {
    /// Clipped screen bounds for accessibility and visibility queries.
    /// Returns `None` for visibility-hidden or fully clipped nodes.
    pub fn visible_bounds(&self) -> Option<Rect> {
        let bounds = self.border_box.intersect(self.paint_clip);
        (self.visible && bounds.is_positive()).then_some(bounds)
    }
}

/// Failure to project a calculated tree. No guessed sizes or partial snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    /// Nonfinite viewport, nonpositive zoom, or inverted viewport.
    InvalidViewport,
    /// Resolver did not provide an identity for a participating node.
    MissingIdentity(usize),
    /// Resolver produced the same identity twice.
    DuplicateIdentity(usize),
    /// Missing/nonfinite calculated geometry, inverted size, or nonfinite scroll.
    InvalidCalculatedGeometry(usize),
    /// Shared calculator does not yet provide usable sticky positions.
    UnsupportedSticky(usize),
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidViewport => f.write_str("invalid geometry viewport or zoom"),
            Self::MissingIdentity(id) => write!(f, "missing geometry identity for container {id}"),
            Self::DuplicateIdentity(id) => {
                write!(f, "duplicate geometry identity at container {id}")
            }
            Self::InvalidCalculatedGeometry(id) => write!(
                f,
                "invalid calculated geometry or scroll for container {id}"
            ),
            Self::UnsupportedSticky(id) => write!(
                f,
                "sticky container {id} requires shared calculator support"
            ),
        }
    }
}

impl std::error::Error for GeometryError {}

/// Immutable projection for one document/layout revision, supplied by the caller.
#[derive(Clone, Debug)]
pub struct GeometrySnapshot<K> {
    /// Caller-owned document/layout generation. Consumers must use the same stamp.
    pub revision: u64,
    nodes: Vec<NodeGeometry<K>>,
    indices: BTreeMap<K, usize>,
}

#[derive(Clone, Copy)]
struct Placement {
    parent: Option<usize>,
    geometry_parent: Option<usize>,
    relative: Option<usize>,
    origin: Pos2,
    visible: bool,
}

impl<K: Copy + Ord> GeometrySnapshot<K> {
    /// Projects already-calculated boxes. Never runs a second layout engine.
    ///
    /// Paint order is DOM preorder with later siblings on top; no CSS stacking
    /// contexts or z-index are invented. Fixed subtrees restart at the viewport
    /// clip and escape ancestor scrolling. Negative overflow is recorded but
    /// scrolling is intentionally nonnegative. Hidden subtrees need no identities.
    ///
    /// # Errors
    /// * Returns an error for invalid/missing geometry or identities, duplicate
    ///   identities, invalid viewport/scroll input, or unsupported sticky nodes.
    pub fn build(
        tree: &Container,
        viewport: GeometryViewport,
        revision: u64,
        scroll: &BTreeMap<K, Vec2>,
        resolve: impl Fn(&Container) -> Option<K>,
    ) -> Result<Self, GeometryError> {
        Self::build_roots(
            std::slice::from_ref(tree),
            viewport,
            revision,
            scroll,
            |_, node| resolve(node),
        )
    }

    /// Projects a calculated forest using stable identities resolved by tree path.
    ///
    /// Paths are `[root_index, child_index, ...]`, including hidden siblings in
    /// indices. Hidden subtrees are skipped without renumbering later siblings.
    /// Calculated roots and the key tree must come from the same document stamp;
    /// calculation must not reorder or insert nodes. Numeric IDs are diagnostic
    /// only and may repeat. Roots overlay at the viewport origin in slice order.
    ///
    /// # Errors
    /// * Returns the same validation errors as `build`, including duplicate
    ///   resolved keys (never duplicate authored numeric IDs).
    pub fn build_roots(
        roots: &[Container],
        viewport: GeometryViewport,
        revision: u64,
        scroll: &BTreeMap<K, Vec2>,
        resolve: impl Fn(&[usize], &Container) -> Option<K>,
    ) -> Result<Self, GeometryError> {
        viewport.logical_size()?;
        let mut snapshot = Self {
            revision,
            nodes: Vec::new(),
            indices: BTreeMap::new(),
        };
        let mut placements = Vec::new();
        let mut overflow = Vec::new();
        for (root_index, tree) in roots.iter().enumerate() {
            snapshot.collect(
                tree,
                Placement {
                    parent: None,
                    geometry_parent: None,
                    relative: None,
                    origin: Pos2::ZERO,
                    visible: true,
                },
                &mut vec![root_index],
                &resolve,
                &mut placements,
                &mut overflow,
            )?;
        }
        // Extents are accumulated in one common, unscrolled document space.
        let mut extents: Vec<Rect> = snapshot.nodes.iter().map(|n| n.content_box).collect();
        for i in (0..snapshot.nodes.len()).rev() {
            let n = &mut snapshot.nodes[i];
            n.scroll_extent = extents[i].translate(-n.content_box.min.to_vec2());
            n.scroll_max = vec2(
                if scrolls(overflow[i].0) {
                    (n.scroll_extent.max.x - n.content_box.width()).max(0.0)
                } else {
                    0.0
                },
                if scrolls(overflow[i].1) {
                    (n.scroll_extent.max.y - n.content_box.height()).max(0.0)
                } else {
                    0.0
                },
            );
            let requested = scroll.get(&n.key).copied().unwrap_or(Vec2::ZERO);
            if !requested.is_finite() {
                return Err(GeometryError::InvalidCalculatedGeometry(n.container_id));
            }
            n.scroll_offset = requested.max(Vec2::ZERO).min(n.scroll_max);
            if let Some(parent) = placements[i].geometry_parent {
                let bounds = n
                    .margin_box
                    .union(clip_axes(extents[i], n.padding_box, overflow[i]));
                extents[parent] = extents[parent].union(bounds);
            }
        }
        // Absolute nodes follow their containing block for scroll and overflow,
        // not intervening static DOM ancestors. CSS2 11.1 excludes clipping by
        // ancestors between an absolute box and its containing block. The
        // containing block's own clip/scroll and all its inherited clips remain.
        // Descendants of that absolute box still inherit its own overflow clip.
        // Only this projection applies zoom and scroll. Local calculator outputs
        // remain untouched, including grid/wrap positions and reserved gutters.
        let mut translations = vec![Vec2::ZERO; snapshot.nodes.len()];
        for i in 0..snapshot.nodes.len() {
            let n = &snapshot.nodes[i];
            let parent = placements[i].geometry_parent;
            let (translation, clip) = if let Some(parent) = parent.filter(|_| !n.fixed) {
                (
                    translations[parent] - snapshot.nodes[parent].scroll_offset,
                    snapshot.nodes[parent].descendant_clip,
                )
            } else {
                (Vec2::ZERO, viewport.rect)
            };
            translations[i] = translation;
            let project = |r: Rect| {
                Rect::from_min_max(
                    viewport.rect.min + (r.min.to_vec2() + translation) * viewport.zoom,
                    viewport.rect.min + (r.max.to_vec2() + translation) * viewport.zoom,
                )
            };
            let n = &mut snapshot.nodes[i];
            n.margin_box = project(n.margin_box);
            n.padding_box = project(n.padding_box);
            n.border_box = project(n.border_box);
            n.content_box = project(n.content_box);
            if !n.margin_box.is_finite()
                || !n.border_box.is_finite()
                || !n.padding_box.is_finite()
                || !n.content_box.is_finite()
            {
                return Err(GeometryError::InvalidCalculatedGeometry(n.container_id));
            }
            n.paint_clip = clip;
            n.descendant_clip = clip_axes(clip, n.padding_box, overflow[i]);
            n.transform = GeometryTransform {
                origin: n.content_box.min,
                scale: viewport.zoom,
            };
        }
        Ok(snapshot)
    }

    /// Looks up geometry without consulting mutable document state.
    pub fn node(&self, key: K) -> Option<&NodeGeometry<K>> {
        self.indices.get(&key).map(|&i| &self.nodes[i])
    }

    /// Nodes in actual paint order. Paint must use this same ordering.
    pub fn paint_order(&self) -> impl DoubleEndedIterator<Item = &NodeGeometry<K>> {
        self.nodes.iter()
    }

    /// Topmost visible, unclipped box accepted by the caller's interaction policy.
    /// Margins are never hit targets. Opacity zero is not visibility-hidden.
    pub fn hit_test(&self, point: Pos2, accepts: impl Fn(K) -> bool) -> Option<K> {
        self.nodes
            .iter()
            .rev()
            .find(|n| {
                n.visible
                    && n.border_box.contains(point)
                    && n.paint_clip.contains(point)
                    && accepts(n.key)
            })
            .map(|n| n.key)
    }

    #[allow(clippy::too_many_lines)]
    fn collect(
        &mut self,
        tree: &Container,
        placement: Placement,
        path: &mut Vec<usize>,
        resolve: &impl Fn(&[usize], &Container) -> Option<K>,
        placements: &mut Vec<Placement>,
        overflow: &mut Vec<(LayoutOverflow, LayoutOverflow)>,
    ) -> Result<(), GeometryError> {
        if tree.is_hidden() {
            return Ok(());
        }
        if tree.position == Some(Position::Sticky) {
            return Err(GeometryError::UnsupportedSticky(tree.id));
        }
        let key = resolve(path, tree).ok_or(GeometryError::MissingIdentity(tree.id))?;
        let index = self.nodes.len();
        if self.indices.insert(key, index).is_some() {
            return Err(GeometryError::DuplicateIdentity(tree.id));
        }
        let invalid = || GeometryError::InvalidCalculatedGeometry(tree.id);
        let width = tree.calculated_width.ok_or_else(invalid)?;
        let height = tree.calculated_height.ok_or_else(invalid)?;
        let ml = tree.calculated_margin_left.unwrap_or_default();
        let mr = tree.calculated_margin_right.unwrap_or_default();
        let mt = tree.calculated_margin_top.unwrap_or_default();
        let mb = tree.calculated_margin_bottom.unwrap_or_default();
        let pl = tree.calculated_padding_left.unwrap_or_default();
        let pr = tree.calculated_padding_right.unwrap_or_default();
        let pt = tree.calculated_padding_top.unwrap_or_default();
        let pb = tree.calculated_padding_bottom.unwrap_or_default();
        let sx = tree.scrollbar_right.unwrap_or_default();
        let sy = tree.scrollbar_bottom.unwrap_or_default();
        let position = if placement.parent.is_none() {
            Vec2::ZERO
        } else {
            vec2(
                tree.calculated_x.ok_or_else(invalid)?,
                tree.calculated_y.ok_or_else(invalid)?,
            )
        };
        let fixed = tree.position == Some(Position::Fixed);
        let containing = if fixed {
            None
        } else if tree.position == Some(Position::Absolute) {
            placement.relative
        } else {
            placement.parent
        };
        let origin = if fixed {
            Pos2::ZERO
        } else if tree.position == Some(Position::Absolute) {
            containing.map_or(Pos2::ZERO, |i| self.nodes[i].content_box.min)
        } else {
            placement.origin
        };
        let slot = origin + position;
        let border_box = Rect::from_min_size(
            slot + vec2(ml, mt),
            vec2(width + pl + pr + sx, height + pt + pb + sy),
        );
        let padding_box =
            Rect::from_min_size(border_box.min, vec2(width + pl + pr, height + pt + pb));
        let content_box = Rect::from_min_size(padding_box.min + vec2(pl, pt), vec2(width, height));
        let margin_box = Rect::from_min_size(slot, border_box.size() + vec2(ml + mr, mt + mb));
        if ![width, height, ml, mr, mt, mb, pl, pr, pt, pb, sx, sy]
            .iter()
            .all(|v| v.is_finite())
            || [width, height, pl, pr, pt, pb, sx, sy]
                .iter()
                .any(|&v| v < 0.0)
            || !margin_box.is_finite()
            || !content_box.is_finite()
            || !padding_box.is_finite()
            || !border_box.is_finite()
            || margin_box.width() < 0.0
            || margin_box.height() < 0.0
        {
            return Err(invalid());
        }
        let visible = tree
            .visibility
            .map_or(placement.visible, |v| v != Visibility::Hidden);
        self.nodes.push(NodeGeometry {
            key,
            container_id: tree.id,
            parent: placement.parent.map(|i| self.nodes[i].key),
            containing_block: containing.map(|i| self.nodes[i].key),
            margin_box,
            padding_box,
            content_box,
            border_box,
            paint_clip: Rect::NOTHING,
            descendant_clip: Rect::NOTHING,
            transform: GeometryTransform {
                origin: content_box.min,
                scale: 1.0,
            },
            scroll_extent: Rect::NOTHING,
            scroll_max: Vec2::ZERO,
            scroll_offset: Vec2::ZERO,
            visible,
            fixed,
        });
        placements.push(Placement {
            geometry_parent: containing,
            ..placement
        });
        overflow.push((tree.overflow_x, tree.overflow_y));
        let relative = if placement.parent.is_none() || tree.position == Some(Position::Relative) {
            Some(index)
        } else {
            placement.relative
        };
        for (child_index, child) in tree.children.iter().enumerate() {
            path.push(child_index);
            self.collect(
                child,
                Placement {
                    parent: Some(index),
                    geometry_parent: Some(index),
                    relative,
                    origin: content_box.min,
                    visible,
                },
                path,
                resolve,
                placements,
                overflow,
            )?;
            path.pop();
        }
        Ok(())
    }
}

const fn scrolls(overflow: LayoutOverflow) -> bool {
    matches!(overflow, LayoutOverflow::Auto | LayoutOverflow::Scroll)
}

const fn clips(overflow: LayoutOverflow) -> bool {
    matches!(
        overflow,
        LayoutOverflow::Hidden | LayoutOverflow::Auto | LayoutOverflow::Scroll
    )
}

fn clip_axes(mut rect: Rect, clip: Rect, overflow: (LayoutOverflow, LayoutOverflow)) -> Rect {
    if clips(overflow.0) {
        rect.min.x = rect.min.x.max(clip.min.x);
        rect.max.x = rect.max.x.min(clip.max.x);
    }
    if clips(overflow.1) {
        rect.min.y = rect.min.y.max(clip.min.y);
        rect.max.y = rect.max.y.min(clip.max.y);
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    fn node(id: usize, x: f32, y: f32, width: f32, height: f32) -> Container {
        Container {
            id,
            calculated_x: Some(x),
            calculated_y: Some(y),
            calculated_width: Some(width),
            calculated_height: Some(height),
            ..Container::default()
        }
    }

    fn build(tree: &Container, scroll: &BTreeMap<usize, Vec2>) -> GeometrySnapshot<usize> {
        GeometrySnapshot::build(
            tree,
            GeometryViewport {
                rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(400.0, 300.0)),
                zoom: 2.0,
            },
            7,
            scroll,
            |c| Some(c.id),
        )
        .unwrap()
    }

    #[test]
    fn nested_padding_margins_zoom_and_inverse() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.calculated_padding_left = Some(3.0);
        let mut child = node(2, 5.0, 6.0, 20.0, 10.0);
        child.calculated_margin_left = Some(2.0);
        child.calculated_padding_left = Some(4.0);
        root.children.push(child);
        let snapshot = build(&root, &BTreeMap::new());
        let child = snapshot.node(2).unwrap();
        assert_eq!(child.content_box.min, pos2(38.0, 32.0));
        let local = pos2(3.0, 7.0);
        assert_eq!(
            child.transform.to_local(child.transform.to_screen(local)),
            local
        );
        assert_eq!(snapshot.revision, 7);
    }

    #[test]
    fn scroll_clamps_clips_and_fixed_escapes() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.overflow_y = LayoutOverflow::Scroll;
        root.children.push(node(2, 0.0, 120.0, 30.0, 20.0));
        let mut fixed = node(3, 50.0, 60.0, 10.0, 10.0);
        fixed.position = Some(Position::Fixed);
        root.children.push(fixed);
        let snapshot = build(&root, &BTreeMap::from([(1, vec2(0.0, 500.0))]));
        assert_eq!(snapshot.node(1).unwrap().scroll_max, vec2(0.0, 40.0));
        assert_eq!(snapshot.node(2).unwrap().content_box.min, pos2(10.0, 180.0));
        assert_eq!(
            snapshot.node(3).unwrap().content_box.min,
            pos2(110.0, 140.0)
        );
        assert_eq!(snapshot.hit_test(pos2(15.0, 215.0), |k| k == 2), Some(2));
        assert_eq!(snapshot.hit_test(pos2(15.0, 225.0), |k| k == 2), None);
    }

    #[test]
    fn absolute_uses_relative_not_immediate_parent() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        let mut relative = node(2, 10.0, 10.0, 80.0, 80.0);
        relative.position = Some(Position::Relative);
        let mut middle = node(3, 20.0, 20.0, 40.0, 40.0);
        let mut absolute = node(4, 5.0, 7.0, 10.0, 10.0);
        absolute.position = Some(Position::Absolute);
        middle.children.push(absolute);
        relative.children.push(middle);
        root.children.push(relative);
        let snapshot = build(&root, &BTreeMap::new());
        assert_eq!(snapshot.node(4).unwrap().containing_block, Some(2));
        assert_eq!(snapshot.node(4).unwrap().content_box.min, pos2(40.0, 54.0));
    }

    #[test]
    fn absolute_escapes_intermediate_static_scroller_but_keeps_containing_clip() {
        for overflow in [LayoutOverflow::Hidden, LayoutOverflow::Scroll] {
            let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
            root.position = Some(Position::Relative);
            root.overflow_y = LayoutOverflow::Hidden;
            let mut middle = node(2, 10.0, 10.0, 20.0, 20.0);
            middle.overflow_x = overflow;
            middle.overflow_y = overflow;
            // Ordinary content gives the intermediate scroller a real range.
            middle.children.push(node(3, 0.0, 80.0, 10.0, 10.0));
            let mut absolute = node(4, 60.0, 70.0, 10.0, 40.0);
            absolute.position = Some(Position::Absolute);
            middle.children.push(absolute);
            root.children.push(middle);
            let snapshot = build(&root, &BTreeMap::from([(2, vec2(0.0, 50.0))]));
            let absolute = snapshot.node(4).unwrap();
            assert_eq!(absolute.parent, Some(2));
            assert_eq!(absolute.containing_block, Some(1));
            assert_eq!(absolute.content_box.min, pos2(130.0, 160.0));
            assert_eq!(snapshot.hit_test(pos2(135.0, 170.0), |k| k == 4), Some(4));
            // Root's own clip is retained even though the static clip is skipped.
            assert_eq!(snapshot.hit_test(pos2(135.0, 230.0), |k| k == 4), None);
            if overflow == LayoutOverflow::Scroll {
                assert_eq!(snapshot.node(2).unwrap().scroll_max.y, 70.0);
                assert_eq!(snapshot.node(2).unwrap().scroll_offset.y, 50.0);
            }
        }
    }

    #[test]
    fn absolute_obeys_positioned_scroller_and_absolute_own_clip() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        let mut relative = node(2, 10.0, 10.0, 30.0, 30.0);
        relative.position = Some(Position::Relative);
        relative.overflow_y = LayoutOverflow::Scroll;
        let mut absolute = node(3, 0.0, 50.0, 20.0, 20.0);
        absolute.position = Some(Position::Absolute);
        absolute.overflow_y = LayoutOverflow::Hidden;
        absolute.children.push(node(4, 0.0, 15.0, 10.0, 20.0));
        relative.children.push(absolute);
        root.children.push(relative);
        let snapshot = build(&root, &BTreeMap::from([(2, vec2(0.0, 40.0))]));
        assert_eq!(snapshot.node(3).unwrap().content_box.min, pos2(30.0, 60.0));
        assert_eq!(snapshot.hit_test(pos2(35.0, 95.0), |k| k == 4), Some(4));
        // The containing scroller and the absolute node's own clip both apply.
        assert_eq!(snapshot.hit_test(pos2(35.0, 105.0), |k| k == 4), None);
    }

    #[test]
    fn visibility_hidden_retains_geometry_display_hidden_drops_subtree() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        let mut hidden = node(2, 0.0, 0.0, 30.0, 30.0);
        hidden.visibility = Some(Visibility::Hidden);
        hidden.children.push(node(3, 0.0, 0.0, 20.0, 20.0));
        root.children.push(hidden);
        root.children.push(node(4, 0.0, 0.0, 30.0, 30.0));
        let mut absent = Container::default();
        absent.hidden = Some(true);
        root.children.push(absent);
        let snapshot = build(&root, &BTreeMap::new());
        assert!(!snapshot.node(2).unwrap().visible);
        assert!(!snapshot.node(3).unwrap().visible);
        assert_eq!(snapshot.paint_order().count(), 4);
        assert_eq!(snapshot.hit_test(pos2(15.0, 25.0), |_| true), Some(4));
    }

    #[test]
    fn axis_clip_and_nested_scroll_do_not_leak_overflow_to_outer_scroller() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.overflow_y = LayoutOverflow::Scroll;
        let mut inner = node(2, 10.0, 10.0, 40.0, 40.0);
        inner.overflow_y = LayoutOverflow::Scroll;
        inner.children.push(node(3, 0.0, 80.0, 80.0, 20.0));
        root.children.push(inner);
        let snapshot = build(&root, &BTreeMap::from([(2, vec2(0.0, 60.0))]));
        assert_eq!(snapshot.node(1).unwrap().scroll_max, Vec2::ZERO);
        assert_eq!(snapshot.node(2).unwrap().scroll_max, vec2(0.0, 60.0));
        // X remains unclipped despite the vertical scroller's narrow width.
        assert_eq!(snapshot.hit_test(pos2(150.0, 110.0), |k| k == 3), Some(3));
        assert_eq!(snapshot.hit_test(pos2(150.0, 130.0), |k| k == 3), None);
        assert!(snapshot.node(3).unwrap().visible_bounds().is_some());
    }

    #[test]
    fn explicit_visible_child_and_missing_or_duplicate_identity() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.visibility = Some(Visibility::Hidden);
        let mut child = node(2, 0.0, 0.0, 20.0, 20.0);
        child.visibility = Some(Visibility::Visible);
        root.children.push(child);
        let snapshot = build(&root, &BTreeMap::new());
        assert!(snapshot.node(2).unwrap().visible);
        let viewport = GeometryViewport {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            zoom: 1.0,
        };
        assert_eq!(
            GeometrySnapshot::<usize>::build(&root, viewport, 0, &BTreeMap::new(), |_| None)
                .unwrap_err(),
            GeometryError::MissingIdentity(1)
        );
        assert_eq!(
            GeometrySnapshot::build(&root, viewport, 0, &BTreeMap::new(), |_| Some(1)).unwrap_err(),
            GeometryError::DuplicateIdentity(2)
        );
    }

    #[test]
    fn forest_paths_preserve_hidden_indices_and_duplicate_numeric_ids() {
        let mut root = node(0, 0.0, 0.0, 100.0, 100.0);
        let mut hidden = node(0, 0.0, 0.0, 5.0, 5.0);
        hidden.hidden = Some(true);
        root.children.push(hidden);
        root.children.push(node(0, 5.0, 5.0, 10.0, 10.0));
        let roots = vec![root, node(0, 0.0, 0.0, 100.0, 100.0)];
        let keys = BTreeMap::from([(vec![0], 10), (vec![0, 1], 11), (vec![1], 12)]);
        let viewport = GeometryViewport {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            zoom: 1.0,
        };
        let snapshot =
            GeometrySnapshot::build_roots(&roots, viewport, 0, &BTreeMap::new(), |path, _| {
                keys.get(path).copied()
            })
            .unwrap();
        assert_eq!(
            snapshot.paint_order().map(|n| n.key).collect::<Vec<_>>(),
            vec![10, 11, 12]
        );
        assert_eq!(snapshot.node(11).unwrap().parent, Some(10));
        assert_eq!(snapshot.hit_test(pos2(6.0, 6.0), |_| true), Some(12));
    }

    #[test]
    fn sticky_and_missing_geometry_are_errors() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.position = Some(Position::Sticky);
        let viewport = GeometryViewport {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            zoom: 1.0,
        };
        assert_eq!(
            GeometrySnapshot::build(&root, viewport, 0, &BTreeMap::new(), |c| Some(c.id))
                .unwrap_err(),
            GeometryError::UnsupportedSticky(1)
        );
        root.position = None;
        root.calculated_width = None;
        assert_eq!(
            GeometrySnapshot::build(&root, viewport, 0, &BTreeMap::new(), |c| Some(c.id))
                .unwrap_err(),
            GeometryError::InvalidCalculatedGeometry(1)
        );
    }
}
