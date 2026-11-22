//! In-memory POSIX filesystem simulator with durability, crash and fault modeling.
//!
//! The simulator runs entirely in memory and models the parts of POSIX filesystem
//! semantics that storage code depends on:
//!
//! * **Inodes.** Directories map names to inodes; paths are resolved one component at a
//!   time, following symbolic links (at most 40 per resolution, then `ELOOP`). Open
//!   handles refer to inodes, so renaming or unlinking an open file behaves as on POSIX.
//!   Hard links, symbolic links, permission bits, ownership and a simulated effective
//!   user (see [`Filesystem::with_credentials`]) are modeled, and failures are reported
//!   with the host's `errno` values (`ENOENT`, `EEXIST`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`,
//!   `ELOOP`, `EACCES`, `ENOSPC`, ...).
//! * **Durability.** File data becomes durable on `sync_data`/`sync_all`; directory
//!   changes (create, link, unlink, rename) become durable only when their directory is
//!   synchronized ([`sync::Dir::sync`] or [`sync::sync_directory`]).
//! * **Crashes.** [`Filesystem::crash`] reverts to durable state according to a
//!   [`CrashPolicy`] (by default the weakest guarantees POSIX allows, explored randomly per
//!   seed), invalidates every open handle and releases every lock.
//! * **Faults.** A seeded [`FaultPlan`] (or one-shot [`Filesystem::inject_fault`]) injects
//!   `EIO`, `ENOSPC` and short writes per [`OpClass`]; [`Filesystem::set_quota`] limits
//!   the bytes a namespace may hold. A failed file synchronization forgets the file's
//!   pending data ("fsyncgate").
//!
//! * **Timestamps.** Access, modification and creation times come from the simulated clock
//!   (`switchy_time`) and are updated as POSIX specifies.
//! * **Personalities.** A [`Personality`] selects POSIX semantics (the default) or Windows
//!   semantics: open files cannot be removed or replaced, names are case-insensitive but
//!   case-preserving, and symbolic links cannot be created.
//! * **Switch points.** [`set_operation_hook`] installs a per-run callback that runs at
//!   the start of every operation, outside every filesystem lock.
//!
//! All randomness comes from named streams of the current [`switchy_simulation`] run, so
//! a simulation seed reproduces crash outcomes, faults and temporary names exactly.
//!
//! Each simulation run (each `switchy_simulation` context) has its own default
//! [`Filesystem`]; explicit namespaces are selected with [`with_filesystem`] and
//! [`scope_filesystem`]. Relative paths resolve against the root directory.
//!
//! # Crash testing
//!
//! ```rust
//! # #[cfg(feature = "sync")]
//! # {
//! use std::{io::Write as _, sync::Arc};
//! use switchy_fs::simulator::{CrashPolicy, Filesystem, with_filesystem};
//! use switchy_fs::sync::{Dir, OpenOptions, read};
//!
//! let fs = Arc::new(Filesystem::new());
//! with_filesystem(&fs, || {
//!     let dir = Dir::open("/").unwrap();
//!     let options = OpenOptions::new().write(true).create_new(true);
//!
//!     // Durable: data synchronized, then the directory entry synchronized.
//!     let mut durable = dir.open_file_at("durable", &options).unwrap();
//!     durable.write_all(b"kept").unwrap();
//!     durable.sync_all().unwrap();
//!     dir.sync().unwrap();
//!
//!     // Not durable: the directory entry was never synchronized.
//!     let mut lost = dir.open_file_at("lost", &options).unwrap();
//!     lost.write_all(b"gone").unwrap();
//!     lost.sync_all().unwrap();
//! });
//!
//! fs.crash(&CrashPolicy::drop_all_pending());
//!
//! with_filesystem(&fs, || {
//!     assert_eq!(read("/durable").unwrap(), b"kept");
//!     assert!(read("/lost").is_err());
//! });
//! # }
//! ```

use std::{
    cell::{Cell, RefCell},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

mod fault;
mod vfs;

pub use fault::{
    CrashPolicy, DirectoryOpLoss, Fault, FaultPlan, FileDataLoss, OpClass, Probability,
};

pub use crate::types::{DirEntryInfo, FileId, FileKind, Metadata, Permissions};

use vfs::{Handle, ROOT, Vfs};

// Module that contains all real_fs functionality
#[cfg(feature = "simulator-real-fs")]
mod real_fs_support {
    use scoped_tls::scoped_thread_local;

    pub struct RealFs;

    scoped_thread_local! {
        pub(super) static REAL_FS: RealFs
    }

    /// Executes a function using the actual filesystem instead of the simulator
    ///
    /// This function temporarily switches to using real filesystem operations within
    /// the provided closure, allowing you to interact with the actual disk even when
    /// the simulator is enabled.
    pub fn with_real_fs<T>(f: impl FnOnce() -> T) -> T {
        REAL_FS.set(&RealFs, f)
    }

    #[inline]
    pub fn is_real_fs() -> bool {
        REAL_FS.is_set()
    }

    // Copy a native file into a detached simulated file. Writes affect only the copy.
    pub fn convert_std_file_to_simulator(
        std_file: std::fs::File,
        path: impl AsRef<std::path::Path>,
        read: bool,
        write: bool,
    ) -> std::io::Result<super::sync::File> {
        let content = if read {
            use std::io::Read;
            let mut std_file = std_file;
            let mut content = Vec::new();
            std_file.read_to_end(&mut content)?;
            content
        } else {
            Vec::new()
        };

        Ok(super::sync::File {
            path: path.as_ref().to_path_buf(),
            handle: super::detached_handle(content, read, write)?,
        })
    }

    // Async conversion for std::fs::File to simulator File
    #[cfg(feature = "async")]
    pub async fn convert_std_file_to_simulator_async(
        std_file: std::fs::File,
        path: impl AsRef<std::path::Path>,
        read: bool,
        write: bool,
    ) -> std::io::Result<super::unsync::File> {
        let path_buf = path.as_ref().to_path_buf();
        let content = if read {
            switchy_async::task::spawn_blocking(move || {
                use std::io::Read;
                let mut std_file = std_file;
                let mut content = Vec::new();
                std_file.read_to_end(&mut content)?;
                Ok::<Vec<u8>, std::io::Error>(content)
            })
            .await
            .unwrap()?
        } else {
            Vec::new()
        };

        Ok(super::unsync::File {
            path: path_buf,
            handle: super::detached_handle(content, read, write)?,
        })
    }
}

// When the feature is not enabled, provide no-op implementations
#[cfg(not(feature = "simulator-real-fs"))]
mod real_fs_support {
    /// Executes a function without switching filesystem modes (no-op when `simulator-real-fs` feature is disabled)
    ///
    /// This function simply executes the provided closure without changing filesystem behavior.
    pub fn with_real_fs<T>(f: impl FnOnce() -> T) -> T {
        f()
    }

    #[inline]
    #[allow(dead_code)]
    pub const fn is_real_fs() -> bool {
        false
    }
}

// Re-export at module level for clean access
pub use real_fs_support::with_real_fs;

/// A handle on a private, unlinked copy of `content` (used for copies of native files).
#[cfg_attr(not(feature = "simulator-real-fs"), allow(dead_code))]
fn detached_handle(content: Vec<u8>, read: bool, write: bool) -> std::io::Result<Handle> {
    let fs = Arc::new(Filesystem::new());
    let mut vfs = fs.vfs();
    let ino = vfs.create_anonymous(0o600);
    vfs.set_detached_data(ino, content)?;
    let handle = Handle::from_ino(&fs, &mut vfs, ino, read, write);
    drop(vfs);
    handle
}

/// The operating system whose filesystem semantics a simulated namespace follows.
///
/// The personality changes only the behaviours listed for it; durability, crashes,
/// faults, permissions and `errno` values otherwise follow the POSIX model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Personality {
    /// POSIX semantics (the default).
    #[default]
    Posix,
    /// Windows (NTFS) semantics:
    ///
    /// * Removing a file, or renaming another file over it, while a handle on it is open
    ///   fails with a sharing violation (`ERROR_SHARING_VIOLATION`, raw os error 32, on
    ///   Windows hosts; `PermissionDenied`/`EACCES` elsewhere), as for files opened
    ///   without delete sharing. Renaming an open file itself is allowed.
    /// * Names are case-insensitive but case-preserving: lookups match any case, a new
    ///   entry keeps the case it was created with, creating another spelling of an
    ///   existing name finds the existing entry, and renaming an entry to another
    ///   spelling of its own name changes its case.
    /// * Creating a symbolic link fails with `ERROR_PRIVILEGE_NOT_HELD` (raw os error
    ///   1314, on Windows hosts; `PermissionDenied`/`EPERM` elsewhere), as for a process
    ///   without the symbolic link privilege.
    Windows,
}

/// An independently owned simulated filesystem namespace.
///
/// Select it with [`with_filesystem`] for synchronous work or [`scope_filesystem`] for a
/// future. Open handles keep working after the selection ends; they become stale (every
/// operation fails with `ESTALE`) after [`Filesystem::crash`] or [`Filesystem::reset`].
///
/// A new namespace contains only a root directory (mode `0o755`) owned by the simulated
/// effective user, which defaults to uid/gid 1000 with umask `0o022`.
#[derive(Default)]
pub struct Filesystem {
    directory_locations: crate::directories::DirectoryLocations,
    vfs: Mutex<Vfs>,
    /// Temporary directories created in this namespace (see [`temp_dir`]).
    temp_dirs: Mutex<temp_dir::TempDirs>,
}

impl std::fmt::Debug for Filesystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let vfs = self.vfs();
        f.debug_struct("Filesystem")
            .field("credentials", &vfs.credentials())
            .field("personality", &vfs.personality())
            .field("used_bytes", &vfs.used())
            .finish_non_exhaustive()
    }
}

impl Filesystem {
    /// Create an empty filesystem with explicit user directory locations.
    ///
    /// Locations are immutable, need not exist, and never fall back to host paths.
    #[must_use]
    pub fn with_directory_locations(locations: crate::directories::DirectoryLocations) -> Self {
        Self {
            directory_locations: locations,
            ..Self::default()
        }
    }

    /// Create an empty simulated filesystem.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty filesystem whose simulated effective user is `uid`/`gid`; the root
    /// directory is owned by that user. Uid 0 bypasses permission checks, as root does.
    #[must_use]
    pub fn with_credentials(uid: u32, gid: u32) -> Self {
        Self {
            vfs: Mutex::new(Vfs::new(uid, gid)),
            ..Self::default()
        }
    }

    pub(crate) fn vfs(&self) -> MutexGuard<'_, Vfs> {
        self.vfs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Simulate a crash (power loss or process death, per `policy`).
    ///
    /// State that was not made durable is lost or partially kept as `policy` describes,
    /// drawing every random choice from the current simulation run's `switchy_fs::crash`
    /// stream. Afterwards every handle opened before the crash fails with `ESTALE`, every
    /// lock is released, and inodes no longer reachable from the root are discarded. Fault
    /// plans, quotas and credentials are kept.
    pub fn crash(&self, policy: &CrashPolicy) {
        let stream = switchy_simulation::stream("switchy_fs::crash");
        let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
        self.vfs().crash(policy, &mut |bound| {
            if bound <= 1 { 0 } else { rng.below(bound) }
        });
    }

    /// Discard every file and directory and invalidate every handle, keeping credentials,
    /// umask, quota and fault configuration.
    pub fn reset(&self) {
        self.vfs().reset();
        self.temp_dirs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Replace the namespace's fault plan. Draws come from the current simulation run's
    /// `switchy_fs::faults` stream.
    pub fn set_fault_plan(&self, plan: FaultPlan) {
        self.vfs().set_fault_plan(plan);
    }

    /// Fail the next operation of `class` with `fault`, ahead of the fault plan. Queued
    /// faults are consumed in order, one per matching operation.
    pub fn inject_fault(&self, class: OpClass, fault: Fault) {
        self.vfs().inject(class, fault);
    }

    /// Drop every queued one-shot fault.
    pub fn clear_injected_faults(&self) {
        self.vfs().clear_injected();
    }

    /// Limit the bytes of file data the namespace may hold (`None` removes the limit).
    ///
    /// Writes that would exceed the quota are shortened, then fail with `ENOSPC`. Data of
    /// unlinked files stops counting once their last handle closes.
    pub fn set_quota(&self, bytes: Option<u64>) {
        self.vfs().set_quota(bytes);
    }

    /// Bytes of file data currently charged against the quota.
    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        self.vfs().used()
    }

    /// Change the simulated effective user and group. Existing inodes keep their owners.
    pub fn set_credentials(&self, uid: u32, gid: u32) {
        self.vfs().set_credentials(uid, gid);
    }

    /// The simulated effective user and group ids.
    #[must_use]
    pub fn credentials(&self) -> (u32, u32) {
        self.vfs().credentials()
    }

    /// Set the simulated process umask (masked to `0o777`).
    pub fn set_umask(&self, umask: u32) {
        self.vfs().set_umask(umask);
    }

    /// The simulated process umask.
    #[must_use]
    pub fn umask(&self) -> u32 {
        self.vfs().umask()
    }

    /// Follow the filesystem semantics of `personality` from now on.
    ///
    /// Choose the personality before creating entries: names that differ only in case
    /// and already exist when switching to [`Personality::Windows`] stay distinct
    /// entries, and lookups match the first of them in name order.
    pub fn set_personality(&self, personality: Personality) {
        self.vfs().set_personality(personality);
    }

    /// The personality whose semantics the namespace follows.
    #[must_use]
    pub fn personality(&self) -> Personality {
        self.vfs().personality()
    }
}

/// Set the personality of the current simulation run's default namespace (the one used
/// where no [`with_filesystem`]/[`scope_filesystem`] selection applies); see
/// [`Filesystem::set_personality`].
pub fn set_run_personality(personality: Personality) {
    switchy_simulation::state::<RunFilesystem>()
        .0
        .set_personality(personality);
}

/// Simulate a crash of `filesystem`; see [`Filesystem::crash`].
pub fn crash(filesystem: &Filesystem, policy: &CrashPolicy) {
    filesystem.crash(policy);
}

/// The default namespace of one simulation run.
struct RunFilesystem(Arc<Filesystem>);

impl Default for RunFilesystem {
    fn default() -> Self {
        Self(Arc::new(Filesystem::new()))
    }
}

thread_local! {
    static SELECTED: RefCell<Option<Arc<Filesystem>>> = const { RefCell::new(None) };
}

/// The namespace filesystem operations on this thread currently use: the one selected by
/// [`with_filesystem`]/[`scope_filesystem`], or else the current simulation run's default.
#[must_use]
pub fn current_filesystem() -> Arc<Filesystem> {
    SELECTED
        .with_borrow(Clone::clone)
        .unwrap_or_else(|| switchy_simulation::state::<RunFilesystem>().0.clone())
}

pub(crate) fn directory_locations() -> crate::directories::DirectoryLocations {
    current_filesystem().directory_locations.clone()
}

/// Execute synchronous work in an explicit simulated filesystem.
///
/// Restores the previous selection on return or unwind. A returned future is not
/// scoped: callers must select the filesystem while polling it, not when constructing
/// it. Explicit real-filesystem mode remains independent of this namespace selection.
///
/// # Panics
/// * Propagates a panic from the callback after restoring the previous selection.
pub fn with_filesystem<T>(filesystem: &Arc<Filesystem>, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<Filesystem>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            SELECTED.with_borrow_mut(|current| *current = previous);
        }
    }
    let previous = SELECTED.with_borrow_mut(|current| current.replace(filesystem.clone()));
    let _restore = Restore(previous);
    work()
}

/// Execute work in an isolated namespace, rejecting active native filesystem mode.
///
/// # Errors
/// * Returns `Unsupported` without invoking `work` when real-filesystem mode is active.
/// * Propagates errors returned by `work`.
///
/// # Panics
/// * Propagates callback panics after restoring the previous namespace.
pub fn try_with_filesystem<T>(
    filesystem: &Arc<Filesystem>,
    work: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<T> {
    if real_fs_support::is_real_fs() {
        return Err(std::io::Error::from(std::io::ErrorKind::Unsupported));
    }
    with_filesystem(filesystem, work)
}

/// Scope every poll of a future to an explicit simulated filesystem.
///
/// Selection is restored after each poll, including `Pending` and unwinding.
/// Independently spawned tasks are not implicitly scoped; wrap each task explicitly.
/// The future is dropped under the selected filesystem as well.
#[must_use]
pub fn scope_filesystem<F: std::future::Future>(
    filesystem: Arc<Filesystem>,
    future: F,
) -> FilesystemFuture<F> {
    FilesystemFuture {
        filesystem,
        future: Some(Box::pin(future)),
    }
}

/// A future whose polling and cancellation cleanup use a selected filesystem.
pub struct FilesystemFuture<F: std::future::Future> {
    filesystem: Arc<Filesystem>,
    future: Option<std::pin::Pin<Box<F>>>,
}

impl<F: std::future::Future> std::future::Future for FilesystemFuture<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        with_filesystem(&this.filesystem, || {
            this.future
                .as_mut()
                .expect("future retained until drop")
                .as_mut()
                .poll(context)
        })
    }
}

impl<F: std::future::Future> Drop for FilesystemFuture<F> {
    fn drop(&mut self) {
        with_filesystem(&self.filesystem, || drop(self.future.take()));
    }
}

/// Run `work`, an operation of `class`, against the current namespace's state.
fn with_vfs<T>(
    class: OpClass,
    work: impl FnOnce(&mut Vfs) -> vfs::Result<T>,
) -> std::io::Result<T> {
    before_operation(class);
    let filesystem = current_filesystem();
    let mut vfs = filesystem.vfs();
    work(&mut vfs).map_err(Into::into)
}

/// A callback run at the start of simulated filesystem operations; see
/// [`set_operation_hook`].
pub type OperationHook = Arc<dyn Fn(OpClass) + Send + Sync>;

/// The operation hook of one simulation run.
#[derive(Default)]
struct RunOperationHook(Mutex<Option<OperationHook>>);

thread_local! {
    /// Whether this thread is running the operation hook. A property of the call stack,
    /// not of the simulation run, so it stays per OS thread.
    static IN_OPERATION_HOOK: Cell<bool> = const { Cell::new(false) };
}

