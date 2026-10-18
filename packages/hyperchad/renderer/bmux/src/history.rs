//! Bounded, context-scoped document snapshots for renderer history.
//!
//! This is storage only: navigation commitment, request cancellation, fresh
//! lifecycle activation and suppression of load effects belong to the program.
//! Snapshots contain no executable queues, response metadata or graphics buffers.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::{self, Write},
    mem::size_of,
};

use crate::layout::ScrollOffset;
use hyperchad_transformer::Container;

const MAX_DOCUMENTS: usize = 8;
// The production 100-album tree has 1,870 Containers: 3,201,440 bytes
// of inline storage alone (1,712 bytes each on the measured 64-bit build).
// Leave room for owned cover URLs, metadata, actions and collection storage.
// Back/forward histories share the aggregate cap enforced by the program:
// at most eight snapshots and 32 MiB charged, with at most 8 MiB per document.
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;

/// An immutable canonical document and its renderer-owned view position.
///
/// Controls are deliberately excluded: native editors contain non-cloneable
/// state and file/password values must not be retained by history storage.
#[derive(Debug)]
pub struct DocumentSnapshot {
    url: String,
    root: Container,
    scroll: BTreeMap<String, ScrollOffset>,
    details: BTreeMap<String, bool>,
    completed_loads: BTreeSet<String>,
    charge: usize,
}

/// Owned document state transferred out of a history snapshot without cloning.
#[derive(Debug)]
pub struct DocumentSnapshotParts {
    /// Canonical successful navigation URL.
    pub url: String,
    /// Complete canonical document tree.
    pub root: Container,
    /// Scroll positions keyed by stable container identity.
    pub scroll: BTreeMap<String, ScrollOffset>,
    /// Expanded detail controls keyed by stable container identity.
    pub details: BTreeMap<String, bool>,
    /// Successfully completed load request identities.
    pub completed_loads: BTreeSet<String>,
}

impl DocumentSnapshot {
    /// Canonical successful URL, including its query string.
    #[cfg(test)]
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Canonical tree after its latest admitted partial response.
    #[cfg(test)]
    #[must_use]
    pub const fn root(&self) -> &Container {
        &self.root
    }

    /// Consume the snapshot without another document clone.
    #[must_use]
    pub fn into_parts(self) -> DocumentSnapshotParts {
        DocumentSnapshotParts {
            url: self.url,
            root: self.root,
            scroll: self.scroll,
            details: self.details,
            completed_loads: self.completed_loads,
        }
    }

    /// Conservative logical memory charge, not an allocator/RSS measurement.
    #[must_use]
    pub const fn charged_bytes(&self) -> usize {
        self.charge
    }
}

/// Eight-document FIFO history stack, bounded to 32 MiB of charged storage.
///
/// A snapshot is limited to 8 MiB. Every operation accepting an epoch first
/// discards entries from a different context, including rejected admissions.
/// The caller must pass the *current* authorization epoch, never an old request's
/// epoch. No snapshot may be installed without rechecking that current epoch.
#[derive(Debug, Default)]
pub struct DocumentHistory {
    epoch: Option<u64>,
    entries: VecDeque<DocumentSnapshot>,
    bytes: usize,
}

impl DocumentHistory {
    /// Purge cached documents when the current resource context changes.
    pub fn set_context(&mut self, epoch: Option<u64>) {
        if self.epoch != epoch {
            self.entries.clear();
            self.bytes = 0;
            self.epoch = epoch;
        }
    }

    /// Forget every cached document without changing the current context.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    /// Number of cached documents (at most eight).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no document is available.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total conservative logical memory charge.
    #[must_use]
    pub const fn charged_bytes(&self) -> usize {
        self.bytes
    }

    /// Copy a document into history, evicting oldest snapshots as necessary.
    ///
    /// Returns false for excessive size/depth, arithmetic overflow, password or
    /// file controls. Rejection never evicts same-context snapshots. All owned
    /// tree fields (including nested actions, overrides, state and expressions)
    /// participate through the complete derived Debug representation; formatting
    /// streams into a bounded counter, never an intermediate unbounded String.
    /// Quoted owned strings are charged fourfold; structural punctuation carries
    /// allocation overhead and per-node inline storage is charged separately.
    /// Fixed field names and None labels do not consume a fictitious text budget.
    /// This is a logical estimate, not exact allocator accounting.
    /// Input spare capacities are not retained: storage owns a fresh clone.
    pub fn push(
        &mut self,
        epoch: Option<u64>,
        url: &str,
        root: &Container,
        scroll: &BTreeMap<String, ScrollOffset>,
        details: &BTreeMap<String, bool>,
    ) -> bool {
        self.push_with_completed(epoch, url, root, scroll, details, &BTreeSet::new())
    }

