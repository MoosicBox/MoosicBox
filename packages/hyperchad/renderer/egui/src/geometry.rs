//! Geometry projected from HyperChad's calculated tree, never from egui layout.
//!
//! Coordinates are egui logical points. Borders overlay the padding box, matching
//! the calculator's bounding helpers (which do not reserve border widths).
//! Only `Relative` establishes an absolute containing block in the current
//! calculator. Fixed coordinates resolve and project relative to the viewport.
//! Sticky uses the shared axis constraint solver without altering normal flow.

use std::collections::BTreeMap;

use eframe::egui::{Pos2, Rect, Vec2, vec2};
use hyperchad_transformer::{
    Container,
    layout::{StickyAxis, sticky_offset},
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
    /// Invalid sticky inset or constraint geometry.
    InvalidStickyGeometry(usize),
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
            Self::InvalidStickyGeometry(id) => {
                write!(f, "invalid sticky geometry for container {id}")
            }
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
    paint_indices: Vec<usize>,
}

#[derive(Clone, Copy)]
struct Placement {
    parent: Option<usize>,
    geometry_parent: Option<usize>,
    relative: Option<usize>,
    origin: Pos2,
    visible: bool,
    sticky_insets: Option<[Option<f32>; 4]>,
    view_size: Vec2,
}

