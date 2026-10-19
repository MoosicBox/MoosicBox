//! Immutable geometry with indexed, transactional viewport projections.

use super::{
    Arc, BTreeMap, BTreeSet, CellRect, InlineFragment, LayoutTree, ResolvedNode, ScrollOffset,
    Size, clip_axis, signed,
};

/// Deterministic work performed while building a viewport candidate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectionWork {
    pub spatial_entries: usize,
    /// Ancestor/node geometry cache misses during candidate construction.
    pub geometry_nodes: usize,
    pub scrollports: usize,
    pub projected_nodes: usize,
    pub projected_fragments: usize,
    pub projected_regions: usize,
}

/// Immutable full geometry and indexes, shared without copy-on-write mutation.
#[derive(Clone)]
pub struct RetainedLayout(Arc<Backing>);

struct Backing {
    tree: Arc<LayoutTree>,
    parents: Vec<Option<usize>>,
    descendant_dependencies: Vec<Option<usize>>,
    groups: BTreeMap<Option<usize>, Spatial>,
}

#[derive(Clone, Copy)]
enum Item {
    Bounds(usize),
    Content(usize),
    Text(usize, usize),
    Region(usize, usize),
    Port(usize),
}

struct Entry {
    bounds: CellRect,
    item: Item,
}

enum Spatial {
    Empty,
    Leaf(Entry),
    Branch {
        bounds: CellRect,
        left: Box<Self>,
        right: Box<Self>,
    },
}

impl Spatial {
    fn bounds(&self) -> CellRect {
        match self {
            Self::Empty => CellRect::default(),
            Self::Leaf(entry) => entry.bounds,
            Self::Branch { bounds, .. } => *bounds,
        }
    }

    fn build(mut entries: Vec<Entry>) -> Self {
        match entries.len() {
            0 => Self::Empty,
            1 => Self::Leaf(entries.pop().unwrap()),
            _ => {
                let bounds = entries
                    .iter()
                    .map(|entry| entry.bounds)
                    .reduce(CellRect::union)
                    .unwrap();
                if bounds.width > bounds.height {
                    entries.sort_by_key(|entry| entry.bounds.x + signed(entry.bounds.width) / 2);
                } else {
                    entries.sort_by_key(|entry| entry.bounds.y + signed(entry.bounds.height) / 2);
                }
                let right = entries.split_off(entries.len() / 2);
                Self::Branch {
                    bounds,
                    left: Box::new(Self::build(entries)),
                    right: Box::new(Self::build(right)),
                }
            }
        }
    }

    fn query(&self, window: CellRect, work: &mut ProjectionWork, found: &mut Vec<Item>) {
        work.spatial_entries += 1;
        if !intersects(self.bounds(), window) {
            return;
        }
        match self {
            Self::Empty => {}
            Self::Leaf(entry) => found.push(entry.item),
            Self::Branch { left, right, .. } => {
                left.query(window, work, found);
                right.query(window, work, found);
            }
        }
    }
}

fn intersects(a: CellRect, b: CellRect) -> bool {
    let intersection = a.intersection(b);
    intersection.width > 0 && intersection.height > 0
}

