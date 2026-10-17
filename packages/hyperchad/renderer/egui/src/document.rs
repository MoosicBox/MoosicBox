//! UI-owned document reconciliation. No layout, I/O, or application callbacks run here.
//!
//! Responses are transactional: fragments, then deletes, then primary. Navigation keeps
//! the primary container; route primaries are response envelopes whose children are the
//! payload. Fragment containers are literal payloads (never implicitly unwrapped).
//! A hidden host permits zero or multiple roots without exposing a synthetic element.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);

use hyperchad_renderer::{ResponseMetadata, View};
use hyperchad_transformer::{
    Container,
    models::{Selector, SwapStrategy},
};

/// Opaque document-scoped identity, independent of source IDs and tree paths.
/// Never transfer keys/tickets between independently constructed documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementKey {
    namespace: u64,
    epoch: u64,
    serial: u64,
}

/// A resource or request may publish only while this content generation is live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementStamp {
    pub key: ElementKey,
    pub generation: u64,
}

/// Context absent from `View` itself.
#[derive(Clone, Debug)]
pub enum ViewOrigin {
    /// A full primary begins a new epoch. Fragment-only navigation keeps the epoch.
    Navigation,
    /// Selectors are document-wide except self and direct-child selectors.
    Element {
        source: ElementKey,
        target: Selector,
        strategy: SwapStrategy,
    },
}

/// Recoverable reconciliation diagnostics; duplicate IDs are never silently rewritten.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Diagnostic {
    DuplicateStringId(String),
    /// Repeated nonzero internal IDs. Zero is ordinary unassigned construction and
    /// is omitted from structural warnings, not removed from the numeric index.
    DuplicateNumericId(usize),
    MissingTarget,
}

/// Net lifecycle changes, not transient nodes inserted and removed in one response.
#[derive(Clone, Debug, Default)]
pub struct MutationSummary {
    pub inserted: Vec<ElementStamp>,
    pub removed: Vec<ElementStamp>,
    pub changed: Vec<ElementStamp>,
    /// Geometry is invalid whenever the authored document changed.
    pub layout_dirty: bool,
    pub diagnostics: Vec<Diagnostic>,
    /// Returned to the host, not executed here. Cookies/navigation never cause I/O here.
    pub response: ResponseMetadata,
}

/// Failure leaves authored content unchanged. A terminal completion also releases
/// its matching request scope, even when reconciliation fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentError {
    StaleSource,
    StaleRequest,
    IdentityExhausted,
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::StaleSource => "view source no longer exists",
            Self::StaleRequest => "request was superseded or its source/targets changed",
            Self::IdentityExhausted => "document identity or generation counter exhausted",
        })
    }
}

impl std::error::Error for DocumentError {}

/// Bound to source content, selected targets, and the latest request in one scope.
#[derive(Clone, Debug)]
pub struct RequestTicket {
    source: ElementStamp,
    scope: String,
    sequence: u64,
    target: Selector,
    strategy: SwapStrategy,
    targets: Vec<ElementStamp>,
}

#[derive(Clone, Debug)]
struct Identity {
    stamp: ElementStamp,
    children: Vec<Self>,
}

/// Sole owner of the authored tree. Geometry consumers calculate a clone of `roots`;
/// layout output never changes document generations or reconciliation equality.
#[derive(Debug)]
pub struct Document {
    namespace: u64,
    host: Container,
    identities: Vec<Identity>,
    paths: BTreeMap<ElementKey, Vec<usize>>,
    string_ids: BTreeMap<String, Vec<ElementKey>>,
    classes: BTreeMap<String, Vec<ElementKey>>,
    numeric_ids: BTreeMap<usize, Vec<ElementKey>>,
    epoch: u64,
    next_serial: u64,
    next_request: u64,
    requests: BTreeMap<(ElementKey, String), RequestTicket>,
}

impl Default for Document {
    fn default() -> Self {
        let namespace = NEXT_DOCUMENT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("document namespace exhausted");
        Self {
            namespace,
            host: Container::default(),
            identities: Vec::new(),
            paths: BTreeMap::new(),
            string_ids: BTreeMap::new(),
            classes: BTreeMap::new(),
            numeric_ids: BTreeMap::new(),
            epoch: 0,
            next_serial: 0,
            next_request: 0,
            requests: BTreeMap::new(),
        }
    }
}

impl Document {
    // Only transactional staging may duplicate a namespace. There is intentionally
    // no public Clone: independently mutable branches must not share ticket authority.
    fn snapshot(&self) -> Self {
        Self {
            namespace: self.namespace,
            host: self.host.clone(),
            identities: self.identities.clone(),
            paths: self.paths.clone(),
            string_ids: self.string_ids.clone(),
            classes: self.classes.clone(),
            numeric_ids: self.numeric_ids.clone(),
            epoch: self.epoch,
            next_serial: self.next_serial,
            next_request: self.next_request,
            requests: self.requests.clone(),
        }
    }
    /// Create an empty document, before the first navigation response.
    ///
    /// # Panics
    /// * If the process has exhausted all document namespaces; identities never wrap.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The real document roots, excluding the hidden host.
    #[must_use]
    pub fn roots(&self) -> &[Container] {
        &self.host.children
    }

    /// Root keys in document order.
    #[must_use]
    pub fn root_keys(&self) -> Vec<ElementKey> {
        self.identities.iter().map(|node| node.stamp.key).collect()
    }

    /// Look up a live identity without relying on an authored ID being unique.
    #[allow(clippy::must_use_candidate)] // Option already conveys optional lookup semantics.
    pub fn get(&self, key: ElementKey) -> Option<&Container> {
        container_at(&self.host.children, self.paths.get(&key)?)
    }

    /// Live keys in document order, including nodes without source IDs.
    #[must_use]
    pub fn keys(&self) -> Vec<ElementKey> {
        self.ordered_keys()
    }

    /// Direct child identities in the same order as `get(key).children`.
    #[must_use]
    pub fn children(&self, key: ElementKey) -> Vec<ElementKey> {
        self.paths
            .get(&key)
            .and_then(|path| identity_at(&self.identities, path))
            .map_or_else(Vec::new, |node| {
                node.children.iter().map(|child| child.stamp.key).collect()
            })
    }

