//! Canonical view tree and transactional reconciliation.
//!
//! Layout is derived from this tree, never the other way around. Control storage
//! belongs to the controls layer; reports describe which identities it may reuse.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::discriminant;
use std::sync::Arc;

use hyperchad_renderer::{ResponseMetadata, View};
use hyperchad_transformer::{Container, Element, Input};
use hyperchad_transformer_models::{Selector, SwapStrategy};

use crate::layout::node_key;

type Path = Vec<usize>;

#[cfg(test)]
mod lifecycle_provenance_tests {
    use super::*;

    #[test]
    fn unrelated_anonymous_sources_retain_lifetimes_but_not_control_compatibility() {
        let root = Container {
            children: vec![
                Container {
                    str_id: Some("connections".into()),
                    ..Default::default()
                },
                Container::default(),
                Container::default(),
            ],
            ..Default::default()
        };
        let mut state = ViewState::from_root(root).unwrap();
        let report = state
            .apply_view(
                View::from(Container {
                    str_id: Some("connections".into()),
                    children: vec![Container::default()],
                    ..Default::default()
                }),
                Some("connections"),
            )
            .unwrap();
        assert!(
            report.removed.contains("@1"),
            "control identity remains conservative"
        );
        assert!(!report.lifecycle_removed.contains("@1"));
        assert!(!report.lifecycle_removed.contains("@2"));
        assert!(report.lifecycle_removed.contains("connections"));
        let report = state
            .apply_view(View::from(Container::default()), Some("@1"))
            .unwrap();
        assert!(
            report.lifecycle_removed.contains("@1"),
            "identical anonymous replacement is a new source"
        );
        assert!(!report.lifecycle_removed.contains("@2"));
    }

    #[test]
    fn structural_shifts_invalidate_anonymous_but_rebind_explicit_sources() {
        let mut state = ViewState::from_root(Container {
            children: vec![
                Container::default(),
                Container {
                    str_id: Some("keyed".into()),
                    children: vec![Container::default()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
        .unwrap();
        let report = state
            .apply_response(
                View::from(Container::default()),
                Some("@0"),
                SwapStrategy::BeforeBegin,
            )
            .unwrap();
        assert!(report.lifecycle_removed.contains("@0"));
        assert!(report.lifecycle_removed.contains("@1/0"));
        assert!(!report.lifecycle_removed.contains("keyed"));
    }
}

/// A failed application leaves both tree and generation unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    /// An identity occurs more than once in a tree.
    #[error("duplicate node identity: {0}")]
    DuplicateIdentity(String),
    /// A selector or explicit trigger did not resolve.
    #[error("view target not found: {0}")]
    MissingTarget(String),
    /// The response belongs to an older canonical tree.
    #[error("stale view generation {expected}; current generation is {actual}")]
    StaleGeneration {
        /// Generation captured when the request started.
        expected: u64,
        /// Current tree generation.
        actual: u64,
    },
    /// A sibling cannot be inserted outside the canonical root.
    #[error("cannot insert a sibling of the canonical root")]
    RootSiblingInsertion,
    /// No further generation can be allocated.
    #[error("view generation exhausted")]
    GenerationExhausted,
}

/// State-bearing element kind. Different kinds must not share local control state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// Single-line text.
    Text,
    /// Secret text (kept separate from ordinary text).
    Password,
    /// Checkbox.
    Checkbox,
    /// File picker.
    File,
    /// Hidden form value.
    Hidden,
    /// Multi-line text.
    Textarea,
    /// Single selection.
    Select,
    /// Multiple selection.
    MultiSelect,
    /// Disclosure widget.
    Details,
}

/// Externally controlled value. `None` on metadata means uncontrolled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlValue {
    /// Text or selected option value, including an explicitly empty string.
    Text(String),
    /// Checkbox/disclosure state, including explicit false.
    Bool(bool),
}

/// Control compatibility and authoritative external value, without local edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlMetadata {
    /// Control type.
    pub kind: ControlKind,
    /// Form field name; changing this resets local state.
    pub name: Option<String>,
    /// Current authoritative value, or uncontrolled when absent.
    pub value: Option<ControlValue>,
}

/// Reconciliation instruction for one control in the new tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlUpdate {
    /// Canonical layout identity.
    pub key: String,
    /// Whether local state from this key may be retained.
    pub compatible: bool,
    /// Metadata before application, if there was a node with this key.
    pub previous: Option<ControlMetadata>,
    /// New control metadata.
    pub current: ControlMetadata,
    /// A controlled value to apply to local storage. Unchanged external values
    /// do not overwrite local edits. New/reset controls receive their value.
    pub external_value: Option<ControlValue>,
}

/// Effects of a committed transaction. Incompatible reused keys appear in both
/// `removed` and `inserted`, and never in `retained`.
#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    /// Generation after the application.
    pub generation: u64,
    /// New or reset identities.
    pub inserted: BTreeSet<String>,
    /// Deleted or reset identities.
    pub removed: BTreeSet<String>,
    /// Sources actually replaced/deleted or anonymous paths shifted by a swap.
    /// Unlike control compatibility, unrelated anonymous sibling edits do not reset these.
    pub lifecycle_removed: BTreeSet<String>,
    /// Identities compatible with their previous state.
    pub retained: BTreeSet<String>,
    /// Reconciliation instructions for every new-tree stateful control.
    pub controls: Vec<ControlUpdate>,
    /// Response effects are returned intact for the embedding host.
    pub response: ResponseMetadata,
}

/// Retained source identity for derived geometry, independent of async generations.
///
/// Equality is deliberately not exposed: only the owning state can validate a
/// snapshot, including mutable access pending a fresh identity.
#[derive(Debug, Clone, Default)]
pub(crate) struct SourceSnapshot(Arc<()>);

/// Canonical root and generation used to reject stale asynchronous responses.
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    root: Container,
    generation: u64,
    source: SourceSnapshot,
    source_pending: bool,
}

