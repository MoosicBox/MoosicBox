//! Inode-based core of the filesystem simulator.
//!
//! The simulated filesystem is an inode table. Directories map names to inode numbers,
//! paths are resolved one component at a time (following symbolic links up to
//! [`MAX_SYMLINKS`]), and open handles refer to inodes, so renaming or unlinking an open
//! file behaves as on POSIX systems.
//!
//! # Durability model
//!
//! Every inode tracks the state a crash would preserve separately from the state reads
//! observe:
//!
//! * File data and length become durable when the file is synchronized
//!   (`sync_data`/`sync_all`). Until then, writes are recorded in a per-file log that a
//!   crash may keep entirely, drop entirely, or keep a torn prefix of.
//! * Permissions become durable on `sync_all` of the inode.
//! * Directory changes (create, link, unlink, rename) are journaled and become durable
//!   only when a directory they touch is synchronized. Synchronizing a directory also
//!   commits every earlier pending change to the same directory entries, so durable state
//!   never observes a later change to an entry without the earlier ones. A rename is a
//!   single journal entry and is therefore atomic across a crash.
//!
//! A crash rebuilds the visible state from the durable state, invalidates every open
//! handle, releases every lock and discards inodes that are no longer reachable.
//!
//! # Timestamps
//!
//! Timestamps come from the simulated clock ([`switchy_time::now`]) and are updated as
//! POSIX specifies: creation sets every timestamp, writing or truncating a file updates
//! its modification time, reading a file, directory or symbolic link updates its access
//! time, and creating, removing or renaming an entry updates its directory's modification
//! time. Timestamps are not part of the durability model: a crash keeps each surviving
//! inode's current timestamps.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use super::{
    Filesystem, Personality,
    fault::{CrashPolicy, DirectoryOpLoss, Fault, FaultPlan, FileDataLoss, OpClass},
};
use crate::{
    errno::Errno,
    types::{DirEntryInfo, FileId, FileKind, Metadata, OpenSpec},
};

pub type Result<T> = std::result::Result<T, Errno>;

/// Inode number.
pub type Ino = u64;

/// Inode number of the root directory.
pub const ROOT: Ino = 1;

/// Maximum number of symbolic links followed while resolving one path (Linux `MAXSYMLINKS`).
pub const MAX_SYMLINKS: u32 = 40;

/// Device identifier reported for simulated inodes.
pub const DEVICE_ID: u64 = 0x5357_4653;

/// Default simulated effective user and group id.
pub const DEFAULT_UID: u32 = 1000;

/// Default simulated process umask.
pub const DEFAULT_UMASK: u32 = 0o022;

const READ: u32 = 4;
const WRITE: u32 = 2;
const SEARCH: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Meta {
    mode: u32,
    uid: u32,
    gid: u32,
}

#[derive(Debug, Clone)]
enum PendingWrite {
    Write { offset: u64, bytes: Vec<u8> },
    SetLen(u64),
}

#[derive(Debug, Default)]
struct FileNode {
    data: Vec<u8>,
    durable: Vec<u8>,
    pending: Vec<PendingWrite>,
}

#[derive(Debug)]
struct DirNode {
    entries: BTreeMap<OsString, Ino>,
    durable: BTreeMap<OsString, Ino>,
    parent: Ino,
}

#[derive(Debug)]
enum Node {
    File(FileNode),
    Dir(DirNode),
    Symlink(PathBuf),
}

/// The timestamps of an inode.
#[derive(Debug, Clone, Copy)]
struct Times {
    accessed: SystemTime,
    modified: SystemTime,
    created: SystemTime,
}

impl Times {
    /// Every timestamp set to the current simulated time.
    fn now() -> Self {
        let now = switchy_time::now();
        Self {
            accessed: now,
            modified: now,
            created: now,
        }
    }
}

#[derive(Debug)]
struct Inode {
    node: Node,
    meta: Meta,
    times: Times,
    durable_meta: Meta,
    /// Current directory entries referring to this inode.
    nlink: u64,
    /// Open handles referring to this inode.
    open: u64,
    /// Durable directory entries referring to this inode.
    durable_links: u64,
    /// Pending journal changes that would link this inode.
    journal_refs: u64,
}

impl Inode {
    const fn kind(&self) -> FileKind {
        match self.node {
            Node::File(_) => FileKind::File,
            Node::Dir(_) => FileKind::Dir,
            Node::Symlink(_) => FileKind::Symlink,
        }
    }

    const fn is_dir(&self) -> bool {
        matches!(self.node, Node::Dir(_))
    }

    const fn is_symlink(&self) -> bool {
        matches!(self.node, Node::Symlink(_))
    }