    /// Roots have no public parent; the hidden host is never an element identity.
    #[allow(clippy::must_use_candidate)] // Do not add redundant must_use to Option getters.
    pub fn parent(&self, key: ElementKey) -> Option<ElementKey> {
        let path = self.paths.get(&key)?;
        identity_at(&self.identities, &path[..path.len() - 1]).map(|node| node.stamp.key)
    }

    /// Current authored-subtree generation; layout measurement never changes it.
    #[allow(clippy::must_use_candidate)] // Do not add redundant must_use to Option getters.
    pub fn stamp(&self, key: ElementKey) -> Option<ElementStamp> {
        identity_at(&self.identities, self.paths.get(&key)?).map(|node| node.stamp)
    }

    /// Test a generation before installing element-owned asynchronous resources.
    #[must_use]
    pub fn is_current(&self, stamp: ElementStamp) -> bool {
        self.stamp(stamp.key) == Some(stamp)
    }

    /// Numeric source IDs are deliberately a multimap: action consumers must diagnose
    /// ambiguity instead of choosing a node by incidental paint order.
    #[must_use]
    pub fn numeric_matches(&self, id: usize) -> &[ElementKey] {
        self.numeric_ids.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Resolve in document order. Class/ID selectors are global; child-class is direct
    /// children only. Without a source, self means roots and child-class means their
    /// direct children. A supplied stale source never falls back to navigation roots.
    /// # Panics
    /// * If the private tree and identity indexes disagree (an internal invariant bug).
    #[must_use]
    pub fn select(&self, selector: &Selector, source: Option<ElementKey>) -> Vec<ElementKey> {
        if source.is_some_and(|key| !self.paths.contains_key(&key)) {
            return Vec::new();
        }
        match selector {
            Selector::Id(id) => self.string_ids.get(id).cloned().unwrap_or_default(),
            Selector::Class(class) => self.classes.get(class).cloned().unwrap_or_default(),
            Selector::SelfTarget => source.map_or_else(|| self.root_keys(), |key| vec![key]),
            Selector::ChildClass(class) => {
                let parents = source.map_or_else(|| self.root_keys(), |key| vec![key]);
                parents
                    .into_iter()
                    .flat_map(|key| {
                        let path = &self.paths[&key];
                        let container =
                            container_at(&self.host.children, path).expect("indexed container");
                        let identity =
                            identity_at(&self.identities, path).expect("indexed identity");
                        container
                            .children
                            .iter()
                            .zip(&identity.children)
                            .filter(|(child, _)| child.classes.contains(class))
                            .map(|(_, child)| child.stamp.key)
                            .collect::<Vec<_>>()
                    })
                    .collect()
            }
        }
    }

    /// Apply atomically. Missing targets are diagnostic no-ops; a stale element source
    /// or exhausted counter rejects the whole response. Metadata is returned unchanged.
    ///
    /// # Errors
    /// * `StaleSource` when an element-origin source is no longer live.
    /// * `IdentityExhausted` when a monotonic counter cannot advance.
    ///
    /// # Panics
    /// * If the private tree and identity indexes disagree (an internal invariant bug).
    pub fn apply_view(
        &mut self,
        view: View,
        origin: ViewOrigin,
    ) -> Result<MutationSummary, DocumentError> {
        let source = match &origin {
            ViewOrigin::Navigation => None,
            ViewOrigin::Element { source, .. } => {
                if self.get(*source).is_none() {
                    return Err(DocumentError::StaleSource);
                }
                Some(*source)
            }
        };
        let before = self.stamps();
        let old_host = self.host.clone();
        let mut staged = self.snapshot();
        let mut diagnostics = Vec::new();
        for fragment in view.fragments {
            let targets = staged.select(&fragment.selector, source);
            staged.swap_targets(
                &targets,
                &[fragment.container],
                &SwapStrategy::This,
                &mut diagnostics,
            )?;
        }
        for selector in view.delete_selectors {
            let targets = staged.select(&selector, source);
            staged.swap_targets(&targets, &[], &SwapStrategy::Delete, &mut diagnostics)?;
        }
        if let Some(primary) = view.primary {
            match origin {
                ViewOrigin::Navigation => {
                    staged.epoch = increment(staged.epoch)?;
                    staged.host.children.clear();
                    staged.identities.clear();
                    let identity = staged.fresh_identity(&primary)?;
                    staged.host.children.push(primary);
                    staged.identities.push(identity);
                    staged.reindex();
                }
                ViewOrigin::Element {
                    target, strategy, ..
                } => {
                    let targets = staged.select(&target, source);
                    staged.swap_targets(
                        &targets,
                        &primary.children,
                        &strategy,
                        &mut diagnostics,
                    )?;
                }
            }
        }
        // A retained key's generation describes its entire content subtree. Normalize
        // once per transaction, including ancestors changed by descendant operations.
        for key in staged.paths.keys().copied() {
            if let Some(previous) = self.stamp(key) {
                let generation = if self.get(key) == staged.get(key)
                    && self.subtree_keys(key) == staged.subtree_keys(key)
                {
                    previous.generation
                } else {
                    increment(previous.generation)?
                };
                let path = staged.paths[&key].clone();
                identity_at_mut(&mut staged.identities, &path)
                    .expect("indexed node")
                    .stamp
                    .generation = generation;
            }
        }
        staged.prune_requests();
        diagnostics.extend(staged.duplicate_diagnostics());
        let after = staged.stamps();
        let summary = MutationSummary {
            inserted: after
                .iter()
                .filter(|(key, _)| !before.contains_key(*key))
                .map(|(_, stamp)| *stamp)
                .collect(),
            removed: before
                .iter()
                .filter(|(key, _)| !after.contains_key(*key))
                .map(|(_, stamp)| *stamp)
                .collect(),
            changed: after
                .iter()
                .filter(|(key, stamp)| before.get(*key).is_some_and(|old| old != *stamp))
                .map(|(_, stamp)| *stamp)
                .collect(),
            layout_dirty: old_host != staged.host || before.keys().ne(after.keys()),
            diagnostics,
            response: view.response,
        };
        *self = staged;
        Ok(summary)
    }

    /// Latest-wins is scoped by `(source identity, scope)`, not by global document
    /// revision. Unrelated edits do not invalidate a request. Empty targets are allowed.
    ///
    /// # Errors
    /// * `StaleSource` if the source has been removed.
    /// * `IdentityExhausted` if the request sequence cannot advance. Abandoned
    ///   requests must be released with `cancel_request` or `cancel_scope` by the host;
    ///   successful document mutations automatically prune invalidated tickets.
    pub fn begin_request(
        &mut self,
        source: ElementKey,
        scope: String,
        target: Selector,
        strategy: SwapStrategy,
    ) -> Result<RequestTicket, DocumentError> {
        let source = self.stamp(source).ok_or(DocumentError::StaleSource)?;
        let targets = self
            .select(&target, Some(source.key))
            .into_iter()
            .filter_map(|key| self.stamp(key))
            .collect();
        let sequence = increment(self.next_request)?;
        self.next_request = sequence;
        let ticket = RequestTicket {
            source,
            scope,
            sequence,
            target,
            strategy,
            targets,
        };
        self.requests
            .insert((source.key, ticket.scope.clone()), ticket.clone());
        Ok(ticket)
    }

    /// Includes selector membership: a response cannot silently retarget newly inserted
    /// matches. Tickets are single-use after a successful completion.
    #[must_use]
    pub fn request_is_current(&self, ticket: &RequestTicket) -> bool {
        self.is_current(ticket.source)
            && self
                .requests
                .get(&(ticket.source.key, ticket.scope.clone()))
                .map(|pending| pending.sequence)
                == Some(ticket.sequence)
            && ticket.targets.iter().all(|stamp| self.is_current(*stamp))
            && self.select(&ticket.target, Some(ticket.source.key))
                == ticket
                    .targets
                    .iter()
                    .map(|stamp| stamp.key)
                    .collect::<Vec<_>>()
    }

    /// Number of live request scopes; useful for lifecycle diagnostics.
    #[must_use]
    pub fn pending_requests(&self) -> usize {
        self.requests.len()
    }

    /// Cancel exactly this sequence. A late completion/cancellation never removes a
    /// newer request occupying the same scope, and foreign namespaces cannot match.
    pub fn cancel_request(&mut self, ticket: &RequestTicket) -> bool {
        let key = (ticket.source.key, ticket.scope.clone());
        if self
            .requests
            .get(&key)
            .is_some_and(|pending| pending.sequence == ticket.sequence)
        {
            self.requests.remove(&key);
            true
        } else {
            false
        }
    }

    /// Explicitly release an abandoned scope (timeouts, worker cancellation, teardown).
    pub fn cancel_scope(&mut self, source: ElementKey, scope: &str) -> bool {
        self.requests.remove(&(source, scope.to_owned())).is_some()
    }

    fn prune_requests(&mut self) {
        let invalid = self
            .requests
            .values()
            .filter(|ticket| !self.request_is_current(ticket))
            .cloned()
            .collect::<Vec<_>>();
        for ticket in invalid {
            self.cancel_request(&ticket);
        }
    }

    /// Validate before any fragments, deletes or metadata are applied. Completion is
    /// terminal even if reconciliation fails: the matching request is released.
    ///
    /// # Errors
    /// * `StaleRequest` if superseded, consumed, or source/targets changed.
    /// * Any error from `apply_view`; no partial response is committed.
    pub fn complete_request(
        &mut self,
        ticket: &RequestTicket,
        view: View,
    ) -> Result<MutationSummary, DocumentError> {
        if !self.request_is_current(ticket) {
            self.cancel_request(ticket);
            self.prune_requests();
            return Err(DocumentError::StaleRequest);
        }
        let result = self.apply_view(
            view,
            ViewOrigin::Element {
                source: ticket.source.key,
                target: ticket.target.clone(),
                strategy: ticket.strategy.clone(),
            },
        );
        self.cancel_request(ticket);
        result
    }

    fn ordered_keys(&self) -> Vec<ElementKey> {
        let mut entries = self.paths.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(_, path)| *path);
        entries.into_iter().map(|(key, _)| *key).collect()
    }