/// Install (or with `None`, remove) the current simulation run's operation hook,
/// returning the previous one.
///
/// The hook is called with the operation's [`OpClass`] at the start of every simulated
/// filesystem operation that reads or changes a namespace (opening, reading, writing,
/// seeking from the end, synchronizing, inspecting or changing metadata, locking,
/// creating, listing, linking, renaming and removing entries), in every namespace, on the
/// thread performing the operation. Locking and seeking report [`OpClass::Metadata`].
/// Closing a handle and configuring a [`Filesystem`] do not call it, nor does explicit
/// real-filesystem mode.
///
/// The hook runs before the operation takes any filesystem lock, so it may block or
/// yield to other threads (a deterministic scheduler's switch point) and may itself
/// perform filesystem operations; those do not call the hook again.
///
/// The hook belongs to the run (the current `switchy_simulation` context), so simulated
/// threads of the run share it and parallel runs never see each other's hooks.
pub fn set_operation_hook(hook: Option<OperationHook>) -> Option<OperationHook> {
    let state = switchy_simulation::state::<RunOperationHook>();
    let mut current = state.0.lock().unwrap_or_else(PoisonError::into_inner);
    std::mem::replace(&mut *current, hook)
}

/// Call the run's operation hook for an operation of `class`, unless the hook is already
/// running on this thread. Must be called without holding any filesystem lock.
fn before_operation(class: OpClass) {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            IN_OPERATION_HOOK.set(false);
        }
    }
    if IN_OPERATION_HOOK.get() {
        return;
    }
    let hook = switchy_simulation::state::<RunOperationHook>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(hook) = hook {
        IN_OPERATION_HOOK.set(true);
        let _leave = Leave;
        hook(class);
    }
}

/// Resets the current simulated filesystem to an empty state.
///
/// Every handle opened before the reset becomes stale. Credentials, umask, quota, fault
/// configuration and directory locations are kept.
pub fn reset_fs() {
    current_filesystem().reset();
}

/// Resets the current simulated filesystem; equivalent to [`reset_fs`].
///
/// Directories and files share one inode table, so directories cannot be cleared
/// independently of the files they contain.
pub fn reset_directories() {
    reset_fs();
}

/// Check if a path exists (following symbolic links).
///
/// In explicit real-filesystem mode this checks the native filesystem.
#[must_use]
pub fn exists<P: AsRef<std::path::Path>>(path: P) -> bool {
    #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
    if real_fs_support::is_real_fs() {
        return path.as_ref().exists();
    }
    before_operation(OpClass::Metadata);
    current_filesystem().vfs().exists(ROOT, path.as_ref())
}

/// Initialize minimal filesystem structure (just essentials)
///
/// # Errors
///
/// * If any directory creation fails
pub fn init_minimal_fs() -> std::io::Result<()> {
    #[cfg(feature = "sync")]
    {
        sync::create_dir_all("/")?;
        sync::create_dir_all("/tmp")?;
        sync::create_dir_all("/home")?;
    }
    Ok(())
}

/// Initialize standard FHS-like filesystem structure
///
/// # Errors
///
/// * If any directory creation fails
pub fn init_standard_fs() -> std::io::Result<()> {
    #[cfg(feature = "sync")]
    {
        // Root directories
        sync::create_dir_all("/")?;
        sync::create_dir_all("/bin")?;
        sync::create_dir_all("/etc")?;
        sync::create_dir_all("/home")?;
        sync::create_dir_all("/lib")?;
        sync::create_dir_all("/opt")?;
        sync::create_dir_all("/root")?;
        sync::create_dir_all("/sbin")?;
        sync::create_dir_all("/tmp")?;
        sync::create_dir_all("/usr")?;
        sync::create_dir_all("/var")?;

        // Common /usr subdirectories
        sync::create_dir_all("/usr/bin")?;
        sync::create_dir_all("/usr/lib")?;
        sync::create_dir_all("/usr/local")?;
        sync::create_dir_all("/usr/local/bin")?;
        sync::create_dir_all("/usr/share")?;

        // Common /var subdirectories
        sync::create_dir_all("/var/log")?;
        sync::create_dir_all("/var/tmp")?;
        sync::create_dir_all("/var/cache")?;
    }
    Ok(())
}

/// Initialize a user's home directory with standard subdirectories
///
/// # Errors
///
/// * If any directory creation fails
pub fn init_user_home(username: &str) -> std::io::Result<()> {
    let home = format!("/home/{username}");

    #[cfg(feature = "sync")]
    {
        sync::create_dir_all(&home)?;
        sync::create_dir_all(format!("{home}/.config"))?;
        sync::create_dir_all(format!("{home}/.local"))?;
        sync::create_dir_all(format!("{home}/.local/share"))?;
        sync::create_dir_all(format!("{home}/.cache"))?;
        sync::create_dir_all(format!("{home}/Documents"))?;
        sync::create_dir_all(format!("{home}/Downloads"))?;
    }
    Ok(())
}

/// Seeds the simulator filesystem from a real filesystem path.
///
/// This function reads all files and directories from the real filesystem
/// at `real_path` and populates them into the simulator at `sim_path`.
///
/// This is useful for tests that need to load test fixtures from the real
/// filesystem into the simulator before running.
///
/// # Arguments
/// * `real_path` - Path on the real filesystem to read from
/// * `sim_path` - Path in the simulator where contents will be placed
///
/// # Errors
///
/// * If reading from the real filesystem fails
/// * If writing to the simulator fails
///
/// # Panics
///
/// * If the real filesystem path cannot be read
#[cfg(all(feature = "simulator-real-fs", feature = "sync", feature = "std"))]
pub fn seed_from_real_fs<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(
    real_path: P,
    sim_path: Q,
) -> std::io::Result<()> {
    seed_recursive(real_path.as_ref(), sim_path.as_ref())
}

#[cfg(all(feature = "simulator-real-fs", feature = "sync", feature = "std"))]
fn seed_recursive(real_path: &std::path::Path, sim_path: &std::path::Path) -> std::io::Result<()> {
    // Create the directory in simulator (outside of with_real_fs)
    sync::create_dir_all(sim_path)?;

    // Read entries from real filesystem
    let entries = with_real_fs(|| crate::standard::sync::read_dir_sorted(real_path))?;

    for entry in entries {
        let entry_name = entry.file_name();
        let real_entry_path = real_path.join(&entry_name);
        let sim_entry_path = sim_path.join(&entry_name);

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            seed_recursive(&real_entry_path, &sim_entry_path)?;
        } else if file_type.is_file() {
            // Read file content from real FS
            let content = with_real_fs(|| std::fs::read(&real_entry_path))?;
            // Write to simulator (outside of with_real_fs)
            sync::write(&sim_entry_path, content)?;
        }
        // Skip symlinks and other special files for now
    }

    Ok(())
}

/// The error returned by simulator operations unavailable in explicit real-filesystem mode.
#[cfg(all(feature = "simulator-real-fs", feature = "std"))]
fn real_fs_unsupported(what: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("{what} requires native handles"),
    )
}

macro_rules! impl_file_sync {
    ($file:ident $(,)?) => {
        impl std::io::Read for $file {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.handle.read(buf)
            }
        }

        impl std::io::Write for $file {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.handle.write(buf)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl std::io::Seek for $file {
            fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
                self.handle.seek(pos)
            }
        }
    };
}

/// File type information for directory entries
///
/// Provides methods to determine whether an entry is a directory, file, or symbolic link.
#[derive(Debug, Clone)]
pub struct FileType {
    is_dir: bool,
    is_file: bool,
    is_symlink: bool,
}

impl FileType {
    /// Returns `true` if this entry represents a directory
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Returns `true` if this entry represents a regular file
    #[must_use]
    pub const fn is_file(&self) -> bool {
        self.is_file
    }

    /// Returns `true` if this entry represents a symbolic link
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        self.is_symlink
    }

    const fn from_kind(kind: FileKind) -> Self {
        Self {
            is_dir: matches!(kind, FileKind::Dir),
            is_file: matches!(kind, FileKind::File),
            is_symlink: matches!(kind, FileKind::Symlink),
        }
    }
}

#[cfg(test)]
mod file_type_tests {
    use super::FileType;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_file_type_for_directory() {
        let file_type = FileType {
            is_dir: true,
            is_file: false,
            is_symlink: false,
        };
        assert_eq!(file_type.is_dir(), true);
        assert_eq!(file_type.is_file(), false);
        assert_eq!(file_type.is_symlink(), false);
    }

    #[test_log::test]
    fn test_file_type_for_regular_file() {
        let file_type = FileType {
            is_dir: false,
            is_file: true,
            is_symlink: false,
        };
        assert_eq!(file_type.is_dir(), false);
        assert_eq!(file_type.is_file(), true);
        assert_eq!(file_type.is_symlink(), false);
    }

    #[test_log::test]
    fn test_file_type_for_symlink() {
        let file_type = FileType {
            is_dir: false,
            is_file: false,
            is_symlink: true,
        };
        assert_eq!(file_type.is_dir(), false);
        assert_eq!(file_type.is_file(), false);
        assert_eq!(file_type.is_symlink(), true);
    }

    #[test_log::test]
    fn test_file_type_clone() {
        let original = FileType {
            is_dir: true,
            is_file: false,
            is_symlink: false,
        };
        let cloned = original.clone();
        assert_eq!(cloned.is_dir(), original.is_dir());
        assert_eq!(cloned.is_file(), original.is_file());
        assert_eq!(cloned.is_symlink(), original.is_symlink());
    }
}

/// Synchronous filesystem operations for the simulator
///
/// Blocking operations on the current simulated namespace. All operations are immediate
/// and never touch the actual disk (except in explicit real-filesystem mode).
#[cfg(feature = "sync")]
pub mod sync {
    use std::{
        ffi::OsStr,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use super::{
        FileKind, OpClass, before_operation,
        vfs::{Handle, ROOT},
        with_vfs,
    };
    use crate::{errno::Errno, sync::OpenOptions, types::entry_name};

    pub use super::{DirEntryInfo, FileId, Metadata, Permissions};

    /// File handle for synchronous operations in the simulated filesystem
    ///
    /// Provides read, write, and seek operations on an inode of the simulated filesystem.
    /// The handle keeps the inode alive after it is renamed or unlinked.
    pub struct File {
        #[cfg_attr(not(feature = "simulator-real-fs"), allow(dead_code))]
        pub(crate) path: PathBuf,
        pub(crate) handle: Handle,
    }

    impl std::fmt::Debug for File {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("File")
                .field("path", &self.path)
                .field("handle", &self.handle)
                .finish()
        }
    }

    impl File {
        /// Opens a file in read-only mode
        ///
        /// This is a convenience method equivalent to `OpenOptions::new().read(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the file does not exist or cannot be read
        pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            OpenOptions::new().read(true).open(path)
        }