impl ViewState {
    /// Construct an empty, safe canonical root at generation zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct state from a validated existing root.
    ///
    /// # Errors
    /// * Returns [`ApplyError::DuplicateIdentity`] for ambiguous identities.
    pub fn from_root(root: Container) -> Result<Self, ApplyError> {
        index(&root)?;
        Ok(Self {
            root,
            generation: 0,
            ..Self::default()
        })
    }

    /// Borrow the canonical root.
    #[must_use]
    pub const fn root(&self) -> &Container {
        &self.root
    }

    /// Borrow the root for direct mutation without advancing its epoch.
    ///
    /// Call [`Self::mark_changed`] after actual changes, or prefer
    /// [`Self::mutate_root`] for validated, change-detecting transactions.
    /// Merely borrowing the tree must not invalidate asynchronous work, but does
    /// conservatively invalidate retained source snapshots even without a write.
    pub const fn root_mut(&mut self) -> &mut Container {
        // Defer Arc renewal so this public accessor remains const. The pending
        // flag rejects retained geometry immediately, without invalidating async work.
        self.source_pending = true;
        &mut self.root
    }

    /// Capture an O(1) identity for the current source after any mutable access.
    #[must_use]
    pub(crate) fn source_snapshot(&mut self) -> SourceSnapshot {
        if self.source_pending {
            self.renew_source();
        }
        self.source.clone()
    }

    /// Validate a retained source identity without walking the canonical tree.
    #[must_use]
    pub(crate) fn matches_source(&self, snapshot: &SourceSnapshot) -> bool {
        !self.source_pending && Arc::ptr_eq(&self.source.0, &snapshot.0)
    }

    fn renew_source(&mut self) {
        self.source = SourceSnapshot::default();
        self.source_pending = false;
    }

    /// Explicitly advance the tree epoch after a direct mutation.
    ///
    /// # Errors
    /// * Returns a duplicate-identity error if the current tree is invalid.
    /// * Returns [`ApplyError::GenerationExhausted`] if no epoch remains.
    pub fn mark_changed(&mut self) -> Result<u64, ApplyError> {
        index(&self.root)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(ApplyError::GenerationExhausted)?;
        self.renew_source();
        Ok(self.generation)
    }

    /// Mutate a cloned root, validate it, and advance only for an actual change.
    /// Returns whether a change was committed. Failed validation is atomic.
    ///
    /// # Errors
    /// * Returns a duplicate-identity error for the old or candidate tree.
    /// * Returns [`ApplyError::GenerationExhausted`] if a change needs an epoch.
    pub fn mutate_root(&mut self, mutate: impl FnOnce(&mut Container)) -> Result<bool, ApplyError> {
        index(&self.root)?;
        let mut candidate = self.root.clone();
        mutate(&mut candidate);
        index(&candidate)?;
        if candidate == self.root {
            return Ok(false);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ApplyError::GenerationExhausted)?;
        self.root = candidate;
        self.generation = generation;
        self.renew_source();
        Ok(true)
    }

    /// Current canonical generation (capture this before starting async work).
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether an asynchronous result still belongs to the current tree.
    #[must_use]
    pub const fn is_current_generation(&self, generation: u64) -> bool {
        self.generation == generation
    }

    /// Find a node by its exact layout key. Ambiguous invalid trees return `None`.
    #[must_use = "lookup does not modify the canonical tree"]
    pub fn find_node(&self, key: &str) -> Option<&Container> {
        let paths = index(&self.root).ok()?;
        at(&self.root, paths.get(key)?)
    }

    /// Find a mutable node without advancing the epoch merely for borrowing.
    /// Call [`Self::mark_changed`] after actual direct changes.
    pub fn find_node_mut(&mut self, key: &str) -> Option<&mut Container> {
        let path = index(&self.root).ok()?.remove(key)?;
        self.source_pending = true;
        at_mut(&mut self.root, &path)
    }

    /// Apply only if the request's captured generation is still current.
    ///
    /// # Errors
    /// * Returns [`ApplyError::StaleGeneration`] for a stale response.
    /// * Returns any error documented by [`Self::apply_view`].
    pub fn apply_view_if_generation(
        &mut self,
        view: View,
        target: Option<&str>,
        generation: u64,
    ) -> Result<ApplyReport, ApplyError> {
        if generation != self.generation {
            return Err(ApplyError::StaleGeneration {
                expected: generation,
                actual: self.generation,
            });
        }
        self.apply_view(view, target)
    }

    /// Apply a strategy-bearing async response only at its captured generation.
    ///
    /// # Errors
    /// * Returns [`ApplyError::StaleGeneration`] for an obsolete response.
    /// * Returns any error documented by [`Self::apply_response`].
    pub fn apply_response_if_generation(
        &mut self,
        view: View,
        target: Option<&str>,
        strategy: SwapStrategy,
        generation: u64,
    ) -> Result<ApplyReport, ApplyError> {
        if generation != self.generation {
            return Err(ApplyError::StaleGeneration {
                expected: generation,
                actual: self.generation,
            });
        }
        self.apply_response(view, target, strategy)
    }

    /// Apply fragments, deletions, then primary content as one transaction.
    ///
    /// An absent trigger means the root. Explicit triggers accept an exact
    /// layout key, a string ID, or `#id`. ID/class selectors search the whole
    /// tree; child-class selectors search direct children of the trigger.
    /// Multi-match selectors affect all matches, suppressing nested matches
    /// whose ancestor is already replaced/deleted. Deleting the root restores
    /// an empty root. Primary content replaces the trigger, not its children.
    ///
    /// # Errors
    /// * Duplicate string IDs, nonzero numeric IDs, or layout keys are rejected
    ///   before mutation and after every operation. Numeric zero is anonymous.
    /// * Missing selectors/triggers (including a trigger deleted before primary
    ///   application) reject the entire response rather than dropping updates.
    /// * Exhausting the generation counter rejects the application.
    pub fn apply_view(
        &mut self,
        view: View,
        target: Option<&str>,
    ) -> Result<ApplyReport, ApplyError> {
        self.apply_response(view, target, SwapStrategy::This)
    }