    /// Bytes this inode charges against the namespace quota.
    fn live_len(&self) -> u64 {
        match &self.node {
            Node::File(file) if self.nlink > 0 || self.open > 0 => len_u64(&file.data),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
struct Change {
    dir: Ino,
    name: OsString,
    new: Option<Ino>,
}

#[derive(Debug, Clone)]
struct JournalOp {
    changes: Vec<Change>,
}

#[derive(Debug, Default)]
struct LockState {
    exclusive: Option<u64>,
    shared: BTreeSet<u64>,
}

/// A path component after lexical splitting.
#[derive(Debug, Clone)]
enum Comp {
    Root,
    Parent,
    Name(OsString),
}

/// The result of resolving every component of a path except (possibly) the last.
#[derive(Debug, Clone)]
pub enum Entry {
    /// The path names a directory without a final name (`/`, `..`, `.`).
    Dir(Ino),
    /// The path's final component `name` within directory `dir`.
    Named {
        dir: Ino,
        name: OsString,
        ino: Option<Ino>,
        trailing: bool,
    },
}

fn len_u64(data: &[u8]) -> u64 {
    u64::try_from(data.len()).unwrap_or(u64::MAX)
}

fn to_usize(value: u64) -> Result<usize> {
    usize::try_from(value).map_err(|_| Errno::Inval)
}

fn split(path: &Path) -> Result<(Vec<Comp>, bool)> {
    if path.as_os_str().is_empty() {
        return Err(Errno::NoEnt);
    }
    let trailing = path
        .as_os_str()
        .as_encoded_bytes()
        .last()
        .is_some_and(|byte| std::path::is_separator(char::from(*byte)));
    let comps = path
        .components()
        .filter_map(|component| match component {
            Component::Prefix(_) | Component::RootDir => Some(Comp::Root),
            Component::CurDir => None,
            Component::ParentDir => Some(Comp::Parent),
            Component::Normal(name) => Some(Comp::Name(name.to_os_string())),
        })
        .collect();
    Ok((comps, trailing))
}

/// Apply one logged write to `data`.
fn apply_write(data: &mut Vec<u8>, write: &PendingWrite) {
    match write {
        PendingWrite::Write { offset, bytes } => {
            let Ok(start) = usize::try_from(*offset) else {
                return;
            };
            let end = start.saturating_add(bytes.len());
            if data.len() < end {
                data.resize(end, 0);
            }
            data[start..end].copy_from_slice(bytes);
        }
        PendingWrite::SetLen(len) => {
            if let Ok(len) = usize::try_from(*len) {
                data.resize(len, 0);
            }
        }
    }
}

/// The complete state of one simulated filesystem namespace.
#[derive(Debug)]
pub struct Vfs {
    inodes: BTreeMap<Ino, Inode>,
    next_ino: Ino,
    pub generation: u64,
    next_handle: u64,
    journal: Vec<JournalOp>,
    uid: u32,
    gid: u32,
    umask: u32,
    quota: Option<u64>,
    used: u64,
    plan: FaultPlan,
    injected: VecDeque<(OpClass, Fault)>,
    locks: BTreeMap<Ino, LockState>,
    personality: Personality,
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new(DEFAULT_UID, DEFAULT_UID)
    }
}

impl Vfs {
    /// An empty filesystem whose root directory (`0o755`) is owned by `uid`/`gid`, which
    /// are also the simulated effective credentials.
    pub fn new(uid: u32, gid: u32) -> Self {
        let meta = Meta {
            mode: 0o755,
            uid,
            gid,
        };
        let root = Inode {
            node: Node::Dir(DirNode {
                entries: BTreeMap::new(),
                durable: BTreeMap::new(),
                parent: ROOT,
            }),
            meta,
            times: Times::now(),
            durable_meta: meta,
            nlink: 1,
            open: 0,
            durable_links: 1,
            journal_refs: 0,
        };
        Self {
            inodes: BTreeMap::from([(ROOT, root)]),
            next_ino: ROOT + 1,
            generation: 0,
            next_handle: 1,
            journal: Vec::new(),
            uid,
            gid,
            umask: DEFAULT_UMASK,
            quota: None,
            used: 0,
            plan: FaultPlan::new(),
            injected: VecDeque::new(),
            locks: BTreeMap::new(),
            personality: Personality::Posix,
        }
    }

    /// Replace the whole state with an empty filesystem, keeping credentials, umask,
    /// quota and fault configuration, and invalidating every handle.
    pub fn reset(&mut self) {
        let mut fresh = Self::new(self.uid, self.gid);
        fresh.generation = self.generation + 1;
        fresh.next_handle = self.next_handle;
        fresh.umask = self.umask;
        fresh.quota = self.quota;
        fresh.personality = self.personality;
        fresh.plan = std::mem::take(&mut self.plan);
        fresh.injected = std::mem::take(&mut self.injected);
        *self = fresh;
    }

    // ----- configuration -------------------------------------------------------------

    pub const fn credentials(&self) -> (u32, u32) {
        (self.uid, self.gid)
    }

    pub const fn set_credentials(&mut self, uid: u32, gid: u32) {
        self.uid = uid;
        self.gid = gid;
    }

    pub const fn umask(&self) -> u32 {
        self.umask
    }

    pub const fn set_umask(&mut self, umask: u32) {
        self.umask = umask & 0o777;
    }

    pub const fn personality(&self) -> Personality {
        self.personality
    }

    pub const fn set_personality(&mut self, personality: Personality) {
        self.personality = personality;
    }

    const fn is_windows(&self) -> bool {
        matches!(self.personality, Personality::Windows)
    }

    pub const fn set_quota(&mut self, quota: Option<u64>) {
        self.quota = quota;
    }

    pub const fn used(&self) -> u64 {
        self.used
    }

    pub fn set_fault_plan(&mut self, plan: FaultPlan) {
        self.plan = plan;
    }

    pub fn inject(&mut self, class: OpClass, fault: Fault) {
        self.injected.push_back((class, fault));
    }

    pub fn clear_injected(&mut self) {
        self.injected.clear();
    }

    // ----- faults ----------------------------------------------------------------------

    /// Decide whether an operation of `class` fails, consuming a queued one-shot fault or
    /// drawing from the plan. `Io` and `NoSpace` become errors; other faults are returned.
    fn fault(&mut self, class: OpClass) -> Result<Option<Fault>> {
        let fault = if let Some(position) = self.injected.iter().position(|(c, _)| *c == class) {
            self.injected.remove(position).map(|(_, fault)| fault)
        } else {
            self.plan
                .rules
                .iter()
                .filter(|rule| rule.class == class)
                .find(|rule| {
                    rule.probability
                        .fires(|bound| draw("switchy_fs::faults", bound))
                })
                .map(|rule| rule.fault)
        };
        match fault {
            Some(Fault::Io) => Err(Errno::Io),
            Some(Fault::NoSpace) => Err(Errno::NoSpc),
            other => Ok(other),
        }
    }

    // ----- inode helpers ---------------------------------------------------------------

    fn inode(&self, ino: Ino) -> Result<&Inode> {
        self.inodes.get(&ino).ok_or(Errno::Stale)
    }

    fn inode_mut(&mut self, ino: Ino) -> Result<&mut Inode> {
        self.inodes.get_mut(&ino).ok_or(Errno::Stale)
    }

    fn dir(&self, ino: Ino) -> Result<&DirNode> {
        match &self.inode(ino)?.node {
            Node::Dir(dir) => Ok(dir),
            _ => Err(Errno::NotDir),
        }
    }

    fn dir_mut(&mut self, ino: Ino) -> Result<&mut DirNode> {
        match &mut self.inode_mut(ino)?.node {
            Node::Dir(dir) => Ok(dir),
            _ => Err(Errno::NotDir),
        }
    }

    fn is_symlink(&self, ino: Ino) -> bool {
        self.inode(ino).is_ok_and(Inode::is_symlink)
    }

    fn is_dir(&self, ino: Ino) -> bool {
        self.inode(ino).is_ok_and(Inode::is_dir)
    }

    /// Check `want` (a combination of read/write/search bits) against the simulated
    /// effective credentials. Root bypasses read/write/search checks.
    fn check_access(&self, ino: Ino, want: u32) -> Result<()> {
        if self.uid == 0 {
            return Ok(());
        }
        let inode = self.inode(ino)?;
        let mode = inode.meta.mode;
        let bits = if inode.meta.uid == self.uid {
            mode >> 6
        } else if inode.meta.gid == self.gid {
            mode >> 3
        } else {
            mode
        } & 0o7;
        if bits & want == want {
            Ok(())
        } else {
            Err(Errno::Access)
        }
    }

    /// The entry `name` of directory `dir`: its stored name and inode. Under the Windows
    /// personality names match case-insensitively (an exact match wins).
    fn find(&self, dir: Ino, name: &OsStr) -> Result<Option<(&OsString, Ino)>> {
        let entries = &self.dir(dir)?.entries;
        if let Some((stored, ino)) = entries.get_key_value(name) {
            return Ok(Some((stored, *ino)));
        }
        if !self.is_windows() {
            return Ok(None);
        }
        Ok(entries
            .iter()
            .find(|(stored, _)| eq_ignoring_case(stored, name))
            .map(|(stored, ino)| (stored, *ino)))
    }

    /// The spelling the final component of `to` requests when, under the Windows
    /// personality, it differs from the `stored` spelling of the entry it names.
    fn respelling(&self, to: &Path, stored: &OsStr) -> Option<OsString> {
        to.file_name()
            .filter(|requested| self.is_windows() && *requested != stored)
            .map(OsStr::to_os_string)
    }

    /// Under the Windows personality an open file cannot be removed or replaced: files are
    /// opened without delete sharing, so the attempt is a sharing violation.
    fn ensure_not_open(&self, ino: Ino) -> Result<()> {
        let inode = self.inode(ino)?;
        if self.is_windows() && inode.open > 0 && !inode.is_dir() {
            Err(Errno::SharingViolation)
        } else {
            Ok(())
        }
    }

    /// Creating entries in a removed (but still open) directory fails with `ENOENT`.
    fn ensure_linked(&self, dir: Ino) -> Result<()> {
        if dir != ROOT && self.inode(dir)?.nlink == 0 {
            Err(Errno::NoEnt)
        } else {
            Ok(())
        }
    }

    /// Mutate an inode while keeping the quota accounting in sync.
    fn account<T>(&mut self, ino: Ino, work: impl FnOnce(&mut Inode) -> T) -> Result<T> {
        let inode = self.inodes.get_mut(&ino).ok_or(Errno::Stale)?;
        let before = inode.live_len();
        let result = work(inode);
        let after = inode.live_len();
        self.used = self.used.saturating_sub(before).saturating_add(after);
        Ok(result)
    }

    /// Set the modification time of `ino` to the current simulated time.
    fn mark_modified(&mut self, ino: Ino) {
        if let Some(inode) = self.inodes.get_mut(&ino) {
            inode.times.modified = switchy_time::now();
        }
    }

    /// Set the access time of `ino` to the current simulated time.
    fn mark_accessed(&mut self, ino: Ino) {
        if let Some(inode) = self.inodes.get_mut(&ino) {
            inode.times.accessed = switchy_time::now();
        }
    }

    fn alloc(&mut self, node: Node, mode: u32) -> Ino {
        let ino = self.next_ino;
        self.next_ino += 1;
        let meta = Meta {
            mode: mode & 0o7777,
            uid: self.uid,
            gid: self.gid,
        };
        self.inodes.insert(
            ino,
            Inode {
                node,
                meta,
                times: Times::now(),
                durable_meta: meta,
                nlink: 0,
                open: 0,
                durable_links: 0,
                journal_refs: 0,
            },
        );
        ino
    }

    /// Allocate an unlinked regular file (an anonymous temporary file).
    pub fn create_anonymous(&mut self, mode: u32) -> Ino {
        self.alloc(Node::File(FileNode::default()), mode & !self.umask)
    }

    /// Replace the data of an unlinked file, treating it as durable (used for detached
    /// copies of native files).
    pub fn set_detached_data(&mut self, ino: Ino, data: Vec<u8>) -> Result<()> {
        self.account(ino, |inode| {
            if let Node::File(file) = &mut inode.node {
                file.durable.clone_from(&data);
                file.data = data;
            }
        })
    }

    fn link(&mut self, dir: Ino, name: OsString, ino: Ino) -> Result<()> {
        self.dir_mut(dir)?.entries.insert(name.clone(), ino);
        self.mark_modified(dir);
        self.account(ino, |inode| {
            inode.nlink += 1;
            inode.journal_refs += 1;
        })?;
        self.journal.push(JournalOp {
            changes: vec![Change {
                dir,
                name,
                new: Some(ino),
            }],
        });
        Ok(())
    }

    fn unlink(&mut self, dir: Ino, name: &OsStr) -> Result<()> {
        let ino = self
            .dir_mut(dir)?
            .entries
            .remove(name)
            .ok_or(Errno::NoEnt)?;
        self.mark_modified(dir);
        self.account(ino, |inode| inode.nlink = inode.nlink.saturating_sub(1))?;
        self.journal.push(JournalOp {
            changes: vec![Change {
                dir,
                name: name.to_os_string(),
                new: None,
            }],
        });
        self.maybe_gc(ino);
        Ok(())
    }

    /// Remove an inode once nothing (current entries, durable entries, pending journal
    /// changes or open handles) can reach it.
    fn maybe_gc(&mut self, ino: Ino) {
        if ino == ROOT {
            return;
        }
        let collectable = self.inodes.get(&ino).is_some_and(|inode| {
            inode.nlink == 0
                && inode.open == 0
                && inode.durable_links == 0
                && inode.journal_refs == 0
        });
        if !collectable {
            return;
        }
        if let Some(inode) = self.inodes.remove(&ino) {
            self.used = self.used.saturating_sub(inode.live_len());
            self.locks.remove(&ino);
            if let Node::Dir(dir) = inode.node {
                for child in dir.durable.into_values() {
                    if let Some(child_inode) = self.inodes.get_mut(&child) {
                        child_inode.durable_links = child_inode.durable_links.saturating_sub(1);
                    }
                    self.maybe_gc(child);
                }
            }
        }
    }

    // ----- path resolution -------------------------------------------------------------

    /// Resolve every component of `comps` as a directory lookup, following symbolic links.
    fn walk(&self, start: Ino, comps: &[Comp], depth: &mut u32) -> Result<Ino> {
        let mut current = start;
        for comp in comps {
            current = match comp {
                Comp::Root => ROOT,
                Comp::Parent => {
                    let parent = self.dir(current)?.parent;
                    self.check_access(current, SEARCH)?;
                    parent
                }
                Comp::Name(name) => {
                    let (_, child) = self.find(current, name)?.ok_or_else(|| {
                        self.check_access(current, SEARCH)
                            .err()
                            .unwrap_or(Errno::NoEnt)
                    })?;
                    self.check_access(current, SEARCH)?;
                    self.follow(current, child, depth)?
                }
            };
        }
        Ok(current)
    }

    /// Follow `ino` (found in `dir`) through symbolic links to a non-link inode.
    fn follow(&self, dir: Ino, ino: Ino, depth: &mut u32) -> Result<Ino> {
        let Node::Symlink(target) = &self.inode(ino)?.node else {
            return Ok(ino);
        };
        *depth += 1;
        if *depth > MAX_SYMLINKS {
            return Err(Errno::Loop);
        }
        let target = target.clone();
        self.resolve_ino_depth(dir, &target, true, depth)
    }

    /// Resolve all but the final component of `path`.
    fn lookup_parent(&self, start: Ino, path: &Path, depth: &mut u32) -> Result<Entry> {
        let (comps, trailing) = split(path)?;
        let Some((last, init)) = comps.split_last() else {
            return Ok(Entry::Dir(start));
        };
        match last {
            Comp::Name(name) => {
                let dir = self.walk(start, init, depth)?;
                // An existing entry is named as stored (its case may differ under the
                // Windows personality); a missing one as requested.
                let (name, ino) = match self.find(dir, name)? {
                    Some((stored, ino)) => (stored.clone(), Some(ino)),
                    None => (name.clone(), None),
                };
                self.check_access(dir, SEARCH)?;
                Ok(Entry::Named {
                    dir,
                    name,
                    ino,
                    trailing,
                })
            }
            Comp::Root | Comp::Parent => Ok(Entry::Dir(self.walk(start, &comps, depth)?)),
        }
    }

    fn resolve_entry_depth(
        &self,
        start: Ino,
        path: &Path,
        follow: bool,
        depth: &mut u32,
    ) -> Result<Entry> {
        let mut current = self.lookup_parent(start, path, depth)?;
        loop {
            let Entry::Named {
                dir,
                ino: Some(ino),
                trailing,
                ..
            } = &current
            else {
                return Ok(current);
            };
            if !(follow || *trailing) || !self.is_symlink(*ino) {
                return Ok(current);
            }
            *depth += 1;
            if *depth > MAX_SYMLINKS {
                return Err(Errno::Loop);
            }
            let Node::Symlink(target) = &self.inode(*ino)?.node else {
                return Ok(current);
            };
            let (dir, trailing) = (*dir, *trailing);
            let next = self.lookup_parent(dir, &target.clone(), depth)?;
            current = match next {
                Entry::Named {
                    dir,
                    name,
                    ino,
                    trailing: next_trailing,
                } => Entry::Named {
                    dir,
                    name,
                    ino,
                    trailing: trailing || next_trailing,
                },
                dir @ Entry::Dir(_) => dir,
            };
        }
    }

    /// Resolve `path` to an entry, following a final symbolic link when `follow` (or when
    /// the path has a trailing slash).
    pub fn resolve_entry(&self, start: Ino, path: &Path, follow: bool) -> Result<Entry> {
        self.resolve_entry_depth(start, path, follow, &mut 0)
    }

    fn resolve_ino_depth(
        &self,
        start: Ino,
        path: &Path,
        follow: bool,
        depth: &mut u32,
    ) -> Result<Ino> {
        match self.resolve_entry_depth(start, path, follow, depth)? {
            Entry::Dir(ino) => Ok(ino),
            Entry::Named {
                ino: Some(ino),
                trailing,
                ..
            } => {
                if trailing && !self.is_dir(ino) {
                    Err(Errno::NotDir)
                } else {
                    Ok(ino)
                }
            }
            Entry::Named { ino: None, .. } => Err(Errno::NoEnt),
        }
    }

    /// Resolve `path` to an existing inode.
    pub fn resolve(&self, start: Ino, path: &Path, follow: bool) -> Result<Ino> {
        self.resolve_ino_depth(start, path, follow, &mut 0)
    }

    /// The absolute path of directory `dir`, by walking parent links.
    fn dir_path(&self, dir: Ino) -> Result<PathBuf> {
        let mut names = Vec::new();
        let mut current = dir;
        while current != ROOT {
            if self.inode(current)?.nlink == 0 {
                return Err(Errno::NoEnt);
            }
            let parent = self.dir(current)?.parent;
            let name = self
                .dir(parent)?
                .entries
                .iter()
                .find_map(|(name, ino)| (*ino == current).then(|| name.clone()))
                .ok_or(Errno::NoEnt)?;
            names.push(name);
            current = parent;
        }
        let mut path = PathBuf::from("/");
        for name in names.into_iter().rev() {
            path.push(name);
        }
        Ok(path)
    }

    /// Resolve `path` following every symbolic link and return its absolute path.
    pub fn canonicalize(&mut self, start: Ino, path: &Path) -> Result<PathBuf> {
        self.fault(OpClass::Metadata)?;
        match self.resolve_entry(start, path, true)? {
            Entry::Dir(ino) => self.dir_path(ino),
            Entry::Named {
                dir,
                name,
                ino: Some(ino),
                trailing,
            } => {
                if self.is_dir(ino) {
                    self.dir_path(ino)
                } else if trailing {
                    Err(Errno::NotDir)
                } else {
                    Ok(self.dir_path(dir)?.join(name))
                }
            }
            Entry::Named { ino: None, .. } => Err(Errno::NoEnt),
        }
    }

    // ----- open / handles --------------------------------------------------------------

    /// Open (or create) `path` relative to `start`, returning the opened inode.
    pub fn open(&mut self, start: Ino, path: &Path, spec: &OpenSpec) -> Result<Ino> {
        if !spec.is_valid() {
            return Err(Errno::Inval);
        }
        self.fault(OpClass::Open)?;
        let follow = !(spec.nofollow || spec.create_new);
        match self.resolve_entry(start, path, follow)? {
            Entry::Dir(ino) => {
                if spec.create_new {
                    return Err(Errno::Exist);
                }
                self.open_existing(ino, spec, false)
            }
            Entry::Named {
                ino: Some(ino),
                trailing,
                ..
            } => {
                if spec.create_new {
                    return Err(Errno::Exist);
                }
                if self.is_symlink(ino) {
                    // The directory check wins over the no-follow check.
                    return Err(if spec.directory {
                        Errno::NotDir
                    } else {
                        Errno::Loop
                    });
                }
                self.open_existing(ino, spec, trailing)
            }
            Entry::Named {
                dir,
                name,
                ino: None,
                trailing,
            } => {
                if !(spec.create || spec.create_new) {
                    return Err(Errno::NoEnt);
                }
                if trailing {
                    return Err(Errno::IsDir);
                }
                self.ensure_linked(dir)?;
                self.check_access(dir, WRITE | SEARCH)?;
                let ino = self.alloc(Node::File(FileNode::default()), spec.mode & !self.umask);
                self.link(dir, name, ino)?;
                Ok(ino)
            }
        }
    }

    fn open_existing(&mut self, ino: Ino, spec: &OpenSpec, trailing: bool) -> Result<Ino> {
        let is_dir = self.inode(ino)?.is_dir();
        if (trailing || spec.directory) && !is_dir {
            return Err(Errno::NotDir);
        }
        if is_dir && spec.writable() {
            return Err(Errno::IsDir);
        }
        if spec.read {
            self.check_access(ino, READ)?;
        }
        if spec.writable() {
            self.check_access(ino, WRITE)?;
        }
        if spec.truncate && spec.writable() && !is_dir {
            self.truncate_to(ino, 0)?;
        }
        Ok(ino)
    }

    /// Register an open handle on `ino`, returning its handle id.
    pub fn open_handle(&mut self, ino: Ino) -> Result<u64> {
        self.account(ino, |inode| inode.open += 1)?;
        let id = self.next_handle;
        self.next_handle += 1;
        Ok(id)
    }

    /// Release handle `id` on `ino`: drop its locks and its reference.
    pub fn close_handle(&mut self, ino: Ino, id: u64) {
        self.unlock(ino, id);
        if self
            .account(ino, |inode| inode.open = inode.open.saturating_sub(1))
            .is_ok()
        {
            self.maybe_gc(ino);
        }
    }

    // ----- data ------------------------------------------------------------------------

    pub fn read_at(&mut self, ino: Ino, position: u64, buf: &mut [u8]) -> Result<usize> {
        self.fault(OpClass::Read)?;
        let count = match &self.inode(ino)?.node {
            Node::File(file) => {
                let Ok(start) = usize::try_from(position) else {
                    return Ok(0);
                };
                let available = file.data.len().saturating_sub(start);
                let count = available.min(buf.len());
                if count > 0 {
                    buf[..count].copy_from_slice(&file.data[start..start + count]);
                }
                count
            }
            Node::Dir(_) => return Err(Errno::IsDir),
            Node::Symlink(_) => return Err(Errno::Inval),
        };
        self.mark_accessed(ino);
        Ok(count)
    }

    pub fn len(&self, ino: Ino) -> Result<u64> {
        match &self.inode(ino)?.node {
            Node::File(file) => Ok(len_u64(&file.data)),
            _ => Ok(0),
        }
    }

    /// Write `buf` at `offset` (or at the end when `None`), returning the number of bytes
    /// written and the position after the write.
    pub fn write_at(&mut self, ino: Ino, offset: Option<u64>, buf: &[u8]) -> Result<(usize, u64)> {
        if !matches!(self.inode(ino)?.node, Node::File(_)) {
            return Err(Errno::IsDir);
        }
        if buf.is_empty() {
            let len = self.len(ino)?;
            return Ok((0, offset.unwrap_or(len)));
        }
        let fault = self.fault(OpClass::Write)?;
        let mut count = buf.len();
        if fault == Some(Fault::ShortWrite) && count > 1 {
            let short = draw("switchy_fs::faults", (count - 1) as u64);
            count = 1 + to_usize(short)?;
        }
        let len = self.len(ino)?;
        let start = offset.unwrap_or(len);
        if let Some(quota) = self.quota {
            let available = quota.saturating_sub(self.used);
            let end = start.saturating_add(count as u64);
            if end.saturating_sub(len) > available {
                let limit = len.saturating_add(available);
                count = to_usize(limit.saturating_sub(start))?.min(count);
                if count == 0 {
                    return Err(Errno::NoSpc);
                }
            }
        }
        to_usize(start.saturating_add(count as u64))?;
        let record = PendingWrite::Write {
            offset: start,
            bytes: buf[..count].to_vec(),
        };
        self.account(ino, |inode| {
            if let Node::File(file) = &mut inode.node {
                apply_write(&mut file.data, &record);
                file.pending.push(record);
            }
        })?;
        self.mark_modified(ino);
        Ok((count, start + count as u64))
    }

    fn truncate_to(&mut self, ino: Ino, len: u64) -> Result<()> {
        let record = PendingWrite::SetLen(len);
        self.account(ino, |inode| {
            if let Node::File(file) = &mut inode.node {
                apply_write(&mut file.data, &record);
                file.pending.push(record);
            }
        })?;
        self.mark_modified(ino);
        Ok(())
    }

    pub fn set_len(&mut self, ino: Ino, len: u64) -> Result<()> {
        if !matches!(self.inode(ino)?.node, Node::File(_)) {
            return Err(Errno::Inval);
        }
        self.fault(OpClass::SetLen)?;
        to_usize(len)?;
        let current = self.len(ino)?;
        if let Some(quota) = self.quota
            && len.saturating_sub(current) > quota.saturating_sub(self.used)
        {
            return Err(Errno::NoSpc);
        }
        self.truncate_to(ino, len)
    }

    // ----- durability ------------------------------------------------------------------

    /// Make `ino` durable: file data (and permissions unless `data_only`), or, for a
    /// directory, every pending change to its entries.
    pub fn sync(&mut self, ino: Ino, data_only: bool) -> Result<()> {
        if let Err(error) = self.fault(OpClass::Sync) {
            if let Node::File(file) = &mut self.inode_mut(ino)?.node {
                // fsyncgate: the failed writeback is forgotten, never retried.
                file.pending.clear();
            }
            return Err(error);
        }
        let is_dir = self.inode(ino)?.is_dir();
        if is_dir {
            self.commit_dir(ino);
        }
        let inode = self.inode_mut(ino)?;
        if let Node::File(file) = &mut inode.node {
            // Replay rather than copy: writes forgotten by an earlier failed
            // synchronization must stay non-durable.
            for write in std::mem::take(&mut file.pending) {
                apply_write(&mut file.durable, &write);
            }
        }
        if !data_only || is_dir {
            inode.durable_meta = inode.meta;
        }
        Ok(())
    }

    /// Commit every pending journal change touching `dir`, plus earlier changes to the
    /// same entries, in journal order.
    fn commit_dir(&mut self, dir: Ino) {
        let mut slots: BTreeSet<(Ino, OsString)> = BTreeSet::new();
        let mut selected = vec![false; self.journal.len()];
        for (index, op) in self.journal.iter().enumerate().rev() {
            let relevant = op.changes.iter().any(|change| {
                change.dir == dir || slots.contains(&(change.dir, change.name.clone()))
            });
            if relevant {
                selected[index] = true;
                slots.extend(
                    op.changes
                        .iter()
                        .map(|change| (change.dir, change.name.clone())),
                );
            }
        }
        let journal = std::mem::take(&mut self.journal);
        let mut retained = Vec::new();
        for (op, selected) in journal.into_iter().zip(selected) {
            if selected {
                self.apply_durable(op);
            } else {
                retained.push(op);
            }
        }
        self.journal = retained;
    }

    fn apply_durable(&mut self, op: JournalOp) {
        let mut released = Vec::new();
        for change in op.changes {
            if let Some(new) = change.new
                && let Some(inode) = self.inodes.get_mut(&new)
            {
                inode.journal_refs = inode.journal_refs.saturating_sub(1);
                inode.durable_links += 1;
            }
            let previous = match self.dir_mut(change.dir) {
                Ok(dir) => match change.new {
                    Some(new) => dir.durable.insert(change.name, new),
                    None => dir.durable.remove(&change.name),
                },
                Err(_) => change.new,
            };
            if let Some(previous) = previous {
                if let Some(inode) = self.inodes.get_mut(&previous) {
                    inode.durable_links = inode.durable_links.saturating_sub(1);
                }
                released.push(previous);
            }
            if let Some(new) = change.new {
                released.push(new);
            }
        }
        for ino in released {
            self.maybe_gc(ino);
        }
    }

    /// Simulate a crash: rebuild the visible state from durable state per `policy`.
    /// Every handle becomes stale and every lock is released.
    pub fn crash(&mut self, policy: &CrashPolicy, draw: &mut dyn FnMut(u64) -> u64) {
        // Directory changes.
        let journal = std::mem::take(&mut self.journal);
        let mut lost: BTreeSet<(Ino, OsString)> = BTreeSet::new();
        for op in journal {
            let keep = match policy.directory_ops {
                DirectoryOpLoss::DropPending => false,
                DirectoryOpLoss::KeepPending => true,
                DirectoryOpLoss::OrderedSubset => {
                    !op.changes
                        .iter()
                        .any(|change| lost.contains(&(change.dir, change.name.clone())))
                        && draw(2) == 1
                }
            };
            if keep {
                for change in op.changes {
                    if let Ok(dir) = self.dir_mut(change.dir) {
                        match change.new {
                            Some(new) => dir.durable.insert(change.name, new),
                            None => dir.durable.remove(&change.name),
                        };
                    }
                }
            } else {
                lost.extend(
                    op.changes
                        .into_iter()
                        .map(|change| (change.dir, change.name)),
                );
            }
        }

        // File data and metadata.
        for inode in self.inodes.values_mut() {
            match &mut inode.node {
                Node::File(file) => {
                    let data = match policy.file_data {
                        FileDataLoss::DropPending => std::mem::take(&mut file.durable),
                        FileDataLoss::KeepPending => std::mem::take(&mut file.data),
                        FileDataLoss::TornPrefix { sector_size } => {
                            torn_prefix(file, sector_size.max(1), draw)
                        }
                    };
                    file.data.clone_from(&data);
                    file.durable = data;
                    file.pending.clear();
                }
                Node::Dir(dir) => dir.entries = dir.durable.clone(),
                Node::Symlink(_) => {}
            }
            let keep_meta = match policy.file_data {
                FileDataLoss::DropPending => false,
                FileDataLoss::KeepPending => true,
                FileDataLoss::TornPrefix { .. } => inode.meta != inode.durable_meta && draw(2) == 1,
            };
            if keep_meta {
                inode.durable_meta = inode.meta;
            } else {
                inode.meta = inode.durable_meta;
            }
            inode.nlink = 0;
            inode.open = 0;
            inode.durable_links = 0;
            inode.journal_refs = 0;
        }

        let reachable = self.relink_from_root();
        self.inodes.retain(|ino, _| reachable.contains(ino));
        if let Some(root) = self.inodes.get_mut(&ROOT) {
            root.nlink = 1;
            root.durable_links = 1;
        }
        self.used = self.inodes.values().map(Inode::live_len).sum();
        self.locks.clear();
        self.generation += 1;
    }

    /// Recompute link counts and parent pointers from the root after a crash, dropping
    /// entries that would give a directory a second parent. Returns the reachable inodes.
    fn relink_from_root(&mut self) -> BTreeSet<Ino> {
        let mut reachable = BTreeSet::from([ROOT]);
        let mut queue = VecDeque::from([ROOT]);
        while let Some(dir_ino) = queue.pop_front() {
            let entries = match self.inodes.get(&dir_ino).map(|inode| &inode.node) {
                Some(Node::Dir(dir)) => dir.entries.clone(),
                _ => continue,
            };
            let mut duplicates = Vec::new();
            for (name, child) in entries {
                let Some(inode) = self.inodes.get_mut(&child) else {
                    duplicates.push(name);
                    continue;
                };
                if let Node::Dir(dir) = &mut inode.node {
                    if reachable.contains(&child) {
                        // A directory has exactly one parent.
                        duplicates.push(name);
                        continue;
                    }
                    dir.parent = dir_ino;
                    queue.push_back(child);
                }
                inode.nlink += 1;
                inode.durable_links += 1;
                reachable.insert(child);
            }
            if !duplicates.is_empty()
                && let Ok(dir) = self.dir_mut(dir_ino)
            {
                for name in duplicates {
                    dir.entries.remove(&name);
                    dir.durable.remove(&name);
                }
            }
        }
        reachable
    }

    // ----- metadata --------------------------------------------------------------------

    pub fn metadata_of(&self, ino: Ino) -> Result<Metadata> {
        let inode = self.inode(ino)?;
        let (len, nlink) = match &inode.node {
            Node::File(file) => (len_u64(&file.data), inode.nlink),
            Node::Dir(dir) => (
                0,
                2 + dir
                    .entries
                    .values()
                    .filter(|child| self.is_dir(**child))
                    .count() as u64,
            ),
            Node::Symlink(target) => (len_u64(target.as_os_str().as_encoded_bytes()), inode.nlink),
        };
        Ok(Metadata {
            len,
            kind: inode.kind(),
            mode: inode.meta.mode,
            uid: Some(inode.meta.uid),
            gid: Some(inode.meta.gid),
            nlink: Some(nlink),
            file_id: Some(FileId {
                dev: DEVICE_ID,
                ino,
            }),
            accessed: Some(inode.times.accessed),
            modified: Some(inode.times.modified),
            created: Some(inode.times.created),
        })
    }

    pub fn metadata_handle(&mut self, ino: Ino) -> Result<Metadata> {
        self.fault(OpClass::Metadata)?;
        self.metadata_of(ino)
    }

    pub fn stat(&mut self, start: Ino, path: &Path, follow: bool) -> Result<Metadata> {
        self.fault(OpClass::Metadata)?;
        let ino = self.resolve(start, path, follow)?;
        self.metadata_of(ino)
    }

    /// Whether `path` resolves (following symbolic links), ignoring faults.
    pub fn exists(&self, start: Ino, path: &Path) -> bool {
        self.resolve(start, path, true).is_ok()
    }

    pub fn kind_of(&self, start: Ino, path: &Path, follow: bool) -> Option<FileKind> {
        self.resolve(start, path, follow)
            .ok()
            .and_then(|ino| self.inode(ino).ok().map(Inode::kind))
    }

    fn set_mode_ino(&mut self, ino: Ino, mode: u32) -> Result<()> {
        let uid = self.uid;
        let inode = self.inode_mut(ino)?;
        if uid != 0 && inode.meta.uid != uid {
            return Err(Errno::Perm);
        }
        inode.meta.mode = mode & 0o7777;
        Ok(())
    }

    pub fn set_mode_handle(&mut self, ino: Ino, mode: u32) -> Result<()> {
        self.fault(OpClass::Metadata)?;
        self.set_mode_ino(ino, mode)
    }

    pub fn set_mode(&mut self, start: Ino, path: &Path, mode: u32) -> Result<()> {
        self.fault(OpClass::Metadata)?;
        let ino = self.resolve(start, path, true)?;
        self.set_mode_ino(ino, mode)
    }

    // ----- directory operations --------------------------------------------------------

    pub fn list(&mut self, ino: Ino) -> Result<Vec<DirEntryInfo>> {
        self.fault(OpClass::DirOp)?;
        let entries = self.entry_infos(ino)?;
        self.mark_accessed(ino);
        Ok(entries)
    }

    /// The entries of directory `ino`, sorted by name.
    fn entry_infos(&self, ino: Ino) -> Result<Vec<DirEntryInfo>> {
        Ok(self
            .dir(ino)?
            .entries
            .iter()
            .map(|(name, child)| DirEntryInfo {
                name: name.clone(),
                kind: self.inode(*child).map_or(FileKind::Other, Inode::kind),
            })
            .collect())
    }

    pub fn read_dir(&mut self, start: Ino, path: &Path) -> Result<Vec<DirEntryInfo>> {
        self.fault(OpClass::DirOp)?;
        let ino = self.resolve(start, path, true)?;
        self.dir(ino)?;
        self.check_access(ino, READ)?;
        let entries = self.entry_infos(ino)?;
        self.mark_accessed(ino);
        Ok(entries)
    }

    pub fn create_dir(&mut self, start: Ino, path: &Path, mode: u32) -> Result<()> {
        self.fault(OpClass::DirOp)?;
        match self.resolve_entry(start, path, false)? {
            Entry::Dir(_) | Entry::Named { ino: Some(_), .. } => Err(Errno::Exist),
            Entry::Named {
                dir,
                name,
                ino: None,
                ..
            } => {
                self.ensure_linked(dir)?;
                self.check_access(dir, WRITE | SEARCH)?;
                let ino = self.alloc(
                    Node::Dir(DirNode {
                        entries: BTreeMap::new(),
                        durable: BTreeMap::new(),
                        parent: dir,
                    }),
                    mode & !self.umask,
                );
                self.link(dir, name, ino)
            }
        }
    }

    /// `create_dir_all` with the exact semantics of `std::fs::create_dir_all`.
    pub fn create_dir_all(&mut self, start: Ino, path: &Path, mode: u32) -> Result<()> {
        if path.as_os_str().is_empty() {
            return Ok(());
        }
        match self.create_dir(start, path, mode) {
            Ok(()) => return Ok(()),
            Err(Errno::NoEnt) => {}
            Err(_) if self.kind_of(start, path, true) == Some(FileKind::Dir) => return Ok(()),
            Err(error) => return Err(error),
        }
        match path.parent() {
            Some(parent) => self.create_dir_all(start, parent, mode)?,
            None => return Err(Errno::NoEnt),
        }
        match self.create_dir(start, path, mode) {
            Ok(()) => Ok(()),
            Err(_) if self.kind_of(start, path, true) == Some(FileKind::Dir) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn remove_file(&mut self, start: Ino, path: &Path) -> Result<()> {
        self.fault(OpClass::DirOp)?;
        match self.resolve_entry(start, path, false)? {
            Entry::Dir(_) => Err(unlink_dir_errno()),
            Entry::Named { ino: None, .. } => Err(Errno::NoEnt),
            Entry::Named {
                dir,
                name,
                ino: Some(ino),
                trailing,
            } => {
                self.check_access(dir, WRITE | SEARCH)?;
                if self.is_dir(ino) {
                    return Err(unlink_dir_errno());
                }
                if trailing {
                    return Err(Errno::NotDir);
                }
                self.ensure_not_open(ino)?;
                self.unlink(dir, &name)
            }
        }
    }

    pub fn remove_dir(&mut self, start: Ino, path: &Path) -> Result<()> {
        self.fault(OpClass::DirOp)?;
        match self.resolve_entry(start, path, false)? {
            Entry::Dir(ino) => Err(if ino == ROOT {
                Errno::Busy
            } else {
                Errno::Inval
            }),
            Entry::Named { ino: None, .. } => Err(Errno::NoEnt),
            Entry::Named {
                dir,
                name,
                ino: Some(ino),
                ..
            } => {
                if !self.is_dir(ino) {
                    return Err(Errno::NotDir);
                }
                self.check_access(dir, WRITE | SEARCH)?;
                if !self.dir(ino)?.entries.is_empty() {
                    return Err(Errno::NotEmpty);
                }
                self.unlink(dir, &name)
            }
        }
    }

    /// `remove_dir_all` with the semantics of `std::fs::remove_dir_all`: a symbolic link
    /// is removed itself, never followed.
    pub fn remove_dir_all(&mut self, start: Ino, path: &Path) -> Result<()> {
        match self.kind_of(start, path, false) {
            None => {
                self.resolve(start, path, false)?;
                Err(Errno::NoEnt)
            }
            Some(FileKind::Symlink) => self.remove_file(start, path),
            Some(FileKind::Dir) => {
                let ino = self.resolve(start, path, false)?;
                self.remove_children(ino)?;
                self.remove_dir(start, path)
            }
            Some(_) => Err(Errno::NotDir),
        }
    }

    fn remove_children(&mut self, dir: Ino) -> Result<()> {
        self.check_access(dir, READ | WRITE | SEARCH)?;
        let entries: Vec<(OsString, Ino)> = self
            .dir(dir)?
            .entries
            .iter()
            .map(|(name, ino)| (name.clone(), *ino))
            .collect();
        for (name, ino) in entries {
            self.fault(OpClass::DirOp)?;
            if self.is_dir(ino) {
                self.remove_children(ino)?;
            } else {
                self.ensure_not_open(ino)?;
            }
            self.unlink(dir, &name)?;
        }
        Ok(())
    }

    /// Rename `from` (relative to `from_start`) to `to` (relative to `to_start`).
    pub fn rename(
        &mut self,
        from_start: Ino,
        from: &Path,
        to_start: Ino,
        to: &Path,
        noreplace: bool,
    ) -> Result<()> {
        self.fault(OpClass::Rename)?;
        let Entry::Named {
            dir: from_dir,
            name: from_name,
            ino: source,
            trailing: from_trailing,
        } = self.resolve_entry(from_start, from, false)?
        else {
            return Err(Errno::Busy);
        };
        let source = source.ok_or(Errno::NoEnt)?;
        let Entry::Named {
            dir: to_dir,
            name: mut to_name,
            ino: mut target,
            trailing: to_trailing,
        } = self.resolve_entry(to_start, to, false)?
        else {
            return Err(if noreplace { Errno::Exist } else { Errno::Busy });
        };
        if noreplace && target.is_some() {
            return Err(Errno::Exist);
        }
        let source_is_dir = self.is_dir(source);
        if (from_trailing || to_trailing) && !source_is_dir {
            return Err(Errno::NotDir);
        }
        if target == Some(source) {
            // Under the Windows personality, renaming an entry to another spelling of its
            // own name changes the stored case; otherwise renaming onto the same inode
            // does nothing.
            let respelled = (from_dir == to_dir && to_name == from_name)
                .then(|| self.respelling(to, &from_name))
                .flatten();
            let Some(requested) = respelled else {
                return Ok(());
            };
            target = None;
            to_name = requested;
        }
        self.check_access(from_dir, WRITE | SEARCH)?;
        self.check_access(to_dir, WRITE | SEARCH)?;
        if let Some(target) = target {
            match (source_is_dir, self.is_dir(target)) {
                (true, false) => return Err(Errno::NotDir),
                (false, true) => return Err(Errno::IsDir),
                (true, true) if !self.dir(target)?.entries.is_empty() => {
                    return Err(Errno::NotEmpty);
                }
                _ => {}
            }
            self.ensure_not_open(target)?;
        }
        if source_is_dir {
            let mut ancestor = to_dir;
            loop {
                if ancestor == source {
                    return Err(Errno::Inval);
                }
                if ancestor == ROOT {
                    break;
                }
                ancestor = self.dir(ancestor)?.parent;
            }
        }
        self.ensure_linked(to_dir)?;

        let mut changes = vec![Change {
            dir: from_dir,
            name: from_name.clone(),
            new: None,
        }];
        if let Some(target) = target {
            self.dir_mut(to_dir)?.entries.remove(&to_name);
            self.account(target, |inode| inode.nlink = inode.nlink.saturating_sub(1))?;
            // Under the Windows personality the replacing entry takes the requested
            // spelling of the name.
            if let Some(requested) = self.respelling(to, &to_name) {
                changes.push(Change {
                    dir: to_dir,
                    name: std::mem::replace(&mut to_name, requested),
                    new: None,
                });
            }
        }
        changes.push(Change {
            dir: to_dir,
            name: to_name.clone(),
            new: Some(source),
        });
        self.dir_mut(from_dir)?.entries.remove(&from_name);
        self.dir_mut(to_dir)?.entries.insert(to_name, source);
        self.mark_modified(from_dir);
        self.mark_modified(to_dir);
        if let Node::Dir(dir) = &mut self.inode_mut(source)?.node {
            dir.parent = to_dir;
        }
        self.inode_mut(source)?.journal_refs += 1;
        self.journal.push(JournalOp { changes });
        if let Some(target) = target {
            self.maybe_gc(target);
        }
        Ok(())
    }

    pub fn hard_link(
        &mut self,
        from_start: Ino,
        from: &Path,
        to_start: Ino,
        to: &Path,
    ) -> Result<()> {
        self.fault(OpClass::DirOp)?;
        let source = match self.resolve_entry(from_start, from, false)? {
            Entry::Dir(_) => return Err(Errno::Perm),
            Entry::Named { ino: None, .. } => return Err(Errno::NoEnt),
            Entry::Named { ino: Some(ino), .. } => ino,
        };
        match self.resolve_entry(to_start, to, false)? {
            Entry::Dir(_) | Entry::Named { ino: Some(_), .. } => Err(Errno::Exist),
            Entry::Named {
                dir,
                name,
                ino: None,
                ..
            } => {
                if self.is_dir(source) {
                    return Err(Errno::Perm);
                }
                self.ensure_linked(dir)?;
                self.check_access(dir, WRITE | SEARCH)?;
                self.link(dir, name, source)
            }
        }
    }

    pub fn symlink(&mut self, target: &Path, start: Ino, link: &Path) -> Result<()> {
        self.fault(OpClass::DirOp)?;
        if self.is_windows() {
            // Creating symbolic links needs `SeCreateSymbolicLinkPrivilege`, which an
            // ordinary (non-elevated, non-developer-mode) process lacks.
            return Err(Errno::PrivilegeNotHeld);
        }
        if target.as_os_str().is_empty() {
            return Err(Errno::NoEnt);
        }
        match self.resolve_entry(start, link, false)? {
            Entry::Dir(_) | Entry::Named { ino: Some(_), .. } => Err(Errno::Exist),
            Entry::Named {
                dir,
                name,
                ino: None,
                ..
            } => {
                self.ensure_linked(dir)?;
                self.check_access(dir, WRITE | SEARCH)?;
                let mode = if cfg!(any(target_os = "linux", target_os = "android")) {
                    0o777
                } else {
                    0o777 & !self.umask
                };
                let ino = self.alloc(Node::Symlink(target.to_path_buf()), mode);
                self.link(dir, name, ino)
            }
        }
    }

    pub fn read_link(&mut self, start: Ino, path: &Path) -> Result<PathBuf> {
        self.fault(OpClass::Read)?;
        let ino = self.resolve(start, path, false)?;
        let target = match &self.inode(ino)?.node {
            Node::Symlink(target) => target.clone(),
            _ => return Err(Errno::Inval),
        };
        self.mark_accessed(ino);
        Ok(target)
    }

    /// Apply the open fault class (for anonymous files that resolve no path).
    pub fn check_open_fault(&mut self) -> Result<()> {
        self.fault(OpClass::Open).map(|_| ())
    }

    /// Check that `start` may be searched and written (for anonymous files in a directory).
    pub fn check_temp_dir(&mut self, start: Ino, path: &Path) -> Result<()> {
        self.fault(OpClass::Open)?;
        let ino = self.resolve(start, path, true)?;
        self.dir(ino)?;
        self.ensure_linked(ino)?;
        self.check_access(ino, WRITE | SEARCH)
    }

    // ----- locks -----------------------------------------------------------------------

    /// Try to take a flock-style lock for handle `id`. Converting between shared and
    /// exclusive is allowed when no other handle conflicts.
    pub fn try_lock(&mut self, ino: Ino, id: u64, exclusive: bool) -> Result<()> {
        let state = self.locks.entry(ino).or_default();
        let other_exclusive = state.exclusive.is_some_and(|holder| holder != id);
        if exclusive {
            let other_shared = state.shared.iter().any(|holder| *holder != id);
            if other_exclusive || other_shared {
                return Err(Errno::WouldBlock);
            }
            state.shared.remove(&id);
            state.exclusive = Some(id);
        } else {
            if other_exclusive {
                return Err(Errno::WouldBlock);
            }
            if state.exclusive == Some(id) {
                state.exclusive = None;
            }
            state.shared.insert(id);
        }
        Ok(())
    }

    pub fn unlock(&mut self, ino: Ino, id: u64) {
        if let Some(state) = self.locks.get_mut(&ino) {
            if state.exclusive == Some(id) {
                state.exclusive = None;
            }
            state.shared.remove(&id);
            if state.exclusive.is_none() && state.shared.is_empty() {
                self.locks.remove(&ino);
            }
        }
    }
}

/// `unlink` of a directory: `EISDIR` on Linux, `EPERM` elsewhere (POSIX).
const fn unlink_dir_errno() -> Errno {
    if cfg!(any(target_os = "linux", target_os = "android")) {
        Errno::IsDir
    } else {
        Errno::Perm
    }
}

/// Whether `a` and `b` are equal ignoring case, as Windows compares names: character by
/// character with simple (one-to-one) uppercase mapping. Names that are not valid
/// Unicode compare ignoring ASCII case.
fn eq_ignoring_case(a: &OsStr, b: &OsStr) -> bool {
    fn upper(c: char) -> char {
        let mut upper = c.to_uppercase();
        match (upper.next(), upper.next()) {
            (Some(single), None) => single,
            _ => c,
        }
    }
    match (a.to_str(), b.to_str()) {
        (Some(a), Some(b)) => a.chars().map(upper).eq(b.chars().map(upper)),
        _ => a
            .as_encoded_bytes()
            .eq_ignore_ascii_case(b.as_encoded_bytes()),
    }
}

/// Draw a uniform value below `bound` from the current simulation run's named stream.
fn draw(stream: &str, bound: u64) -> u64 {
    if bound <= 1 {
        return 0;
    }
    switchy_simulation::stream(stream)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .below(bound)
}

/// Rebuild a file from its durable data plus a random prefix of its pending writes, with
/// the first incomplete write torn at `sector_size` granularity.
fn torn_prefix(file: &FileNode, sector_size: u64, draw: &mut dyn FnMut(u64) -> u64) -> Vec<u8> {
    let mut data = file.durable.clone();
    if file.pending.is_empty() {
        return data;
    }
    let total = file.pending.len();
    let kept = usize::try_from(draw(total as u64 + 1))
        .unwrap_or(total)
        .min(total);
    for write in &file.pending[..kept] {
        apply_write(&mut data, write);
    }
    if let Some(PendingWrite::Write { offset, bytes }) = file.pending.get(kept) {
        let end = offset.saturating_add(bytes.len() as u64);
        let mut chunk_start = *offset;
        while chunk_start < end {
            let chunk_end = ((chunk_start / sector_size) + 1)
                .saturating_mul(sector_size)
                .min(end);
            if draw(2) == 1 {
                let (Ok(from), Ok(to)) = (
                    usize::try_from(chunk_start - offset),
                    usize::try_from(chunk_end - offset),
                ) else {
                    break;
                };
                apply_write(
                    &mut data,
                    &PendingWrite::Write {
                        offset: chunk_start,
                        bytes: bytes[from..to].to_vec(),
                    },
                );
            }
            chunk_start = chunk_end;
        }
    }
    data
}

/// An open handle on a simulated inode.
///
/// Handles keep their inode alive after it is unlinked, are invalidated by a crash or
/// reset of their filesystem, and release their locks when dropped.
pub struct Handle {
    fs: Arc<Filesystem>,
    ino: Ino,
    generation: u64,
    id: u64,
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub position: u64,
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("ino", &self.ino)
            .field("generation", &self.generation)
            .field("id", &self.id)
            .field("read", &self.read)
            .field("write", &self.write)
            .field("append", &self.append)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

impl Handle {
    /// Open `path` relative to `start` (or the root) in `fs`.
    pub fn open(
        fs: &Arc<Filesystem>,
        start: Option<&Self>,
        path: &Path,
        spec: &OpenSpec,
    ) -> std::io::Result<Self> {
        spec.validate()?;
        super::before_operation(OpClass::Open);
        let mut vfs = fs.vfs();
        let start = match start {
            Some(handle) => handle.start_in(&vfs)?,
            None => ROOT,
        };
        let ino = vfs.open(start, path, spec)?;
        let id = vfs.open_handle(ino)?;
        Ok(Self {
            fs: fs.clone(),
            ino,
            generation: vfs.generation,
            id,
            read: spec.read,
            write: spec.writable(),
            append: spec.append,
            position: 0,
        })
    }

    /// Open an already-resolved inode (anonymous files, directories).
    pub fn from_ino(
        fs: &Arc<Filesystem>,
        vfs: &mut Vfs,
        ino: Ino,
        read: bool,
        write: bool,
    ) -> std::io::Result<Self> {
        let id = vfs.open_handle(ino)?;
        Ok(Self {
            fs: fs.clone(),
            ino,
            generation: vfs.generation,
            id,
            read,
            write,
            append: false,
            position: 0,
        })
    }

    /// The inode to resolve relative paths from, checking the handle is still valid.
    pub fn start_in(&self, vfs: &Vfs) -> std::io::Result<Ino> {
        if vfs.generation == self.generation {
            Ok(self.ino)
        } else {
            Err(Errno::Stale.into())
        }
    }

    pub const fn filesystem(&self) -> &Arc<Filesystem> {
        &self.fs
    }

    /// Run `work`, an operation of `class`, against this handle's inode.
    ///
    /// # Errors
    /// * `ESTALE` after a crash or reset of the filesystem.
    pub fn with<T>(
        &self,
        class: OpClass,
        work: impl FnOnce(&mut Vfs, Ino) -> Result<T>,
    ) -> std::io::Result<T> {
        super::before_operation(class);
        let mut vfs = self.fs.vfs();
        if vfs.generation != self.generation {
            return Err(Errno::Stale.into());
        }
        let result = work(&mut vfs, self.ino);
        drop(vfs);
        result.map_err(Into::into)
    }

    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.read {
            return Err(Errno::BadF.into());
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let position = self.position;
        let count = self.with(OpClass::Read, |vfs, ino| vfs.read_at(ino, position, buf))?;
        self.position += count as u64;
        Ok(count)
    }

    pub fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.write {
            return Err(Errno::BadF.into());
        }
        let offset = (!self.append).then_some(self.position);
        let (count, position) = self.with(OpClass::Write, |vfs, ino| vfs.write_at(ino, offset, buf))?;
        self.position = position;
        Ok(count)
    }

    pub fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            std::io::SeekFrom::Start(offset) => Some(offset),
            std::io::SeekFrom::End(offset) => {
                let len = self.with(OpClass::Metadata, |vfs, ino| vfs.len(ino))?;
                len.checked_add_signed(offset)
            }
            std::io::SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
        }
        .ok_or_else(|| std::io::Error::from(Errno::Inval))?;
        self.position = next;
        Ok(next)
    }

    pub fn set_len(&self, len: u64) -> std::io::Result<()> {
        if !self.write {
            return Err(Errno::Inval.into());
        }
        self.with(OpClass::SetLen, |vfs, ino| vfs.set_len(ino, len))
    }

    pub fn sync(&self, data_only: bool) -> std::io::Result<()> {
        self.with(OpClass::Sync, |vfs, ino| vfs.sync(ino, data_only))
    }

    pub fn metadata(&self) -> std::io::Result<Metadata> {
        self.with(OpClass::Metadata, Vfs::metadata_handle)
    }

    pub fn set_mode(&self, mode: u32) -> std::io::Result<()> {
        self.with(OpClass::Metadata, |vfs, ino| vfs.set_mode_handle(ino, mode))
    }

    pub fn try_lock(&self, exclusive: bool) -> std::io::Result<()> {
        let id = self.id;
        self.with(OpClass::Metadata, |vfs, ino| vfs.try_lock(ino, id, exclusive))
    }

    pub fn unlock(&self) -> std::io::Result<()> {
        let id = self.id;
        self.with(OpClass::Metadata, |vfs, ino| {
            vfs.unlock(ino, id);
            Ok(())
        })
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let mut vfs = self.fs.vfs();
        if vfs.generation == self.generation {
            vfs.close_handle(self.ino, self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> OpenSpec {
        OpenSpec {
            read: false,
            write: true,
            append: false,
            truncate: false,
            create: true,
            create_new: false,
            nofollow: false,
            directory: false,
            mode: 0o666,
        }
    }

    #[test]
    fn symlink_loops_are_detected() {
        let mut vfs = Vfs::new(0, 0);
        vfs.symlink(Path::new("b"), ROOT, Path::new("/a")).unwrap();
        vfs.symlink(Path::new("a"), ROOT, Path::new("/b")).unwrap();
        assert_eq!(vfs.resolve(ROOT, Path::new("/a"), true), Err(Errno::Loop));
        assert!(vfs.resolve(ROOT, Path::new("/a"), false).is_ok());
    }

    #[test]
    fn dangling_symlink_creation_creates_target() {
        let mut vfs = Vfs::new(0, 0);
        vfs.create_dir(ROOT, Path::new("/d"), 0o755).unwrap();
        vfs.symlink(Path::new("d/target"), ROOT, Path::new("/link"))
            .unwrap();
        let ino = vfs.open(ROOT, Path::new("/link"), &spec()).unwrap();
        assert_eq!(vfs.resolve(ROOT, Path::new("/d/target"), false), Ok(ino));
    }

    #[test]
    fn commit_includes_earlier_changes_to_same_entry() {
        let mut vfs = Vfs::new(0, 0);
        vfs.create_dir(ROOT, Path::new("/a"), 0o755).unwrap();
        vfs.create_dir(ROOT, Path::new("/b"), 0o755).unwrap();
        vfs.sync(ROOT, false).unwrap();
        vfs.open(ROOT, Path::new("/a/f"), &spec()).unwrap();
        vfs.rename(ROOT, Path::new("/a/f"), ROOT, Path::new("/b/f"), false)
            .unwrap();
        // Synchronizing only `b` commits the rename and the creation it depends on.
        let a = vfs.resolve(ROOT, Path::new("/a"), false).unwrap();
        let b = vfs.resolve(ROOT, Path::new("/b"), false).unwrap();
        vfs.sync(b, false).unwrap();
        assert!(vfs.journal.is_empty());
        assert!(vfs.dir(a).unwrap().durable.is_empty());
        assert_eq!(vfs.dir(b).unwrap().durable.len(), 1);
    }

    #[test]
    fn torn_prefix_keeps_whole_sectors() {
        let file = FileNode {
            data: vec![1; 8],
            durable: vec![0; 8],
            pending: vec![PendingWrite::Write {
                offset: 0,
                bytes: vec![1; 8],
            }],
        };
        // Keep zero complete writes, then keep only the second 4-byte sector.
        let mut draws = vec![0, 0, 1].into_iter();
        let data = torn_prefix(&file, 4, &mut |_| draws.next().unwrap_or(0));
        assert_eq!(data, vec![0, 0, 0, 0, 1, 1, 1, 1]);
    }
}