    /// Cache a document with the identities of successfully completed load GETs.
    /// The set is charged within the same per-document and aggregate limits.
    pub fn push_with_completed(
        &mut self,
        epoch: Option<u64>,
        url: &str,
        root: &Container,
        scroll: &BTreeMap<String, ScrollOffset>,
        details: &BTreeMap<String, bool>,
        completed_loads: &BTreeSet<String>,
    ) -> bool {
        self.set_context(epoch);
        let Some(charge) = charge(url, root, scroll, details) else {
            log::debug!(
                "bmux history admission rejected reason=document-budget-depth-or-sensitive-control"
            );
            return false;
        };
        let mut counter = Charge::new(charge);
        if write!(&mut counter, "{completed_loads:?}").is_err() {
            log::debug!("bmux history admission rejected reason=completed-load-budget");
            return false;
        }
        let charge = counter.bytes;
        let snapshot = DocumentSnapshot {
            url: url.to_owned(),
            root: root.clone(),
            scroll: scroll.clone(),
            details: details.clone(),
            completed_loads: completed_loads.clone(),
            charge,
        };
        while self.entries.len() >= MAX_DOCUMENTS || self.bytes > MAX_TOTAL_BYTES - charge {
            let Some(oldest) = self.entries.pop_front() else {
                return false;
            };
            self.bytes -= oldest.charge;
        }
        self.bytes += charge;
        self.entries.push_back(snapshot);
        log::debug!(
            "bmux history admission accepted charge={charge} entries={} total={}",
            self.entries.len(),
            self.bytes
        );
        true
    }

    /// Evict the oldest snapshot, for a shared back/forward aggregate budget.
    pub fn discard_oldest(&mut self) {
        if let Some(snapshot) = self.entries.pop_front() {
            self.bytes -= snapshot.charge;
        }
    }

    /// Take the most recently cached document, purging first on context change.
    pub fn pop(&mut self, epoch: Option<u64>) -> Option<DocumentSnapshot> {
        self.set_context(epoch);
        let snapshot = self.entries.pop_back()?;
        self.bytes -= snapshot.charge;
        Some(snapshot)
    }
}

struct Charge {
    bytes: usize,
    quoted: bool,
    escaped: bool,
}
impl Charge {
    const fn new(bytes: usize) -> Self {
        Self {
            bytes,
            quoted: false,
            escaped: false,
        }
    }
}
impl fmt::Write for Charge {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            let cost = if self.quoted {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.quoted = false;
                }
                4
            } else {
                match byte {
                    b'"' => {
                        self.quoted = true;
                        32
                    }
                    b'{' => 128,
                    b'[' => 64,
                    _ => 0,
                }
            };
            self.bytes = self.bytes.checked_add(cost).ok_or(fmt::Error)?;
            if self.bytes > MAX_DOCUMENT_BYTES {
                return Err(fmt::Error);
            }
        }
        Ok(())
    }
}