    /// Apply a response using the route's primary-content swap strategy.
    ///
    /// Fragments and delete selectors retain their ordinary semantics even for
    /// `None`. `Children` replaces the target's children with the primary
    /// container; insertion strategies insert that container at the named edge.
    /// `Delete` removes the target even when the response has no primary.
    /// `This` and insertion/children strategies do nothing without a primary.
    /// A primary marked as a fragment contributes sibling nodes, never its synthetic
    /// envelope. Explicit containers are inserted intact. Root fragments retain one
    /// canonical synthetic root. Empty insertion fragments are no-ops.
    /// Metadata is returned intact. Root deletion restores the safe empty root.
    /// No-op responses do not advance the generation.
    ///
    /// # Errors
    /// * Returns the validation/selection errors documented by [`Self::apply_view`].
    /// * Returns [`ApplyError::RootSiblingInsertion`] for sibling insertion at root.
    /// * All failures leave the original tree and generation unchanged.
    pub fn apply_response(
        &mut self,
        view: View,
        target: Option<&str>,
        strategy: SwapStrategy,
    ) -> Result<ApplyReport, ApplyError> {
        let old = index(&self.root)?;
        let mut candidate = self.root.clone();
        let mut replaced = Vec::<Path>::new();
        let mut affected = Vec::<(Path, bool)>::new();
        let mut lifecycle_removed = BTreeSet::new();
        let mut anchor = Some(resolve_trigger(&candidate, target)?);
        for fragment in view.fragments {
            index(&fragment.container)?;
            let paths = select(&candidate, &fragment.selector, anchor.as_deref())?;
            for path in outermost(paths).into_iter().rev() {
                affected.push((path.clone(), false));
                invalidate_lifecycles(&candidate, &path, false, &mut lifecycle_removed)?;
                replaced.push(path.clone());
                *at_mut(&mut candidate, &path).ok_or_else(|| missing(&fragment.selector))? =
                    fragment.container.clone();
                // A replacement at/above the trigger detaches the old trigger.
                // Replacing the trigger itself keeps the slot for subsequent
                // SelfTarget/primary operations, even when its identity changes.
                if anchor
                    .as_ref()
                    .is_some_and(|a| a.starts_with(&path) && *a != path)
                {
                    anchor = None;
                }
            }
            index(&candidate)?;
        }
        for selector in view.delete_selectors {
            let paths = select(&candidate, &selector, anchor.as_deref())?;
            for path in outermost(paths).into_iter().rev() {
                affected.push((
                    path[..path.len().saturating_sub(1)].to_vec(),
                    !path.is_empty(),
                ));
                invalidate_lifecycles(&candidate, &path, true, &mut lifecycle_removed)?;
                replaced.push(path.clone());
                delete(&mut candidate, &path);
                adjust_anchor(&mut anchor, &path);
            }
            index(&candidate)?;
        }
        if let Some(scope) = primary_affected_scope(
            view.primary.as_ref(),
            view.primary_is_fragment,
            &strategy,
            anchor.as_deref(),
        ) {
            affected.push(scope);
        }
        apply_primary(
            &mut candidate,
            &mut replaced,
            &mut lifecycle_removed,
            anchor.as_deref(),
            (strategy, view.primary, view.primary_is_fragment),
        )?;
        let new = index(&candidate)?;
        // Replacing identical anonymous content still invalidates that identity;
        // a metadata-only/no-swap response is not a tree mutation.
        let anonymous_reset = has_anonymous_replacements(&candidate, &new, &replaced);
        let tree_changed = affected_changed(&self.root, &candidate, &affected);
        let changed = tree_changed || anonymous_reset;
        let anonymous_children_unchanged =
            anonymous_forest_unchanged(&self.root, &candidate, &affected, tree_changed);
        let generation = if changed {
            self.generation
                .checked_add(1)
                .ok_or(ApplyError::GenerationExhausted)?
        } else {
            self.generation
        };
        let mut report = ApplyReport {
            generation,
            response: view.response,
            lifecycle_removed,
            ..ApplyReport::default()
        };
        for (key, path) in &new {
            let node =
                at(&candidate, path).ok_or_else(|| ApplyError::MissingTarget(key.clone()))?;
            let previous = old.get(key).and_then(|p| at(&self.root, p));
            let compatible = previous.is_some_and(|before| {
                compatible_nodes(before, node)
                    && (has_identity(node)
                        || (!replaced.iter().any(|p| path.starts_with(p))
                            && if path.is_empty() {
                                !tree_changed
                            } else {
                                anonymous_children_unchanged
                            }))
            });
            if compatible {
                report.retained.insert(key.clone());
            } else {
                report.inserted.insert(key.clone());
            }
            record_control_update(&mut report, key, node, previous, compatible);
        }
        record_removed_identities(&self.root, &candidate, &old, &new, &mut report);
        self.root = candidate;
        self.generation = generation;
        if changed {
            self.renew_source();
        }
        Ok(report)
    }
}

fn record_control_update(
    report: &mut ApplyReport,
    key: &str,
    node: &Container,
    previous: Option<&Container>,
    compatible: bool,
) {
    if let Some(current) = control_metadata(node) {
        let previous = previous.and_then(control_metadata);
        let external_value =
            if !compatible || previous.as_ref().is_none_or(|p| p.value != current.value) {
                current.value.clone()
            } else {
                None
            };
        report.controls.push(ControlUpdate {
            key: key.to_owned(),
            compatible,
            previous,
            current,
            external_value,
        });
    }
}

// Control compatibility and lifecycle provenance have different removal rules;
// finalize both against the validated before/after indexes before publication.
fn record_removed_identities(
    before: &Container,
    after: &Container,
    old: &BTreeMap<String, Path>,
    new: &BTreeMap<String, Path>,
    report: &mut ApplyReport,
) {
    report.removed.extend(
        old.keys()
            .filter(|key| !report.retained.contains(*key))
            .cloned(),
    );
    reconcile_lifecycle_identities(before, after, old, new, report);
}