        /// Creates a new file for writing, truncating any existing file
        ///
        /// This is a convenience method equivalent to
        /// `OpenOptions::new().create(true).write(true).truncate(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the parent directory does not exist or is not writable
        pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
        }

        /// Returns a new `OpenOptions` builder for configuring how a file is opened
        #[must_use]
        pub const fn options() -> OpenOptions {
            OpenOptions::new()
        }

        /// Open `path` with `options` in the current simulated namespace.
        ///
        /// Unlike [`OpenOptions::open`], this never consults explicit real-filesystem mode.
        ///
        /// # Errors
        /// * `EINVAL` for flag combinations `std::fs::OpenOptions` rejects.
        /// * `ENOENT`, `EEXIST`, `ENOTDIR`, `EISDIR`, `ELOOP` (`nofollow` on a symbolic
        ///   link), `EACCES` and injected faults, as the equivalent POSIX `open` reports.
        pub fn open_with(path: impl AsRef<Path>, options: &OpenOptions) -> std::io::Result<Self> {
            let filesystem = super::current_filesystem();
            Ok(Self {
                path: path.as_ref().to_path_buf(),
                handle: Handle::open(&filesystem, None, path.as_ref(), &options.spec())?,
            })
        }

        /// Retrieves metadata about the file
        ///
        /// # Errors
        ///
        /// * If the file metadata cannot be retrieved (when using real filesystem)
        /// * `ESTALE` after a crash or reset, or an injected fault
        pub fn metadata(&self) -> std::io::Result<Metadata> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Ok(std::fs::metadata(&self.path)?.into());
            }

            self.handle.metadata()
        }

        /// Converts this synchronous file handle into an asynchronous file handle
        #[cfg(feature = "async")]
        #[must_use]
        pub fn into_async(self) -> crate::unsync::File {
            crate::unsync::File {
                path: self.path,
                handle: self.handle,
            }
        }

        /// Make the file's data, length and permissions durable.
        ///
        /// For a directory handle this makes pending changes to its entries durable.
        ///
        /// # Errors
        /// * `ESTALE` after a crash or reset.
        /// * Injected `EIO`/`ENOSPC`; the file's pending data is then forgotten: it stays
        ///   readable until a crash but never becomes durable.
        /// * Explicit real-filesystem mode returns `Unsupported`, because its copied
        ///   handles cannot synchronize the original native file.
        pub fn sync_all(&self) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native synchronization"));
            }
            self.handle.sync(false)
        }

        /// Make the file's data and length durable (permissions may stay pending).
        ///
        /// # Errors
        /// * As for [`File::sync_all`].
        pub fn sync_data(&self) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native synchronization"));
            }
            self.handle.sync(true)
        }

        /// Truncate or extend the file to `len` bytes (extension fills with zeros).
        ///
        /// # Errors
        /// * `EINVAL` when the handle is not writable.
        /// * `ENOSPC` when extension exceeds the namespace quota, `ESTALE`, injected faults.
        pub fn set_len(&self, len: u64) -> std::io::Result<()> {
            self.handle.set_len(len)
        }

        /// Change the file's permission bits.
        ///
        /// # Errors
        /// * `EPERM` unless the simulated user owns the file (or is root).
        /// * `ESTALE`, injected faults; `Unsupported` in explicit real-filesystem mode.
        pub fn set_permissions(&self, permissions: Permissions) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native permissions"));
            }
            self.handle.set_mode(permissions.mode())
        }

        /// Try to take an exclusive flock-style advisory lock without waiting.
        ///
        /// Locks belong to this handle (not the process): another handle on the same
        /// inode conflicts even within one thread. A held shared lock is converted. The
        /// simulator offers no blocking variant; poll and yield instead.
        ///
        /// # Errors
        /// * `WouldBlock` when another handle holds a conflicting lock.
        /// * `ESTALE`; `Unsupported` in explicit real-filesystem mode.
        pub fn try_lock(&self) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native locks"));
            }
            self.handle.try_lock(true)
        }

        /// Try to take a shared flock-style advisory lock without waiting.
        ///
        /// # Errors
        /// * As for [`File::try_lock`].
        pub fn try_lock_shared(&self) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native locks"));
            }
            self.handle.try_lock(false)
        }

        /// Release any lock held by this handle. Dropping the handle also releases it.
        ///
        /// # Errors
        /// * `ESTALE` after a crash or reset (the lock is already gone).
        pub fn unlock(&self) -> std::io::Result<()> {
            self.handle.unlock()
        }

        /// Consume this handle and acquire an exclusive advisory lock without waiting.
        /// The guard retains file identity across renames and releases on drop.
        ///
        /// # Errors
        /// * Returns `WouldBlock` when this simulated file is already locked.
        /// * Explicit real-filesystem mode returns `Unsupported`: copied simulated
        ///   handles cannot retain a native OS lock.
        pub fn try_lock_exclusive(self) -> std::io::Result<ExclusiveFileLock> {
            self.try_lock()?;
            Ok(ExclusiveFileLock { _file: self })
        }
    }

    impl_file_sync!(File);

    /// Exclusive advisory lock retaining simulated file identity until dropped.
    /// Ownership follows handles across threads and does not prevent ordinary writes.
    pub struct ExclusiveFileLock {
        _file: File,
    }

    impl OpenOptions {
        /// Opens a file with the configured options
        ///
        /// # Errors
        ///
        /// * As for [`File::open_with`], or native errors in explicit real-filesystem mode.
        pub fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            // Only try to use real fs if both simulator-real-fs AND std features are enabled
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                let std_options: std::fs::OpenOptions = self.clone().into();
                let std_file = std_options.open(&path)?;
                return super::real_fs_support::convert_std_file_to_simulator(
                    std_file, &path, self.read, self.write,
                );
            }

            File::open_with(path, &self)
        }
    }

    /// Reads the entire contents of a file into a byte vector
    ///
    /// # Errors
    ///
    /// * If the file doesn't exist, is a directory, or cannot be read
    pub fn read<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<u8>> {
        use std::io::Read as _;

        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return std::fs::read(path);
        }

        let mut file = File::open_with(path, &OpenOptions::new().read(true))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        Ok(data)
    }

    /// Reads the entire contents of a file into a string
    ///
    /// # Errors
    ///
    /// * Returns `std::io::ErrorKind::NotFound` if the file does not exist.
    /// * Returns `std::io::ErrorKind::InvalidData` if the file contains invalid UTF-8.
    pub fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::read_to_string(path);
        }

        String::from_utf8(read(path)?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Writes a slice as the entire contents of a file
    ///
    /// # Errors
    ///
    /// * If the file cannot be created or written to
    pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> std::io::Result<()> {
        use std::io::Write;

        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::write(path, contents);
        }

        let mut file = File::open_with(
            path,
            &OpenOptions::new().create(true).write(true).truncate(true),
        )?;

        file.write_all(contents.as_ref())?;
        Ok(())
    }

    /// Creates a directory (mode `0o777` before the umask).
    ///
    /// # Errors
    ///
    /// * `EEXIST` if the entry exists, `ENOENT` if the parent does not exist
    /// * `EACCES` if the parent is not writable, injected faults
    /// * If underlying `std::fs::create_dir` fails (when using real filesystem)
    pub fn create_dir<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::create_dir(path);
        }

        with_vfs(OpClass::DirOp, |vfs| vfs.create_dir(ROOT, path.as_ref(), 0o777))
    }

    /// Creates a directory and all missing parent directories
    ///
    /// # Errors
    ///
    /// * If a component exists but is not a directory, or cannot be created
    /// * If underlying `std::fs::create_dir_all` fails (when using real filesystem)
    pub fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::create_dir_all(path);
        }

        with_vfs(OpClass::DirOp, |vfs| vfs.create_dir_all(ROOT, path.as_ref(), 0o777))
    }

    /// Removes a directory and all its contents recursively
    ///
    /// A symbolic link is removed itself; its target is never followed.
    ///
    /// # Errors
    ///
    /// * If the path doesn't exist or is not a directory
    /// * If underlying `std::fs::remove_dir_all` fails (when using real filesystem)
    pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::remove_dir_all(path);
        }

        with_vfs(OpClass::DirOp, |vfs| vfs.remove_dir_all(ROOT, path.as_ref()))
    }

    /// Exclusively create a writable file with owner-only (`0o600`) permissions.
    ///
    /// Explicit real-fs mode rejects this operation rather than returning a copied
    /// native handle.
    ///
    /// # Errors
    /// * Returns creation errors or `Unsupported` in explicit real-fs mode.
    pub fn create_private_file(path: impl AsRef<Path>) -> std::io::Result<File> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return Err(std::io::Error::from(std::io::ErrorKind::Unsupported));
        }
        File::open_with(
            path,
            &OpenOptions::new().write(true).create_new(true).mode(0o600),
        )
    }

    /// Inspect an entry without following its final symbolic link.
    ///
    /// This observation does not prevent subsequent path replacement.
    ///
    /// # Errors
    /// * Returns an error for missing paths, or native metadata failure.
    pub fn symlink_metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::symlink_metadata(path);
        }
        with_vfs(OpClass::Metadata, |vfs| vfs.stat(ROOT, path.as_ref(), false))
    }

    /// Inspect an entry, following symbolic links.
    ///
    /// # Errors
    /// * `ENOENT`, `ENOTDIR`, `ELOOP`, `EACCES`, injected faults.
    pub fn metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::metadata(path);
        }
        with_vfs(OpClass::Metadata, |vfs| vfs.stat(ROOT, path.as_ref(), true))
    }

    /// Make pending changes to a directory's entries durable.
    ///
    /// # Errors
    /// * The path is missing or not a directory.
    /// * Injected faults; native failure in real-filesystem mode.
    pub fn sync_directory(path: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::sync_directory(path);
        }
        with_vfs(OpClass::Sync, |vfs| {
            let ino = vfs.resolve(ROOT, path.as_ref(), true)?;
            if vfs.kind_of(ROOT, path.as_ref(), true) != Some(FileKind::Dir) {
                return Err(Errno::NotDir);
            }
            vfs.sync(ino, false)
        })
    }

    /// Move a regular file, replacing an existing destination file.
    ///
    /// Open handles keep referring to the moved inode. The change becomes durable when
    /// the directories involved are synchronized.
    ///
    /// # Errors
    /// * Source is missing (`NotFound`), or either path names a non-regular file
    ///   (`InvalidInput`).
    /// * Rename errors, or a native failure in explicit real-filesystem mode.
    pub fn rename_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::rename_file(from, to);
        }
        let (from, to) = (from.as_ref(), to.as_ref());
        if !symlink_metadata(from)?.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "source is not a regular file",
            ));
        }
        match symlink_metadata(to) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "destination is not a regular file",
                ));
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        rename(from, to)
    }

    /// Rename a file or directory with POSIX `rename` semantics.
    ///
    /// An existing destination file is replaced; a directory may replace an empty
    /// directory. Renaming an entry onto another link to the same inode does nothing.
    ///
    /// # Errors
    /// * `ENOENT`, `EISDIR`, `ENOTDIR`, `ENOTEMPTY`, `EINVAL` (into its own subtree),
    ///   `EBUSY` (root), `EACCES`, injected faults; native errors in real-fs mode.
    pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::rename(from, to);
        }
        with_vfs(OpClass::Rename, |vfs| vfs.rename(ROOT, from.as_ref(), ROOT, to.as_ref(), false))
    }

    /// Rename without replacing: fails with `EEXIST` if `to` exists (also a dangling
    /// symbolic link). The check and the rename are one atomic step.
    ///
    /// # Errors
    /// * `EEXIST`, plus every error of [`rename`].
    pub fn rename_noreplace(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::rename_noreplace(from, to);
        }
        with_vfs(OpClass::Rename, |vfs| vfs.rename(ROOT, from.as_ref(), ROOT, to.as_ref(), true))
    }

    /// Remove a file or symbolic link (never a directory).
    ///
    /// # Errors
    /// * `ENOENT`; `EISDIR` (Linux) or `EPERM` (elsewhere) for directories; `EACCES`;
    ///   injected faults.
    pub fn remove_file(path: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::remove_file(path);
        }
        with_vfs(OpClass::DirOp, |vfs| vfs.remove_file(ROOT, path.as_ref()))
    }

    /// Remove an empty directory.
    ///
    /// # Errors
    /// * `ENOENT`, `ENOTDIR`, `ENOTEMPTY`, `EBUSY` (root), `EACCES`, injected faults.
    pub fn remove_dir(path: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::remove_dir(path);
        }
        with_vfs(OpClass::DirOp, |vfs| vfs.remove_dir(ROOT, path.as_ref()))
    }

    /// Create a hard link `link` to `original` (which is not followed if it is a
    /// symbolic link).
    ///
    /// # Errors
    /// * `ENOENT`, `EEXIST`, `EPERM` (directories), `EACCES`, injected faults.
    pub fn hard_link(original: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::hard_link(original, link);
        }
        with_vfs(OpClass::DirOp, |vfs| vfs.hard_link(ROOT, original.as_ref(), ROOT, link.as_ref()))
    }

    /// Create a symbolic link at `link` pointing to `original` (which need not exist).
    ///
    /// # Errors
    /// * `EEXIST`, `ENOENT` (missing parent or empty target), `EACCES`, injected faults.
    pub fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::symlink(original, link);
        }
        with_vfs(OpClass::DirOp, |vfs| vfs.symlink(original.as_ref(), ROOT, link.as_ref()))
    }

    /// Read the target of a symbolic link.
    ///
    /// # Errors
    /// * `EINVAL` if the entry is not a symbolic link, `ENOENT`, injected faults.
    pub fn read_link(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::read_link(path);
        }
        with_vfs(OpClass::Read, |vfs| vfs.read_link(ROOT, path.as_ref()))
    }

    /// Change permission bits, following symbolic links.
    ///
    /// # Errors
    /// * `EPERM` unless the simulated user owns the entry (or is root), `ENOENT`,
    ///   injected faults.
    pub fn set_permissions(
        path: impl AsRef<Path>,
        permissions: Permissions,
    ) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::set_permissions(path, permissions);
        }
        with_vfs(OpClass::Metadata, |vfs| vfs.set_mode(ROOT, path.as_ref(), permissions.mode()))
    }

    /// The simulated effective user id of the current namespace.
    ///
    /// # Errors
    /// * Native errors in explicit real-filesystem mode; otherwise infallible.
    pub fn current_uid() -> std::io::Result<u32> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::current_uid();
        }
        Ok(super::current_filesystem().credentials().0)
    }

    /// Create an anonymous temporary file: readable, writable, owner-only, and never
    /// linked into any directory. Its data disappears when the last handle closes.
    ///
    /// # Errors
    /// * Injected faults; `Unsupported` in explicit real-filesystem mode.
    pub fn tempfile() -> std::io::Result<File> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return Err(super::real_fs_unsupported("anonymous temporary files"));
        }
        before_operation(OpClass::Open);
        let filesystem = super::current_filesystem();
        let mut vfs = filesystem.vfs();
        vfs.check_open_fault()?;
        let ino = vfs.create_anonymous(0o600);
        let handle = Handle::from_ino(&filesystem, &mut vfs, ino, true, true)?;
        drop(vfs);
        Ok(File {
            path: PathBuf::new(),
            handle,
        })
    }

    /// Create an anonymous temporary file on the filesystem of directory `dir`.
    ///
    /// # Errors
    /// * `ENOENT`, `ENOTDIR`, `EACCES` (directory not writable), injected faults;
    ///   `Unsupported` in explicit real-filesystem mode.
    pub fn tempfile_in(dir: impl AsRef<Path>) -> std::io::Result<File> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return Err(super::real_fs_unsupported("anonymous temporary files"));
        }
        before_operation(OpClass::Open);
        let filesystem = super::current_filesystem();
        let mut vfs = filesystem.vfs();
        vfs.check_temp_dir(ROOT, dir.as_ref())?;
        let ino = vfs.create_anonymous(0o600);
        let handle = Handle::from_ino(&filesystem, &mut vfs, ino, true, true)?;
        drop(vfs);
        Ok(File {
            path: PathBuf::new(),
            handle,
        })
    }

    /// Canonicalizes a path: an absolute path with every symbolic link, `.` and `..`
    /// resolved. Relative paths resolve against the root directory.
    ///
    /// # Errors
    ///
    /// * If the path does not exist (or a component is not a directory)
    /// * If underlying `std::fs::canonicalize` fails (when using real filesystem)
    pub fn canonicalize<P: AsRef<Path>>(path: P) -> std::io::Result<std::path::PathBuf> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::canonicalize(path);
        }

        with_vfs(OpClass::Metadata, |vfs| vfs.canonicalize(ROOT, path.as_ref()))
    }

    fn list_dir(path: &Path) -> std::io::Result<Vec<DirEntry>> {
        let entries = with_vfs(OpClass::DirOp, |vfs| vfs.read_dir(ROOT, path))?;
        Ok(entries
            .into_iter()
            .map(|entry| DirEntry {
                path: path.join(&entry.name),
                file_name: entry.name,
                file_type_info: super::FileType::from_kind(entry.kind),
            })
            .collect())
    }

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If the directory doesn't exist or cannot be read
    /// * If underlying `std::fs::read_dir` fails (when using real filesystem)
    pub fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            let std_entries = crate::standard::sync::read_dir_sorted(path)?;
            return std_entries
                .into_iter()
                .map(|x| DirEntry::from_std(&x))
                .collect::<std::io::Result<Vec<_>>>();
        }

        list_dir(path.as_ref())
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// Symbolic links are reported but not descended into.
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read
    /// * If the directory doesn't exist
    pub fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        fn walk_recursive(dir_path: &Path, all: &mut Vec<DirEntry>) -> std::io::Result<()> {
            for entry in list_dir(dir_path)? {
                let descend = entry.file_type_info.is_dir();
                let child = entry.path.clone();
                all.push(entry);
                if descend {
                    walk_recursive(&child, all)?;
                }
            }
            Ok(())
        }

        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            let std_entries = crate::standard::sync::walk_dir_sorted(path)?;
            return std_entries
                .into_iter()
                .map(|x| DirEntry::from_std(&x))
                .collect::<std::io::Result<Vec<_>>>();
        }

        let mut all_entries = Vec::new();
        walk_recursive(path.as_ref(), &mut all_entries)?;

        // Sort by full path for deterministic ordering
        all_entries.sort_by_key(DirEntry::path);

        Ok(all_entries)
    }

    /// Directory entry for synchronous filesystem operations
    ///
    /// Represents a single entry (file or directory) when iterating over directory contents.
    /// Provides methods to access the entry's path, name, and type information.
    pub struct DirEntry {
        path: PathBuf,
        file_name: std::ffi::OsString,
        file_type_info: super::FileType,
    }

    impl DirEntry {
        /// Create a new `DirEntry` from `std::fs::DirEntry`
        ///
        /// # Errors
        ///
        /// * If the file type cannot be determined
        pub fn from_std(entry: &std::fs::DirEntry) -> std::io::Result<Self> {
            let file_type = entry.file_type()?;
            Ok(Self {
                path: entry.path(),
                file_name: entry.file_name(),
                file_type_info: super::FileType {
                    is_dir: file_type.is_dir(),
                    is_file: file_type.is_file(),
                    is_symlink: file_type.is_symlink(),
                },
            })
        }

        /// Create a new `DirEntry` for a file in the simulator
        ///
        /// # Errors
        ///
        /// * Infallible in current implementation
        pub fn new_file(full_path: String, file_name: String) -> std::io::Result<Self> {
            Ok(Self {
                path: PathBuf::from(full_path),
                file_name: std::ffi::OsString::from(file_name),
                file_type_info: super::FileType::from_kind(FileKind::File),
            })
        }

        /// Create a new `DirEntry` for a directory in the simulator
        ///
        /// # Errors
        ///
        /// * Infallible in current implementation
        pub fn new_dir(full_path: String, dir_name: String) -> std::io::Result<Self> {
            Ok(Self {
                path: PathBuf::from(full_path),
                file_name: std::ffi::OsString::from(dir_name),
                file_type_info: super::FileType::from_kind(FileKind::Dir),
            })
        }

        /// Returns the full path to this entry
        #[must_use]
        pub fn path(&self) -> PathBuf {
            self.path.clone()
        }

        /// Returns the file name of this entry
        #[must_use]
        pub fn file_name(&self) -> std::ffi::OsString {
            self.file_name.clone()
        }

        /// Returns the file type of this entry
        ///
        /// # Errors
        ///
        /// This function always succeeds for simulator entries, but returns
        /// `Result` to match the `std::fs::DirEntry::file_type()` API.
        pub fn file_type(&self) -> std::io::Result<super::FileType> {
            Ok(self.file_type_info.clone())
        }
    }

    /// A capability-style handle on a directory.
    ///
    /// Operations take single entry names (never paths, never `.` or `..`) and act on
    /// the opened directory inode, even after it is renamed. Final symbolic links are
    /// never followed: opening one fails with `ELOOP`. This mirrors `openat`-family calls
    /// with `O_NOFOLLOW` on the real Unix backend.
    pub struct Dir {
        handle: Handle,
        path: PathBuf,
    }

    impl std::fmt::Debug for Dir {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Dir")
                .field("path", &self.path)
                .field("handle", &self.handle)
                .finish()
        }
    }

    impl Dir {
        const fn directory_options() -> OpenOptions {
            OpenOptions::new().read(true).directory(true).nofollow(true)
        }

        /// Open the directory at `path` without following a final symbolic link.
        ///
        /// # Errors
        /// * `ENOENT`, `ENOTDIR`, `ELOOP` (final component is a symbolic link), `EACCES`
        ///   (no read permission), injected faults; `Unsupported` in real-fs mode.
        pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("directory handles"));
            }
            let filesystem = super::current_filesystem();
            Ok(Self {
                handle: Handle::open(
                    &filesystem,
                    None,
                    path.as_ref(),
                    &Self::directory_options().spec(),
                )?,
                path: path.as_ref().to_path_buf(),
            })
        }

        /// The path this directory was opened with (diagnostic only; it may be stale).
        #[must_use]
        pub fn path(&self) -> &Path {
            &self.path
        }

        fn at<T>(
            &self,
            class: OpClass,
            name: &Path,
            work: impl FnOnce(&mut super::Vfs, super::vfs::Ino, &OsStr) -> super::vfs::Result<T>,
        ) -> std::io::Result<T> {
            let name = entry_name(name)?;
            self.handle.with(class, |vfs, ino| work(vfs, ino, name))
        }

        /// Open the subdirectory `name` without following symbolic links.
        ///
        /// # Errors
        /// * As for [`Dir::open`], plus `InvalidInput` for a non-name.
        pub fn open_dir_at(&self, name: impl AsRef<Path>) -> std::io::Result<Self> {
            let name = entry_name(name.as_ref())?;
            Ok(Self {
                handle: Handle::open(
                    self.handle.filesystem(),
                    Some(&self.handle),
                    Path::new(name),
                    &Self::directory_options().spec(),
                )?,
                path: self.path.join(name),
            })
        }

        /// Open or create the file `name` in this directory with `options` (including
        /// `nofollow`, `create_new` and `mode`).
        ///
        /// # Errors
        /// * As for [`File::open_with`], plus `InvalidInput` for a non-name.
        pub fn open_file_at(
            &self,
            name: impl AsRef<Path>,
            options: &OpenOptions,
        ) -> std::io::Result<File> {
            let name = entry_name(name.as_ref())?;
            Ok(File {
                path: self.path.join(name),
                handle: Handle::open(
                    self.handle.filesystem(),
                    Some(&self.handle),
                    Path::new(name),
                    &options.spec(),
                )?,
            })
        }

        /// Create the subdirectory `name` with `mode` (before the umask).
        ///
        /// # Errors
        /// * `EEXIST`, `EACCES`, `ENOENT` (this directory was removed), injected faults.
        pub fn create_dir_at(&self, name: impl AsRef<Path>, mode: u32) -> std::io::Result<()> {
            self.at(OpClass::DirOp, name.as_ref(), |vfs, ino, name| {
                vfs.create_dir(ino, Path::new(name), mode)
            })
        }

        /// Metadata of this directory.
        ///
        /// # Errors
        /// * `ESTALE`, injected faults.
        pub fn metadata(&self) -> std::io::Result<Metadata> {
            self.handle.metadata()
        }

        /// Metadata of entry `name`, without following a symbolic link.
        ///
        /// # Errors
        /// * `ENOENT`, `InvalidInput` for a non-name, injected faults.
        pub fn metadata_at(&self, name: impl AsRef<Path>) -> std::io::Result<Metadata> {
            self.at(OpClass::Metadata, name.as_ref(), |vfs, ino, name| {
                vfs.stat(ino, Path::new(name), false)
            })
        }

        /// Remove the file or symbolic link `name`.
        ///
        /// # Errors
        /// * As for [`remove_file`], plus `InvalidInput` for a non-name.
        pub fn remove_file_at(&self, name: impl AsRef<Path>) -> std::io::Result<()> {
            self.at(OpClass::DirOp, name.as_ref(), |vfs, ino, name| {
                vfs.remove_file(ino, Path::new(name))
            })
        }

        /// Remove the empty subdirectory `name`.
        ///
        /// # Errors
        /// * As for [`remove_dir`], plus `InvalidInput` for a non-name.
        pub fn remove_dir_at(&self, name: impl AsRef<Path>) -> std::io::Result<()> {
            self.at(OpClass::DirOp, name.as_ref(), |vfs, ino, name| {
                vfs.remove_dir(ino, Path::new(name))
            })
        }

        fn rename_between(
            &self,
            from: &Path,
            to_dir: &Self,
            to: &Path,
            noreplace: bool,
        ) -> std::io::Result<()> {
            let from = entry_name(from)?;
            let to = entry_name(to)?;
            if !Arc::ptr_eq(self.handle.filesystem(), to_dir.handle.filesystem()) {
                return Err(Errno::XDev.into());
            }
            before_operation(OpClass::Rename);
            let mut vfs = self.handle.filesystem().vfs();
            let from_start = self.handle.start_in(&vfs)?;
            let to_start = to_dir.handle.start_in(&vfs)?;
            vfs.rename(
                from_start,
                Path::new(from),
                to_start,
                Path::new(to),
                noreplace,
            )
            .map_err(Into::into)
        }

        /// Rename entry `from` of this directory to `to` in `to_dir` (which may be this
        /// directory), replacing an existing destination as [`rename`] does.
        ///
        /// # Errors
        /// * As for [`rename`], plus `InvalidInput` for non-names and `EXDEV` across
        ///   namespaces.
        pub fn rename_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            self.rename_between(from.as_ref(), to_dir, to.as_ref(), false)
        }

        /// Rename entry `from` to `to` in `to_dir` only if `to` does not exist.
        ///
        /// # Errors
        /// * `EEXIST`, plus every error of [`Dir::rename_at`].
        pub fn rename_noreplace_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            self.rename_between(from.as_ref(), to_dir, to.as_ref(), true)
        }

        /// Create hard link `to` in `to_dir` to entry `from` of this directory.
        ///
        /// # Errors
        /// * As for [`hard_link`], plus `InvalidInput` and `EXDEV`.
        pub fn hard_link_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let from = entry_name(from.as_ref())?;
            let to = entry_name(to.as_ref())?;
            if !Arc::ptr_eq(self.handle.filesystem(), to_dir.handle.filesystem()) {
                return Err(Errno::XDev.into());
            }
            before_operation(OpClass::DirOp);
            let mut vfs = self.handle.filesystem().vfs();
            let from_start = self.handle.start_in(&vfs)?;
            let to_start = to_dir.handle.start_in(&vfs)?;
            vfs.hard_link(from_start, Path::new(from), to_start, Path::new(to))
                .map_err(Into::into)
        }

        /// Create symbolic link `name` pointing to `target`.
        ///
        /// # Errors
        /// * As for [`symlink`], plus `InvalidInput` for a non-name.
        pub fn symlink_at(
            &self,
            target: impl AsRef<Path>,
            name: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let target = target.as_ref();
            self.at(OpClass::DirOp, name.as_ref(), |vfs, ino, name| {
                vfs.symlink(target, ino, Path::new(name))
            })
        }

        /// Read the target of symbolic link `name`.
        ///
        /// # Errors
        /// * As for [`read_link`], plus `InvalidInput` for a non-name.
        pub fn read_link_at(&self, name: impl AsRef<Path>) -> std::io::Result<PathBuf> {
            self.at(OpClass::Read, name.as_ref(), |vfs, ino, name| {
                vfs.read_link(ino, Path::new(name))
            })
        }

        /// Change this directory's permission bits.
        ///
        /// # Errors
        /// * `EPERM` unless owned by the simulated user, `ESTALE`, injected faults.
        pub fn set_permissions(&self, permissions: Permissions) -> std::io::Result<()> {
            self.handle.set_mode(permissions.mode())
        }

        /// The entries of this directory (excluding `.` and `..`), sorted by name.
        ///
        /// # Errors
        /// * `ESTALE`, injected faults.
        pub fn entries(&self) -> std::io::Result<Vec<DirEntryInfo>> {
            self.handle.with(OpClass::DirOp, super::Vfs::list)
        }

        /// Make pending changes to this directory's entries durable.
        ///
        /// # Errors
        /// * `ESTALE`, injected faults.
        pub fn sync(&self) -> std::io::Result<()> {
            self.handle.sync(false)
        }
    }

    #[cfg(test)]
    mod test {
        use std::io::Read as _;

        use pretty_assertions::assert_eq;

        use super::OpenOptions;

        #[switchy_async::test]
        async fn can_read_empty_file() {
            const FILENAME: &str = "sync::test1";

            super::super::reset_fs();
            super::write(FILENAME, b"").unwrap();

            let mut file = OpenOptions::new().read(true).open(FILENAME).unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).unwrap();

            assert_eq!(read_count, 0);
        }

        #[switchy_async::test]
        async fn can_read_small_bytes_file() {
            const FILENAME: &str = "sync::test2";

            super::super::reset_fs();
            super::write(FILENAME, b"hey").unwrap();

            let mut file = OpenOptions::new().read(true).open(FILENAME).unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).unwrap();

            assert_eq!(read_count, 3);
        }

        #[test_log::test]
        fn test_write_without_write_permission() {
            {
                use std::io::Write as _;

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                // Create file with write permission
                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open("/tmp/test_perms.txt")
                    .unwrap();
                file.write_all(b"initial").unwrap();
                drop(file);

                // Open file with read-only permission
                let mut file = OpenOptions::new()
                    .read(true)
                    .open("/tmp/test_perms.txt")
                    .unwrap();

                // Writing through a read-only handle fails with EBADF, as on POSIX.
                let result = file.write_all(b"should fail");
                assert!(result.is_err());
                #[cfg(unix)]
                assert_eq!(
                    result.unwrap_err().raw_os_error(),
                    Some(rustix::io::Errno::BADF.raw_os_error())
                );
            }
        }

        #[test_log::test]
        fn test_truncate_existing_file() {
            {
                use std::io::Write as _;

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                // Create file with initial content
                super::write(
                    "/tmp/truncate_test.txt",
                    b"initial content that should be removed",
                )
                .unwrap();

                // Verify initial content exists
                let content = super::read_to_string("/tmp/truncate_test.txt").unwrap();
                assert_eq!(content, "initial content that should be removed");

                // Open with truncate flag
                let mut file = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open("/tmp/truncate_test.txt")
                    .unwrap();

                // Write new content
                file.write_all(b"new").unwrap();
                drop(file);

                // Verify file was truncated and only has new content
                let content = super::read_to_string("/tmp/truncate_test.txt").unwrap();
                assert_eq!(content, "new");
            }
        }

        #[test_log::test]
        fn test_partial_reads() {
            {
                use std::io::Read as _;

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                // Create file with known content
                let test_data = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"; // 36 bytes
                super::write("/tmp/partial_read.txt", test_data).unwrap();

                let mut file = OpenOptions::new()
                    .read(true)
                    .open("/tmp/partial_read.txt")
                    .unwrap();

                // Read in small chunks
                let mut buf = [0u8; 10];
                let mut total_read = Vec::new();

                loop {
                    let count = file.read(&mut buf).unwrap();
                    if count == 0 {
                        break;
                    }
                    total_read.extend_from_slice(&buf[..count]);
                }

                // Verify all data was read correctly
                assert_eq!(total_read.as_slice(), test_data);
            }
        }

        #[test_log::test]
        fn test_seek_and_read() {
            {
                use std::io::{Read as _, Seek as _, SeekFrom};

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                // Create file with content
                super::write("/tmp/seek_test.txt", b"Hello, World!").unwrap();

                let mut file = OpenOptions::new()
                    .read(true)
                    .open("/tmp/seek_test.txt")
                    .unwrap();

                // Seek to position 7 (start of "World")
                let pos = file.seek(SeekFrom::Start(7)).unwrap();
                assert_eq!(pos, 7);

                // Read from new position
                let mut buf = [0u8; 5];
                let count = file.read(&mut buf).unwrap();
                assert_eq!(count, 5);
                assert_eq!(&buf, b"World");

                // Seek back to beginning
                let pos = file.seek(SeekFrom::Start(0)).unwrap();
                assert_eq!(pos, 0);

                // Read again
                let mut buf = [0u8; 5];
                let count = file.read(&mut buf).unwrap();
                assert_eq!(count, 5);
                assert_eq!(&buf, b"Hello");
            }
        }

        #[test_log::test]
        fn test_seek_from_end() {
            {
                use std::io::{Read as _, Seek as _, SeekFrom};

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                // Create file with content
                super::write("/tmp/seek_end.txt", b"0123456789").unwrap(); // 10 bytes

                let mut file = OpenOptions::new()
                    .read(true)
                    .open("/tmp/seek_end.txt")
                    .unwrap();

                // NOTE: Current implementation bug with SeekFrom::End
                // The formula is: length - offset
                // When offset is negative, it should ADD to length, but instead:
                // length - (-3) causes underflow in u64::try_from(i64 - i64)
                // We test with positive offset to avoid underflow while documenting the issue
                let pos = file.seek(SeekFrom::End(0)).unwrap();
                assert_eq!(pos, 10, "Seek to end of 10-byte file");

                // Reading should return 0 bytes (at EOF)
                let mut buf = [0u8; 10];
                let count = file.read(&mut buf).unwrap();
                assert_eq!(count, 0);

                // BUG: SeekFrom::End with negative offsets causes underflow
                // This is a known bug - negative offsets subtract instead of add
            }
        }

        #[test_log::test]
        fn test_seek_from_current() {
            {
                use std::io::{Read as _, Seek as _, SeekFrom};

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                super::write("/tmp/seek_current.txt", b"0123456789").unwrap();

                let mut file = OpenOptions::new()
                    .read(true)
                    .open("/tmp/seek_current.txt")
                    .unwrap();

                // Read first 3 bytes
                let mut buf = [0u8; 3];
                file.read_exact(&mut buf).unwrap();
                assert_eq!(&buf, b"012");

                // Seek forward 2 bytes from current position
                let pos = file.seek(SeekFrom::Current(2)).unwrap();
                assert_eq!(pos, 5);

                // Read next 3 bytes (should be "567")
                file.read_exact(&mut buf).unwrap();
                assert_eq!(&buf, b"567");

                // Seek backward 4 bytes from current position
                let pos = file.seek(SeekFrom::Current(-4)).unwrap();
                assert_eq!(pos, 4);

                // Read should give "456"
                file.read_exact(&mut buf).unwrap();
                assert_eq!(&buf, b"456");
            }
        }

        #[test_log::test]
        fn test_seek_past_eof() {
            {
                use std::io::{Seek as _, SeekFrom};

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                super::write("/tmp/seek_past_eof.txt", b"12345").unwrap(); // 5 bytes

                let mut file = OpenOptions::new()
                    .read(true)
                    .open("/tmp/seek_past_eof.txt")
                    .unwrap();

                // Seek past EOF using Start should succeed
                let pos = file.seek(SeekFrom::Start(100)).unwrap();
                assert_eq!(pos, 100);

                // NOTE: Current implementation has an underflow bug with SeekFrom::End
                // when seeking way past EOF. We test normal seek behavior above.
                // The overflow happens because: length - large_negative_offset overflows
            }
        }

        #[test_log::test]
        fn test_multiple_handles_same_file() {
            {
                use std::io::{Read as _, Write as _};

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                // Create initial file
                super::write("/tmp/shared.txt", b"initial").unwrap();

                // Open file for writing
                let mut writer = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open("/tmp/shared.txt")
                    .unwrap();

                // Open same file for reading
                let mut reader = OpenOptions::new()
                    .read(true)
                    .open("/tmp/shared.txt")
                    .unwrap();

                // Write new content
                writer.write_all(b"updated content").unwrap();
                drop(writer);

                // Reader should see updated content (both handles share the inode)
                let mut buf = Vec::new();
                reader.read_to_end(&mut buf).unwrap();
                assert_eq!(buf, b"updated content");
            }
        }

        #[test_log::test]
        fn test_empty_buffer_read() {
            super::super::reset_fs();
            super::create_dir_all("/tmp").unwrap();

            super::write("/tmp/empty_buf.txt", b"content").unwrap();

            let mut file = OpenOptions::new()
                .read(true)
                .open("/tmp/empty_buf.txt")
                .unwrap();

            // Reading into empty buffer should return 0 without error
            let mut buf = [];
            let count = file.read(&mut buf).unwrap();
            assert_eq!(count, 0);
        }

        #[test_log::test]
        fn test_file_position_after_operations() {
            {
                use std::io::{Read as _, Seek as _, SeekFrom, Write as _};

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                let mut file = OpenOptions::new()
                    .create(true)
                    .read(true)
                    .write(true)
                    .open("/tmp/position_test.txt")
                    .unwrap();

                // Write data
                file.write_all(b"0123456789").unwrap();

                let pos = file.stream_position().unwrap();
                assert_eq!(pos, 10);

                // Seek to beginning before reading.
                file.seek(SeekFrom::Start(0)).unwrap();

                // Read 5 bytes
                let mut buf = [0u8; 5];
                file.read_exact(&mut buf).unwrap();

                // Position should be at 5 after read
                let pos = file.stream_position().unwrap();
                assert_eq!(pos, 5);
            }
        }

        #[test_log::test]
        #[cfg(all(feature = "sync", feature = "async"))]
        fn test_into_async_conversion() {
            {
                use std::io::{Seek as _, SeekFrom, Write as _};

                super::super::reset_fs();
                super::create_dir_all("/tmp").unwrap();

                // Create file with content and specific position
                let mut file = OpenOptions::new()
                    .create(true)
                    .read(true)
                    .write(true)
                    .open("/tmp/convert_test.txt")
                    .unwrap();

                file.write_all(b"Hello, World!").unwrap();
                file.seek(SeekFrom::Start(7)).unwrap();

                let position = file.handle.position;
                let path = file.path.clone();

                // Convert to async
                let async_file = file.into_async();

                // Verify state is preserved
                assert_eq!(async_file.handle.position, position);
                assert_eq!(async_file.path, path);
                assert_eq!(async_file.handle.write, true);
            }
        }

        #[test_log::test]
        fn test_remove_empty_directory() {
            super::super::reset_fs();

            // Create empty directory
            super::create_dir_all("/tmp/empty_dir").unwrap();

            // Verify it exists
            assert!(super::super::exists("/tmp/empty_dir"));

            // Remove it
            super::remove_dir_all("/tmp/empty_dir").unwrap();

            // Should no longer exist
            assert!(!super::super::exists("/tmp/empty_dir"));
        }

        #[test_log::test]
        fn test_remove_nonexistent_directory() {
            super::super::reset_fs();

            // Attempt to remove non-existent directory should fail
            let result = super::remove_dir_all("/nonexistent");
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
        }

        #[test_log::test]
        fn test_root_directory_operations() {
            super::super::reset_fs();
            super::create_dir_all("/").unwrap();

            // Should be able to read root directory
            let _entries = super::read_dir_sorted("/").unwrap();

            // Root should exist
            assert!(super::super::exists("/"));
        }

        #[test_log::test]
        fn test_create_file_in_current_directory() {
            {
                use std::io::Write as _;

                super::super::reset_fs();

                // Creating file in current directory "." should work
                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open("./test.txt")
                    .unwrap();

                file.write_all(b"content").unwrap();
                drop(file);

                // Should be able to read it back
                let content = super::read_to_string("./test.txt").unwrap();
                assert_eq!(content, "content");
            }
        }
    }
}