impl RetainedLayout {
    /// Index an existing complete layout without cloning its nodes. Rebuild after
    /// source, viewport, metrics or intrinsic geometry changes, not on scrolling.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one-time construction of immutable geometry indexes"
    )]
    pub fn new(tree: Arc<LayoutTree>) -> Self {
        let parents: Vec<_> = tree
            .nodes
            .iter()
            .map(|node| {
                node.parent_key
                    .as_ref()
                    .and_then(|key| tree.indices.get(key))
                    .copied()
            })
            .collect();
        let mut owners = vec![None; tree.nodes.len()];
        let mut dependencies = vec![None; tree.nodes.len()];
        let mut entries: BTreeMap<Option<usize>, Vec<Entry>> = BTreeMap::new();
        for (ordinal, node) in tree.nodes.iter().enumerate() {
            let flags = &tree.scroll_index.nodes[ordinal];
            owners[ordinal] = if flags.fixed {
                None
            } else {
                parents[ordinal].and_then(|parent| {
                    let parent_flags = &tree.scroll_index.nodes[parent];
                    if parent_flags.scroll.x() || parent_flags.scroll.y() {
                        Some(parent)
                    } else {
                        owners[parent]
                    }
                })
            };
            let group = entries.entry(owners[ordinal]).or_default();
            if node.visible {
                group.push(Entry {
                    bounds: node.bounds,
                    item: Item::Bounds(ordinal),
                });
                group.push(Entry {
                    bounds: node.content_bounds,
                    item: Item::Content(ordinal),
                });
                group.extend(
                    node.hit_regions
                        .iter()
                        .enumerate()
                        .map(|(index, &bounds)| Entry {
                            bounds,
                            item: Item::Region(ordinal, index),
                        }),
                );
            }
            let text_owner = if flags.scroll.x() || flags.scroll.y() {
                Some(ordinal)
            } else {
                owners[ordinal]
            };
            if node.visible {
                entries
                    .entry(text_owner)
                    .or_default()
                    .extend(node.text.iter().enumerate().map(|(index, fragment)| Entry {
                        bounds: fragment.bounds,
                        item: Item::Text(ordinal, index),
                    }));
            }
            dependencies[ordinal] = flags.dependency;
        }
        // Dependency summaries exclude the port itself, exactly as legacy
        // admission does. They include offscreen descendants, never a node budget.
        let mut descendant_dependencies: Vec<Option<usize>> = vec![None; tree.nodes.len()];
        for ordinal in (0..tree.nodes.len()).rev() {
            if let Some(parent) = parents[ordinal] {
                descendant_dependencies[parent] =
                    match (descendant_dependencies[parent], dependencies[ordinal]) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (a, b) => a.or(b),
                    };
                dependencies[parent] = match (dependencies[parent], dependencies[ordinal]) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
        }
        let mut groups = BTreeMap::new();
        for &port in tree.scroll_index.scrollports.iter().rev() {
            let group = Spatial::build(entries.remove(&Some(port)).unwrap_or_default());
            let flags = &tree.scroll_index.nodes[port];
            let mut bounds = group.bounds();
            // Descendants can overflow on the other axis; only constrain axes
            // actually clipped by this port. This keeps portal queries sound.
            if flags.clip.x() {
                bounds.x = tree.nodes[port].content_bounds.x;
                bounds.width = tree.nodes[port].content_bounds.width;
            }
            if flags.clip.y() {
                bounds.y = tree.nodes[port].content_bounds.y;
                bounds.height = tree.nodes[port].content_bounds.height;
            }
            entries.entry(owners[port]).or_default().push(Entry {
                bounds,
                item: Item::Port(port),
            });
            groups.insert(Some(port), group);
        }
        groups.insert(
            None,
            Spatial::build(entries.remove(&None).unwrap_or_default()),
        );
        Self(Arc::new(Backing {
            tree,
            parents,
            descendant_dependencies,
            groups,
        }))
    }

    /// Project absolute requested offsets. Unsupported positioned dependencies
    /// return `None`; callers must then perform ordinary full layout.
    #[allow(clippy::must_use_candidate)]
    pub fn project(&self, offsets: &BTreeMap<String, ScrollOffset>) -> Option<ViewportProjection> {
        self.project_counted(offsets).0
    }

    /// Like `project`, retaining admission work even when projection rejects.
    #[must_use]
    pub fn project_counted(
        &self,
        offsets: &BTreeMap<String, ScrollOffset>,
    ) -> (Option<ViewportProjection>, ProjectionWork) {
        let mut work = ProjectionWork::default();
        let candidate = self.project_from(offsets, None, &mut work);
        (candidate, work)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "transactionally construct and populate the sparse viewport candidate"
    )]
    fn project_from(
        &self,
        offsets: &BTreeMap<String, ScrollOffset>,
        previous: Option<&ViewportProjection>,
        work: &mut ProjectionWork,
    ) -> Option<ViewportProjection> {
        let backing = &self.0;
        let mut positions = BTreeMap::new();
        let mut changed = 0;
        for &ordinal in &backing.tree.scroll_index.scrollports {
            work.scrollports += 1;
            let source = &backing.tree.nodes[ordinal];
            let flags = &backing.tree.scroll_index.nodes[ordinal];
            let requested = offsets.get(&source.key).copied().unwrap_or_default();
            let next = ScrollOffset {
                x: if flags.scroll.x() {
                    requested.x.min(
                        source
                            .content_extents
                            .width
                            .saturating_sub(source.content_bounds.width),
                    )
                } else {
                    0
                },
                y: if flags.scroll.y() {
                    requested.y.min(
                        source
                            .content_extents
                            .height
                            .saturating_sub(source.content_bounds.height),
                    )
                } else {
                    0
                },
            };
            let before = previous.map_or(source.scroll_offset, |projection| {
                projection.offset(ordinal)
            });
            if next != before {
                changed += 1;
                let depth = source.path.len();
                // Exclude this node's own dependency from its descendant summary.
                if backing.descendant_dependencies[ordinal].is_some_and(|anchor| anchor <= depth) {
                    return None;
                }
            }
            if next != source.scroll_offset {
                positions.insert(ordinal, next);
            }
        }
        if changed > 1 {
            return None;
        }
        let mut projection = ViewportProjection {
            retained: self.clone(),
            positions: Arc::new(positions),
            cache: Arc::new(Cache::default()),
        };
        let mut candidates = BTreeMap::<usize, Candidates>::new();
        projection.query_group(None, &mut candidates, work);
        let mut geometry = BTreeMap::new();
        let mut nodes = Vec::new();
        let mut ordinals = Vec::new();
        for (ordinal, candidates) in candidates {
            let scalar = projection.geometry(ordinal, &mut geometry);
            let source = &backing.tree.nodes[ordinal];
            let bounds_visible = intersects(scalar.bounds, scalar.clip)
                || intersects(scalar.content_bounds, scalar.clip);
            let regions: Vec<_> = candidates
                .regions
                .into_iter()
                .filter_map(|index| {
                    let rect = source.hit_regions[index].translated(scalar.dx, scalar.dy);
                    intersects(rect, scalar.clip).then_some(rect)
                })
                .collect();
            let text: Vec<_> = candidates
                .text
                .into_iter()
                .filter_map(|index| {
                    let fragment = &source.text[index];
                    let bounds = fragment.bounds.translated(scalar.tx, scalar.ty);
                    if !intersects(bounds, scalar.text_clip) {
                        return None;
                    }
                    let mut fragment = fragment.clone();
                    fragment.bounds = bounds;
                    Some(fragment)
                })
                .collect();
            if !bounds_visible && regions.is_empty() && text.is_empty() {
                continue;
            }
            work.projected_nodes += 1;
            work.projected_regions += regions.len();
            work.projected_fragments += text.len();
            let mut node = projection.make_node(ordinal, scalar, regions, text, false);
            // The compact cache intentionally has no full-document adjacency.
            node.children.clear();
            nodes.push(node);
            ordinals.push(ordinal);
        }
        work.geometry_nodes += geometry.len();
        projection.cache = Arc::new(Cache {
            nodes,
            ordinals,
            work: *work,
        });
        Some(projection)
    }
}