fn reconcile_lifecycle_identities(
    before: &Container,
    after: &Container,
    old: &BTreeMap<String, Path>,
    new: &BTreeMap<String, Path>,
    report: &mut ApplyReport,
) {
    for (key, path) in old {
        if new
            .get(key)
            .and_then(|path| at(after, path))
            .is_none_or(|node| {
                at(before, path).is_none_or(|before| !compatible_nodes(before, node))
            })
        {
            report.lifecycle_removed.insert(key.clone());
        }
    }
}

fn has_anonymous_replacements(
    root: &Container,
    nodes: &BTreeMap<String, Path>,
    replaced: &[Path],
) -> bool {
    nodes.values().any(|path| {
        at(root, path).is_some_and(|node| !has_identity(node))
            && replaced
                .iter()
                .any(|replacement| path.starts_with(replacement))
    })
}

// Collect exact current source keys before a mutation. Anonymous nodes in shifted
// sibling subtrees cannot transfer identity; explicit IDs can rebind by key.
fn invalidate_shifted_lifecycles(
    root: &Container,
    slot: &[usize],
    removed: &mut BTreeSet<String>,
) -> Result<(), ApplyError> {
    let Some((&offset, parent)) = slot.split_last() else {
        return Ok(());
    };
    for (key, path) in index(root)? {
        if path.len() > parent.len()
            && path.starts_with(parent)
            && path[parent.len()] >= offset
            && at(root, &path).is_some_and(|node| !has_identity(node))
        {
            removed.insert(key);
        }
    }
    Ok(())
}

fn invalidate_lifecycles(
    root: &Container,
    boundary: &[usize],
    shifts: bool,
    removed: &mut BTreeSet<String>,
) -> Result<(), ApplyError> {
    for (key, path) in index(root)? {
        if path.starts_with(boundary) {
            removed.insert(key);
        }
    }
    if shifts {
        invalidate_shifted_lifecycles(root, boundary, removed)?;
    }
    Ok(())
}

fn invalidate_primary_lifecycles(
    candidate: &Container,
    path: &[usize],
    strategy: &SwapStrategy,
    removed: &mut BTreeSet<String>,
) -> Result<(), ApplyError> {
    match strategy {
        SwapStrategy::This | SwapStrategy::Delete => {
            invalidate_lifecycles(candidate, path, *strategy == SwapStrategy::Delete, removed)?;
        }
        SwapStrategy::Children => {
            for (key, child) in index(candidate)? {
                if child.len() > path.len() && child.starts_with(path) {
                    removed.insert(key);
                }
            }
        }
        SwapStrategy::AfterBegin => {
            let mut slot = path.to_vec();
            slot.push(0);
            invalidate_shifted_lifecycles(candidate, &slot, removed)?;
        }
        SwapStrategy::BeforeBegin | SwapStrategy::AfterEnd => {
            let mut slot = path.to_vec();
            if *strategy == SwapStrategy::AfterEnd {
                *slot.last_mut().ok_or(ApplyError::RootSiblingInsertion)? += 1;
            }
            invalidate_shifted_lifecycles(candidate, &slot, removed)?;
        }
        SwapStrategy::BeforeEnd | SwapStrategy::None => {}
    }
    Ok(())
}

fn apply_primary(
    candidate: &mut Container,
    replaced: &mut Vec<Path>,
    lifecycle_removed: &mut BTreeSet<String>,
    anchor: Option<&[usize]>,
    swap: (SwapStrategy, Option<Container>, bool),
) -> Result<(), ApplyError> {
    let (strategy, primary, is_fragment) = swap;
    if strategy == SwapStrategy::Delete || (primary.is_some() && strategy != SwapStrategy::None) {
        let path = anchor.ok_or_else(|| ApplyError::MissingTarget("deleted trigger".into()))?;
        if strategy != SwapStrategy::Delete
            && let Some(primary) = &primary
        {
            index(primary)?;
        }
        let nodes = primary
            .map(|primary| {
                if is_fragment {
                    primary.children
                } else {
                    vec![primary]
                }
            })
            .unwrap_or_default();
        // Empty insertion fragments are true no-ops, not lifecycle resets.
        if nodes.is_empty()
            && matches!(
                strategy,
                SwapStrategy::BeforeBegin
                    | SwapStrategy::AfterBegin
                    | SwapStrategy::BeforeEnd
                    | SwapStrategy::AfterEnd
            )
        {
            if path.is_empty()
                && matches!(strategy, SwapStrategy::BeforeBegin | SwapStrategy::AfterEnd)
            {
                return Err(ApplyError::RootSiblingInsertion);
            }
            return Ok(());
        }
        invalidate_primary_lifecycles(candidate, path, &strategy, lifecycle_removed)?;
        if strategy == SwapStrategy::This && !path.is_empty() && nodes.len() != 1 {
            invalidate_shifted_lifecycles(candidate, path, lifecycle_removed)?;
        }
        match strategy {
            SwapStrategy::This if !path.is_empty() => {
                let (&slot, parent) = path.split_last().expect("non-root target");
                let siblings = &mut at_mut(candidate, parent)
                    .ok_or_else(|| ApplyError::MissingTarget("primary parent".into()))?
                    .children;
                let count = nodes.len();
                siblings.splice(slot..=slot, nodes);
                for index in slot..slot + count {
                    let mut inserted = parent.to_vec();
                    inserted.push(index);
                    replaced.push(inserted);
                }
            }
            SwapStrategy::This => {
                replaced.push(path.to_vec());
                // A document fragment still needs one canonical root. Explicit
                // containers retain their identity and all layout attributes.
                *candidate = if is_fragment {
                    Container::from(nodes)
                } else {
                    nodes.into_iter().next().expect("primary checked")
                };
            }
            SwapStrategy::Children | SwapStrategy::AfterBegin | SwapStrategy::BeforeEnd => {
                let node = at_mut(candidate, path)
                    .ok_or_else(|| ApplyError::MissingTarget("primary trigger".into()))?;
                match strategy {
                    SwapStrategy::Children => {
                        node.children = nodes;
                        for index in 0..node.children.len() {
                            let mut child = path.to_vec();
                            child.push(index);
                            replaced.push(child);
                        }
                    }
                    SwapStrategy::AfterBegin => {
                        node.children.splice(0..0, nodes);
                    }
                    SwapStrategy::BeforeEnd => node.children.extend(nodes),
                    _ => unreachable!(),
                }
            }
            SwapStrategy::BeforeBegin | SwapStrategy::AfterEnd => {
                let (&slot, parent) = path.split_last().ok_or(ApplyError::RootSiblingInsertion)?;
                let offset = usize::from(strategy == SwapStrategy::AfterEnd);
                at_mut(candidate, parent)
                    .ok_or_else(|| ApplyError::MissingTarget("primary parent".into()))?
                    .children
                    .splice(slot + offset..slot + offset, nodes);
            }
            SwapStrategy::Delete => {
                replaced.push(path.to_vec());
                delete(candidate, path);
            }
            SwapStrategy::None => unreachable!(),
        }
    }
    Ok(())
}