    fn subtree_keys(&self, key: ElementKey) -> Vec<ElementKey> {
        fn collect(node: &Identity, keys: &mut Vec<ElementKey>) {
            keys.push(node.stamp.key);
            for child in &node.children {
                collect(child, keys);
            }
        }
        let mut keys = Vec::new();
        if let Some(node) = self
            .paths
            .get(&key)
            .and_then(|path| identity_at(&self.identities, path))
        {
            collect(node, &mut keys);
        }
        keys
    }

    fn stamps(&self) -> BTreeMap<ElementKey, ElementStamp> {
        self.paths
            .keys()
            .filter_map(|key| self.stamp(*key).map(|stamp| (*key, stamp)))
            .collect()
    }

    fn fresh_identity(&mut self, container: &Container) -> Result<Identity, DocumentError> {
        self.next_serial = increment(self.next_serial)?;
        let stamp = ElementStamp {
            key: ElementKey {
                namespace: self.namespace,
                epoch: self.epoch,
                serial: self.next_serial,
            },
            generation: 0,
        };
        let children = container
            .children
            .iter()
            .map(|child| self.fresh_identity(child))
            .collect::<Result<_, _>>()?;
        Ok(Identity { stamp, children })
    }

    // Only Children reconciliation reuses keys, and only unique keyed siblings in both
    // old and new sets. Explicit This, insertions and unkeyed replacements allocate.
    fn reconcile_children(
        &mut self,
        old: &[Container],
        identities: &[Identity],
        new: &[Container],
    ) -> Result<Vec<Identity>, DocumentError> {
        let unique = |nodes: &[Container]| {
            let mut counts = BTreeMap::<String, usize>::new();
            for node in nodes {
                if let Some(id) = &node.str_id {
                    *counts.entry(id.clone()).or_default() += 1;
                }
            }
            counts
        };
        let old_counts = unique(old);
        let new_counts = unique(new);
        new.iter()
            .map(|node| {
                let previous = node
                    .str_id
                    .as_ref()
                    .filter(|id| old_counts.get(*id) == Some(&1) && new_counts.get(*id) == Some(&1))
                    .and_then(|id| {
                        old.iter()
                            .position(|candidate| candidate.str_id.as_ref() == Some(id))
                    });
                if let Some(index) = previous {
                    let mut identity = identities[index].clone();
                    identity.children = self.reconcile_children(
                        &old[index].children,
                        &identity.children,
                        &node.children,
                    )?;
                    Ok(identity)
                } else {
                    self.fresh_identity(node)
                }
            })
            .collect()
    }