#[derive(Default)]
struct Candidates {
    regions: BTreeSet<usize>,
    text: BTreeSet<usize>,
}

#[derive(Default)]
struct Cache {
    nodes: Vec<ResolvedNode>,
    ordinals: Vec<usize>,
    work: ProjectionWork,
}

/// A candidate/committed viewport. Cloning shares immutable cache and backing;
/// reprojection never mutates either the preceding frame or the full layout.
#[derive(Clone)]
pub struct ViewportProjection {
    retained: RetainedLayout,
    positions: Arc<BTreeMap<usize, ScrollOffset>>,
    cache: Arc<Cache>,
}

#[derive(Clone, Copy)]
struct Geometry {
    dx: i64,
    dy: i64,
    tx: i64,
    ty: i64,
    bounds: CellRect,
    content_bounds: CellRect,
    clip: CellRect,
    text_clip: CellRect,
}

impl ViewportProjection {
    /// Prepare another transactional candidate from this committed projection.
    #[allow(clippy::must_use_candidate)]
    pub fn reproject(&self, offsets: &BTreeMap<String, ScrollOffset>) -> Option<Self> {
        self.reproject_counted(offsets).0
    }

    /// Like `reproject`, retaining work for rejected candidates too.
    #[must_use]
    pub fn reproject_counted(
        &self,
        offsets: &BTreeMap<String, ScrollOffset>,
    ) -> (Option<Self>, ProjectionWork) {
        let mut work = ProjectionWork::default();
        let candidate = self.retained.project_from(offsets, Some(self), &mut work);
        (candidate, work)
    }