#[cfg(test)]
mod primary_fragment_tests {
    use super::*;

    fn node(id: &str) -> Container {
        Container {
            str_id: Some(id.into()),
            ..Default::default()
        }
    }

    fn initial() -> Container {
        Container {
            children: vec![
                node("left"),
                Container {
                    children: vec![node("old")],
                    ..node("target")
                },
                node("right"),
            ],
            ..node("root")
        }
    }

    const STRATEGIES: [SwapStrategy; 8] = [
        SwapStrategy::This,
        SwapStrategy::Children,
        SwapStrategy::BeforeBegin,
        SwapStrategy::AfterBegin,
        SwapStrategy::BeforeEnd,
        SwapStrategy::AfterEnd,
        SwapStrategy::Delete,
        SwapStrategy::None,
    ];

    #[test]
    fn fragment_swaps_preserve_order_metadata_and_lifecycles_for_empty_and_multiple_nodes() {
        for strategy in STRATEGIES {
            for count in [0, 1, 2] {
                let mut state = ViewState::from_root(initial()).unwrap();
                let mut response = View::from(
                    (0..count)
                        .map(|i| node(&format!("new-{i}")))
                        .collect::<Vec<_>>(),
                );
                response.response.navigation =
                    Some(hyperchad_renderer::ResponseNavigation::internal("/next").unwrap());
                response
                    .response
                    .cookies
                    .push(hyperchad_renderer::ResponseCookie::secure(
                        "session", "test",
                    ));
                let metadata = response.response.clone();
                let report = state
                    .apply_response(response, Some("target"), strategy.clone())
                    .unwrap();
                assert_eq!(report.response, metadata);
                let inserted: Vec<_> = (0..count).map(|i| node(&format!("new-{i}"))).collect();
                let mut expected = initial();
                match strategy {
                    SwapStrategy::This => {
                        expected.children.splice(1..=1, inserted);
                    }
                    SwapStrategy::Children => expected.children[1].children = inserted,
                    SwapStrategy::BeforeBegin => {
                        expected.children.splice(1..1, inserted);
                    }
                    SwapStrategy::AfterEnd => {
                        expected.children.splice(2..2, inserted);
                    }
                    SwapStrategy::AfterBegin => {
                        expected.children[1].children.splice(0..0, inserted);
                    }
                    SwapStrategy::BeforeEnd => expected.children[1].children.extend(inserted),
                    SwapStrategy::Delete => {
                        expected.children.remove(1);
                    }
                    SwapStrategy::None => {}
                }
                assert_eq!(state.root(), &expected, "{strategy:?} count={count}");
                assert_eq!(
                    report.lifecycle_removed.contains("target"),
                    matches!(strategy, SwapStrategy::This | SwapStrategy::Delete)
                );
                assert_eq!(
                    report.lifecycle_removed.contains("old"),
                    matches!(
                        strategy,
                        SwapStrategy::This | SwapStrategy::Children | SwapStrategy::Delete
                    )
                );
                assert!(!report.lifecycle_removed.contains("left"));
                assert!(!report.lifecycle_removed.contains("right"));
            }
        }
    }

    #[test]
    fn explicit_default_div_is_never_flattened_by_any_swap() {
        for strategy in STRATEGIES {
            let explicit = Container::from(vec![node("inside")]);
            assert!(!View::from(explicit.clone()).primary_is_fragment);
            let mut state = ViewState::from_root(initial()).unwrap();
            state
                .apply_response(
                    View::from(explicit.clone()),
                    Some("target"),
                    strategy.clone(),
                )
                .unwrap();
            match strategy {
                SwapStrategy::This | SwapStrategy::BeforeBegin => {
                    assert_eq!(state.root().children[1], explicit);
                }
                SwapStrategy::Children | SwapStrategy::AfterBegin => {
                    assert_eq!(state.root().children[1].children[0], explicit);
                }
                SwapStrategy::BeforeEnd => {
                    assert_eq!(state.root().children[1].children[1], explicit);
                }
                SwapStrategy::AfterEnd => assert_eq!(state.root().children[2], explicit),
                SwapStrategy::Delete | SwapStrategy::None => {}
            }
        }
    }

    #[test]
    fn fragment_duplicate_detection_is_atomic_for_internal_and_retained_collisions() {
        for strategy in STRATEGIES
            .into_iter()
            .filter(|s| !matches!(s, SwapStrategy::Delete | SwapStrategy::None))
        {
            for nodes in [
                vec![node("duplicate"), node("duplicate")],
                vec![node("left")],
            ] {
                let root = initial();
                let mut state = ViewState::from_root(root.clone()).unwrap();
                let generation = state.generation();
                assert!(
                    matches!(
                        state.apply_response(View::from(nodes), Some("target"), strategy.clone()),
                        Err(ApplyError::DuplicateIdentity(_))
                    ),
                    "{strategy:?}"
                );
                assert_eq!(state.root(), &root);
                assert_eq!(state.generation(), generation);
            }
        }
    }