impl<K: Copy + Ord> GeometrySnapshot<K> {
    /// Projects already-calculated boxes. Never runs a second layout engine.
    ///
    /// Paint order is DOM preorder, with sticky subtrees above ordinary sibling
    /// subtrees. There is no authored z-index model. Fixed subtrees restart at the viewport
    /// clip and escape ancestor scrolling. Negative overflow is recorded but
    /// scrolling is intentionally nonnegative. Hidden subtrees need no identities.
    ///
    /// # Errors
    /// * Returns an error for invalid/missing geometry or identities, duplicate
    ///   identities, invalid viewport/scroll input, or invalid sticky constraints.
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
            paint_indices: Vec::new(),
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
                    sticky_insets: None,
                    view_size: viewport.logical_size()?,
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
        snapshot.project_nodes(&placements, &overflow, viewport)?;
        // Sticky subtrees paint above ordinary sibling subtrees. There is no
        // authored z-index model; preserve DOM order inside each group.
        let mut ranks: Vec<Vec<(bool, usize)>> = Vec::with_capacity(placements.len());
        for (index, placement) in placements.iter().enumerate() {
            let mut rank = placement.parent.map_or_else(Vec::new, |p| ranks[p].clone());
            rank.push((placement.sticky_insets.is_some(), index));
            ranks.push(rank);
        }
        snapshot.paint_indices = (0..snapshot.nodes.len()).collect();
        snapshot
            .paint_indices
            .sort_by(|&a, &b| ranks[a].cmp(&ranks[b]));
        Ok(snapshot)
    }

    fn project_nodes(
        &mut self,
        placements: &[Placement],
        overflow: &[(LayoutOverflow, LayoutOverflow)],
        viewport: GeometryViewport,
    ) -> Result<(), GeometryError> {
        // Absolute nodes follow their containing block for scroll and overflow,
        // not intervening static DOM ancestors. CSS2 11.1 excludes clipping by
        // ancestors between an absolute box and its containing block. The
        // containing block's own clip/scroll and all its inherited clips remain.
        // Descendants of that absolute box still inherit its own overflow clip.
        // Only this projection applies zoom and scroll. Local calculator outputs
        // remain untouched, including grid/wrap positions and reserved gutters.
        let mut translations = vec![Vec2::ZERO; self.nodes.len()];
        let mut sticky_translations = vec![Vec2::ZERO; self.nodes.len()];
        for i in 0..self.nodes.len() {
            let n = &self.nodes[i];
            let parent = placements[i].geometry_parent;
            let (translation, clip) = if let Some(parent) = parent.filter(|_| !n.fixed) {
                (
                    translations[parent] - self.nodes[parent].scroll_offset,
                    self.nodes[parent].descendant_clip,
                )
            } else {
                (Vec2::ZERO, viewport.rect)
            };
            // Sticky is a DOM-subtree displacement, even when an absolute
            // descendant skips static ancestors for its scroll/clip chain.
            let inherited_sticky = if n.fixed {
                Vec2::ZERO
            } else {
                placements[i]
                    .parent
                    .map_or(Vec2::ZERO, |p| sticky_translations[p])
            };
            let geometry_sticky = parent.map_or(Vec2::ZERO, |p| sticky_translations[p]);
            let translation =
                translation + inherited_sticky - if n.fixed { Vec2::ZERO } else { geometry_sticky };
            translations[i] = translation;
            let project = |r: Rect| {
                Rect::from_min_max(
                    viewport.rect.min + (r.min.to_vec2() + translation) * viewport.zoom,
                    viewport.rect.min + (r.max.to_vec2() + translation) * viewport.zoom,
                )
            };
            let displacement = self.sticky_displacement(
                i,
                placements,
                overflow,
                viewport,
                (
                    project(self.nodes[i].border_box),
                    project(self.nodes[i].margin_box),
                ),
            )?;
            translations[i] += displacement / viewport.zoom;
            sticky_translations[i] = inherited_sticky + displacement / viewport.zoom;
            let n = &mut self.nodes[i];
            n.margin_box = project(n.margin_box);
            n.padding_box = project(n.padding_box);
            n.border_box = project(n.border_box);
            n.content_box = project(n.content_box);
            n.margin_box = n.margin_box.translate(displacement);
            n.padding_box = n.padding_box.translate(displacement);
            n.border_box = n.border_box.translate(displacement);
            n.content_box = n.content_box.translate(displacement);
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
        Ok(())
    }

    fn sticky_displacement(
        &self,
        index: usize,
        placements: &[Placement],
        overflow: &[(LayoutOverflow, LayoutOverflow)],
        viewport: GeometryViewport,
        boxes: (Rect, Rect),
    ) -> Result<Vec2, GeometryError> {
        let (normal, margin) = boxes;
        let Some(insets) = placements[index].sticky_insets else {
            return Ok(Vec2::ZERO);
        };
        let containing = placements[index].parent.map_or(viewport.rect, |p| {
            let parent = &self.nodes[p];
            let mut bounds = parent
                .content_box
                .translate(-parent.scroll_offset * viewport.zoom);
            // A scroll container's content extends through its scrollable
            // area; a non-scrolling wrapper still constrains sticky to
            // its own allocated content height/width.
            if scrolls(overflow[p].0) {
                bounds.max.x = parent
                    .scroll_extent
                    .max
                    .x
                    .mul_add(viewport.zoom, bounds.min.x);
            }
            if scrolls(overflow[p].1) {
                bounds.max.y = parent
                    .scroll_extent
                    .max
                    .y
                    .mul_add(viewport.zoom, bounds.min.y);
            }
            bounds
        });
        let mut displacement = Vec2::ZERO;
        for axis in 0..2 {
            let mut ancestor = placements[index].parent;
            let mut port = viewport.rect;
            while let Some(p) = ancestor {
                let mode = if axis == 0 {
                    overflow[p].0
                } else {
                    overflow[p].1
                };
                if clips(mode) {
                    port = self.nodes[p].padding_box;
                    break;
                }
                if self.nodes[p].fixed {
                    break;
                }
                ancestor = placements[p].parent;
            }
            // The position box uses margins, reduced where the normal
            // margin already crosses its containing-block boundary.
            let start_margin =
                (normal.min[axis] - margin.min[axis]).min(normal.min[axis] - containing.min[axis]);
            let end_margin =
                (margin.max[axis] - normal.max[axis]).min(containing.max[axis] - normal.max[axis]);
            displacement[axis] = sticky_offset(StickyAxis {
                normal_start: normal.min[axis],
                size: normal.size()[axis],
                containing_start: containing.min[axis] + start_margin,
                containing_end: containing.max[axis] - end_margin,
                scrollport_start: port.min[axis],
                scrollport_end: port.max[axis],
                start_inset: insets[axis * 2].map(|x| x * viewport.zoom),
                end_inset: insets[axis * 2 + 1].map(|x| x * viewport.zoom),
            })
            .map_err(|_| GeometryError::InvalidStickyGeometry(self.nodes[index].container_id))?;
        }
        Ok(displacement)
    }

    /// Looks up geometry without consulting mutable document state.
    pub fn node(&self, key: K) -> Option<&NodeGeometry<K>> {
        self.indices.get(&key).map(|&i| &self.nodes[i])
    }

    /// Nodes in actual paint order. Paint must use this same ordering.
    pub fn paint_order(&self) -> impl DoubleEndedIterator<Item = &NodeGeometry<K>> {
        self.paint_indices.iter().map(|&i| &self.nodes[i])
    }

    /// Topmost visible, unclipped box accepted by the caller's interaction policy.
    /// Margins are never hit targets. Opacity zero is not visibility-hidden.
    pub fn hit_test(&self, point: Pos2, accepts: impl Fn(K) -> bool) -> Option<K> {
        self.paint_order()
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
        let view_size = placement.view_size;
        let sticky_insets = if tree.position == Some(Position::Sticky) {
            let mut basis = view_size;
            for axis in 0..2 {
                let mut ancestor = placement.parent;
                while let Some(p) = ancestor {
                    let mode = if axis == 0 {
                        overflow[p].0
                    } else {
                        overflow[p].1
                    };
                    if clips(mode) {
                        basis[axis] = self.nodes[p].padding_box.size()[axis];
                        break;
                    }
                    if self.nodes[p].fixed {
                        break;
                    }
                    ancestor = placements[p].parent;
                }
            }
            Some([
                tree.left
                    .as_ref()
                    .map(|n| n.calc(basis.x, view_size.x, view_size.y)),
                tree.right
                    .as_ref()
                    .map(|n| n.calc(basis.x, view_size.x, view_size.y)),
                tree.top
                    .as_ref()
                    .map(|n| n.calc(basis.y, view_size.x, view_size.y)),
                tree.bottom
                    .as_ref()
                    .map(|n| n.calc(basis.y, view_size.x, view_size.y)),
            ])
        } else {
            None
        };
        placements.push(Placement {
            geometry_parent: containing,
            sticky_insets,
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
                    sticky_insets: None,
                    view_size,
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

const fn clip_axes(mut rect: Rect, clip: Rect, overflow: (LayoutOverflow, LayoutOverflow)) -> Rect {
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
    fn nested_relative_fixed_projection_ignores_ancestor_scroll_and_clip() {
        // Calculator regression nested_fixed_percentages_use_viewport_absolute_uses_relative_container
        // verifies these calculated fixed outputs for a 400x200 viewport.
        let mut root = node(1, 0.0, 0.0, 400.0, 200.0);
        root.overflow_y = LayoutOverflow::Scroll;
        root.children.push(node(2, 0.0, 300.0, 10.0, 10.0));
        let mut relative = node(3, 20.0, 30.0, 100.0, 60.0);
        relative.position = Some(Position::Relative);
        relative.overflow_y = LayoutOverflow::Hidden;
        let mut middle = node(4, 5.0, 5.0, 20.0, 20.0);
        let mut fixed = node(5, 160.0, 60.0, 200.0, 100.0);
        fixed.position = Some(Position::Fixed);
        fixed.children.push(node(6, 3.0, 4.0, 10.0, 10.0));
        middle.children.push(fixed);
        relative.children.push(middle);
        root.children.push(relative);
        let before = build(&root, &BTreeMap::new());
        let after = build(&root, &BTreeMap::from([(1, vec2(0.0, 80.0))]));
        assert_ne!(
            before.node(3).unwrap().content_box,
            after.node(3).unwrap().content_box
        );
        for key in [5, 6] {
            assert_eq!(
                before.node(key).unwrap().content_box,
                after.node(key).unwrap().content_box
            );
            assert_eq!(
                before.node(key).unwrap().paint_clip,
                after.node(key).unwrap().paint_clip
            );
        }
        assert_eq!(after.node(5).unwrap().content_box.min, pos2(330.0, 140.0));
        assert_eq!(after.hit_test(pos2(350.0, 150.0), |key| key == 5), Some(5));
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
        let absent = Container {
            hidden: Some(true),
            ..Container::default()
        };
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
    fn modal_top_zero_header_moves_subtree_without_changing_flow() {
        use hyperchad_transformer::Number;

        // Same positioning/overflow contract as native UI modal(): fixed host,
        // overflow-auto dialog and a sticky top=0 header before dialog content.
        let mut modal = node(1, 0.0, 0.0, 100.0, 100.0);
        modal.position = Some(Position::Fixed);
        let mut dialog = node(2, 5.0, 5.0, 80.0, 80.0);
        dialog.overflow_y = LayoutOverflow::Auto;
        let mut header = node(3, 0.0, 0.0, 80.0, 20.0);
        header.position = Some(Position::Sticky);
        header.top = Some(Number::Integer(0));
        header.children.push(node(4, 5.0, 4.0, 10.0, 10.0));
        // Absolute DOM descendants also move with a sticky ancestor even when
        // their containing-block scroll chain skips that ancestor.
        let mut absolute = node(7, 10.0, 10.0, 5.0, 5.0);
        absolute.position = Some(Position::Absolute);
        header.children.push(absolute);
        let mut fixed = node(8, 60.0, 60.0, 5.0, 5.0);
        fixed.position = Some(Position::Fixed);
        header.children.push(fixed);
        dialog.children.push(header);
        dialog.children.push(node(5, 0.0, 20.0, 80.0, 180.0));
        modal.children.push(dialog);
        let before = build(&modal, &BTreeMap::new());
        let after = build(&modal, &BTreeMap::from([(2, vec2(0.0, 40.0))]));
        assert_eq!(
            after.node(8).unwrap().border_box,
            before.node(8).unwrap().border_box
        );
        assert_eq!(
            after.node(8).unwrap().paint_clip,
            before.node(8).unwrap().paint_clip
        );
        assert_eq!(after.node(3).unwrap().content_box.min, pos2(20.0, 30.0));
        assert_eq!(
            after.node(3).unwrap().border_box,
            before.node(3).unwrap().border_box
        );
        assert_eq!(after.node(4).unwrap().content_box.min, pos2(30.0, 38.0));
        assert_eq!(
            after.node(7).unwrap().content_box.min.y - before.node(7).unwrap().content_box.min.y,
            80.0
        );
        assert_eq!(after.hit_test(pos2(35.0, 40.0), |_| true), Some(4));
        assert_eq!(
            after.node(4).unwrap().visible_bounds(),
            Some(after.node(4).unwrap().border_box)
        );
        assert_eq!(
            after.node(2).unwrap().scroll_max,
            before.node(2).unwrap().scroll_max
        );
        assert_eq!(
            after.node(5).unwrap().content_box.min.y,
            before.node(5).unwrap().content_box.min.y - 80.0
        );
        assert_eq!(modal.children[0].children[0].calculated_y, Some(0.0));
    }

    #[test]
    fn parsed_modal_header_uses_shared_calculator_flow_then_sticky_projection() {
        use hyperchad_transformer::layout::{
            Calc as _,
            calc::{Calculator, CalculatorDefaults},
            font::{FontMetrics, FontMetricsBounds},
        };

        struct NoText;
        impl FontMetrics for NoText {
            fn measure_text(&self, _: &str, _: f32, _: f32) -> FontMetricsBounds {
                FontMetricsBounds { rows: Vec::new() }
            }
        }
        let defaults = CalculatorDefaults {
            font_size: 14.0,
            font_margin_top: 0.0,
            font_margin_bottom: 0.0,
            h1_font_size: 28.0,
            h1_font_margin_top: 0.0,
            h1_font_margin_bottom: 0.0,
            h2_font_size: 21.0,
            h2_font_margin_top: 0.0,
            h2_font_margin_bottom: 0.0,
            h3_font_size: 18.0,
            h3_font_margin_top: 0.0,
            h3_font_margin_bottom: 0.0,
            h4_font_size: 14.0,
            h4_font_margin_top: 0.0,
            h4_font_margin_bottom: 0.0,
            h5_font_size: 12.0,
            h5_font_margin_top: 0.0,
            h5_font_margin_bottom: 0.0,
            h6_font_size: 10.0,
            h6_font_margin_top: 0.0,
            h6_font_margin_bottom: 0.0,
        };
        // Modal's authored position/overflow/top contract, with explicit sizes
        // to isolate sticky behavior from unrelated intrinsic-control gaps.
        let mut tree: Container = "<div sx-position=\"fixed\" sx-width=\"100\" sx-height=\"100\"><div sx-width=\"80\" sx-height=\"80\" sx-overflow-y=\"auto\"><div sx-position=\"sticky\" sx-top=\"0\" sx-width=\"80\" sx-height=\"20\"></div><div sx-width=\"80\" sx-height=\"180\"></div></div></div>".parse().unwrap();
        tree.calculated_width = Some(100.0);
        tree.calculated_height = Some(100.0);
        Calculator::new(NoText, defaults).calc(&mut tree);
        let viewport = GeometryViewport {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            zoom: 1.0,
        };
        // Resolve by structural path, never parser-generated numeric IDs.
        let resolve = |path: &[usize], _: &Container| match path {
            [0] => Some(1),
            [0, 0] => Some(2),
            [0, 0, 0] => Some(3),
            [0, 0, 0, 0] => Some(4),
            [0, 0, 0, 1] => Some(5),
            _ => None,
        };
        let before = GeometrySnapshot::build_roots(
            std::slice::from_ref(&tree),
            viewport,
            1,
            &BTreeMap::new(),
            resolve,
        )
        .unwrap();
        let after = GeometrySnapshot::build_roots(
            std::slice::from_ref(&tree),
            viewport,
            2,
            &BTreeMap::from([(3, vec2(0.0, 40.0))]),
            resolve,
        )
        .unwrap();
        assert_eq!(
            after.node(4).unwrap().border_box.min.y,
            after.node(3).unwrap().content_box.min.y
        );
        assert_eq!(
            after.node(4).unwrap().border_box,
            before.node(4).unwrap().border_box
        );
        assert_eq!(
            after.node(5).unwrap().border_box.min.y,
            before.node(5).unwrap().border_box.min.y - 40.0
        );
    }

    #[test]
    fn sticky_uses_nearest_nested_scrollport_and_containing_end() {
        use hyperchad_transformer::Number;

        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.overflow_y = LayoutOverflow::Scroll;
        let mut inner = node(2, 0.0, 40.0, 80.0, 50.0);
        inner.overflow_y = LayoutOverflow::Auto;
        let mut section = node(3, 0.0, 0.0, 80.0, 70.0);
        let mut header = node(4, 0.0, 0.0, 80.0, 10.0);
        header.position = Some(Position::Sticky);
        header.top = Some(Number::Integer(0));
        section.children.push(header);
        section.children.push(node(5, 0.0, 10.0, 80.0, 60.0));
        inner.children.push(section);
        inner.children.push(node(6, 0.0, 70.0, 80.0, 100.0));
        root.children.push(inner);
        root.children.push(node(7, 0.0, 200.0, 10.0, 10.0));
        let active = build(
            &root,
            &BTreeMap::from([(1, vec2(0.0, 20.0)), (2, vec2(0.0, 30.0))]),
        );
        assert_eq!(
            active.node(4).unwrap().content_box.min.y,
            active.node(2).unwrap().content_box.min.y
        );
        let ended = build(
            &root,
            &BTreeMap::from([(1, vec2(0.0, 20.0)), (2, vec2(0.0, 90.0))]),
        );
        assert_eq!(
            ended.node(4).unwrap().border_box.max.y,
            ended.node(3).unwrap().content_box.max.y
        );
        assert!(ended.node(4).unwrap().visible_bounds().is_none());
    }

    #[test]
    fn sticky_both_axes_no_inset_and_oversized_scrollport() {
        use hyperchad_transformer::Number;

        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.overflow_x = LayoutOverflow::Scroll;
        root.overflow_y = LayoutOverflow::Scroll;
        let mut sticky = node(2, 20.0, 20.0, 150.0, 150.0);
        sticky.position = Some(Position::Sticky);
        sticky.left = Some(Number::IntegerPercent(10));
        sticky.top = Some(Number::Integer(0));
        sticky.bottom = Some(Number::Integer(0));
        root.children.push(sticky);
        root.children.push(node(3, 300.0, 300.0, 10.0, 10.0));
        let active = build(&root, &BTreeMap::from([(1, vec2(60.0, 60.0))]));
        assert_eq!(active.node(2).unwrap().border_box.min, pos2(30.0, 20.0));
        root.children[0].left = None;
        root.children[0].top = None;
        root.children[0].bottom = None;
        let unpinned = build(&root, &BTreeMap::from([(1, vec2(60.0, 60.0))]));

        assert_eq!(unpinned.node(2).unwrap().border_box.min, pos2(-70.0, -60.0));
    }

    #[test]
    fn sticky_end_insets_and_hidden_scrollport_keep_ancestor_clipping() {
        use hyperchad_transformer::Number;

        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.overflow_y = LayoutOverflow::Scroll;
        let mut hidden_port = node(2, 0.0, 20.0, 100.0, 40.0);
        hidden_port.overflow_y = LayoutOverflow::Hidden;
        let mut sticky = node(3, 0.0, 60.0, 20.0, 10.0);
        sticky.position = Some(Position::Sticky);
        sticky.bottom = Some(Number::Integer(5));
        hidden_port.children.push(sticky);
        root.children.push(hidden_port);
        root.children.push(node(4, 0.0, 200.0, 10.0, 10.0));
        let snapshot = build(&root, &BTreeMap::from([(1, vec2(0.0, 30.0))]));
        let port = snapshot.node(2).unwrap();
        let sticky = snapshot.node(3).unwrap();
        assert_eq!(sticky.border_box.bottom(), port.padding_box.bottom() - 10.0);
        assert_eq!(sticky.paint_clip, port.descendant_clip);
        assert_eq!(
            snapshot.hit_test(sticky.border_box.center(), |key| key == 3),
            Some(3)
        );
    }

    #[test]
    fn invalid_sticky_inset_and_missing_geometry_are_errors() {
        let mut root = node(1, 0.0, 0.0, 100.0, 100.0);
        root.position = Some(Position::Sticky);
        root.top = Some(hyperchad_transformer::Number::Real(f32::NAN));
        let viewport = GeometryViewport {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            zoom: 1.0,
        };
        assert_eq!(
            GeometrySnapshot::build(&root, viewport, 0, &BTreeMap::new(), |c| Some(c.id))
                .unwrap_err(),
            GeometryError::InvalidStickyGeometry(1)
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