    #[must_use]
    pub fn viewport(&self) -> Size {
        self.retained.0.tree.viewport
    }

    /// Compact paint-order candidates. Text and hit-region vectors contain only
    /// viewport candidates and children are omitted; use `node` for complete
    /// adjacency and arbitrary-node geometry.
    #[must_use]
    pub fn visible_nodes(&self) -> &[ResolvedNode] {
        &self.cache.nodes
    }

    /// Original full-tree ordinals, corresponding one-to-one to visible nodes.
    #[must_use]
    pub fn visible_ordinals(&self) -> &[usize] {
        &self.cache.ordinals
    }

    #[must_use]
    pub fn work(&self) -> ProjectionWork {
        self.cache.work
    }

    /// Complete transformed node, including offscreen fragments, for explicit
    /// focus/capture and ancestry queries. Cost is this node and its ancestors,
    /// not the number of loaded siblings or descendants.
    #[allow(clippy::must_use_candidate)]
    pub fn node(&self, key: &str) -> Option<ResolvedNode> {
        let &ordinal = self.retained.0.tree.indices.get(key)?;
        let geometry = self.geometry(ordinal, &mut BTreeMap::new());
        let source = &self.retained.0.tree.nodes[ordinal];
        let regions = source
            .hit_regions
            .iter()
            .map(|rect| rect.translated(geometry.dx, geometry.dy))
            .collect();
        let text = source
            .text
            .iter()
            .map(|fragment| {
                let mut fragment = fragment.clone();
                fragment.bounds = fragment.bounds.translated(geometry.tx, geometry.ty);
                fragment
            })
            .collect();
        Some(self.make_node(ordinal, geometry, regions, text, true))
    }

    /// Scalar projected geometry for ancestry/input queries. Omits adjacency,
    /// text and hit regions; unlike `node`, cost is independent of child count.
    #[allow(clippy::must_use_candidate)]
    pub fn geometry_node(&self, key: &str) -> Option<ResolvedNode> {
        let &ordinal = self.retained.0.tree.indices.get(key)?;
        let geometry = self.geometry(ordinal, &mut BTreeMap::new());
        Some(self.make_node(ordinal, geometry, Vec::new(), Vec::new(), false))
    }

    /// Topmost visible source geometry; unlike legacy fallback logic, an empty
    /// filtered region vector never turns an inline span into a bounding-box hit.
    #[allow(clippy::must_use_candidate)]
    pub fn hit_test(&self, x: i64, y: i64) -> Option<&ResolvedNode> {
        self.cache
            .nodes
            .iter()
            .zip(&self.cache.ordinals)
            .rev()
            .find_map(|(node, &ordinal)| {
                let source = &self.retained.0.tree.nodes[ordinal];
                (node.visible
                    && node.clip.contains(x, y)
                    && if source.hit_regions.is_empty() {
                        node.bounds.contains(x, y)
                    } else {
                        node.hit_regions.iter().any(|rect| rect.contains(x, y))
                    })
                .then_some(node)
            })
    }

    fn offset(&self, ordinal: usize) -> ScrollOffset {
        self.positions
            .get(&ordinal)
            .copied()
            .unwrap_or(self.retained.0.tree.nodes[ordinal].scroll_offset)
    }