    #[test]
    fn document_fragment_root_is_safe_and_root_siblings_remain_rejected() {
        for nodes in [vec![], vec![node("one")], vec![node("one"), node("two")]] {
            let mut state = ViewState::from_root(initial()).unwrap();
            state
                .apply_response(View::from(nodes.clone()), None, SwapStrategy::This)
                .unwrap();
            assert_eq!(state.root(), &Container::from(nodes.clone()));
            for strategy in [SwapStrategy::BeforeBegin, SwapStrategy::AfterEnd] {
                let root = state.root().clone();
                assert_eq!(
                    state
                        .apply_response(View::from(nodes.clone()), None, strategy)
                        .unwrap_err(),
                    ApplyError::RootSiblingInsertion
                );
                assert_eq!(state.root(), &root);
            }
        }
    }

    #[test]
    fn identical_anonymous_fragment_replacements_reset_every_inserted_lifecycle() {
        for strategy in [SwapStrategy::This, SwapStrategy::Children] {
            let mut state = ViewState::from_root(Container {
                children: vec![Container {
                    children: vec![Container::default(), Container::default()],
                    ..Default::default()
                }],
                ..Default::default()
            })
            .unwrap();
            let nodes = if strategy == SwapStrategy::This {
                vec![state.root().children[0].clone()]
            } else {
                state.root().children[0].children.clone()
            };
            let generation = state.generation();
            let report = state
                .apply_response(View::from(nodes), Some("@0"), strategy)
                .unwrap();
            assert!(state.generation() > generation);
            assert!(report.lifecycle_removed.contains("@0/0"));
            assert!(report.lifecycle_removed.contains("@0/1"));
        }
    }
}

/// Describe state-bearing fields without retaining a stale copy of the tree.
pub fn control_metadata(node: &Container) -> Option<ControlMetadata> {
    let (kind, name, value) = match &node.element {
        Element::Input { input, name, .. } => {
            let (kind, value) = match input {
                Input::Text { value, .. } => {
                    (ControlKind::Text, value.clone().map(ControlValue::Text))
                }
                Input::Password { value, .. } => {
                    (ControlKind::Password, value.clone().map(ControlValue::Text))
                }
                Input::Checkbox { checked } => {
                    (ControlKind::Checkbox, checked.map(ControlValue::Bool))
                }
                Input::File => (ControlKind::File, None),
                Input::Hidden { value } => {
                    (ControlKind::Hidden, value.clone().map(ControlValue::Text))
                }
            };
            (kind, name.clone(), value)
        }
        Element::Textarea { name, value, .. } => (
            ControlKind::Textarea,
            name.clone(),
            Some(ControlValue::Text(value.clone())),
        ),
        Element::Select {
            name,
            selected,
            multiple,
            ..
        } => (
            if multiple.unwrap_or(false) {
                ControlKind::MultiSelect
            } else {
                ControlKind::Select
            },
            name.clone(),
            selected.clone().map(ControlValue::Text),
        ),
        Element::Details { open } => (ControlKind::Details, None, open.map(ControlValue::Bool)),
        _ => return None,
    };
    Some(ControlMetadata { kind, name, value })
}

const fn has_identity(node: &Container) -> bool {
    node.str_id.is_some() || node.id != 0
}

fn compatible_nodes(before: &Container, after: &Container) -> bool {
    // Layout keys are intentionally human-readable: string "12", numeric 12,
    // and a literal string "@0" can collide across successive trees. Equal key
    // strings alone never justify transferring state between identity domains.
    let same_identity = match (&before.str_id, &after.str_id) {
        (Some(a), Some(b)) => a == b,
        (None, None) => before.id == after.id,
        _ => false,
    };
    if !same_identity {
        return false;
    }
    if discriminant(&before.element) != discriminant(&after.element) {
        return false;
    }
    match (control_metadata(before), control_metadata(after)) {
        (Some(a), Some(b)) => a.kind == b.kind && a.name == b.name,
        (None, None) => true,
        _ => false,
    }
}

// Every non-root anonymous identity historically requires the entire root
// child forest unchanged. Compute that proof once rather than per descendant.
fn anonymous_forest_unchanged(
    before: &Container,
    after: &Container,
    affected: &[(Path, bool)],
    tree_changed: bool,
) -> bool {
    if !tree_changed {
        return true;
    }
    if affected
        .iter()
        .all(|(path, children_only)| !path.is_empty() || *children_only)
    {
        return false;
    }
    before.children == after.children
}

// Single-node replacement compares that subtree; insertion/deletion compares
// the containing child forest because sibling indexes can shift.
fn primary_affected_scope(
    primary: Option<&Container>,
    is_fragment: bool,
    strategy: &SwapStrategy,
    anchor: Option<&[usize]>,
) -> Option<(Path, bool)> {
    if *strategy != SwapStrategy::Delete && (primary.is_none() || *strategy == SwapStrategy::None) {
        return None;
    }
    let path = anchor?;
    let exact_replacement = *strategy == SwapStrategy::This
        && primary.is_some_and(|primary| !is_fragment || primary.children.len() == 1);
    let children_only = matches!(
        strategy,
        SwapStrategy::Children | SwapStrategy::AfterBegin | SwapStrategy::BeforeEnd
    );
    let scope = if children_only || exact_replacement || path.is_empty() {
        path.to_vec()
    } else {
        path[..path.len() - 1].to_vec()
    };
    Some((
        scope,
        children_only || (!exact_replacement && !path.is_empty()),
    ))
}

