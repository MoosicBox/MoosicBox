//! Crash policies and fault plans for the filesystem simulator.
//!
//! Every random decision draws from a named stream of the current
//! [`switchy_simulation`] run (`switchy_fs::crash` and `switchy_fs::faults`), so a
//! simulation seed reproduces the exact same crash outcomes and injected faults.

/// What happens to file data written since the file's last successful `sync_data`/`sync_all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDataLoss {
    /// Every unsynchronized write is lost.
    DropPending,
    /// Every unsynchronized write survives (a process crash: the page cache is kept).
    KeepPending,
    /// A random prefix of each file's unsynchronized writes survives. The first write that
    /// does not fully survive is torn: each `sector_size`-aligned chunk of it independently
    /// survives or is lost. Metadata changes (permissions) independently survive or not.
    TornPrefix {
        /// Atomic write unit in bytes. Values below one are treated as one.
        sector_size: u64,
    },
}

/// What happens to directory changes (create, link, unlink, rename) not yet made durable
/// by synchronizing their directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryOpLoss {
    /// Every unsynchronized directory change is lost.
    DropPending,
    /// Every unsynchronized directory change survives.
    KeepPending,
    /// A random subset of unsynchronized changes survives, applied in their original order.
    /// A change never survives when an earlier change to the same directory entry was lost,
    /// and a rename always survives or is lost as a whole.
    OrderedSubset,
}

/// How a simulated crash treats state that was not made durable.
///
/// The default is [`CrashPolicy::power_loss`]: the weakest guarantees POSIX allows,
/// explored randomly per seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrashPolicy {
    /// Treatment of unsynchronized file data and permissions.
    pub file_data: FileDataLoss,
    /// Treatment of unsynchronized directory changes.
    pub directory_ops: DirectoryOpLoss,
}

impl CrashPolicy {
    /// Power loss: torn prefixes of unsynchronized writes (512-byte sectors) and an ordered
    /// random subset of unsynchronized directory changes survive.
    #[must_use]
    pub const fn power_loss() -> Self {
        Self {
            file_data: FileDataLoss::TornPrefix { sector_size: 512 },
            directory_ops: DirectoryOpLoss::OrderedSubset,
        }
    }

    /// Lose everything that was not made durable. Deterministic, and the canonical worst
    /// case for recovery code that relies only on explicit synchronization.
    #[must_use]
    pub const fn drop_all_pending() -> Self {
        Self {
            file_data: FileDataLoss::DropPending,
            directory_ops: DirectoryOpLoss::DropPending,
        }
    }

    /// Process crash (for example `SIGKILL`): every completed write and directory change
    /// survives, but handles are invalidated and locks released.
    #[must_use]
    pub const fn process_crash() -> Self {
        Self {
            file_data: FileDataLoss::KeepPending,
            directory_ops: DirectoryOpLoss::KeepPending,
        }
    }
}

impl Default for CrashPolicy {
    fn default() -> Self {
        Self::power_loss()
    }
}

/// A class of filesystem operations that faults can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpClass {
    /// Opening files, directories and anonymous temporary files.
    Open,
    /// Reading file data and symbolic link targets.
    Read,
    /// Writing file data.
    Write,
    /// `sync_all`, `sync_data` and directory synchronization.
    Sync,
    /// Renames, including no-replace renames.
    Rename,
    /// Changing a file's length.
    SetLen,
    /// Reading metadata and changing permissions.
    Metadata,
    /// Creating and removing directory entries (directories, links, symlinks, unlink) and
    /// listing directories.
    DirOp,
}

/// A fault injected into an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fault {
    /// Fail with `EIO`. A failed file synchronization also discards the file's pending
    /// data: it is never made durable, although reads keep returning it until a crash
    /// (Linux "fsyncgate" semantics).
    Io,
    /// Fail with `ENOSPC`.
    NoSpace,
    /// Write only a random non-empty prefix of the buffer. Ignored by other operations and
    /// by single-byte writes.
    ShortWrite,
}

/// The probability that a fault rule fires for a matching operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probability {
    numerator: u64,
    denominator: u64,
}

impl Probability {
    /// Fire for every matching operation.
    pub const ALWAYS: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    /// Fire with probability `numerator / denominator` (clamped to `[0, 1]`).
    ///
    /// A zero denominator never fires.
    #[must_use]
    pub const fn ratio(numerator: u64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Fire with probability `1 / n`. `n == 0` never fires.
    #[must_use]
    pub const fn one_in(n: u64) -> Self {
        Self::ratio(1, n)
    }

    /// Fire with probability `percent / 100`.
    #[must_use]
    pub const fn percent(percent: u64) -> Self {
        Self::ratio(percent, 100)
    }

    /// Whether the rule fires, drawing from `draw` only for probabilities in `(0, 1)`.
    pub(crate) fn fires(self, draw: impl FnOnce(u64) -> u64) -> bool {
        if self.denominator == 0 || self.numerator == 0 {
            false
        } else if self.numerator >= self.denominator {
            true
        } else {
            draw(self.denominator) < self.numerator
        }
    }
}

/// One rule of a [`FaultPlan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaultRule {
    pub(crate) class: OpClass,
    pub(crate) fault: Fault,
    pub(crate) probability: Probability,
}

/// A seeded plan of faults injected into one [`super::Filesystem`] namespace.
///
/// Rules are evaluated in insertion order; the first rule that fires for an operation
/// injects its fault. Faults are injected before the operation has any effect, except
/// [`Fault::ShortWrite`] and the fsyncgate behavior of a failed synchronization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FaultPlan {
    pub(crate) rules: Vec<FaultRule>,
}

impl FaultPlan {
    /// An empty plan: no faults.
    #[must_use]
    pub const fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule injecting `fault` into operations of `class` with `probability`.
    #[must_use]
    pub fn with(mut self, class: OpClass, fault: Fault, probability: Probability) -> Self {
        self.rules.push(FaultRule {
            class,
            fault,
            probability,
        });
        self
    }

    /// Whether the plan has no rules.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