    fn swap_targets(
        &mut self,
        targets: &[ElementKey],
        payload: &[Container],
        strategy: &SwapStrategy,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<(), DocumentError> {
        if *strategy == SwapStrategy::None {
            return Ok(());
        }
        if targets.is_empty() {
            diagnostics.push(Diagnostic::MissingTarget);
        }
        // Targets are frozen per operation. Ancestors removed earlier in document
        // order make their snapshotted descendants detached and therefore skipped.
        for key in targets {
            let Some(path) = self.paths.get(key).cloned() else {
                continue;
            };
            let index = *path.last().expect("real node path");
            let parent = &path[..path.len() - 1];
            match strategy {
                SwapStrategy::Children => {
                    let old = container_at(&self.host.children, &path)
                        .expect("indexed node")
                        .children
                        .clone();
                    let old_identities = identity_at(&self.identities, &path)
                        .expect("indexed node")
                        .children
                        .clone();
                    let children = self.reconcile_children(&old, &old_identities, payload)?;
                    let node =
                        container_at_mut(&mut self.host.children, &path).expect("indexed node");
                    node.children = payload.to_vec();
                    let identity =
                        identity_at_mut(&mut self.identities, &path).expect("indexed node");
                    identity.children = children;
                }
                SwapStrategy::AfterBegin | SwapStrategy::BeforeEnd => {
                    let inserted = payload
                        .iter()
                        .map(|node| self.fresh_identity(node))
                        .collect::<Result<Vec<_>, _>>()?;
                    let node =
                        container_at_mut(&mut self.host.children, &path).expect("indexed node");
                    let identity =
                        identity_at_mut(&mut self.identities, &path).expect("indexed node");
                    let at = if *strategy == SwapStrategy::AfterBegin {
                        0
                    } else {
                        node.children.len()
                    };
                    node.children.splice(at..at, payload.iter().cloned());
                    identity.children.splice(at..at, inserted);
                }
                SwapStrategy::This
                | SwapStrategy::Delete
                | SwapStrategy::BeforeBegin
                | SwapStrategy::AfterEnd => {
                    let payload = if *strategy == SwapStrategy::Delete {
                        &[][..]
                    } else {
                        payload
                    };
                    let inserted = payload
                        .iter()
                        .map(|node| self.fresh_identity(node))
                        .collect::<Result<Vec<_>, _>>()?;
                    let (start, end) = match strategy {
                        SwapStrategy::BeforeBegin => (index, index),
                        SwapStrategy::AfterEnd => (index + 1, index + 1),
                        _ => (index, index + 1),
                    };
                    let nodes = if parent.is_empty() {
                        &mut self.host.children
                    } else {
                        &mut container_at_mut(&mut self.host.children, parent)
                            .expect("indexed parent")
                            .children
                    };
                    nodes.splice(start..end, payload.iter().cloned());
                    let identities = if parent.is_empty() {
                        &mut self.identities
                    } else {
                        &mut identity_at_mut(&mut self.identities, parent)
                            .expect("indexed parent")
                            .children
                    };
                    identities.splice(start..end, inserted);
                }
                SwapStrategy::None => unreachable!(),
            }
            self.reindex();
        }
        Ok(())
    }

    fn reindex(&mut self) {
        fn visit(
            nodes: &[Container],
            identities: &[Identity],
            path: &mut Vec<usize>,
            paths: &mut BTreeMap<ElementKey, Vec<usize>>,
            strings: &mut BTreeMap<String, Vec<ElementKey>>,
            classes: &mut BTreeMap<String, Vec<ElementKey>>,
            numbers: &mut BTreeMap<usize, Vec<ElementKey>>,
        ) {
            for (index, (node, identity)) in nodes.iter().zip(identities).enumerate() {
                path.push(index);
                let key = identity.stamp.key;
                paths.insert(key, path.clone());
                if let Some(id) = &node.str_id {
                    strings.entry(id.clone()).or_default().push(key);
                }
                for class in &node.classes {
                    let matches = classes.entry(class.clone()).or_default();
                    if matches.last() != Some(&key) {
                        matches.push(key);
                    }
                }
                numbers.entry(node.id).or_default().push(key);
                visit(
                    &node.children,
                    &identity.children,
                    path,
                    paths,
                    strings,
                    classes,
                    numbers,
                );
                path.pop();
            }
        }
        self.paths.clear();
        self.string_ids.clear();
        self.classes.clear();
        self.numeric_ids.clear();
        visit(
            &self.host.children,
            &self.identities,
            &mut Vec::new(),
            &mut self.paths,
            &mut self.string_ids,
            &mut self.classes,
            &mut self.numeric_ids,
        );
    }

    fn duplicate_diagnostics(&self) -> Vec<Diagnostic> {
        self.string_ids
            .iter()
            .filter(|(_, keys)| keys.len() > 1)
            .map(|(id, _)| Diagnostic::DuplicateStringId(id.clone()))
            .chain(
                self.numeric_ids
                    .iter()
                    .filter(|(id, keys)| **id != 0 && keys.len() > 1)
                    .map(|(id, _)| Diagnostic::DuplicateNumericId(*id)),
            )
            .collect()
    }
}

fn increment(value: u64) -> Result<u64, DocumentError> {
    value.checked_add(1).ok_or(DocumentError::IdentityExhausted)
}

fn container_at<'a>(nodes: &'a [Container], path: &[usize]) -> Option<&'a Container> {
    let (index, rest) = path.split_first()?;
    let node = nodes.get(*index)?;
    if rest.is_empty() {
        Some(node)
    } else {
        container_at(&node.children, rest)
    }
}
fn container_at_mut<'a>(nodes: &'a mut [Container], path: &[usize]) -> Option<&'a mut Container> {
    let (index, rest) = path.split_first()?;
    let node = nodes.get_mut(*index)?;
    if rest.is_empty() {
        Some(node)
    } else {
        container_at_mut(&mut node.children, rest)
    }
}
fn identity_at<'a>(nodes: &'a [Identity], path: &[usize]) -> Option<&'a Identity> {
    let (index, rest) = path.split_first()?;
    let node = nodes.get(*index)?;
    if rest.is_empty() {
        Some(node)
    } else {
        identity_at(&node.children, rest)
    }
}
fn identity_at_mut<'a>(nodes: &'a mut [Identity], path: &[usize]) -> Option<&'a mut Identity> {
    let (index, rest) = path.split_first()?;
    let node = nodes.get_mut(*index)?;
    if rest.is_empty() {
        Some(node)
    } else {
        identity_at_mut(&mut node.children, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_renderer::ReplaceContainer;

    fn node(id: usize, name: Option<&str>, children: Vec<Container>) -> Container {
        Container {
            id,
            str_id: name.map(str::to_owned),
            children,
            ..Container::default()
        }
    }
    fn page(root: Container) -> View {
        View {
            primary: Some(root),
            ..View::default()
        }
    }
    fn fixture() -> Document {
        let mut document = Document::new();
        document
            .apply_view(
                page(node(
                    1,
                    Some("root"),
                    vec![node(2, Some("a"), vec![]), node(3, Some("b"), vec![])],
                )),
                ViewOrigin::Navigation,
            )
            .unwrap();
        document
    }
    fn key(document: &Document, id: &str) -> ElementKey {
        document.select(&Selector::Id(id.into()), None)[0]
    }
    fn route(
        document: &mut Document,
        source: ElementKey,
        target: Selector,
        strategy: SwapStrategy,
        children: Vec<Container>,
    ) -> MutationSummary {
        document
            .apply_view(
                page(Container::from(children)),
                ViewOrigin::Element {
                    source,
                    target,
                    strategy,
                },
            )
            .unwrap()
    }
    fn names(nodes: &[Container]) -> Vec<&str> {
        nodes
            .iter()
            .map(|node| node.str_id.as_deref().unwrap_or("unkeyed"))
            .collect()
    }

    #[test]
    fn navigation_keeps_root_and_route_unwraps_response_envelope() {
        let mut document = fixture();
        assert_eq!(document.roots()[0].str_id.as_deref(), Some("root"));
        let source = key(&document, "a");
        route(
            &mut document,
            source,
            Selector::SelfTarget,
            SwapStrategy::This,
            vec![node(4, Some("x"), vec![]), node(5, Some("y"), vec![])],
        );
        assert_eq!(names(&document.roots()[0].children), ["x", "y", "b"]);
        assert!(document.get(source).is_none());
    }

    #[test]
    fn every_strategy_has_explicit_order_and_root_behavior() {
        for (strategy, expected) in [
            (SwapStrategy::This, vec!["x", "y", "b"]),
            (SwapStrategy::BeforeBegin, vec!["x", "y", "a", "b"]),
            (SwapStrategy::AfterEnd, vec!["a", "x", "y", "b"]),
            (SwapStrategy::Delete, vec!["b"]),
            (SwapStrategy::None, vec!["a", "b"]),
        ] {
            let mut document = fixture();
            let source = key(&document, "a");
            route(
                &mut document,
                source,
                Selector::SelfTarget,
                strategy,
                vec![node(4, Some("x"), vec![]), node(5, Some("y"), vec![])],
            );
            assert_eq!(names(&document.roots()[0].children), expected);
        }
        for (strategy, expected) in [
            (SwapStrategy::Children, vec!["x", "y"]),
            (SwapStrategy::AfterBegin, vec!["x", "y", "a", "b"]),
            (SwapStrategy::BeforeEnd, vec!["a", "b", "x", "y"]),
        ] {
            let mut document = fixture();
            let source = key(&document, "root");
            route(
                &mut document,
                source,
                Selector::SelfTarget,
                strategy,
                vec![node(4, Some("x"), vec![]), node(5, Some("y"), vec![])],
            );
            assert_eq!(names(&document.roots()[0].children), expected);
            assert_eq!(key(&document, "root"), source);
        }
        let mut document = fixture();
        let root = key(&document, "root");
        route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::BeforeBegin,
            vec![node(4, Some("sibling"), vec![])],
        );
        assert_eq!(names(document.roots()), ["sibling", "root"]);
        route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Delete,
            vec![],
        );
        assert_eq!(names(document.roots()), ["sibling"]);
        let root = key(&document, "sibling");
        route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Delete,
            vec![],
        );
        assert_eq!(document.roots(), []);
    }

    #[test]
    fn fragment_delete_primary_order_and_metadata() {
        let mut document = fixture();
        let source = key(&document, "root");
        let response = ResponseMetadata {
            navigation: Some(hyperchad_renderer::ResponseNavigation::Internal(
                "/next".into(),
            )),
            ..ResponseMetadata::default()
        };
        let view = View {
            fragments: vec![ReplaceContainer {
                selector: Selector::Id("a".into()),
                container: node(4, Some("replacement"), vec![]),
            }],
            delete_selectors: vec![Selector::Id("replacement".into())],
            primary: Some(Container::from(vec![node(5, Some("last"), vec![])])),
            response: response.clone(),
        };
        let summary = document
            .apply_view(
                view,
                ViewOrigin::Element {
                    source,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::BeforeEnd,
                },
            )
            .unwrap();
        assert_eq!(names(&document.roots()[0].children), ["b", "last"]);
        assert_eq!(summary.response, response);
        assert_eq!(summary.inserted.len(), 1); // transient fragment is not a live insertion
        assert_eq!(summary.removed.len(), 1);
    }

    #[test]
    fn fragment_only_navigation_and_none_still_apply_side_effects() {
        let mut document = fixture();
        let root = key(&document, "root");
        let summary = document
            .apply_view(
                View {
                    fragments: vec![node(4, Some("a"), vec![]).into()],
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        assert_eq!(key(&document, "root"), root);
        assert_eq!(summary.removed.len(), 1);
        document
            .apply_view(
                View {
                    primary: Some(Container::from(vec![node(5, Some("ignored"), vec![])])),
                    delete_selectors: vec![Selector::Id("b".into())],
                    ..View::default()
                },
                ViewOrigin::Element {
                    source: root,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::None,
                },
            )
            .unwrap();
        assert_eq!(names(&document.roots()[0].children), ["a"]);
    }

    #[test]
    fn stable_keyed_sibling_reorder_and_unkeyed_replacement_lifecycles() {
        let mut document = fixture();
        let root = key(&document, "root");
        let a = key(&document, "a");
        let b = key(&document, "b");
        let b_stamp = document.stamp(b).unwrap();
        route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::BeforeEnd,
            vec![node(6, None, vec![])],
        );
        let unkeyed = *document.numeric_matches(6).first().unwrap();
        let mut changed_a = node(2, Some("a"), vec![]);
        changed_a.classes.push("edited".into());
        let summary = route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Children,
            vec![node(3, Some("b"), vec![]), changed_a, node(6, None, vec![])],
        );
        assert_eq!(key(&document, "a"), a);
        assert_eq!(key(&document, "b"), b);
        assert!(document.is_current(b_stamp));
        assert_eq!(document.stamp(a).unwrap().generation, 1);
        assert!(document.get(unkeyed).is_none());
        assert_eq!(summary.inserted.len(), 1);
        assert_eq!(summary.removed.len(), 1);
        assert!(summary.changed.iter().any(|stamp| stamp.key == a));
    }

    #[test]
    fn children_reconciliation_replaces_unkeyed_descendants_even_in_equal_keyed_parent() {
        let mut document = fixture();
        let root = key(&document, "root");
        let payload = vec![node(4, Some("stable"), vec![node(5, None, vec![])])];
        route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Children,
            payload.clone(),
        );
        let stable = key(&document, "stable");
        let old_child = document.children(stable)[0];
        let stable_stamp = document.stamp(stable).unwrap();
        let root_stamp = document.stamp(root).unwrap();
        let summary = route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Children,
            payload,
        );
        assert_eq!(key(&document, "stable"), stable);
        assert_ne!(document.children(stable)[0], old_child);
        assert!(!document.is_current(stable_stamp));
        assert!(!document.is_current(root_stamp));
        assert_eq!(document.parent(stable), Some(root));
        assert_eq!(document.parent(root), None);
        assert_eq!(summary.inserted.len(), 1);
        assert_eq!(summary.removed.len(), 1);
    }

    #[test]
    fn unchanged_keyed_children_keep_generations_and_layout_clean() {
        let mut document = fixture();
        let root = key(&document, "root");
        let before = document.stamp(root).unwrap();
        let children = document.get(root).unwrap().children.clone();
        let summary = route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Children,
            children,
        );
        assert!(document.is_current(before));
        assert!(!summary.layout_dirty);
        assert_eq!(summary.inserted, []);
        assert_eq!(summary.removed, []);
        assert_eq!(summary.changed, []);
    }

    #[test]
    fn explicit_replacement_never_reuses_same_string_identity() {
        let mut document = fixture();
        let source = key(&document, "a");
        let stamp = document.stamp(source).unwrap();
        let summary = route(
            &mut document,
            source,
            Selector::SelfTarget,
            SwapStrategy::This,
            vec![node(2, Some("a"), vec![])],
        );
        assert_ne!(key(&document, "a"), source);
        assert!(!document.is_current(stamp));
        assert!(summary.layout_dirty); // authored content equal, but widget identity changed
    }

    #[test]
    fn default_numeric_ids_keep_distinct_keys_and_string_self_selection_without_warning() {
        let mut document = Document::new();
        let summary = document
            .apply_view(
                page(Container {
                    str_id: Some("root".into()),
                    children: vec![
                        Container {
                            str_id: Some("a".into()),
                            ..Container::default()
                        },
                        Container {
                            str_id: Some("b".into()),
                            ..Container::default()
                        },
                    ],
                    ..Container::default()
                }),
                ViewOrigin::Navigation,
            )
            .unwrap();
        assert_eq!(summary.diagnostics, []);
        let root = key(&document, "root");
        let a = key(&document, "a");
        let b = key(&document, "b");
        assert_ne!(root, a);
        assert_ne!(a, b);
        assert_ne!(root, b);
        assert_eq!(document.numeric_matches(0), [root, a, b]);
        for identity in [root, a, b] {
            assert_eq!(document.get(identity).unwrap().id, 0);
            assert_eq!(
                document.select(&Selector::SelfTarget, Some(identity)),
                [identity]
            );
        }
        assert_eq!(document.select(&Selector::Id("b".into()), Some(a)), [b]);
    }

    #[test]
    fn nonzero_numeric_and_string_duplicate_warnings_survive_default_zero_policy() {
        let mut document = Document::new();
        let summary = document
            .apply_view(
                page(Container {
                    children: vec![
                        Container::default(),
                        node(7, Some("duplicate"), vec![]),
                        node(7, Some("duplicate"), vec![]),
                    ],
                    ..Container::default()
                }),
                ViewOrigin::Navigation,
            )
            .unwrap();
        assert_eq!(
            summary.diagnostics,
            [
                Diagnostic::DuplicateStringId("duplicate".into()),
                Diagnostic::DuplicateNumericId(7),
            ]
        );
        assert_eq!(document.numeric_matches(0).len(), 2);
        assert_eq!(document.numeric_matches(7).len(), 2);
    }

    #[test]
    fn duplicate_ids_are_diagnostic_and_do_not_alias_keys() {
        let mut document = fixture();
        let source = key(&document, "root");
        let summary = route(
            &mut document,
            source,
            Selector::SelfTarget,
            SwapStrategy::Children,
            vec![node(7, Some("same"), vec![]), node(7, Some("same"), vec![])],
        );
        let matches = document.select(&Selector::Id("same".into()), None);
        assert_eq!(matches.len(), 2);
        assert_ne!(matches[0], matches[1]);
        assert_eq!(document.numeric_matches(7), matches);
        assert!(
            summary
                .diagnostics
                .contains(&Diagnostic::DuplicateStringId("same".into()))
        );
        assert!(
            summary
                .diagnostics
                .contains(&Diagnostic::DuplicateNumericId(7))
        );
        let previous = matches;
        route(
            &mut document,
            source,
            Selector::SelfTarget,
            SwapStrategy::Children,
            vec![node(7, Some("same"), vec![])],
        );
        assert!(!previous.contains(&key(&document, "same")));
    }

    #[test]
    fn selectors_use_live_global_or_direct_child_scope() {
        let mut document = fixture();
        let root = key(&document, "root");
        let mut nested = node(2, Some("a"), vec![node(4, Some("nested"), vec![])]);
        nested.classes.push("match".into());
        nested.children[0].classes.push("match".into());
        route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Children,
            vec![nested],
        );
        assert_eq!(
            document
                .select(&Selector::Class("match".into()), Some(root))
                .len(),
            2
        );
        assert_eq!(
            document.select(&Selector::ChildClass("match".into()), Some(root)),
            vec![key(&document, "a")]
        );
        assert_eq!(document.select(&Selector::SelfTarget, None), vec![root]);
        let detached = key(&document, "a");
        route(
            &mut document,
            root,
            Selector::Id("a".into()),
            SwapStrategy::Delete,
            vec![],
        );
        assert_eq!(
            document.select(&Selector::Id("root".into()), Some(detached)),
            []
        );
    }

    #[test]
    fn multi_target_snapshot_does_not_match_new_payloads_or_detached_descendants() {
        let mut document = fixture();
        let root = key(&document, "root");
        let mut parent = node(2, Some("a"), vec![node(4, Some("nested"), vec![])]);
        parent.classes.push("match".into());
        parent.children[0].classes.push("match".into());
        let mut sibling = node(3, Some("b"), vec![]);
        sibling.classes.push("match".into());
        route(
            &mut document,
            root,
            Selector::SelfTarget,
            SwapStrategy::Children,
            vec![parent, sibling],
        );
        let mut payload = node(5, Some("new"), vec![]);
        payload.classes.push("match".into());
        let summary = document
            .apply_view(
                View {
                    fragments: vec![ReplaceContainer {
                        selector: Selector::Class("match".into()),
                        container: payload,
                    }],
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        assert_eq!(names(&document.roots()[0].children), ["new", "new"]);
        assert_eq!(summary.inserted.len(), 2);
        assert_eq!(summary.removed.len(), 3);
        assert_eq!(document.numeric_matches(5).len(), 2);
    }

    #[test]
    fn source_replacement_does_not_retarget_primary_by_reused_string_id() {
        let mut document = fixture();
        let source = key(&document, "a");
        let summary = document
            .apply_view(
                View {
                    fragments: vec![node(4, Some("a"), vec![]).into()],
                    primary: Some(Container::from(vec![node(5, Some("unexpected"), vec![])])),
                    ..View::default()
                },
                ViewOrigin::Element {
                    source,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::This,
                },
            )
            .unwrap();
        assert_eq!(names(&document.roots()[0].children), ["a", "b"]);
        assert!(summary.diagnostics.contains(&Diagnostic::MissingTarget));
    }

    #[test]
    fn transaction_rolls_back_on_counter_exhaustion() {
        let mut document = fixture();
        let before = document.roots().to_vec();
        let source = key(&document, "root");
        document.next_serial = u64::MAX;
        let result = document.apply_view(
            View {
                delete_selectors: vec![Selector::Id("b".into())],
                primary: Some(Container::from(vec![node(
                    5,
                    Some("cannot-insert"),
                    vec![],
                )])),
                ..View::default()
            },
            ViewOrigin::Element {
                source,
                target: Selector::SelfTarget,
                strategy: SwapStrategy::BeforeEnd,
            },
        );
        assert_eq!(result.unwrap_err(), DocumentError::IdentityExhausted);
        assert_eq!(document.roots(), before);
        assert!(document.get(key(&document, "b")).is_some());
    }

    #[test]
    fn tickets_are_scoped_single_use_and_ignore_unrelated_changes() {
        let mut document = fixture();
        let source = key(&document, "a");
        let old = document
            .begin_request(
                source,
                "search".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        let current = document
            .begin_request(
                source,
                "search".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        let other = document
            .begin_request(
                source,
                "other".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        assert!(!document.request_is_current(&old));
        let root = key(&document, "root");
        route(
            &mut document,
            root,
            Selector::Id("b".into()),
            SwapStrategy::This,
            vec![node(5, Some("b"), vec![])],
        );
        assert!(document.request_is_current(&current));
        assert!(document.request_is_current(&other));
        document
            .complete_request(&current, View::default())
            .unwrap();
        assert!(!document.request_is_current(&current));
        assert!(document.request_is_current(&other));
        assert_eq!(
            document
                .complete_request(&old, View::default())
                .unwrap_err(),
            DocumentError::StaleRequest
        );
    }

    #[test]
    fn target_content_membership_and_navigation_invalidate_tickets() {
        let mut document = fixture();
        let source = key(&document, "a");
        let target = key(&document, "b");
        let ticket = document
            .begin_request(
                source,
                "route".into(),
                Selector::Id("b".into()),
                SwapStrategy::Children,
            )
            .unwrap();
        route(
            &mut document,
            target,
            Selector::SelfTarget,
            SwapStrategy::BeforeEnd,
            vec![node(5, Some("child"), vec![])],
        );
        assert!(!document.request_is_current(&ticket));
        let ticket = document
            .begin_request(
                source,
                "route".into(),
                Selector::Class("new-class".into()),
                SwapStrategy::Children,
            )
            .unwrap();
        let mut matching = node(6, Some("matching"), vec![]);
        matching.classes.push("new-class".into());
        route(
            &mut document,
            target,
            Selector::SelfTarget,
            SwapStrategy::BeforeEnd,
            vec![matching],
        );
        assert!(!document.request_is_current(&ticket));
        let stamp = document.stamp(source).unwrap();
        document
            .apply_view(page(node(1, Some("root"), vec![])), ViewOrigin::Navigation)
            .unwrap();
        assert!(!document.is_current(stamp));
        assert!(!document.request_is_current(&ticket));
    }

    #[test]
    fn navigation_and_stale_responses_do_not_accumulate_pending_scopes() {
        let mut document = fixture();
        for iteration in 0..128 {
            let source = key(&document, "a");
            let ticket = document
                .begin_request(
                    source,
                    format!("scope-{iteration}"),
                    Selector::SelfTarget,
                    SwapStrategy::None,
                )
                .unwrap();
            assert_eq!(document.pending_requests(), 1);
            document
                .apply_view(
                    page(node(1, Some("root"), vec![node(2, Some("a"), vec![])])),
                    ViewOrigin::Navigation,
                )
                .unwrap();
            assert_eq!(document.pending_requests(), 0);
            assert_eq!(
                document
                    .complete_request(&ticket, View::default())
                    .unwrap_err(),
                DocumentError::StaleRequest
            );
            assert_eq!(document.pending_requests(), 0);
        }
    }

    #[test]
    fn removed_or_changed_sources_and_targets_prune_pending_requests() {
        let mut document = fixture();
        let a = key(&document, "a");
        let b = key(&document, "b");
        document
            .begin_request(a, "source".into(), Selector::SelfTarget, SwapStrategy::None)
            .unwrap();
        document
            .begin_request(
                b,
                "target".into(),
                Selector::Id("a".into()),
                SwapStrategy::Children,
            )
            .unwrap();
        route(
            &mut document,
            a,
            Selector::SelfTarget,
            SwapStrategy::BeforeEnd,
            vec![node(7, None, vec![])],
        );
        assert_eq!(document.pending_requests(), 0);
        document
            .begin_request(
                b,
                "removed".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        route(
            &mut document,
            b,
            Selector::SelfTarget,
            SwapStrategy::Delete,
            vec![],
        );
        assert_eq!(document.pending_requests(), 0);
    }

    #[test]
    fn cancellation_releases_abandoned_scopes_without_cancelling_successors() {
        let mut document = fixture();
        let source = key(&document, "a");
        let old = document
            .begin_request(
                source,
                "scope".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        let current = document
            .begin_request(
                source,
                "scope".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        assert!(!document.cancel_request(&old));
        assert_eq!(
            document
                .complete_request(&old, View::default())
                .unwrap_err(),
            DocumentError::StaleRequest
        );
        assert!(document.request_is_current(&current));
        assert_eq!(document.pending_requests(), 1);
        assert!(document.cancel_request(&current));
        assert!(!document.cancel_request(&current));
        assert_eq!(document.pending_requests(), 0);
        document
            .begin_request(
                source,
                "abandoned".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        assert!(document.cancel_scope(source, "abandoned"));
        assert!(!document.cancel_scope(source, "abandoned"));
        assert_eq!(document.pending_requests(), 0);
    }

    #[test]
    fn concurrent_namespace_allocation_never_aliases_equal_documents() {
        let workers: [_; 4] = std::array::from_fn(|_| {
            std::thread::spawn(|| {
                (0..32)
                    .map(|_| fixture().root_keys()[0])
                    .collect::<Vec<_>>()
            })
        });
        let keys = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        let unique = keys
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(keys.len(), 128);
        assert_eq!(unique.len(), keys.len());
    }

    #[test]
    fn independent_and_recreated_documents_reject_foreign_keys_and_tickets() {
        let mut first = fixture();
        let mut second = fixture();
        let first_key = key(&first, "a");
        let second_key = key(&second, "a");
        assert_ne!(first_key, second_key);
        let foreign = first
            .begin_request(
                first_key,
                "equal".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        let local = second
            .begin_request(
                second_key,
                "equal".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        assert!(!second.request_is_current(&foreign));
        assert!(second.get(first_key).is_none());
        assert_eq!(
            second
                .complete_request(&foreign, View::default())
                .unwrap_err(),
            DocumentError::StaleRequest
        );
        assert!(second.request_is_current(&local));
        drop(first);
        let recreated = fixture();
        assert_ne!(key(&recreated, "a"), first_key);
        assert!(!recreated.request_is_current(&foreign));
    }

    #[test]
    fn private_transaction_snapshots_preserve_namespace_and_request_authority() {
        let mut document = fixture();
        let source = key(&document, "a");
        let ticket = document
            .begin_request(
                source,
                "route".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        // Document intentionally has no public Clone. This private snapshot is used
        // only for atomic staging and cannot be handed out as an independent document.
        let snapshot = document.snapshot();
        assert_eq!(snapshot.root_keys(), document.root_keys());
        assert!(snapshot.request_is_current(&ticket));
        document
            .apply_view(View::default(), ViewOrigin::Navigation)
            .unwrap();
        assert!(document.request_is_current(&ticket));
        document.complete_request(&ticket, View::default()).unwrap();
        assert_eq!(document.pending_requests(), 0);
    }

    #[test]
    fn failed_completion_is_terminal_but_keeps_authored_tree_unchanged() {
        let mut document = fixture();
        let source = key(&document, "a");
        let ticket = document
            .begin_request(
                source,
                "route".into(),
                Selector::SelfTarget,
                SwapStrategy::BeforeEnd,
            )
            .unwrap();
        let before = document.roots().to_vec();
        document.next_serial = u64::MAX;
        assert_eq!(
            document
                .complete_request(&ticket, page(Container::from(vec![node(9, None, vec![])])))
                .unwrap_err(),
            DocumentError::IdentityExhausted
        );
        assert_eq!(document.roots(), before);
        assert_eq!(document.pending_requests(), 0);
    }

    #[test]
    fn stale_completion_cannot_apply_fragments_deletes_or_metadata() {
        let mut document = fixture();
        let source = key(&document, "a");
        let ticket = document
            .begin_request(
                source,
                "route".into(),
                Selector::SelfTarget,
                SwapStrategy::None,
            )
            .unwrap();
        route(
            &mut document,
            source,
            Selector::SelfTarget,
            SwapStrategy::Delete,
            vec![],
        );
        let before = document.roots().to_vec();
        let view = View {
            fragments: vec![node(9, Some("b"), vec![]).into()],
            delete_selectors: vec![Selector::Id("root".into())],
            ..View::default()
        };
        assert_eq!(
            document.complete_request(&ticket, view).unwrap_err(),
            DocumentError::StaleRequest
        );
        assert_eq!(document.roots(), before);
    }
}