// Compare only mutation boundaries. Parent child-list scopes cover index shifts;
// ancestor scopes subsume earlier paths even when later operations detach them.
// Compare final state, not intermediate writes, so canceling updates stay no-ops.
fn affected_changed(before: &Container, after: &Container, affected: &[(Path, bool)]) -> bool {
    affected.iter().any(|(path, children_only)| {
        if affected.iter().any(|(ancestor, ancestor_children)| {
            path.starts_with(ancestor)
                && (ancestor.len() < path.len() || (!ancestor_children && *children_only))
        }) {
            return false;
        }
        match (at(before, path), at(after, path)) {
            (Some(a), Some(b)) => {
                if *children_only {
                    a.children != b.children
                } else {
                    a != b
                }
            }
            (None, None) => false,
            _ => true,
        }
    })
}

#[cfg(test)]
mod affected_scope_regressions {
    use super::*;

    #[test]
    fn scoped_comparison_matches_full_tree_for_large_library_edits_and_noops() {
        let before = Container {
            children: (0..193)
                .map(|index| Container {
                    str_id: Some(format!("album-{index}")),
                    children: vec![Container::default(); 8],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut after = before.clone();
        let exact = vec![(vec![190, 3], false)];
        assert!(!affected_changed(&before, &after, &exact));
        after.children[190].children[3].hidden = Some(true);
        assert_eq!(affected_changed(&before, &after, &exact), before != after);
        // An insertion shifts previously recorded descendants. The parent
        // forest is authoritative and subsumes that stale descendant scope.
        after.children[190].children.insert(0, Container::default());
        let shifted = vec![(vec![190, 3], false), (vec![190], true)];
        assert_eq!(affected_changed(&before, &after, &shifted), before != after);
        // Final-state comparison preserves cancelling transactions.
        after = before.clone();
        assert!(!affected_changed(&before, &after, &shifted));
        assert!(!affected_changed(&before, &after, &[]));
        after.hidden = Some(true);
        assert!(!affected_changed(&before, &after, &[(vec![], true)]));
        assert!(affected_changed(&before, &after, &[(vec![], false)]));
    }
}

fn index(root: &Container) -> Result<BTreeMap<String, Path>, ApplyError> {
    fn visit(
        node: &Container,
        path: &mut Path,
        keys: &mut BTreeMap<String, Path>,
        ids: &mut BTreeSet<usize>,
        strings: &mut BTreeSet<String>,
    ) -> Result<(), ApplyError> {
        let key = node_key(node, path);
        if keys.insert(key.clone(), path.clone()).is_some() {
            return Err(ApplyError::DuplicateIdentity(key));
        }
        if node.id != 0 && !ids.insert(node.id) {
            return Err(ApplyError::DuplicateIdentity(format!(
                "numeric:{}",
                node.id
            )));
        }
        if let Some(id) = &node.str_id
            && !strings.insert(id.clone())
        {
            return Err(ApplyError::DuplicateIdentity(format!("id:{id}")));
        }
        for (i, child) in node.children.iter().enumerate() {
            path.push(i);
            visit(child, path, keys, ids, strings)?;
            path.pop();
        }
        Ok(())
    }
    let mut keys = BTreeMap::new();
    visit(
        root,
        &mut Vec::new(),
        &mut keys,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )?;
    Ok(keys)
}

fn at<'a>(root: &'a Container, path: &[usize]) -> Option<&'a Container> {
    path.iter().try_fold(root, |node, &i| node.children.get(i))
}

fn at_mut<'a>(root: &'a mut Container, path: &[usize]) -> Option<&'a mut Container> {
    path.iter()
        .try_fold(root, |node, &i| node.children.get_mut(i))
}

fn resolve_trigger(root: &Container, target: Option<&str>) -> Result<Path, ApplyError> {
    let Some(target) = target else {
        return Ok(Vec::new());
    };
    let keys = index(root)?;
    if let Some(path) = keys.get(target) {
        return Ok(path.clone());
    }
    let id = target.strip_prefix('#').unwrap_or(target);
    keys.values()
        .find(|path| at(root, path).is_some_and(|n| n.str_id.as_deref() == Some(id)))
        .cloned()
        .ok_or_else(|| ApplyError::MissingTarget(target.into()))
}

fn missing(selector: &Selector) -> ApplyError {
    ApplyError::MissingTarget(format!("{selector:?}"))
}

fn select(
    root: &Container,
    selector: &Selector,
    anchor: Option<&[usize]>,
) -> Result<Vec<Path>, ApplyError> {
    let mut paths = Vec::new();
    match selector {
        Selector::SelfTarget => {
            if let Some(path) = anchor.filter(|p| at(root, p).is_some()) {
                paths.push(path.to_vec());
            }
        }
        Selector::ChildClass(class) => {
            if let Some(path) = anchor
                && let Some(node) = at(root, path)
            {
                for (i, child) in node.children.iter().enumerate() {
                    if child.classes.contains(class) {
                        let mut child_path = path.to_vec();
                        child_path.push(i);
                        paths.push(child_path);
                    }
                }
            }
        }
        Selector::Id(_) | Selector::Class(_) => {
            for path in index(root)?.into_values() {
                let node = at(root, &path).expect("indexed node");
                let matches = match selector {
                    Selector::Id(id) => node.str_id.as_ref() == Some(id),
                    Selector::Class(class) => node.classes.contains(class),
                    _ => unreachable!(),
                };
                if matches {
                    paths.push(path);
                }
            }
        }
    }
    if paths.is_empty() {
        return Err(missing(selector));
    }
    Ok(paths)
}

fn outermost(mut paths: Vec<Path>) -> Vec<Path> {
    paths.sort();
    let mut result: Vec<Path> = Vec::new();
    for path in paths {
        if !result.last().is_some_and(|parent| path.starts_with(parent)) {
            result.push(path);
        }
    }
    result
}

fn delete(root: &mut Container, path: &[usize]) {
    if let Some((&i, parent)) = path.split_last() {
        at_mut(root, parent)
            .expect("selected parent")
            .children
            .remove(i);
    } else {
        *root = Container::default();
    }
}