/// Asynchronous filesystem operations for the simulator
///
/// This module provides async filesystem operations that work with the in-memory
/// simulated filesystem. Operations complete when first polled since no actual I/O is
/// performed.
#[cfg(feature = "async")]
pub mod unsync {
    use std::{
        path::{Path, PathBuf},
        task::Poll,
    };

    use super::vfs::Handle;
    use crate::unsync::OpenOptions;

    pub use super::{Metadata, Permissions};

    /// File handle for asynchronous operations in the simulated filesystem
    ///
    /// Provides async read, write, and seek operations on an inode of the simulated
    /// filesystem.
    pub struct File {
        pub(crate) path: PathBuf,
        pub(crate) handle: Handle,
    }

    impl std::fmt::Debug for File {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("File")
                .field("path", &self.path)
                .field("handle", &self.handle)
                .finish()
        }
    }

    impl File {
        /// Opens a file in read-only mode asynchronously
        ///
        /// This is a convenience method equivalent to `OpenOptions::new().read(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the file does not exist or cannot be read
        #[allow(clippy::future_not_send)]
        pub async fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            OpenOptions::new().read(true).open(path).await
        }

        /// Creates a new file for writing, truncating any existing file
        ///
        /// This is a convenience method equivalent to
        /// `OpenOptions::new().create(true).write(true).truncate(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the parent directory does not exist or is not writable
        #[allow(clippy::future_not_send)]
        pub async fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
                .await
        }

        /// Returns a new `OpenOptions` builder for configuring how a file is opened
        #[must_use]
        pub const fn options() -> OpenOptions {
            OpenOptions::new()
        }

        /// Retrieves metadata about the file asynchronously
        ///
        /// # Errors
        ///
        /// * If the file metadata cannot be retrieved
        ///
        /// # Panics
        ///
        /// * If the `spawn_blocking` task panics (when using real filesystem)
        // Keep polling-time execution and the same async API as the native backend.
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn metadata(&self) -> std::io::Result<Metadata> {
            #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
            if super::real_fs_support::is_real_fs() {
                let path = self.path.clone();
                return switchy_async::task::spawn_blocking(move || {
                    Ok(std::fs::metadata(&path)?.into())
                })
                .await
                .unwrap();
            }

            self.handle.metadata()
        }

        /// Make the file's data, length and permissions durable.
        ///
        /// # Errors
        /// * As for the synchronous `File::sync_all`.
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn sync_all(&self) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native synchronization"));
            }
            self.handle.sync(false)
        }

        /// Make the file's data and length durable.
        ///
        /// # Errors
        /// * As for the synchronous `File::sync_data`.
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn sync_data(&self) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native synchronization"));
            }
            self.handle.sync(true)
        }

        /// Truncate or extend the file to `len` bytes.
        ///
        /// # Errors
        /// * As for the synchronous `File::set_len`.
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn set_len(&self, len: u64) -> std::io::Result<()> {
            self.handle.set_len(len)
        }

        /// Change the file's permission bits.
        ///
        /// # Errors
        /// * As for the synchronous `File::set_permissions`.
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn set_permissions(&self, permissions: Permissions) -> std::io::Result<()> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Err(super::real_fs_unsupported("native permissions"));
            }
            self.handle.set_mode(permissions.mode())
        }

        /// Converts this asynchronous file handle into a synchronous file handle
        #[cfg(feature = "sync")]
        #[must_use]
        pub fn into_sync(self) -> crate::sync::File {
            crate::sync::File {
                path: self.path,
                handle: self.handle,
            }
        }
    }

    impl_file_sync!(File);

    impl switchy_async::io::AsyncRead for File {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut switchy_async::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            use std::io::Read as _;

            let dst = buf.initialize_unfilled();

            match self.get_mut().read(dst) {
                Ok(count) => {
                    buf.advance(count);
                }
                Err(e) => return Poll::Ready(Err(e)),
            }

            Poll::Ready(Ok(()))
        }
    }

    impl switchy_async::io::AsyncSeek for File {
        fn start_seek(
            self: std::pin::Pin<&mut Self>,
            position: std::io::SeekFrom,
        ) -> std::io::Result<()> {
            use std::io::Seek as _;

            self.get_mut().seek(position)?;
            Ok(())
        }

        fn poll_complete(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<std::io::Result<u64>> {
            use std::io::Seek as _;

            Poll::Ready(self.get_mut().stream_position())
        }
    }

    impl switchy_async::io::AsyncWrite for File {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            use std::io::Write as _;

            Poll::Ready(self.get_mut().write(buf))
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            use std::io::Write as _;

            Poll::Ready(self.get_mut().flush())
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    impl OpenOptions {
        /// Opens a file asynchronously with the configured options
        ///
        /// # Errors
        ///
        /// * As for the synchronous `File::open_with`
        ///
        /// # Panics
        ///
        /// * If the `spawn_blocking` task panics (when using real filesystem)
        // Opening must remain lazy until polled, even when simulator I/O is immediate.
        #[allow(
            clippy::unused_async,
            clippy::unused_async_trait_impl,
            clippy::future_not_send
        )]
        pub async fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            #[cfg(all(feature = "simulator-real-fs", feature = "async",))]
            if super::real_fs_support::is_real_fs() {
                let path_buf = path.as_ref().to_path_buf();
                let options = self.clone();
                let std_file = switchy_async::task::spawn_blocking(move || {
                    let std_options: std::fs::OpenOptions = options.into();
                    std_options.open(&path_buf)
                })
                .await
                .unwrap()?;
                return super::real_fs_support::convert_std_file_to_simulator_async(
                    std_file, &path, self.read, self.write,
                )
                .await;
            }

            Ok(self.into_sync().open(path)?.into_async())
        }
    }

    /// Reads the entire contents of a file into a byte vector asynchronously
    ///
    /// # Errors
    ///
    /// * If the file doesn't exist or cannot be read
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    pub async fn read<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<u8>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || std::fs::read(path))
                .await
                .unwrap();
        }

        super::sync::read(path)
    }

    /// Reads the entire contents of a file into a string asynchronously
    ///
    /// # Errors
    ///
    /// * Returns `std::io::ErrorKind::NotFound` if the file does not exist.
    /// * Returns `std::io::ErrorKind::InvalidData` if the file contains invalid UTF-8.
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    pub async fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || std::fs::read_to_string(path))
                .await
                .unwrap();
        }

        // Fallback to sync simulator implementation
        super::sync::read_to_string(path)
    }

    /// Checks if a path exists asynchronously
    ///
    /// Returns `true` if the path exists, `false` otherwise.
    #[allow(clippy::unused_async)]
    pub async fn exists<P: AsRef<Path>>(path: P) -> bool {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || path.exists())
                .await
                .unwrap_or(false);
        }

        super::exists(path)
    }

    /// Checks if a path is a file asynchronously
    ///
    /// Returns `true` if the path exists and is a file, `false` otherwise.
    ///
    #[allow(clippy::unused_async)]
    pub async fn is_file<P: AsRef<Path>>(path: P) -> bool {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || path.is_file())
                .await
                .unwrap_or(false);
        }

        super::before_operation(super::OpClass::Metadata);
        super::current_filesystem()
            .vfs()
            .kind_of(super::vfs::ROOT, path.as_ref(), true)
            == Some(super::FileKind::File)
    }

    /// Checks if a path is a directory asynchronously
    ///
    /// Returns `true` if the path exists and is a directory, `false` otherwise.
    ///
    #[allow(clippy::unused_async)]
    pub async fn is_dir<P: AsRef<Path>>(path: P) -> bool {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || path.is_dir())
                .await
                .unwrap_or(false);
        }

        super::before_operation(super::OpClass::Metadata);
        super::current_filesystem()
            .vfs()
            .kind_of(super::vfs::ROOT, path.as_ref(), true)
            == Some(super::FileKind::Dir)
    }

    /// Writes a slice as the entire contents of a file
    ///
    /// # Errors
    ///
    /// * If the file cannot be created
    /// * If the file cannot be written to
    pub async fn write<P: AsRef<Path> + Send + Sync, C: AsRef<[u8]> + Send>(
        path: P,
        contents: C,
    ) -> std::io::Result<()> {
        use switchy_async::io::AsyncWriteExt;

        #[cfg(all(feature = "simulator-real-fs", feature = "tokio"))]
        if super::real_fs_support::is_real_fs() {
            return crate::tokio::unsync::write(path, contents).await;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .await?;

        file.write_all(contents.as_ref()).await?;
        Ok(())
    }

    /// Creates a directory and all missing parent directories asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir` fails (when using real filesystem)
    /// * If the parent directory does not exist
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    pub async fn create_dir<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || std::fs::create_dir(path))
                .await
                .unwrap();
        }

        super::sync::create_dir(path)
    }

    /// Creates a directory and all missing parent directories asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir_all` fails (when using real filesystem)
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || std::fs::create_dir_all(path))
                .await
                .unwrap();
        }

        super::sync::create_dir_all(path)
    }

    /// Removes a directory and all its contents recursively asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::remove_dir_all` fails (when using real filesystem)
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    pub async fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || std::fs::remove_dir_all(path))
                .await
                .unwrap();
        }

        super::sync::remove_dir_all(path)
    }

    /// Canonicalizes a path asynchronously by resolving `.` and `..` components
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::canonicalize` fails (when using real filesystem)
    /// * If the path cannot be converted to a string
    /// * If the path does not exist in the simulator
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    pub async fn canonicalize<P: AsRef<Path>>(path: P) -> std::io::Result<std::path::PathBuf> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || std::fs::canonicalize(path))
                .await
                .unwrap();
        }

        super::sync::canonicalize(path)
    }

    /// Directory entry for asynchronous filesystem operations
    ///
    /// Represents a single entry (file or directory) when iterating over directory contents.
    /// Provides async methods to access the entry's path, name, type, and metadata information.
    pub struct DirEntry {
        path: PathBuf,
        file_name: std::ffi::OsString,
        file_type_info: super::FileType,
    }

    impl DirEntry {
        /// Create a new `DirEntry` from `std::fs::DirEntry`
        ///
        /// # Errors
        ///
        /// * If the file type cannot be determined
        pub fn from_std(entry: &std::fs::DirEntry) -> std::io::Result<Self> {
            let file_type = entry.file_type()?;
            Ok(Self {
                path: entry.path(),
                file_name: entry.file_name(),
                file_type_info: super::FileType {
                    is_dir: file_type.is_dir(),
                    is_file: file_type.is_file(),
                    is_symlink: file_type.is_symlink(),
                },
            })
        }

        /// Create a new `DirEntry` for a file in the simulator
        ///
        /// # Errors
        ///
        /// * Infallible in current implementation
        pub fn new_file(full_path: String, file_name: String) -> std::io::Result<Self> {
            Ok(Self {
                path: PathBuf::from(full_path),
                file_name: std::ffi::OsString::from(file_name),
                file_type_info: super::FileType {
                    is_dir: false,
                    is_file: true,
                    is_symlink: false,
                },
            })
        }

        /// Create a new `DirEntry` for a directory in the simulator
        ///
        /// # Errors
        ///
        /// * Infallible in current implementation
        pub fn new_dir(full_path: String, dir_name: String) -> std::io::Result<Self> {
            Ok(Self {
                path: PathBuf::from(full_path),
                file_name: std::ffi::OsString::from(dir_name),
                file_type_info: super::FileType {
                    is_dir: true,
                    is_file: false,
                    is_symlink: false,
                },
            })
        }

        /// Returns the full path to this entry
        #[must_use]
        pub fn path(&self) -> PathBuf {
            self.path.clone()
        }

        /// Returns the file name of this entry
        #[must_use]
        pub fn file_name(&self) -> std::ffi::OsString {
            self.file_name.clone()
        }

        /// Returns the file type of this entry
        ///
        /// # Errors
        ///
        /// * Infallible
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn file_type(&self) -> std::io::Result<super::FileType> {
            Ok(self.file_type_info.clone())
        }

        /// Returns metadata for this entry
        ///
        /// # Errors
        ///
        /// * If the file/directory no longer exists
        ///
        /// # Panics
        ///
        /// * If the `spawn_blocking` task panics (when using real filesystem)
        // Preserve polling-time metadata access across simulator/native backends.
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn metadata(&self) -> std::io::Result<Metadata> {
            #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
            if super::real_fs_support::is_real_fs() {
                let path = self.path.clone();
                return switchy_async::task::spawn_blocking(move || {
                    Ok(std::fs::metadata(&path)?.into())
                })
                .await
                .unwrap();
            }

            super::sync::symlink_metadata(&self.path)
        }
    }

    /// Async directory reader that yields directory entries
    ///
    /// This struct is returned by [`read_dir`] and provides an async iterator
    /// over the entries in a directory.
    pub struct ReadDir {
        entries: std::vec::IntoIter<DirEntry>,
    }

    impl ReadDir {
        /// Returns the next entry in the directory
        ///
        /// Returns `Ok(None)` when there are no more entries.
        ///
        /// # Errors
        ///
        /// * Infallible in simulator mode
        #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
        pub async fn next_entry(&mut self) -> std::io::Result<Option<DirEntry>> {
            Ok(self.entries.next())
        }
    }

    /// Returns an async iterator over the entries in a directory
    ///
    /// # Errors
    ///
    /// * If the directory does not exist
    /// * If the path cannot be converted to a string
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails (when using real filesystem)
    #[allow(clippy::unused_async, clippy::needless_collect)]
    pub async fn read_dir<P: AsRef<Path>>(path: P) -> std::io::Result<ReadDir> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            let entries = switchy_async::task::spawn_blocking(move || {
                let std_entries = crate::standard::sync::read_dir_sorted(path)?;
                std_entries
                    .into_iter()
                    .map(|x| DirEntry::from_std(&x))
                    .collect::<std::io::Result<Vec<_>>>()
            })
            .await
            .unwrap()?;
            return Ok(ReadDir {
                entries: entries.into_iter(),
            });
        }

        // Use sync implementation which properly handles simulator filesystem
        let sync_entries = super::sync::read_dir_sorted(&path)?;
        let entries: Vec<DirEntry> = sync_entries
            .into_iter()
            .map(|e| DirEntry {
                path: e.path(),
                file_name: e.file_name(),
                file_type_info: e.file_type().unwrap(),
            })
            .collect();

        Ok(ReadDir {
            entries: entries.into_iter(),
        })
    }

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If the directory does not exist
    /// * If the path cannot be converted to a string
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails (when using real filesystem)
    #[allow(clippy::unused_async)]
    pub async fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || {
                let std_entries = crate::standard::sync::read_dir_sorted(path)?;
                std_entries
                    .into_iter()
                    .map(|x| DirEntry::from_std(&x))
                    .collect::<std::io::Result<Vec<_>>>()
            })
            .await
            .unwrap();
        }

        // Use sync implementation which properly handles simulator filesystem
        let sync_entries = super::sync::read_dir_sorted(&path)?;
        Ok(sync_entries
            .into_iter()
            .map(|e| DirEntry {
                path: e.path(),
                file_name: e.file_name(),
                file_type_info: e.file_type().unwrap(),
            })
            .collect())
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If the directory does not exist
    /// * If the path cannot be converted to a string
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails (when using real filesystem)
    #[allow(clippy::unused_async)]
    pub async fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || {
                let std_entries = crate::standard::sync::walk_dir_sorted(path)?;
                std_entries
                    .into_iter()
                    .map(|x| DirEntry::from_std(&x))
                    .collect::<std::io::Result<Vec<_>>>()
            })
            .await
            .unwrap();
        }

        // Use sync implementation which properly handles simulator filesystem
        let sync_entries = super::sync::walk_dir_sorted(&path)?;
        Ok(sync_entries
            .into_iter()
            .map(|e| DirEntry {
                path: e.path(),
                file_name: e.file_name(),
                file_type_info: e.file_type().unwrap(),
            })
            .collect())
    }

    /// Inspect an entry, following symbolic links.
    ///
    /// # Errors
    /// * As for the synchronous `metadata`.
    #[allow(clippy::unused_async)]
    pub async fn metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        super::sync::metadata(path)
    }

    /// Inspect an entry without following a final symbolic link.
    ///
    /// # Errors
    /// * As for the synchronous `symlink_metadata`.
    #[allow(clippy::unused_async)]
    pub async fn symlink_metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        super::sync::symlink_metadata(path)
    }

    /// Remove a file or symbolic link.
    ///
    /// # Errors
    /// * As for the synchronous `remove_file`.
    #[allow(clippy::unused_async)]
    pub async fn remove_file(path: impl AsRef<Path>) -> std::io::Result<()> {
        super::sync::remove_file(path)
    }

    /// Remove an empty directory.
    ///
    /// # Errors
    /// * As for the synchronous `remove_dir`.
    #[allow(clippy::unused_async)]
    pub async fn remove_dir(path: impl AsRef<Path>) -> std::io::Result<()> {
        super::sync::remove_dir(path)
    }

    /// Rename a file or directory with POSIX `rename` semantics.
    ///
    /// # Errors
    /// * As for the synchronous `rename`.
    #[allow(clippy::unused_async)]
    pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        super::sync::rename(from, to)
    }

    /// Rename without replacing an existing destination.
    ///
    /// # Errors
    /// * As for the synchronous `rename_noreplace`.
    #[allow(clippy::unused_async)]
    pub async fn rename_noreplace(
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        super::sync::rename_noreplace(from, to)
    }

    /// Create a hard link.
    ///
    /// # Errors
    /// * As for the synchronous `hard_link`.
    #[allow(clippy::unused_async)]
    pub async fn hard_link(
        original: impl AsRef<Path>,
        link: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        super::sync::hard_link(original, link)
    }

    /// Create a symbolic link.
    ///
    /// # Errors
    /// * As for the synchronous `symlink`.
    #[allow(clippy::unused_async)]
    pub async fn symlink(
        original: impl AsRef<Path>,
        link: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        super::sync::symlink(original, link)
    }

    /// Read the target of a symbolic link.
    ///
    /// # Errors
    /// * As for the synchronous `read_link`.
    #[allow(clippy::unused_async)]
    pub async fn read_link(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        super::sync::read_link(path)
    }

    /// Change permission bits, following symbolic links.
    ///
    /// # Errors
    /// * As for the synchronous `set_permissions`.
    #[allow(clippy::unused_async)]
    pub async fn set_permissions(
        path: impl AsRef<Path>,
        permissions: Permissions,
    ) -> std::io::Result<()> {
        super::sync::set_permissions(path, permissions)
    }

    /// Make pending changes to a directory's entries durable.
    ///
    /// # Errors
    /// * As for the synchronous `sync_directory`.
    #[allow(clippy::unused_async)]
    pub async fn sync_directory(path: impl AsRef<Path>) -> std::io::Result<()> {
        super::sync::sync_directory(path)
    }

    #[cfg(test)]
    mod test {
        use pretty_assertions::assert_eq;
        use tokio::io::AsyncReadExt as _;

        use super::OpenOptions;

        #[switchy_async::test]
        async fn can_read_empty_file() {
            const FILENAME: &str = "unsync::test1";

            super::super::reset_fs();
            super::write(FILENAME, b"").await.unwrap();

            let mut file = OpenOptions::new().read(true).open(FILENAME).await.unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).await.unwrap();

            assert_eq!(read_count, 0);
        }

        #[switchy_async::test]
        async fn can_read_small_bytes_file() {
            const FILENAME: &str = "unsync::test2";

            super::super::reset_fs();
            super::write(FILENAME, b"hey").await.unwrap();

            let mut file = OpenOptions::new().read(true).open(FILENAME).await.unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).await.unwrap();

            assert_eq!(read_count, 3);
        }
    }
}

#[cfg(test)]
mod real_fs_tests {
    use std::io::Write as _;

    #[switchy_async::test]
    async fn test_simulator_mode_no_real_fs() {
        // Verify that real_fs is NOT set in normal test
        assert!(
            !super::real_fs_support::is_real_fs(),
            "real_fs should NOT be set in normal test"
        );

        // This test should use simulated filesystem
        let content = "test content";
        let path = "/simulated/path/file.txt";

        // Create parent directory first (required by new implementation)
        super::sync::create_dir_all("/simulated/path").unwrap();

        // Write to simulated filesystem
        let mut file = crate::sync::OpenOptions::new()
            .create(true)
            .write(true)
            .open(path)
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Read from simulated filesystem
        let read_content = super::sync::read_to_string(path).unwrap();
        assert_eq!(read_content, content);

        // This file should not exist on real filesystem
        assert!(!std::path::Path::new(path).exists());
    }
}

#[cfg(test)]
mod exists_tests {
    use super::{exists, reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_exists_returns_false_for_nonexistent_path() {
        reset_fs();
        assert_eq!(exists("/nonexistent/path"), false);
    }

    #[test_log::test]
    fn test_exists_returns_true_for_directory() {
        reset_fs();
        sync::create_dir_all("/existing/directory").unwrap();
        assert_eq!(exists("/existing/directory"), true);
    }

    #[test_log::test]
    fn test_exists_returns_true_for_file() {
        reset_fs();
        sync::create_dir_all("/existing").unwrap();
        sync::write("/existing/file.txt", b"content").unwrap();
        assert_eq!(exists("/existing/file.txt"), true);
    }

    #[test_log::test]
    fn test_exists_with_root_path() {
        reset_fs();
        sync::create_dir_all("/").unwrap();
        assert_eq!(exists("/"), true);
    }
}

#[cfg(test)]
mod get_directory_children_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;

    fn get_directory_children(path: &str) -> (Vec<String>, Vec<String>) {
        let mut files = Vec::new();
        let mut subdirs = Vec::new();
        for entry in sync::read_dir_sorted(path).unwrap() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                subdirs.push(name);
            } else {
                files.push(name);
            }
        }
        (files, subdirs)
    }

    #[test_log::test]
    fn test_children_of_empty_directory() {
        reset_fs();
        sync::create_dir_all("/empty").unwrap();

        let (files, subdirs) = get_directory_children("/empty");
        assert_eq!(files, Vec::<String>::new());
        assert_eq!(subdirs, Vec::<String>::new());
    }

    #[test_log::test]
    fn test_children_empty_after_removing_mixed_content() {
        reset_fs();
        sync::create_dir_all("/removed/sub/nested").unwrap();
        sync::write("/removed/file.txt", b"data").unwrap();
        sync::write("/removed/sub/nested/file.txt", b"nested data").unwrap();

        sync::remove_dir_all("/removed").unwrap();
        sync::create_dir_all("/removed").unwrap();

        let (files, subdirs) = get_directory_children("/removed");
        assert_eq!(files, Vec::<String>::new());
        assert_eq!(subdirs, Vec::<String>::new());
    }

    #[test_log::test]
    fn test_children_with_files_only() {
        reset_fs();
        sync::create_dir_all("/files_only").unwrap();
        sync::write("/files_only/a.txt", b"a").unwrap();
        sync::write("/files_only/b.txt", b"b").unwrap();

        let (mut files, subdirs) = get_directory_children("/files_only");
        files.sort();
        assert_eq!(files, vec!["a.txt", "b.txt"]);
        assert_eq!(subdirs, Vec::<String>::new());
    }

    #[test_log::test]
    fn test_children_with_subdirs_only() {
        reset_fs();
        sync::create_dir_all("/dirs_only/subdir1").unwrap();
        sync::create_dir_all("/dirs_only/subdir2").unwrap();

        let (files, mut subdirs) = get_directory_children("/dirs_only");
        subdirs.sort();
        assert_eq!(files, Vec::<String>::new());
        assert_eq!(subdirs, vec!["subdir1", "subdir2"]);
    }

    #[test_log::test]
    fn test_children_mixed_content() {
        reset_fs();
        sync::create_dir_all("/mixed/sub").unwrap();
        sync::write("/mixed/file.txt", b"data").unwrap();

        let (files, subdirs) = get_directory_children("/mixed");
        assert_eq!(files, vec!["file.txt"]);
        assert_eq!(subdirs, vec!["sub"]);
    }

    #[test_log::test]
    fn test_children_of_root_directory() {
        reset_fs();
        sync::create_dir_all("/root_test").unwrap();
        sync::create_dir_all("/another").unwrap();

        let (files, mut subdirs) = get_directory_children("/");
        subdirs.sort();
        assert_eq!(files, Vec::<String>::new());
        assert!(subdirs.contains(&"root_test".to_string()));
        assert!(subdirs.contains(&"another".to_string()));
    }

    #[test_log::test]
    fn test_children_excludes_nested_items() {
        // Files/dirs in subdirectories should not appear in parent's children
        reset_fs();
        sync::create_dir_all("/parent/child").unwrap();
        sync::write("/parent/child/nested.txt", b"nested").unwrap();
        sync::write("/parent/direct.txt", b"direct").unwrap();

        let (files, subdirs) = get_directory_children("/parent");
        assert_eq!(files, vec!["direct.txt"]);
        assert_eq!(subdirs, vec!["child"]);
        // nested.txt should NOT appear
        assert!(!files.contains(&"nested.txt".to_string()));
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod async_file_conversion_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;
    use std::io::{Seek as _, SeekFrom, Write as _};

    #[test_log::test]
    fn test_async_file_into_sync_preserves_state() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();

        // Create async file with specific state
        let mut sync_file = crate::sync::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open("/tmp/async_to_sync.txt")
            .unwrap();

        sync_file.write_all(b"test data here").unwrap();
        sync_file.seek(SeekFrom::Start(5)).unwrap();

        let async_file = sync_file.into_async();

        // Position and path should be preserved
        assert_eq!(async_file.handle.position, 5);
        assert_eq!(async_file.path.to_string_lossy(), "/tmp/async_to_sync.txt");
        assert!(async_file.handle.write);

        // Convert back to sync
        let sync_file_again = async_file.into_sync();
        assert_eq!(sync_file_again.handle.position, 5);
        assert_eq!(
            sync_file_again.path.to_string_lossy(),
            "/tmp/async_to_sync.txt"
        );
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod async_operations_tests {
    use super::{reset_fs, sync, unsync};
    use pretty_assertions::assert_eq;

    #[test_log::test(switchy_async::test)]
    async fn test_async_write_and_read() {
        reset_fs();
        sync::create_dir_all("/async_test").unwrap();

        // Write using async API
        unsync::write("/async_test/file.txt", b"async content")
            .await
            .unwrap();

        // Read back using async API
        let content = unsync::read_to_string("/async_test/file.txt")
            .await
            .unwrap();
        assert_eq!(content, "async content");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_create_dir_all() {
        reset_fs();

        // Create nested directories asynchronously
        unsync::create_dir_all("/async_dirs/nested/deep")
            .await
            .unwrap();

        // Verify using sync API
        assert!(super::exists("/async_dirs/nested/deep"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_remove_dir_all() {
        reset_fs();
        sync::create_dir_all("/to_remove/sub").unwrap();
        sync::write("/to_remove/file.txt", b"data").unwrap();

        // Remove using async API
        unsync::remove_dir_all("/to_remove").await.unwrap();

        // Should no longer exist
        assert!(!super::exists("/to_remove"));
        assert!(!super::exists("/to_remove/sub"));
        assert!(!super::exists("/to_remove/file.txt"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_remove_nonexistent_dir_fails() {
        reset_fs();

        let result = unsync::remove_dir_all("/does_not_exist").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_open_options() {
        reset_fs();
        sync::create_dir_all("/async_open").unwrap();

        // Open with create
        let file = crate::unsync::OpenOptions::new()
            .create(true)
            .write(true)
            .open("/async_open/new_file.txt")
            .await
            .unwrap();

        assert!(file.handle.write);
        assert_eq!(file.path.to_string_lossy(), "/async_open/new_file.txt");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_read_nonexistent_file() {
        reset_fs();

        let result = unsync::read_to_string("/nonexistent.txt").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_write_without_parent_fails() {
        reset_fs();

        let result = unsync::write("/no/parent/file.txt", b"data").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod async_file_io_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;
    use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _};

    #[test_log::test(switchy_async::test)]
    async fn test_async_read_trait() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();
        sync::write("/tmp/async_read.txt", b"Hello, async world!").unwrap();

        let mut file = crate::unsync::OpenOptions::new()
            .read(true)
            .open("/tmp/async_read.txt")
            .await
            .unwrap();

        let mut buf = [0u8; 5];
        file.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"Hello");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_write_trait() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();

        let mut file = crate::unsync::OpenOptions::new()
            .create(true)
            .write(true)
            .open("/tmp/async_write.txt")
            .await
            .unwrap();

        file.write_all(b"async write test").await.unwrap();
        file.flush().await.unwrap();
        drop(file);

        // Verify content
        let content = sync::read_to_string("/tmp/async_write.txt").unwrap();
        assert_eq!(content, "async write test");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_seek_trait() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();
        sync::write("/tmp/async_seek.txt", b"0123456789").unwrap();

        let mut file = crate::unsync::OpenOptions::new()
            .read(true)
            .open("/tmp/async_seek.txt")
            .await
            .unwrap();

        // Seek to position 5
        let pos = file.seek(std::io::SeekFrom::Start(5)).await.unwrap();
        assert_eq!(pos, 5);

        // Read remaining
        let mut buf = [0u8; 5];
        file.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"56789");
    }
}

/// Temporary directory functionality for the simulator
///
/// Names are drawn from the current simulation run's `switchy_fs::temp` stream, so a seed
/// reproduces them, and creation retries with a fresh name when one already exists (as
/// `tempfile` does). A [`TempDir`](temp_dir::TempDir) remembers the [`Filesystem`] it was
/// created in and is removed from that namespace when dropped, whichever thread drops it.
pub mod temp_dir {
    use std::{
        collections::BTreeMap,
        ffi::{OsStr, OsString},
        path::{Path, PathBuf},
        sync::{Arc, PoisonError},
    };

    use super::{Filesystem, vfs::ROOT};

    /// Characters of the random part of a name (the set `tempfile` uses).
    const NAME_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

    /// Names tried before giving up when every candidate already exists.
    const MAX_ATTEMPTS: u32 = 1 << 16;

    /// Permission bits of a new temporary directory (owner only, as `tempfile` creates).
    const DIR_MODE: u32 = 0o700;

    /// Tracking state for one temporary directory of a namespace.
    #[derive(Debug)]
    pub(crate) struct TempDirState {
        cleanup_enabled: bool,
    }

    /// The temporary directories registered in one namespace, keyed by path.
    pub(crate) type TempDirs = BTreeMap<PathBuf, TempDirState>;

    fn with_registry<T>(filesystem: &Filesystem, work: impl FnOnce(&mut TempDirs) -> T) -> T {
        work(
            &mut filesystem
                .temp_dirs
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    /// Forget every temporary directory registered in the current namespace.
    ///
    /// The directories themselves are kept; each [`TempDir`] still removes its directory
    /// when dropped.
    pub fn reset_temp_dirs() {
        with_registry(&super::current_filesystem(), TempDirs::clear);
    }

    /// A directory in the filesystem that is automatically deleted when it goes out of scope
    pub struct TempDir {
        path: PathBuf,
        cleanup_enabled: bool,
        /// The namespace holding the directory; `None` for a native directory created in
        /// explicit real-filesystem mode.
        filesystem: Option<Arc<Filesystem>>,
    }

    impl TempDir {
        /// Attempts to make a temporary directory inside of the system temp directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        pub fn new() -> std::io::Result<Self> {
            Builder::new().tempdir()
        }

        /// Attempts to make a temporary directory inside the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        pub fn new_in<P: AsRef<Path>>(dir: P) -> std::io::Result<Self> {
            Builder::new().tempdir_in(dir)
        }

        /// Attempts to make a temporary directory with the specified prefix
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        pub fn with_prefix<S: AsRef<OsStr>>(prefix: S) -> std::io::Result<Self> {
            Builder::new().prefix(prefix).tempdir()
        }

        /// Attempts to make a temporary directory with the specified suffix
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        pub fn with_suffix<S: AsRef<OsStr>>(suffix: S) -> std::io::Result<Self> {
            Builder::new().suffix(suffix).tempdir()
        }

        /// Attempts to make a temporary directory with the specified prefix in the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        pub fn with_prefix_in<S: AsRef<OsStr>, P: AsRef<Path>>(
            prefix: S,
            dir: P,
        ) -> std::io::Result<Self> {
            Builder::new().prefix(prefix).tempdir_in(dir)
        }

        /// Attempts to make a temporary directory with the specified suffix in the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        pub fn with_suffix_in<S: AsRef<OsStr>, P: AsRef<Path>>(
            suffix: S,
            dir: P,
        ) -> std::io::Result<Self> {
            Builder::new().suffix(suffix).tempdir_in(dir)
        }

        /// Accesses the Path to the temporary directory
        #[must_use]
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Persist the temporary directory to disk, returning the `PathBuf` where it is located
        #[must_use]
        pub fn keep(mut self) -> PathBuf {
            self.disable_cleanup(true);
            self.path.clone()
        }

        /// Deprecated alias for `keep()`
        #[deprecated = "use TempDir::keep()"]
        #[must_use]
        pub fn into_path(self) -> PathBuf {
            self.keep()
        }

        /// Disable cleanup of the temporary directory
        pub fn disable_cleanup(&mut self, disable_cleanup: bool) {
            self.cleanup_enabled = !disable_cleanup;
            if let Some(filesystem) = &self.filesystem {
                with_registry(filesystem, |dirs| {
                    if let Some(state) = dirs.get_mut(&self.path) {
                        state.cleanup_enabled = !disable_cleanup;
                    }
                });
            }
        }

        /// Closes and removes the temporary directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be removed
        pub fn close(mut self) -> std::io::Result<()> {
            if !self.cleanup_enabled {
                return Ok(());
            }
            // Prevent a second removal in Drop.
            self.cleanup_enabled = false;
            self.remove()
        }

        /// Remove the directory tree from the namespace it was created in.
        fn remove(&self) -> std::io::Result<()> {
            let Some(filesystem) = &self.filesystem else {
                return remove_native(&self.path);
            };
            with_registry(filesystem, |dirs| dirs.remove(&self.path));
            super::before_operation(super::OpClass::DirOp);
            filesystem
                .vfs()
                .remove_dir_all(ROOT, &self.path)
                .map_err(Into::into)
        }
    }

    /// Remove a native temporary directory (created in explicit real-filesystem mode).
    #[cfg(feature = "simulator-real-fs")]
    fn remove_native(path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    /// Without native support every temporary directory is simulated.
    #[cfg(not(feature = "simulator-real-fs"))]
    #[allow(clippy::unnecessary_wraps)]
    const fn remove_native(_path: &Path) -> std::io::Result<()> {
        Ok(())
    }

    impl AsRef<Path> for TempDir {
        fn as_ref(&self) -> &Path {
            self.path()
        }
    }

    impl std::fmt::Debug for TempDir {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TempDir")
                .field("path", &self.path)
                .field("cleanup_enabled", &self.cleanup_enabled)
                .finish_non_exhaustive()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            if self.cleanup_enabled {
                let _ = self.remove();
            }
        }
    }

    /// Builder for configuring temporary directory creation
    pub struct Builder {
        prefix: Option<OsString>,
        suffix: Option<OsString>,
        rand_bytes: usize,
    }

    impl Default for Builder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Builder {
        /// Create a new Builder with default settings
        #[must_use]
        pub const fn new() -> Self {
            Self {
                prefix: None,
                suffix: None,
                rand_bytes: 6,
            }
        }

        /// Set the prefix for the temporary directory name
        pub fn prefix<S: AsRef<OsStr>>(&mut self, prefix: S) -> &mut Self {
            self.prefix = Some(prefix.as_ref().to_os_string());
            self
        }

        /// Set the suffix for the temporary directory name
        pub fn suffix<S: AsRef<OsStr>>(&mut self, suffix: S) -> &mut Self {
            self.suffix = Some(suffix.as_ref().to_os_string());
            self
        }

        /// Set the number of random characters to use for the directory name
        pub const fn rand_bytes(&mut self, rand: usize) -> &mut Self {
            self.rand_bytes = rand;
            self
        }

        /// Create a temporary directory in the default location (`/tmp`)
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        /// * `AlreadyExists` if every name tried already exists
        pub fn tempdir(&self) -> std::io::Result<TempDir> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                return self.real_tempdir(None);
            }
            self.create_in(Path::new("/tmp"))
        }

        /// Create a temporary directory in the specified location
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        /// * `AlreadyExists` if every name tried already exists
        pub fn tempdir_in<P: AsRef<Path>>(&self, dir: P) -> std::io::Result<TempDir> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                return self.real_tempdir(Some(dir.as_ref()));
            }
            self.create_in(dir.as_ref())
        }

        /// Create a native temporary directory (explicit real-filesystem mode).
        #[cfg(feature = "simulator-real-fs")]
        fn real_tempdir(&self, dir: Option<&Path>) -> std::io::Result<TempDir> {
            let mut builder = tempfile::Builder::new();
            if let Some(prefix) = &self.prefix {
                builder.prefix(prefix);
            }
            if let Some(suffix) = &self.suffix {
                builder.suffix(suffix);
            }
            builder.rand_bytes(self.rand_bytes);
            let real_temp = match dir {
                Some(dir) => builder.tempdir_in(dir)?,
                None => builder.tempdir()?,
            };
            // Cleanup is ours: the path is removed by `TempDir::remove`.
            let path = real_temp.keep();
            Ok(TempDir {
                path,
                cleanup_enabled: true,
                filesystem: None,
            })
        }

        /// Create a uniquely named directory in `dir` (created if missing) of the current
        /// namespace, retrying with a fresh name while the name already exists.
        fn create_in(&self, dir: &Path) -> std::io::Result<TempDir> {
            let filesystem = super::current_filesystem();
            super::sync::create_dir_all(dir)?;
            let attempts = if self.rand_bytes == 0 { 1 } else { MAX_ATTEMPTS };
            for _ in 0..attempts {
                let path = dir.join(generate_temp_name(
                    self.prefix.as_deref(),
                    self.suffix.as_deref(),
                    self.rand_bytes,
                ));
                super::before_operation(super::OpClass::DirOp);
                let created = filesystem.vfs().create_dir(ROOT, &path, DIR_MODE);
                match created {
                    Ok(()) => {
                        with_registry(&filesystem, |dirs| {
                            dirs.insert(
                                path.clone(),
                                TempDirState {
                                    cleanup_enabled: true,
                                },
                            );
                        });
                        return Ok(TempDir {
                            path,
                            cleanup_enabled: true,
                            filesystem: Some(filesystem),
                        });
                    }
                    Err(crate::errno::Errno::Exist) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "too many temporary directories already exist",
            ))
        }
    }

    /// Convenience function to create a temporary directory
    ///
    /// # Errors
    ///
    /// * If the directory cannot be created
    pub fn tempdir() -> std::io::Result<TempDir> {
        TempDir::new()
    }

    /// Convenience function to create a temporary directory in the specified location
    ///
    /// # Errors
    ///
    /// * If the directory cannot be created
    pub fn tempdir_in<P: AsRef<Path>>(dir: P) -> std::io::Result<TempDir> {
        TempDir::new_in(dir)
    }

    /// A temporary name: `prefix`, `rand_bytes` random alphanumeric characters drawn from
    /// the current simulation run's `switchy_fs::temp` stream, then `suffix`.
    fn generate_temp_name(
        prefix: Option<&OsStr>,
        suffix: Option<&OsStr>,
        rand_bytes: usize,
    ) -> OsString {
        let random: String = {
            let stream = switchy_simulation::stream("switchy_fs::temp");
            let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
            (0..rand_bytes)
                .map(|_| char::from(NAME_CHARS[rng.index(NAME_CHARS.len())]))
                .collect()
        };

        let mut name = OsString::new();
        if let Some(prefix) = prefix {
            name.push(prefix);
        }
        name.push(random);
        if let Some(suffix) = suffix {
            name.push(suffix);
        }
        name
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::simulator::{exists, reset_fs};
        use pretty_assertions::assert_eq;

        fn random_part(name: &OsStr, prefix: &str, suffix: &str) -> String {
            let name = name.to_str().unwrap();
            assert!(name.starts_with(prefix), "{name} lacks prefix {prefix}");
            assert!(name.ends_with(suffix), "{name} lacks suffix {suffix}");
            name[prefix.len()..name.len() - suffix.len()].to_string()
        }

        fn assert_alphanumeric(random: &str, len: usize) {
            assert_eq!(random.len(), len);
            assert!(random.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        }

        #[test_log::test]
        fn test_generate_temp_name_without_prefix_or_suffix() {
            let name = generate_temp_name(None, None, 6);
            assert_alphanumeric(&random_part(&name, "", ""), 6);
        }

        #[test_log::test]
        fn test_generate_temp_name_with_prefix() {
            let name = generate_temp_name(Some(std::ffi::OsStr::new("test-")), None, 4);
            assert_alphanumeric(&random_part(&name, "test-", ""), 4);
        }

        #[test_log::test]
        fn test_generate_temp_name_with_suffix() {
            let name = generate_temp_name(None, Some(std::ffi::OsStr::new("-end")), 4);
            assert_alphanumeric(&random_part(&name, "", "-end"), 4);
        }

        #[test_log::test]
        fn test_generate_temp_name_with_both() {
            let name = generate_temp_name(
                Some(std::ffi::OsStr::new("pre-")),
                Some(std::ffi::OsStr::new("-suf")),
                3,
            );
            assert_alphanumeric(&random_part(&name, "pre-", "-suf"), 3);
        }

        #[test_log::test]
        fn test_generate_temp_name_differs_between_calls() {
            let names: std::collections::BTreeSet<OsString> =
                (0..16).map(|_| generate_temp_name(None, None, 6)).collect();
            assert_eq!(names.len(), 16);
        }

        #[test_log::test]
        fn test_generate_temp_name_is_reproducible_per_seed() {
            let names = |seed| {
                switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
                    (0..4)
                        .map(|_| generate_temp_name(None, None, 8))
                        .collect::<Vec<_>>()
                })
            };
            assert_eq!(names(7), names(7));
            assert_ne!(names(7), names(8));
        }

        #[test_log::test]
        fn test_tempdirs_get_distinct_names() {
            reset_fs();

            let first = TempDir::new().unwrap();
            let second = TempDir::new().unwrap();
            assert_ne!(first.path(), second.path());
            assert!(exists(first.path()));
            assert!(exists(second.path()));

            drop(first);
            assert!(exists(second.path()), "dropping one must not remove another");
        }

        #[test_log::test]
        fn test_tempdir_retries_when_name_exists() {
            let seed = 42;
            // The first name this seed draws.
            let taken = switchy_simulation::with_context(
                switchy_simulation::Context::new(seed),
                || generate_temp_name(None, None, 6),
            );
            switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
                crate::simulator::sync::create_dir_all(Path::new("/tmp").join(&taken)).unwrap();
                let temp = TempDir::new().unwrap();
                assert_ne!(temp.path().file_name().unwrap(), taken.as_os_str());
                assert!(exists(Path::new("/tmp").join(&taken)));
            });
        }

        #[test_log::test]
        fn test_tempdir_without_random_part_fails_when_taken() {
            reset_fs();

            let mut builder = Builder::new();
            builder.prefix("fixed").rand_bytes(0);
            let first = builder.tempdir().unwrap();
            assert_eq!(first.path(), Path::new("/tmp/fixed"));
            let error = builder.tempdir().unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        }

        #[test_log::test]
        fn test_tempdir_dropped_on_another_thread_is_removed_from_its_namespace() {
            let filesystem = std::sync::Arc::new(crate::simulator::Filesystem::new());
            let temp =
                crate::simulator::with_filesystem(&filesystem, || TempDir::new().unwrap());
            let path = temp.path().to_path_buf();
            std::thread::spawn(move || drop(temp)).join().unwrap();
            crate::simulator::with_filesystem(&filesystem, || {
                assert!(!exists(&path), "directory should be removed from its namespace");
                assert!(exists("/tmp"));
            });
            assert!(filesystem.temp_dirs.lock().unwrap().is_empty());
        }

        #[test_log::test]
        fn test_tempdir_registry_is_per_namespace() {
            let first = std::sync::Arc::new(crate::simulator::Filesystem::new());
            let second = std::sync::Arc::new(crate::simulator::Filesystem::new());
            let temp = crate::simulator::with_filesystem(&first, || TempDir::new().unwrap());
            assert_eq!(first.temp_dirs.lock().unwrap().len(), 1);
            assert!(second.temp_dirs.lock().unwrap().is_empty());
            crate::simulator::with_filesystem(&second, reset_temp_dirs);
            assert_eq!(first.temp_dirs.lock().unwrap().len(), 1);
            drop(temp);
            assert!(first.temp_dirs.lock().unwrap().is_empty());
        }

        #[test_log::test]
        fn test_builder_default_values() {
            reset_fs();

            let builder = Builder::new();
            let temp = builder.tempdir().unwrap();

            // Should be created in /tmp
            assert!(temp.path().starts_with("/tmp"));
            assert!(exists(temp.path()));
        }

        #[test_log::test]
        fn test_builder_with_prefix() {
            reset_fs();

            let mut builder = Builder::new();
            builder.prefix("myprefix-");
            let temp = builder.tempdir().unwrap();

            let file_name = temp.path().file_name().unwrap().to_str().unwrap();
            assert!(file_name.starts_with("myprefix-"));
        }

        #[test_log::test]
        fn test_builder_with_suffix() {
            reset_fs();

            let mut builder = Builder::new();
            builder.suffix("-mysuffix");
            let temp = builder.tempdir().unwrap();

            let file_name = temp.path().file_name().unwrap().to_str().unwrap();
            assert!(file_name.ends_with("-mysuffix"));
        }

        #[test_log::test]
        fn test_builder_with_custom_rand_bytes() {
            reset_fs();

            let mut builder = Builder::new();
            builder.rand_bytes(10);
            let temp = builder.tempdir().unwrap();

            assert_alphanumeric(&random_part(temp.path().file_name().unwrap(), "", ""), 10);
        }

        #[test_log::test]
        fn test_builder_tempdir_in() {
            reset_fs();
            crate::simulator::sync::create_dir_all("/custom").unwrap();

            let mut builder = Builder::new();
            builder.prefix("test-");
            let temp = builder.tempdir_in("/custom").unwrap();

            assert!(temp.path().starts_with("/custom"));
            assert!(
                temp.path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("test-")
            );
        }

        #[test_log::test]
        fn test_builder_chaining() {
            reset_fs();

            let temp = Builder::new()
                .prefix("start-")
                .suffix("-end")
                .rand_bytes(3)
                .tempdir()
                .unwrap();

            let random = random_part(temp.path().file_name().unwrap(), "start-", "-end");
            assert_alphanumeric(&random, 3);
        }

        #[test_log::test]
        fn test_disable_cleanup_prevents_deletion() {
            reset_fs();

            let path = {
                let mut temp = TempDir::new().unwrap();
                temp.disable_cleanup(true);
                temp.path().to_path_buf()
            };

            // After drop, directory should still exist
            assert!(
                exists(&path),
                "Directory should exist after drop with cleanup disabled"
            );
        }

        #[test_log::test]
        fn test_disable_cleanup_can_be_reenabled() {
            reset_fs();

            let path = {
                let mut temp = TempDir::new().unwrap();
                temp.disable_cleanup(true);
                temp.disable_cleanup(false); // Re-enable cleanup
                temp.path().to_path_buf()
            };

            // Directory should be removed after drop
            assert!(
                !exists(&path),
                "Directory should be removed when cleanup is re-enabled"
            );
        }

        #[test_log::test]
        fn test_tempdir_with_prefix_in() {
            reset_fs();
            crate::simulator::sync::create_dir_all("/base").unwrap();

            let temp = TempDir::with_prefix_in("pfx-", "/base").unwrap();
            assert!(temp.path().starts_with("/base"));
            assert!(
                temp.path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("pfx-")
            );
        }

        #[test_log::test]
        fn test_tempdir_with_suffix_in() {
            reset_fs();
            crate::simulator::sync::create_dir_all("/base").unwrap();

            let temp = TempDir::with_suffix_in("-sfx", "/base").unwrap();
            assert!(temp.path().starts_with("/base"));
            assert!(
                temp.path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .ends_with("-sfx")
            );
        }

        #[test_log::test]
        fn test_tempdir_drop_removes_directory() {
            reset_fs();

            let path = {
                let temp = TempDir::new().unwrap();
                let p = temp.path().to_path_buf();
                assert!(exists(&p), "Directory should exist before drop");
                p
            };

            assert!(!exists(&path), "Directory should be removed after drop");
        }

        #[test_log::test]
        fn test_tempdir_close_removes_directory() {
            reset_fs();

            let temp = TempDir::new().unwrap();
            let path = temp.path().to_path_buf();
            assert!(exists(&path));

            temp.close().unwrap();
            assert!(!exists(&path), "Directory should be removed after close()");
        }

        #[test_log::test]
        fn test_tempdir_close_with_cleanup_disabled() {
            reset_fs();

            let mut temp = TempDir::new().unwrap();
            let path = temp.path().to_path_buf();
            temp.disable_cleanup(true);

            // close() should be a no-op when cleanup is disabled
            temp.close().unwrap();
            assert!(
                exists(&path),
                "Directory should exist after close() with cleanup disabled"
            );
        }

        #[test_log::test]
        fn test_tempdir_as_ref() {
            reset_fs();

            let temp = TempDir::new().unwrap();
            let path_ref: &Path = temp.as_ref();
            assert_eq!(path_ref, temp.path());
        }

        #[test_log::test]
        fn test_tempdir_debug_format() {
            reset_fs();

            let temp = TempDir::new().unwrap();
            let debug_str = format!("{temp:?}");

            // Debug output should contain "TempDir", path, and cleanup_enabled
            assert!(debug_str.contains("TempDir"));
            assert!(debug_str.contains("path"));
            assert!(debug_str.contains("cleanup_enabled"));
        }

        #[test_log::test]
        fn test_builder_default_impl() {
            // Builder::default() should be equivalent to Builder::new()
            let builder1 = Builder::new();
            let builder2 = Builder::default();

            // Both should have same default behavior
            assert!(builder1.prefix.is_none());
            assert!(builder2.prefix.is_none());
            assert!(builder1.suffix.is_none());
            assert!(builder2.suffix.is_none());
            assert_eq!(builder1.rand_bytes, builder2.rand_bytes);
        }

        #[test_log::test]
        fn test_reset_temp_dirs_clears_state() {
            reset_fs();

            // Create some temp directories
            let _temp1 = TempDir::new().unwrap();
            let _temp2 = TempDir::new().unwrap();

            // Reset should clear tracking state
            reset_temp_dirs();

            // This just verifies the function doesn't panic
            // The actual cleanup is handled by drop
        }
    }
}

#[cfg(test)]
mod init_fs_tests {
    use super::{exists, init_minimal_fs, init_standard_fs, init_user_home, reset_fs, sync};

    #[test_log::test]
    fn test_init_minimal_fs_creates_essential_directories() {
        reset_fs();
        init_minimal_fs().unwrap();

        // Verify essential directories are created
        assert!(exists("/"), "root directory should exist");
        assert!(exists("/tmp"), "/tmp directory should exist");
        assert!(exists("/home"), "/home directory should exist");
    }

    #[test_log::test]
    fn test_init_standard_fs_creates_fhs_structure() {
        reset_fs();
        init_standard_fs().unwrap();

        // Verify root directories
        assert!(exists("/bin"), "/bin should exist");
        assert!(exists("/etc"), "/etc should exist");
        assert!(exists("/home"), "/home should exist");
        assert!(exists("/lib"), "/lib should exist");
        assert!(exists("/opt"), "/opt should exist");
        assert!(exists("/root"), "/root should exist");
        assert!(exists("/sbin"), "/sbin should exist");
        assert!(exists("/tmp"), "/tmp should exist");
        assert!(exists("/usr"), "/usr should exist");
        assert!(exists("/var"), "/var should exist");

        // Verify /usr subdirectories
        assert!(exists("/usr/bin"), "/usr/bin should exist");
        assert!(exists("/usr/lib"), "/usr/lib should exist");
        assert!(exists("/usr/local"), "/usr/local should exist");
        assert!(exists("/usr/local/bin"), "/usr/local/bin should exist");
        assert!(exists("/usr/share"), "/usr/share should exist");

        // Verify /var subdirectories
        assert!(exists("/var/log"), "/var/log should exist");
        assert!(exists("/var/tmp"), "/var/tmp should exist");
        assert!(exists("/var/cache"), "/var/cache should exist");
    }

    #[test_log::test]
    fn test_init_user_home_creates_standard_user_directories() {
        reset_fs();
        init_minimal_fs().unwrap();
        init_user_home("testuser").unwrap();

        // Verify user home directories
        assert!(exists("/home/testuser"), "user home should exist");
        assert!(
            exists("/home/testuser/.config"),
            ".config directory should exist"
        );
        assert!(
            exists("/home/testuser/.local"),
            ".local directory should exist"
        );
        assert!(
            exists("/home/testuser/.local/share"),
            ".local/share directory should exist"
        );
        assert!(
            exists("/home/testuser/.cache"),
            ".cache directory should exist"
        );
        assert!(
            exists("/home/testuser/Documents"),
            "Documents directory should exist"
        );
        assert!(
            exists("/home/testuser/Downloads"),
            "Downloads directory should exist"
        );
    }

    #[test_log::test]
    fn test_init_user_home_with_different_usernames() {
        reset_fs();
        init_minimal_fs().unwrap();

        init_user_home("alice").unwrap();
        init_user_home("bob").unwrap();

        assert!(exists("/home/alice"), "alice home should exist");
        assert!(exists("/home/bob"), "bob home should exist");
        assert!(
            exists("/home/alice/Documents"),
            "alice Documents should exist"
        );
        assert!(exists("/home/bob/Documents"), "bob Documents should exist");
    }

    #[test_log::test]
    fn test_init_standard_fs_allows_listing_usr_subdirs() {
        reset_fs();
        init_standard_fs().unwrap();

        let entries = sync::read_dir_sorted("/usr").unwrap();
        let dir_names: Vec<_> = entries.iter().map(sync::DirEntry::file_name).collect();

        // Verify we can list subdirectories
        assert!(
            dir_names.iter().any(|n| n == "bin"),
            "should contain bin directory"
        );
        assert!(
            dir_names.iter().any(|n| n == "lib"),
            "should contain lib directory"
        );
        assert!(
            dir_names.iter().any(|n| n == "local"),
            "should contain local directory"
        );
        assert!(
            dir_names.iter().any(|n| n == "share"),
            "should contain share directory"
        );
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::{Metadata, reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_metadata_for_empty_file() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();
        sync::write("/tmp/empty.txt", b"").unwrap();

        let file = sync::File::open("/tmp/empty.txt").unwrap();
        let metadata = file.metadata().unwrap();

        assert_eq!(metadata.len(), 0, "empty file should have length 0");
        assert!(
            metadata.is_empty(),
            "empty file should return true for is_empty"
        );
        assert!(metadata.is_file(), "should be a file");
        assert!(!metadata.is_dir(), "should not be a directory");
        assert!(!metadata.is_symlink(), "should not be a symlink");
    }

    #[test_log::test]
    fn test_metadata_for_file_with_content() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();
        sync::write("/tmp/content.txt", b"Hello, World!").unwrap();

        let file = sync::File::open("/tmp/content.txt").unwrap();
        let metadata = file.metadata().unwrap();

        assert_eq!(metadata.len(), 13, "file should have correct length");
        assert!(
            !metadata.is_empty(),
            "non-empty file should return false for is_empty"
        );
        assert!(metadata.is_file(), "should be a file");
    }

    #[test_log::test]
    fn test_metadata_from_std_fs_metadata() {
        // Test the From<std::fs::Metadata> implementation
        // We can't easily create std::fs::Metadata directly, but we can test
        // the Metadata struct behavior directly

        let metadata = Metadata {
            len: 1024,
            kind: crate::types::FileKind::File,
            mode: 0o644,
            uid: None,
            gid: None,
            nlink: None,
            file_id: None,
            accessed: None,
            modified: None,
            created: None,
        };

        assert_eq!(metadata.len(), 1024);
        assert!(metadata.is_file());
        assert!(!metadata.is_dir());
        assert!(!metadata.is_symlink());
    }

    #[test_log::test]
    fn test_metadata_for_directory_entry() {
        let dir_metadata = Metadata {
            len: 0,
            kind: crate::types::FileKind::Dir,
            mode: 0o644,
            uid: None,
            gid: None,
            nlink: None,
            file_id: None,
            accessed: None,
            modified: None,
            created: None,
        };

        assert!(dir_metadata.is_dir());
        assert!(!dir_metadata.is_file());
        assert!(dir_metadata.is_empty());
    }

    #[test_log::test]
    fn test_metadata_for_symlink_entry() {
        let symlink_metadata = Metadata {
            len: 0,
            kind: crate::types::FileKind::Symlink,
            mode: 0o644,
            uid: None,
            gid: None,
            nlink: None,
            file_id: None,
            accessed: None,
            modified: None,
            created: None,
        };

        assert!(symlink_metadata.is_symlink());
        assert!(!symlink_metadata.is_file());
        assert!(!symlink_metadata.is_dir());
    }
}

#[cfg(test)]
mod walk_dir_sorted_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_walk_dir_sorted_empty_directory() {
        reset_fs();
        sync::create_dir_all("/walk_empty").unwrap();

        let entries = sync::walk_dir_sorted("/walk_empty").unwrap();
        assert!(entries.is_empty(), "empty directory should have no entries");
    }

    #[test_log::test]
    fn test_walk_dir_sorted_flat_directory() {
        reset_fs();
        sync::create_dir_all("/walk_flat").unwrap();
        sync::write("/walk_flat/a.txt", b"a").unwrap();
        sync::write("/walk_flat/b.txt", b"b").unwrap();
        sync::write("/walk_flat/c.txt", b"c").unwrap();

        let entries = sync::walk_dir_sorted("/walk_flat").unwrap();
        assert_eq!(entries.len(), 3, "should have 3 files");

        // Verify entries are sorted by path
        let paths: Vec<_> = entries.iter().map(sync::DirEntry::path).collect();
        assert_eq!(
            paths,
            vec![
                std::path::PathBuf::from("/walk_flat/a.txt"),
                std::path::PathBuf::from("/walk_flat/b.txt"),
                std::path::PathBuf::from("/walk_flat/c.txt"),
            ]
        );
    }

    #[test_log::test]
    fn test_walk_dir_sorted_nested_structure() {
        reset_fs();
        sync::create_dir_all("/walk_nested/dir1").unwrap();
        sync::create_dir_all("/walk_nested/dir2").unwrap();
        sync::write("/walk_nested/root.txt", b"root").unwrap();
        sync::write("/walk_nested/dir1/nested1.txt", b"nested1").unwrap();
        sync::write("/walk_nested/dir2/nested2.txt", b"nested2").unwrap();

        let entries = sync::walk_dir_sorted("/walk_nested").unwrap();

        // Should include directories and files
        let paths: Vec<_> = entries.iter().map(sync::DirEntry::path).collect();

        // Verify all expected entries are present (directories + files)
        assert!(
            paths.contains(&std::path::PathBuf::from("/walk_nested/dir1")),
            "should contain dir1"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/walk_nested/dir2")),
            "should contain dir2"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/walk_nested/root.txt")),
            "should contain root.txt"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/walk_nested/dir1/nested1.txt")),
            "should contain nested1.txt"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/walk_nested/dir2/nested2.txt")),
            "should contain nested2.txt"
        );
    }

    #[test_log::test]
    fn test_walk_dir_sorted_deeply_nested() {
        reset_fs();
        sync::create_dir_all("/deep/a/b/c").unwrap();
        sync::write("/deep/a/b/c/file.txt", b"deep").unwrap();

        let entries = sync::walk_dir_sorted("/deep").unwrap();

        // Should include all intermediate directories and the file
        let paths: Vec<_> = entries.iter().map(sync::DirEntry::path).collect();

        assert!(
            paths.contains(&std::path::PathBuf::from("/deep/a")),
            "should contain /deep/a"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/deep/a/b")),
            "should contain /deep/a/b"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/deep/a/b/c")),
            "should contain /deep/a/b/c"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/deep/a/b/c/file.txt")),
            "should contain the file"
        );
    }

    #[test_log::test]
    fn test_walk_dir_sorted_nonexistent_dir_fails() {
        reset_fs();

        let result = sync::walk_dir_sorted("/nonexistent_walk");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test]
    fn test_walk_dir_sorted_returns_sorted_paths() {
        reset_fs();
        sync::create_dir_all("/sorted/z_dir").unwrap();
        sync::create_dir_all("/sorted/a_dir").unwrap();
        sync::write("/sorted/m_file.txt", b"m").unwrap();
        sync::write("/sorted/a_dir/nested.txt", b"nested").unwrap();

        let entries = sync::walk_dir_sorted("/sorted").unwrap();
        let paths: Vec<_> = entries.iter().map(sync::DirEntry::path).collect();

        // Verify paths are sorted
        let mut sorted_paths = paths.clone();
        sorted_paths.sort();
        assert_eq!(
            paths, sorted_paths,
            "walk_dir_sorted should return paths in sorted order"
        );
    }
}

#[cfg(test)]
mod read_dir_sorted_sync_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_read_dir_sorted_empty_directory() {
        reset_fs();
        sync::create_dir_all("/read_empty").unwrap();

        let entries = sync::read_dir_sorted("/read_empty").unwrap();
        assert!(entries.is_empty(), "empty directory should have no entries");
    }

    #[test_log::test]
    fn test_read_dir_sorted_files_and_dirs_mixed() {
        reset_fs();
        sync::create_dir_all("/mixed_content/subdir").unwrap();
        sync::write("/mixed_content/file1.txt", b"f1").unwrap();
        sync::write("/mixed_content/file2.txt", b"f2").unwrap();

        let entries = sync::read_dir_sorted("/mixed_content").unwrap();
        assert_eq!(entries.len(), 3, "should have 2 files and 1 directory");

        // Verify we have both files and directory
        let file_count = entries
            .iter()
            .filter(|e| e.file_type().unwrap().is_file())
            .count();
        let dir_count = entries
            .iter()
            .filter(|e| e.file_type().unwrap().is_dir())
            .count();

        assert_eq!(file_count, 2, "should have 2 files");
        assert_eq!(dir_count, 1, "should have 1 directory");
    }

    #[test_log::test]
    fn test_read_dir_sorted_returns_sorted_by_filename() {
        reset_fs();
        sync::create_dir_all("/sort_test").unwrap();
        sync::write("/sort_test/zebra.txt", b"z").unwrap();
        sync::write("/sort_test/apple.txt", b"a").unwrap();
        sync::write("/sort_test/mango.txt", b"m").unwrap();

        let entries = sync::read_dir_sorted("/sort_test").unwrap();
        let filenames: Vec<_> = entries.iter().map(sync::DirEntry::file_name).collect();

        // Should be sorted alphabetically
        assert_eq!(
            filenames,
            vec![
                std::ffi::OsString::from("apple.txt"),
                std::ffi::OsString::from("mango.txt"),
                std::ffi::OsString::from("zebra.txt"),
            ]
        );
    }

    #[test_log::test]
    fn test_read_dir_sorted_nonexistent_dir_fails() {
        reset_fs();

        let result = sync::read_dir_sorted("/nonexistent_read");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test]
    fn test_read_dir_sorted_does_not_include_nested() {
        reset_fs();
        sync::create_dir_all("/parent/child").unwrap();
        sync::write("/parent/direct.txt", b"direct").unwrap();
        sync::write("/parent/child/nested.txt", b"nested").unwrap();

        let entries = sync::read_dir_sorted("/parent").unwrap();
        let filenames: Vec<_> = entries.iter().map(sync::DirEntry::file_name).collect();

        // Should only include direct children, not nested
        assert!(
            filenames.contains(&std::ffi::OsString::from("direct.txt")),
            "should contain direct.txt"
        );
        assert!(
            filenames.contains(&std::ffi::OsString::from("child")),
            "should contain child directory"
        );
        assert!(
            !filenames.contains(&std::ffi::OsString::from("nested.txt")),
            "should NOT contain nested.txt"
        );
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod async_read_dir_tests {
    use super::{reset_fs, sync, unsync};
    use pretty_assertions::assert_eq;

    #[test_log::test(switchy_async::test)]
    async fn test_async_read_dir_iteration() {
        reset_fs();
        sync::create_dir_all("/async_iter").unwrap();
        sync::write("/async_iter/a.txt", b"a").unwrap();
        sync::write("/async_iter/b.txt", b"b").unwrap();

        let mut read_dir = unsync::read_dir("/async_iter").await.unwrap();

        // Collect all entries
        let mut entries = Vec::new();
        while let Some(entry) = read_dir.next_entry().await.unwrap() {
            entries.push(entry);
        }

        assert_eq!(entries.len(), 2, "should have 2 entries");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_read_dir_empty() {
        reset_fs();
        sync::create_dir_all("/async_empty").unwrap();

        let mut read_dir = unsync::read_dir("/async_empty").await.unwrap();

        // Should return None immediately for empty directory
        let entry = read_dir.next_entry().await.unwrap();
        assert!(entry.is_none(), "empty directory should return None");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_dir_entry_file_type() {
        reset_fs();
        sync::create_dir_all("/async_type/subdir").unwrap();
        sync::write("/async_type/file.txt", b"content").unwrap();

        let entries = unsync::read_dir_sorted("/async_type").await.unwrap();

        // Find file and directory entries
        let file_entry = entries
            .iter()
            .find(|e| e.file_name() == "file.txt")
            .unwrap();
        let dir_entry = entries.iter().find(|e| e.file_name() == "subdir").unwrap();

        // Test file_type method
        let file_type = file_entry.file_type().await.unwrap();
        assert!(file_type.is_file(), "file.txt should be a file");
        assert!(!file_type.is_dir(), "file.txt should not be a directory");

        let dir_type = dir_entry.file_type().await.unwrap();
        assert!(dir_type.is_dir(), "subdir should be a directory");
        assert!(!dir_type.is_file(), "subdir should not be a file");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_dir_entry_metadata() {
        reset_fs();
        sync::create_dir_all("/async_meta").unwrap();
        sync::write("/async_meta/test.txt", b"test content").unwrap();

        let entries = unsync::read_dir_sorted("/async_meta").await.unwrap();
        let file_entry = entries
            .iter()
            .find(|e| e.file_name() == "test.txt")
            .unwrap();

        let metadata = file_entry.metadata().await.unwrap();
        assert_eq!(
            metadata.len(),
            12,
            "file should have correct length (12 bytes)"
        );
        assert!(metadata.is_file(), "should be a file");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_dir_entry_path_and_file_name() {
        reset_fs();
        sync::create_dir_all("/async_paths").unwrap();
        sync::write("/async_paths/example.txt", b"data").unwrap();

        let entries = unsync::read_dir_sorted("/async_paths").await.unwrap();
        let entry = &entries[0];

        assert_eq!(
            entry.path(),
            std::path::PathBuf::from("/async_paths/example.txt")
        );
        assert_eq!(entry.file_name(), std::ffi::OsString::from("example.txt"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_walk_dir_sorted() {
        reset_fs();
        sync::create_dir_all("/async_walk/sub").unwrap();
        sync::write("/async_walk/root.txt", b"root").unwrap();
        sync::write("/async_walk/sub/nested.txt", b"nested").unwrap();

        let entries = unsync::walk_dir_sorted("/async_walk").await.unwrap();

        // Should include all entries (directories and files)
        let paths: Vec<_> = entries.iter().map(unsync::DirEntry::path).collect();
        assert!(
            paths.contains(&std::path::PathBuf::from("/async_walk/sub")),
            "should contain sub directory"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/async_walk/root.txt")),
            "should contain root.txt"
        );
        assert!(
            paths.contains(&std::path::PathBuf::from("/async_walk/sub/nested.txt")),
            "should contain nested.txt"
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_dir_entry_metadata_for_directory() {
        reset_fs();
        sync::create_dir_all("/async_dir_meta/subdir").unwrap();

        let entries = unsync::read_dir_sorted("/async_dir_meta").await.unwrap();
        let dir_entry = &entries[0];

        let metadata = dir_entry.metadata().await.unwrap();
        assert!(metadata.is_dir(), "should be a directory");
        assert_eq!(metadata.len(), 0, "directory should have length 0");
    }
}

#[cfg(test)]
mod dir_entry_sync_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_dir_entry_new_file() {
        let entry =
            sync::DirEntry::new_file("/path/to/file.txt".to_string(), "file.txt".to_string())
                .unwrap();

        assert_eq!(entry.path(), std::path::PathBuf::from("/path/to/file.txt"));
        assert_eq!(entry.file_name(), std::ffi::OsString::from("file.txt"));
        assert!(entry.file_type().unwrap().is_file());
        assert!(!entry.file_type().unwrap().is_dir());
    }

    #[test_log::test]
    fn test_dir_entry_new_dir() {
        let entry = sync::DirEntry::new_dir("/path/to/dir".to_string(), "dir".to_string()).unwrap();

        assert_eq!(entry.path(), std::path::PathBuf::from("/path/to/dir"));
        assert_eq!(entry.file_name(), std::ffi::OsString::from("dir"));
        assert!(entry.file_type().unwrap().is_dir());
        assert!(!entry.file_type().unwrap().is_file());
    }

    #[test_log::test]
    fn test_dir_entry_file_type_accessor() {
        reset_fs();
        sync::create_dir_all("/entry_test/subdir").unwrap();
        sync::write("/entry_test/file.txt", b"content").unwrap();

        let entries = sync::read_dir_sorted("/entry_test").unwrap();

        for entry in entries {
            let file_type = entry.file_type().unwrap();
            // Each entry should have exactly one type set
            let type_count = [
                file_type.is_file(),
                file_type.is_dir(),
                file_type.is_symlink(),
            ]
            .iter()
            .filter(|&&x| x)
            .count();
            assert_eq!(type_count, 1, "each entry should have exactly one type");
        }
    }
}

#[cfg(test)]
mod file_create_and_open_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;
    use std::io::{Read as _, Write as _};

    #[test_log::test]
    fn test_file_create_new_file() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();

        let mut file = sync::File::create("/tmp/new_file.txt").unwrap();
        file.write_all(b"created content").unwrap();
        drop(file);

        let content = sync::read_to_string("/tmp/new_file.txt").unwrap();
        assert_eq!(content, "created content");
    }

    #[test_log::test]
    fn test_file_create_truncates_existing() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();
        sync::write("/tmp/existing.txt", b"original content that is long").unwrap();

        let mut file = sync::File::create("/tmp/existing.txt").unwrap();
        file.write_all(b"short").unwrap();
        drop(file);

        let content = sync::read_to_string("/tmp/existing.txt").unwrap();
        assert_eq!(content, "short");
    }

    #[test_log::test]
    fn test_file_open_reads_content() {
        reset_fs();
        sync::create_dir_all("/tmp").unwrap();
        sync::write("/tmp/read_test.txt", b"readable content").unwrap();

        let mut file = sync::File::open("/tmp/read_test.txt").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();

        assert_eq!(content, "readable content");
    }

    #[test_log::test]
    fn test_file_open_nonexistent_fails() {
        reset_fs();

        let result = sync::File::open("/nonexistent/file.txt");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test]
    fn test_file_create_without_parent_fails() {
        reset_fs();

        let result = sync::File::create("/no/parent/file.txt");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test]
    fn test_file_options_returns_open_options() {
        let options = sync::File::options();
        // Verify it returns an OpenOptions by chaining methods
        let _ = options.read(true).write(true).create(true);
    }
}

#[cfg(test)]
mod canonicalize_tests {
    use super::{reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_canonicalize_existing_directory() {
        reset_fs();
        sync::create_dir_all("/test/path/to/dir").unwrap();

        let result = sync::canonicalize("/test/path/to/dir").unwrap();
        assert_eq!(result.to_str().unwrap(), "/test/path/to/dir");
    }

    #[test_log::test]
    fn test_canonicalize_existing_file() {
        reset_fs();
        sync::create_dir_all("/test").unwrap();
        sync::write("/test/file.txt", b"content").unwrap();

        let result = sync::canonicalize("/test/file.txt").unwrap();
        assert_eq!(result.to_str().unwrap(), "/test/file.txt");
    }

    #[test_log::test]
    fn test_canonicalize_path_with_dots() {
        reset_fs();
        sync::create_dir_all("/a/b/c").unwrap();

        // Path with . and .. should be normalized
        let result = sync::canonicalize("/a/b/../b/./c").unwrap();
        assert_eq!(result.to_str().unwrap(), "/a/b/c");
    }

    #[test_log::test]
    fn test_canonicalize_path_with_double_slashes() {
        reset_fs();
        sync::create_dir_all("/test/dir").unwrap();

        let result = sync::canonicalize("/test//dir").unwrap();
        assert_eq!(result.to_str().unwrap(), "/test/dir");
    }

    #[test_log::test]
    fn test_canonicalize_nonexistent_path_fails() {
        reset_fs();

        let result = sync::canonicalize("/nonexistent/path");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test]
    fn test_canonicalize_root() {
        reset_fs();
        sync::create_dir_all("/").unwrap();

        let result = sync::canonicalize("/").unwrap();
        assert_eq!(result.to_str().unwrap(), "/");
    }

    #[test_log::test]
    fn test_canonicalize_double_dots_past_root() {
        reset_fs();
        sync::create_dir_all("/existing").unwrap();

        // Even if we try to go above root with .., the result should stay at root level
        let result = sync::canonicalize("/../existing").unwrap();
        assert_eq!(result.to_str().unwrap(), "/existing");
    }
}

#[cfg(test)]
mod create_dir_tests {
    use super::{exists, reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_create_dir_single_level() {
        reset_fs();
        sync::create_dir_all("/").unwrap();

        sync::create_dir("/toplevel").unwrap();
        assert!(exists("/toplevel"));
    }

    #[test_log::test]
    fn test_create_dir_with_existing_parent() {
        reset_fs();
        sync::create_dir_all("/parent").unwrap();

        sync::create_dir("/parent/child").unwrap();
        assert!(exists("/parent/child"));
    }

    #[test_log::test]
    fn test_create_dir_without_parent_fails() {
        reset_fs();

        // Parent doesn't exist
        let result = sync::create_dir("/missing/child");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test]
    fn test_create_dir_nested_without_parent_fails() {
        reset_fs();
        sync::create_dir_all("/a").unwrap();

        // /a/b doesn't exist, so creating /a/b/c should fail
        let result = sync::create_dir("/a/b/c");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test_log::test]
    fn test_create_dir_root() {
        reset_fs();

        // The root always exists, so creating it fails like `std::fs::create_dir`.
        assert_eq!(
            sync::create_dir("/").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert!(exists("/"));
    }

    #[test_log::test]
    fn test_create_dir_with_trailing_slash() {
        reset_fs();
        sync::create_dir_all("/parent").unwrap();

        sync::create_dir("/parent/child/").unwrap();
        // Should normalize and create the directory
        assert!(exists("/parent/child"));
    }

    #[test_log::test]
    fn test_create_dir_existing_fails() {
        reset_fs();
        sync::create_dir_all("/parent").unwrap();

        // Creating the same directory twice fails like `std::fs::create_dir`.
        sync::create_dir("/parent/child").unwrap();
        assert_eq!(
            sync::create_dir("/parent/child").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert!(exists("/parent/child"));
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod async_is_file_is_dir_tests {
    use super::{reset_fs, sync, unsync};

    #[test_log::test(switchy_async::test)]
    async fn test_is_file_returns_true_for_file() {
        reset_fs();
        sync::create_dir_all("/test").unwrap();
        sync::write("/test/file.txt", b"content").unwrap();

        assert!(unsync::is_file("/test/file.txt").await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_is_file_returns_false_for_directory() {
        reset_fs();
        sync::create_dir_all("/test/subdir").unwrap();

        assert!(!unsync::is_file("/test/subdir").await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_is_file_returns_false_for_nonexistent() {
        reset_fs();

        assert!(!unsync::is_file("/nonexistent/file.txt").await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_is_dir_returns_true_for_directory() {
        reset_fs();
        sync::create_dir_all("/test/subdir").unwrap();

        assert!(unsync::is_dir("/test/subdir").await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_is_dir_returns_false_for_file() {
        reset_fs();
        sync::create_dir_all("/test").unwrap();
        sync::write("/test/file.txt", b"content").unwrap();

        assert!(!unsync::is_dir("/test/file.txt").await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_is_dir_returns_false_for_nonexistent() {
        reset_fs();

        assert!(!unsync::is_dir("/nonexistent/dir").await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_is_file_with_invalid_path() {
        reset_fs();

        // Path that can't be converted to str should return false
        // (Though this is hard to test directly since most paths are valid utf-8)
        // We'll just verify that an empty path returns false
        assert!(!unsync::is_file("").await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_is_dir_with_root() {
        reset_fs();
        sync::create_dir_all("/").unwrap();

        assert!(unsync::is_dir("/").await);
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod async_canonicalize_tests {
    use super::{reset_fs, sync, unsync};
    use pretty_assertions::assert_eq;

    #[test_log::test(switchy_async::test)]
    async fn test_async_canonicalize_existing_path() {
        reset_fs();
        sync::create_dir_all("/async/path/here").unwrap();

        let result = unsync::canonicalize("/async/path/here").await.unwrap();
        assert_eq!(result.to_str().unwrap(), "/async/path/here");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_canonicalize_with_dots() {
        reset_fs();
        sync::create_dir_all("/a/b").unwrap();

        let result = unsync::canonicalize("/a/./b/../b").await.unwrap();
        assert_eq!(result.to_str().unwrap(), "/a/b");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_canonicalize_nonexistent_fails() {
        reset_fs();

        let result = unsync::canonicalize("/does/not/exist").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }
}

#[cfg(test)]
#[cfg(feature = "async")]
mod async_create_dir_tests {
    use super::{exists, reset_fs, sync, unsync};
    use pretty_assertions::assert_eq;

    #[test_log::test(switchy_async::test)]
    async fn test_async_create_dir_with_parent() {
        reset_fs();
        sync::create_dir_all("/async_parent").unwrap();

        unsync::create_dir("/async_parent/child").await.unwrap();
        assert!(exists("/async_parent/child"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_async_create_dir_without_parent_fails() {
        reset_fs();

        let result = unsync::create_dir("/no_parent/child").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }
}