fn charge(
    url: &str,
    root: &Container,
    scroll: &BTreeMap<String, ScrollOffset>,
    details: &BTreeMap<String, bool>,
) -> Option<usize> {
    fn nodes(root: &Container, depth: usize, count: &mut usize) -> Option<()> {
        if depth > 64 {
            return None;
        }
        *count = count.checked_add(1)?;
        if *count > 4096 {
            return None;
        }
        if matches!(
            root.element,
            hyperchad_transformer::Element::Input {
                input: hyperchad_transformer::Input::Password { .. }
                    | hyperchad_transformer::Input::File,
                ..
            }
        ) {
            return None;
        }
        for child in &root.children {
            nodes(child, depth + 1, count)?;
        }
        Some(())
    }
    // Bound recursive Debug/Clone of *all* nested expression/action/JSON fields,
    // not just DOM children. Reuse the renderer's complete admission traversal.
    let config = crate::renderer::BmuxConfig {
        max_nodes: 4096,
        max_depth: 64,
        max_text_bytes: MAX_DOCUMENT_BYTES,
        ..Default::default()
    };
    if let Err(error) = crate::admission::validate_tree(root, &config) {
        log::debug!("bmux history admission rejected reason=tree-admission error={error}");
        return None;
    }
    let mut count = 0;
    if nodes(root, 0, &mut count).is_none() {
        log::debug!(
            "bmux history admission rejected reason=dom-depth-count-or-sensitive-control nodes={count}"
        );
        return None;
    }
    let mut counter = Charge::new(
        size_of::<DocumentSnapshot>().checked_add(count.checked_mul(size_of::<Container>())?)?,
    );
    if write!(&mut counter, "{url:?}{root:?}{scroll:?}{details:?}").is_err() {
        log::debug!(
            "bmux history admission rejected reason=byte-budget nodes={count} inline={} charge={}",
            count * size_of::<Container>(),
            counter.bytes
        );
        return None;
    }
    log::debug!(
        "bmux history measured nodes={count} inline={} charge={}",
        count * size_of::<Container>(),
        counter.bytes
    );
    (counter.bytes <= MAX_DOCUMENT_BYTES).then_some(counter.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_exact_tree_url_and_position() {
        let mut history = DocumentHistory::default();
        let root = Container {
            children: vec![Container::default()],
            ..Default::default()
        };
        let scroll = BTreeMap::from([("main".into(), ScrollOffset { x: 3, y: 72 })]);
        let details = BTreeMap::from([("filters".into(), true)]);
        assert!(history.push(Some(1), "/albums?sort=name", &root, &scroll, &details));
        let DocumentSnapshotParts {
            url,
            root: cached,
            scroll: cached_scroll,
            details: cached_details,
            ..
        } = history.pop(Some(1)).unwrap().into_parts();
        assert_eq!(url, "/albums?sort=name");
        assert_eq!(cached, root);
        assert_eq!(cached_scroll, scroll);
        assert_eq!(cached_details, details);
        assert_eq!(history.charged_bytes(), 0);
    }

    #[test]
    fn fifo_eviction_lifo_restore_and_context_purge() {
        let mut history = DocumentHistory::default();
        for index in 0..12 {
            assert!(history.push(
                Some(1),
                &format!("/{index}"),
                &Container::default(),
                &BTreeMap::new(),
                &BTreeMap::new()
            ));
            assert!(history.len() <= MAX_DOCUMENTS);
            assert!(history.charged_bytes() <= MAX_TOTAL_BYTES);
        }
        for index in (4..12).rev() {
            assert_eq!(history.pop(Some(1)).unwrap().url(), format!("/{index}"));
        }
        assert!(history.is_empty());
        assert!(history.push(
            Some(1),
            "/albums",
            &Container::default(),
            &BTreeMap::new(),
            &BTreeMap::new()
        ));
        assert!(history.pop(Some(2)).is_none());
        assert_eq!(history.charged_bytes(), 0);
    }

    #[test]
    fn oversized_nested_fields_and_scroll_keys_are_rejected_without_eviction() {
        let mut history = DocumentHistory::default();
        assert!(history.push(
            None,
            "/ok",
            &Container::default(),
            &BTreeMap::new(),
            &BTreeMap::new()
        ));
        let root = Container {
            state: Some(serde_json::json!({"deep": ["x".repeat(MAX_DOCUMENT_BYTES)]})),
            ..Default::default()
        };
        assert!(!history.push(None, "/large", &root, &BTreeMap::new(), &BTreeMap::new()));
        let scroll = BTreeMap::from([("x".repeat(MAX_DOCUMENT_BYTES), ScrollOffset::default())]);
        assert!(!history.push(
            None,
            "/large",
            &Container::default(),
            &scroll,
            &BTreeMap::new()
        ));
        assert_eq!(history.len(), 1);
        assert_eq!(history.pop(None).unwrap().url(), "/ok");
    }

    #[test]
    fn rejected_new_context_still_purges_old_documents() {
        let mut history = DocumentHistory::default();
        assert!(history.push(
            Some(1),
            "/ok",
            &Container::default(),
            &BTreeMap::new(),
            &BTreeMap::new()
        ));
        assert!(!history.push(
            Some(2),
            &"x".repeat(MAX_DOCUMENT_BYTES),
            &Container::default(),
            &BTreeMap::new(),
            &BTreeMap::new()
        ));
        assert!(history.is_empty());
        assert_eq!(history.charged_bytes(), 0);
    }
}