fn adjust_anchor(anchor: &mut Option<Path>, deleted: &[usize]) {
    let Some(path) = anchor else {
        return;
    };
    if path.starts_with(deleted) {
        *anchor = None;
        return;
    }
    if let Some((&i, parent)) = deleted.split_last()
        && path.starts_with(parent)
        && path.len() > parent.len()
        && path[parent.len()] > i
    {
        path[parent.len()] -= 1;
    }
}

#[cfg(test)]
mod source_snapshot_tests {
    use super::*;

    fn state() -> ViewState {
        ViewState::from_root(Container {
            str_id: Some("root".into()),
            ..Container::default()
        })
        .unwrap()
    }

    #[test]
    fn immutable_observation_and_repeated_snapshots_preserve_source() {
        let mut state = state();
        let snapshot = state.source_snapshot();
        let _ = state.root();
        let _ = state.find_node("root");
        assert!(state.is_current_generation(state.generation()));
        let repeated = state.source_snapshot();
        assert!(state.matches_source(&snapshot));
        assert!(state.matches_source(&repeated));
        assert!(Arc::ptr_eq(&snapshot.0, &repeated.0));
    }

    #[test]
    fn unmarked_mutable_access_invalidates_source_without_advancing_generation() {
        let mut state = state();
        let snapshot = state.source_snapshot();
        state.root_mut().data.insert("changed".into(), "yes".into());
        assert_eq!(state.generation(), 0);
        assert!(!state.matches_source(&snapshot));
        let fresh = state.source_snapshot();
        assert!(state.matches_source(&fresh));
        assert!(!state.matches_source(&snapshot));
        let _ = state.root_mut();
        assert!(
            !state.matches_source(&fresh),
            "even a no-op mutable borrow invalidates"
        );
        assert_eq!(state.generation(), 0);
    }

    #[test]
    fn mutable_lookup_invalidates_only_when_access_is_granted() {
        let mut state = state();
        let snapshot = state.source_snapshot();
        assert!(state.find_node_mut("missing").is_none());
        assert!(state.matches_source(&snapshot));
        state
            .find_node_mut("root")
            .unwrap()
            .data
            .insert("changed".into(), "yes".into());
        assert!(!state.matches_source(&snapshot));
        assert_eq!(state.generation(), 0);
    }

    #[test]
    fn constructors_and_replacement_have_unique_sources_at_generation_zero() {
        let mut state = state();
        let snapshot = state.source_snapshot();
        let replacement = ViewState::from_root(state.root().clone()).unwrap();
        assert!(!replacement.matches_source(&snapshot));
        state = replacement;
        assert_eq!(state.generation(), 0);
        assert!(!state.matches_source(&snapshot));
        let mut empty = ViewState::default();
        let empty_snapshot = empty.source_snapshot();
        assert!(!ViewState::new().matches_source(&empty_snapshot));
    }

    #[test]
    fn clones_share_valid_source_but_pending_clones_reject_old_snapshots() {
        let mut state = state();
        let snapshot = state.source_snapshot();
        let unchanged = state.clone();
        assert!(unchanged.matches_source(&snapshot));
        let _ = state.root_mut();
        let mut pending = state.clone();
        assert!(!state.matches_source(&snapshot));
        assert!(!pending.matches_source(&snapshot));
        assert!(unchanged.matches_source(&snapshot));
        let renewed = state.source_snapshot();
        assert!(state.matches_source(&renewed));
        assert!(!pending.matches_source(&renewed));
        let independently_renewed = pending.source_snapshot();
        assert!(!state.matches_source(&independently_renewed));
    }

    #[test]
    fn noop_and_failed_transactions_preserve_source_and_changed_commits_renew_it() {
        let mut state = state();
        let snapshot = state.source_snapshot();
        assert!(!state.mutate_root(|_| {}).unwrap());
        state
            .apply_response(View::default(), None, SwapStrategy::None)
            .unwrap();
        assert!(state.matches_source(&snapshot));
        let root = state.root().clone();
        assert!(
            state
                .mutate_root(|candidate| {
                    candidate.children.push(Container {
                        str_id: Some("root".into()),
                        ..Container::default()
                    });
                })
                .is_err()
        );
        assert!(
            state
                .apply_view_if_generation(View::default(), None, 1)
                .is_err()
        );
        assert!(state.apply_view(View::default(), Some("missing")).is_err());
        assert_eq!(state.root(), &root);
        assert_eq!(state.generation(), 0);
        assert!(state.matches_source(&snapshot));
        assert!(
            state
                .mutate_root(|candidate| {
                    candidate.data.insert("changed".into(), "yes".into());
                })
                .unwrap()
        );
        assert!(!state.matches_source(&snapshot));
        let snapshot = state.source_snapshot();
        state.mark_changed().unwrap();
        assert!(!state.matches_source(&snapshot));
        let snapshot = state.source_snapshot();
        state
            .apply_view(View::from(Container::default()), None)
            .unwrap();
        assert!(!state.matches_source(&snapshot));
    }

    #[test]
    fn rejected_generation_exhaustion_preserves_source_and_pending_invalidation() {
        let mut state = state();
        state.generation = u64::MAX;
        let snapshot = state.source_snapshot();
        assert!(state.mark_changed().is_err());
        assert!(
            state
                .mutate_root(|candidate| candidate.str_id = Some("new".into()))
                .is_err()
        );
        assert!(
            state
                .apply_view(View::from(Container::default()), None)
                .is_err()
        );
        assert!(state.matches_source(&snapshot));
        let _ = state.root_mut();
        assert!(!state.mutate_root(|_| {}).unwrap());
        assert!(state.mark_changed().is_err());
        assert!(
            !state.matches_source(&snapshot),
            "failure cannot revive an invalidated source"
        );
    }
}

#[cfg(test)]
mod swaps;
#[cfg(test)]
mod tests;