    fn geometry(&self, ordinal: usize, cache: &mut BTreeMap<usize, Geometry>) -> Geometry {
        if let Some(&geometry) = cache.get(&ordinal) {
            return geometry;
        }
        let backing = &self.retained.0;
        let source = &backing.tree.nodes[ordinal];
        let flags = &backing.tree.scroll_index.nodes[ordinal];
        let parent = backing.parents[ordinal].map(|parent| self.geometry(parent, cache));
        let (dx, dy) = if flags.fixed {
            (0, 0)
        } else {
            backing.parents[ordinal]
                .zip(parent)
                .map_or((0, 0), |(ordinal, geometry)| {
                    let base = backing.tree.nodes[ordinal].scroll_offset;
                    let next = self.offset(ordinal);
                    (
                        geometry.dx + signed(base.x) - signed(next.x),
                        geometry.dy + signed(base.y) - signed(next.y),
                    )
                })
        };
        let base = source.scroll_offset;
        let next = self.offset(ordinal);
        let clip = if flags.fixed {
            source.clip
        } else {
            parent.map_or(source.clip, |geometry| geometry.text_clip)
        };
        let content_bounds = source.content_bounds.translated(dx, dy);
        let mut text_clip = clip;
        if flags.clip.x() {
            text_clip = clip_axis(text_clip, content_bounds, true);
        }
        if flags.clip.y() {
            text_clip = clip_axis(text_clip, content_bounds, false);
        }
        let geometry = Geometry {
            dx,
            dy,
            tx: dx + signed(base.x) - signed(next.x),
            ty: dy + signed(base.y) - signed(next.y),
            bounds: source.bounds.translated(dx, dy),
            content_bounds,
            clip,
            text_clip,
        };
        cache.insert(ordinal, geometry);
        geometry
    }

    fn make_node(
        &self,
        ordinal: usize,
        geometry: Geometry,
        hit_regions: Vec<CellRect>,
        text: Vec<InlineFragment>,
        complete: bool,
    ) -> ResolvedNode {
        let source = &self.retained.0.tree.nodes[ordinal];
        ResolvedNode {
            key: source.key.clone(),
            path: source.path.clone(),
            parent_key: source.parent_key.clone(),
            children: if complete {
                source.children.clone()
            } else {
                Vec::new()
            },
            bounds: geometry.bounds,
            clip: geometry.clip,
            content_bounds: geometry.content_bounds,
            text_clip: geometry.text_clip,
            content_extents: source.content_extents,
            scroll_offset: self.offset(ordinal),
            text_origin: (
                source.text_origin.0 + geometry.tx,
                source.text_origin.1 + geometry.ty,
            ),
            paint_order: source.paint_order,
            visible: source.visible,
            hit_regions,
            text,
            style: source.style.clone(),
        }
    }

    fn query_group(
        &self,
        owner: Option<usize>,
        candidates: &mut BTreeMap<usize, Candidates>,
        work: &mut ProjectionWork,
    ) {
        let backing = &self.retained.0;
        let Some(group) = backing.groups.get(&owner) else {
            return;
        };
        let mut geometry = BTreeMap::new();
        let (dx, dy, clip) = owner.map_or_else(
            || {
                (
                    0,
                    0,
                    CellRect::new(
                        0,
                        0,
                        u64::from(self.viewport().width),
                        u64::from(self.viewport().height),
                    ),
                )
            },
            |ordinal| {
                let geometry = self.geometry(ordinal, &mut geometry);
                (geometry.tx, geometry.ty, geometry.text_clip)
            },
        );
        work.geometry_nodes += geometry.len();
        let mut found = Vec::new();
        group.query(clip.translated(-dx, -dy), work, &mut found);
        for item in found {
            match item {
                Item::Bounds(ordinal) | Item::Content(ordinal) => {
                    candidates.entry(ordinal).or_default();
                }
                Item::Text(ordinal, index) => {
                    candidates.entry(ordinal).or_default().text.insert(index);
                }
                Item::Region(ordinal, index) => {
                    candidates.entry(ordinal).or_default().regions.insert(index);
                }
                Item::Port(ordinal) => self.query_group(Some(ordinal), candidates, work),
            }
        }
    }
}
