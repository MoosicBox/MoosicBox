//! Standard library filesystem operations.
//!
//! This module provides thin wrappers around `std::fs` operations for consistency
//! with the simulator API. It allows you to use the same API whether you're working
//! with the real filesystem or the simulator.
//!
//! On Unix, directory-relative operations ([`sync::Dir`]) use `openat`-family system
//! calls (via `rustix`) with `O_NOFOLLOW`, so they are immune to symbolic-link races in
//! the paths leading to the opened directory.

/// Synchronous filesystem operations using standard library
///
/// This module provides blocking filesystem operations that use `std::fs` for actual
/// disk operations. Operations are simple wrappers around standard library functions.
#[cfg(feature = "sync")]
pub mod sync {
    use std::path::{Path, PathBuf};

    use crate::sync::OpenOptions;

    pub use crate::types::{DirEntryInfo, FileId, FileKind, Metadata, Permissions};

    /// File handle for synchronous operations on the real filesystem
    ///
    /// Wraps `std::fs::File` and provides the same interface as the simulator file handle.
    #[derive(Debug)]
    pub struct File(std::fs::File);

    /// Exclusive OS advisory lock retaining a native file handle until dropped.
    pub struct ExclusiveFileLock {
        _file: File,
    }

    fn map_try_lock(result: Result<(), std::fs::TryLockError>) -> std::io::Result<()> {
        result.map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => {
                std::io::Error::from(std::io::ErrorKind::WouldBlock)
            }
            std::fs::TryLockError::Error(error) => error,
        })
    }

    impl File {
        /// Make the file's data, length and metadata durable (`fsync`).
        ///
        /// # Errors
        /// * Native synchronization failures.
        pub fn sync_all(&self) -> std::io::Result<()> {
            self.0.sync_all()
        }

        /// Make the file's data and length durable (`fdatasync`).
        ///
        /// # Errors
        /// * Native synchronization failures.
        pub fn sync_data(&self) -> std::io::Result<()> {
            self.0.sync_data()
        }

        /// Truncate or extend the file to `len` bytes.
        ///
        /// # Errors
        /// * The handle is not writable, or native failures.
        pub fn set_len(&self, len: u64) -> std::io::Result<()> {
            self.0.set_len(len)
        }

        /// Change the file's permission bits (`fchmod`).
        ///
        /// On Windows only the read-only attribute is applied.
        ///
        /// # Errors
        /// * Native failures (for example `EPERM` when not the owner).
        pub fn set_permissions(&self, permissions: Permissions) -> std::io::Result<()> {
            let current = self.0.metadata()?.permissions();
            self.0.set_permissions(permissions.to_std(current))
        }

        /// Try to take an exclusive OS advisory lock (`flock`) without waiting.
        ///
        /// # Errors
        /// * Returns `WouldBlock` when another handle holds a conflicting lock.
        pub fn try_lock(&self) -> std::io::Result<()> {
            map_try_lock(self.0.try_lock())
        }

        /// Try to take a shared OS advisory lock without waiting.
        ///
        /// # Errors
        /// * Returns `WouldBlock` when another handle holds an exclusive lock.
        pub fn try_lock_shared(&self) -> std::io::Result<()> {
            map_try_lock(self.0.try_lock_shared())
        }

        /// Release any lock held by this handle.
        ///
        /// # Errors
        /// * Native failures.
        pub fn unlock(&self) -> std::io::Result<()> {
            self.0.unlock()
        }

        /// Consume this handle and acquire an exclusive OS advisory lock without waiting.
        ///
        /// # Errors
        /// * Returns `WouldBlock` when another handle holds the lock.
        /// * Returns native I/O errors.
        pub fn try_lock_exclusive(self) -> std::io::Result<ExclusiveFileLock> {
            self.try_lock()?;
            Ok(ExclusiveFileLock { _file: self })
        }

        /// Opens a file in read-only mode
        ///
        /// # Errors
        ///
        /// * If the file does not exist or cannot be opened
        pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            Ok(Self(std::fs::File::open(path)?))
        }

        /// Creates a new file for writing, truncating any existing file
        ///
        /// # Errors
        ///
        /// * If the file cannot be created
        pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
            Ok(Self(std::fs::File::create(path)?))
        }

        /// Returns a new `OpenOptions` builder for configuring how a file is opened
        #[must_use]
        pub const fn options() -> OpenOptions {
            OpenOptions::new()
        }

        /// Open `path` with `options`, including `nofollow`, `directory` and `mode`.
        ///
        /// # Errors
        /// * Native open failures; `Unsupported` for `nofollow`/`directory` on Windows.
        pub fn open_with(path: impl AsRef<Path>, options: &OpenOptions) -> std::io::Result<Self> {
            check_platform_flags(options)?;
            let options: std::fs::OpenOptions = options.clone().into();
            Ok(Self(options.open(path)?))
        }

        /// Retrieves metadata about the file
        ///
        /// # Errors
        ///
        /// * If the file metadata cannot be retrieved
        pub fn metadata(&self) -> std::io::Result<Metadata> {
            Ok(self.0.metadata()?.into())
        }

        /// Returns a reference to the inner `std::fs::File`
        #[must_use]
        pub const fn inner(&self) -> &std::fs::File {
            &self.0
        }

        /// Consumes this wrapper and returns the inner `std::fs::File`
        #[must_use]
        pub fn into_inner(self) -> std::fs::File {
            self.0
        }
    }

    impl From<std::fs::File> for File {
        fn from(file: std::fs::File) -> Self {
            Self(file)
        }
    }

    impl From<File> for std::fs::File {
        fn from(file: File) -> Self {
            file.0
        }
    }

    impl std::io::Read for File {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl std::io::Write for File {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }

    impl std::io::Seek for File {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.0.seek(pos)
        }
    }

    /// Reject flags the platform cannot honor.
    // Not `const`: the non-Unix branch constructs an `std::io::Error`.
    #[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)]
    fn check_platform_flags(options: &OpenOptions) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            let _ = options;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            if options.nofollow || options.directory {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "nofollow and directory open flags require a Unix platform",
                ))
            } else {
                Ok(())
            }
        }
    }

    /// Exclusively create a writable native file with owner-only Unix permissions.
    ///
    /// # Errors
    /// * Returns native creation errors, or `Unsupported` on non-Unix platforms.
    pub fn create_private_file(path: impl AsRef<Path>) -> std::io::Result<File> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .map(File)
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        }
    }

    /// Inspect a native entry without following its final symbolic link.
    ///
    /// # Errors
    /// * Returns native metadata errors.
    pub fn symlink_metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        std::fs::symlink_metadata(path).map(Metadata::from)
    }

    /// Inspect a native entry, following symbolic links.
    ///
    /// # Errors
    /// * Returns native metadata errors.
    pub fn metadata(path: impl AsRef<Path>) -> std::io::Result<Metadata> {
        std::fs::metadata(path).map(Metadata::from)
    }

    /// Reads the entire contents of a file into a byte vector
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::read` fails
    pub fn read<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<u8>> {
        ::std::fs::read(path)
    }

    /// Reads the entire contents of a file into a string
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::read_to_string` fails
    pub fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        ::std::fs::read_to_string(path)
    }

    /// Writes a slice as the entire contents of a file
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::write` fails
    pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> std::io::Result<()> {
        ::std::fs::write(path, contents)
    }

    /// Creates a new directory at the specified path
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir` fails
    pub fn create_dir<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        ::std::fs::create_dir(path)
    }

    /// Creates a directory and all missing parent directories
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir_all` fails
    pub fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        ::std::fs::create_dir_all(path)
    }

    /// Removes a directory and all its contents recursively
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::remove_dir_all` fails
    pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        ::std::fs::remove_dir_all(path)
    }

    /// Remove a file or symbolic link.
    ///
    /// # Errors
    /// * If underlying `std::fs::remove_file` fails
    pub fn remove_file(path: impl AsRef<Path>) -> std::io::Result<()> {
        ::std::fs::remove_file(path)
    }

    /// Remove an empty directory.
    ///
    /// # Errors
    /// * If underlying `std::fs::remove_dir` fails
    pub fn remove_dir(path: impl AsRef<Path>) -> std::io::Result<()> {
        ::std::fs::remove_dir(path)
    }

    /// Synchronize a native directory after entry changes.
    ///
    /// # Errors
    /// * The path cannot be opened, is not a directory, or synchronization fails.
    /// * Returns `Unsupported` on platforms without directory synchronization.
    pub fn sync_directory(path: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            let directory = std::fs::File::open(path)?;
            if !directory.metadata()?.is_dir() {
                return Err(std::io::Error::from(std::io::ErrorKind::NotADirectory));
            }
            directory.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        }
    }

    /// Move a regular file, replacing an existing destination file.
    ///
    /// Callers must exclusively control both paths, including their parent directories.
    ///
    /// # Errors
    /// * Source is missing or not a regular file.
    /// * Destination exists but is not a regular file.
    /// * Native rename fails.
    pub fn rename_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        let from = from.as_ref();
        let to = to.as_ref();
        if !std::fs::symlink_metadata(from)?.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "source is not a regular file",
            ));
        }
        match std::fs::symlink_metadata(to) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "destination is not a regular file",
                ));
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        std::fs::rename(from, to)
    }

    /// Rename a file or directory (`std::fs::rename`).
    ///
    /// # Errors
    /// * If underlying `std::fs::rename` fails
    pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }

    /// Rename without replacing: fails with `EEXIST` if `to` exists.
    ///
    /// Uses `renameat2(RENAME_NOREPLACE)` on Linux/Android and `renameatx_np(RENAME_EXCL)`
    /// on Apple platforms. Elsewhere, or when the filesystem does not support the flag, it
    /// falls back to hard-linking `to` and unlinking `from`, which is atomic with respect
    /// to `to` but does not work for directories.
    ///
    /// # Errors
    /// * `EEXIST`, or native rename/link failures.
    pub fn rename_noreplace(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            crate::native::rename_noreplace(
                rustix::fs::CWD,
                from.as_ref(),
                rustix::fs::CWD,
                to.as_ref(),
            )
        }
        #[cfg(not(unix))]
        {
            std::fs::hard_link(from.as_ref(), to.as_ref())?;
            std::fs::remove_file(from)
        }
    }

    /// Create a hard link `link` to `original`.
    ///
    /// # Errors
    /// * If underlying `std::fs::hard_link` fails
    pub fn hard_link(original: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::hard_link(original, link)
    }

    /// Create a symbolic link at `link` pointing to `original`.
    ///
    /// # Errors
    /// * Native failures; `Unsupported` on non-Unix platforms.
    pub fn symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(original, link)
        }
        #[cfg(not(unix))]
        {
            let _ = (original, link);
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        }
    }

    /// Read the target of a symbolic link.
    ///
    /// # Errors
    /// * If underlying `std::fs::read_link` fails
    pub fn read_link(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        std::fs::read_link(path)
    }

    /// Change permission bits, following symbolic links.
    ///
    /// On Windows only the read-only attribute is applied.
    ///
    /// # Errors
    /// * Native failures.
    pub fn set_permissions(
        path: impl AsRef<Path>,
        permissions: Permissions,
    ) -> std::io::Result<()> {
        let path = path.as_ref();
        let current = std::fs::metadata(path)?.permissions();
        std::fs::set_permissions(path, permissions.to_std(current))
    }

    /// The effective user id of the process.
    ///
    /// # Errors
    /// * Returns `Unsupported` on non-Unix platforms.
    #[cfg_attr(unix, allow(clippy::unnecessary_wraps))]
    pub fn current_uid() -> std::io::Result<u32> {
        #[cfg(unix)]
        {
            Ok(rustix::process::geteuid().as_raw())
        }
        #[cfg(not(unix))]
        {
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        }
    }

    /// Create an anonymous temporary file in the system temporary directory, removed when
    /// its last handle closes.
    ///
    /// # Errors
    /// * Native creation failures.
    pub fn tempfile() -> std::io::Result<File> {
        tempfile::tempfile().map(File)
    }

    /// Create an anonymous temporary file in `dir`.
    ///
    /// # Errors
    /// * Native creation failures.
    pub fn tempfile_in(dir: impl AsRef<Path>) -> std::io::Result<File> {
        tempfile::tempfile_in(dir).map(File)
    }

    /// Canonicalizes a path to its absolute form with all intermediate components normalized
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::canonicalize` fails
    pub fn canonicalize<P: AsRef<Path>>(path: P) -> std::io::Result<std::path::PathBuf> {
        ::std::fs::canonicalize(path)
    }

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::read_dir` fails
    /// * If any directory entry cannot be read
    pub fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<std::fs::DirEntry>> {
        let mut entries: Vec<_> = ::std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        Ok(entries)
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read
    /// * If any directory entry cannot be accessed
    pub fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<std::fs::DirEntry>> {
        fn walk_recursive(
            path: &Path,
            entries: &mut Vec<std::fs::DirEntry>,
        ) -> std::io::Result<()> {
            let mut dir_entries: Vec<_> =
                ::std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
            dir_entries.sort_by_key(std::fs::DirEntry::file_name);

            for entry in dir_entries {
                let path = entry.path();
                entries.push(entry);

                if path.is_dir() {
                    walk_recursive(&path, entries)?;
                }
            }
            Ok(())
        }

        let mut all_entries = Vec::new();
        walk_recursive(path.as_ref(), &mut all_entries)?;

        // Sort all entries by full path for deterministic order
        all_entries.sort_by_key(std::fs::DirEntry::path);
        Ok(all_entries)
    }

    impl From<OpenOptions> for std::fs::OpenOptions {
        fn from(value: OpenOptions) -> Self {
            let mut options = Self::new();

            options
                .create(value.create)
                .append(value.append)
                .read(value.read)
                .write(value.write)
                .create_new(value.create_new)
                .truncate(value.truncate);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(value.mode);
                let flags = unix::extra_flags(value.nofollow, value.directory);
                if flags != 0 {
                    options.custom_flags(flags);
                }
            }

            options
        }
    }

    #[cfg(not(feature = "simulator"))]
    impl OpenOptions {
        /// Opens a file with the configured options
        ///
        /// # Errors
        ///
        /// * If an I/O error occurs
        pub fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            File::open_with(path, &self)
        }
    }

    #[cfg(unix)]
    mod unix {
        use std::{
            ffi::OsStr,
            os::{fd::OwnedFd, unix::ffi::OsStrExt as _},
        };

        use rustix::{
            fs::{AtFlags, FileType, Mode, OFlags, RawMode, Stat},
            io::Errno,
        };

        use crate::types::{DirEntryInfo, FileId, FileKind, Metadata, OpenSpec};

        /// Extra `open(2)` flags for `std::fs::OpenOptionsExt::custom_flags`.
        #[allow(clippy::cast_possible_wrap)] // flag bits fit in a C int on every Unix
        pub(super) fn extra_flags(nofollow: bool, directory: bool) -> i32 {
            let mut flags = OFlags::empty();
            if nofollow {
                flags |= OFlags::NOFOLLOW;
            }
            if directory {
                flags |= OFlags::DIRECTORY;
            }
            flags.bits() as i32
        }

        /// `open(2)` flags equivalent to `std::fs::OpenOptions` on Unix, plus `O_CLOEXEC`.
        pub(super) fn open_flags(spec: &OpenSpec) -> std::io::Result<OFlags> {
            spec.validate()?;
            let mut flags = match (spec.read, spec.write, spec.append) {
                (true, false, false) => OFlags::RDONLY,
                (false, true, false) => OFlags::WRONLY,
                (true, true, false) => OFlags::RDWR,
                (false, _, true) => OFlags::WRONLY | OFlags::APPEND,
                (true, _, true) => OFlags::RDWR | OFlags::APPEND,
                (false, false, false) => return Err(Errno::INVAL.into()),
            };
            flags |= match (spec.create, spec.truncate, spec.create_new) {
                (_, _, true) => OFlags::CREATE | OFlags::EXCL,
                (true, true, false) => OFlags::CREATE | OFlags::TRUNC,
                (true, false, false) => OFlags::CREATE,
                (false, true, false) => OFlags::TRUNC,
                (false, false, false) => OFlags::empty(),
            };
            if spec.nofollow {
                flags |= OFlags::NOFOLLOW;
            }
            if spec.directory {
                flags |= OFlags::DIRECTORY;
            }
            Ok(flags | OFlags::CLOEXEC)
        }

        /// A permission mode in the platform's `mode_t` width.
        // `mode_t` is 16 bits on Apple platforms and 32 bits elsewhere.
        #[allow(clippy::cast_possible_truncation, clippy::unnecessary_cast)]
        pub(super) const fn mode(mode: u32) -> Mode {
            Mode::from_raw_mode((mode & 0o7777) as RawMode)
        }

        /// Metadata from a raw `stat` structure.
        // Field widths and signedness differ between Unix platforms.
        #[allow(
            clippy::cast_sign_loss,
            clippy::unnecessary_cast,
            clippy::useless_conversion,
            clippy::cast_lossless
        )]
        pub(super) const fn metadata_from_stat(stat: &Stat) -> Metadata {
            let kind = match FileType::from_raw_mode(stat.st_mode as RawMode) {
                FileType::RegularFile => FileKind::File,
                FileType::Directory => FileKind::Dir,
                FileType::Symlink => FileKind::Symlink,
                _ => FileKind::Other,
            };
            Metadata {
                len: stat.st_size as u64,
                kind,
                mode: (stat.st_mode as u32) & 0o7777,
                uid: Some(stat.st_uid),
                gid: Some(stat.st_gid),
                nlink: Some(stat.st_nlink as u64),
                file_id: Some(FileId {
                    dev: stat.st_dev as u64,
                    ino: stat.st_ino as u64,
                }),
            }
        }

        pub(super) const fn kind_of(file_type: FileType) -> FileKind {
            match file_type {
                FileType::RegularFile => FileKind::File,
                FileType::Directory => FileKind::Dir,
                FileType::Symlink => FileKind::Symlink,
                _ => FileKind::Other,
            }
        }

        /// List a directory's entries (excluding `.` and `..`), sorted by name.
        pub(super) fn entries(fd: &OwnedFd) -> std::io::Result<Vec<DirEntryInfo>> {
            let dir = rustix::fs::Dir::read_from(fd)?;
            let mut entries = Vec::new();
            for entry in dir {
                let entry = entry?;
                let name = entry.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                let name = OsStr::from_bytes(name).to_os_string();
                let kind = match entry.file_type() {
                    FileType::Unknown => {
                        let stat = rustix::fs::statat(fd, &*name, AtFlags::SYMLINK_NOFOLLOW)?;
                        metadata_from_stat(&stat).kind()
                    }
                    other => kind_of(other),
                };
                entries.push(DirEntryInfo { name, kind });
            }
            entries.sort();
            Ok(entries)
        }
    }

    /// A capability-style handle on a native directory.
    ///
    /// On Unix the handle is a directory file descriptor opened with
    /// `O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, and every operation is an
    /// `*at` system call relative to it taking a single entry name, never following a
    /// final symbolic link. On other platforms every operation returns `Unsupported`.
    #[derive(Debug)]
    pub struct Dir {
        #[cfg(unix)]
        fd: std::os::fd::OwnedFd,
        path: PathBuf,
    }

    #[cfg(not(unix))]
    fn dir_unsupported() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "directory handles require a Unix platform",
        )
    }

    #[cfg(unix)]
    impl Dir {
        fn directory_flags() -> rustix::fs::OFlags {
            use rustix::fs::OFlags;
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
        }

        /// Open the directory at `path` without following a final symbolic link.
        ///
        /// # Errors
        /// * Native `open` failures (`ENOENT`, `ENOTDIR`, `ELOOP`, `EACCES`, ...).
        pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            let fd = rustix::fs::open(
                path.as_ref(),
                Self::directory_flags(),
                rustix::fs::Mode::empty(),
            )?;
            Ok(Self {
                fd,
                path: path.as_ref().to_path_buf(),
            })
        }

        /// The path this directory was opened with (diagnostic only; it may be stale).
        #[must_use]
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Open the subdirectory `name` without following symbolic links.
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, or native `openat` failures.
        pub fn open_dir_at(&self, name: impl AsRef<Path>) -> std::io::Result<Self> {
            let name = crate::types::entry_name(name.as_ref())?;
            let fd = rustix::fs::openat(
                &self.fd,
                name,
                Self::directory_flags(),
                rustix::fs::Mode::empty(),
            )?;
            Ok(Self {
                fd,
                path: self.path.join(name),
            })
        }

        /// Open or create the file `name` in this directory with `options`.
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, `EINVAL` for invalid flag combinations, or
        ///   native `openat` failures.
        pub fn open_file_at(
            &self,
            name: impl AsRef<Path>,
            options: &OpenOptions,
        ) -> std::io::Result<File> {
            let name = crate::types::entry_name(name.as_ref())?;
            let spec = options.spec();
            let flags = unix::open_flags(&spec)?;
            let fd = rustix::fs::openat(&self.fd, name, flags, unix::mode(spec.mode))?;
            Ok(File(std::fs::File::from(fd)))
        }

        /// Create the subdirectory `name` with `mode` (before the umask).
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, or native `mkdirat` failures.
        pub fn create_dir_at(&self, name: impl AsRef<Path>, mode: u32) -> std::io::Result<()> {
            let name = crate::types::entry_name(name.as_ref())?;
            rustix::fs::mkdirat(&self.fd, name, unix::mode(mode))?;
            Ok(())
        }

        /// Metadata of this directory (`fstat`).
        ///
        /// # Errors
        /// * Native failures.
        pub fn metadata(&self) -> std::io::Result<Metadata> {
            Ok(unix::metadata_from_stat(&rustix::fs::fstat(&self.fd)?))
        }

        /// Metadata of entry `name`, without following a symbolic link.
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, or native `fstatat` failures.
        pub fn metadata_at(&self, name: impl AsRef<Path>) -> std::io::Result<Metadata> {
            let name = crate::types::entry_name(name.as_ref())?;
            let stat = rustix::fs::statat(&self.fd, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?;
            Ok(unix::metadata_from_stat(&stat))
        }

        /// Remove the file or symbolic link `name`.
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, or native `unlinkat` failures.
        pub fn remove_file_at(&self, name: impl AsRef<Path>) -> std::io::Result<()> {
            let name = crate::types::entry_name(name.as_ref())?;
            rustix::fs::unlinkat(&self.fd, name, rustix::fs::AtFlags::empty())?;
            Ok(())
        }

        /// Remove the empty subdirectory `name`.
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, or native `unlinkat(AT_REMOVEDIR)` failures.
        pub fn remove_dir_at(&self, name: impl AsRef<Path>) -> std::io::Result<()> {
            let name = crate::types::entry_name(name.as_ref())?;
            rustix::fs::unlinkat(&self.fd, name, rustix::fs::AtFlags::REMOVEDIR)?;
            Ok(())
        }

        /// Rename entry `from` of this directory to `to` in `to_dir`.
        ///
        /// # Errors
        /// * `InvalidInput` for non-names, or native `renameat` failures.
        pub fn rename_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let from = crate::types::entry_name(from.as_ref())?;
            let to = crate::types::entry_name(to.as_ref())?;
            rustix::fs::renameat(&self.fd, from, &to_dir.fd, to)?;
            Ok(())
        }

        /// Rename entry `from` to `to` in `to_dir` only if `to` does not exist.
        ///
        /// See [`rename_noreplace`] for the system calls used and the fallback.
        ///
        /// # Errors
        /// * `InvalidInput` for non-names, `EEXIST`, or native failures.
        pub fn rename_noreplace_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let from = crate::types::entry_name(from.as_ref())?;
            let to = crate::types::entry_name(to.as_ref())?;
            crate::native::rename_noreplace(&self.fd, Path::new(from), &to_dir.fd, Path::new(to))
        }

        /// Create hard link `to` in `to_dir` to entry `from` of this directory.
        ///
        /// # Errors
        /// * `InvalidInput` for non-names, or native `linkat` failures.
        pub fn hard_link_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let from = crate::types::entry_name(from.as_ref())?;
            let to = crate::types::entry_name(to.as_ref())?;
            rustix::fs::linkat(&self.fd, from, &to_dir.fd, to, rustix::fs::AtFlags::empty())?;
            Ok(())
        }

        /// Create symbolic link `name` pointing to `target`.
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, or native `symlinkat` failures.
        pub fn symlink_at(
            &self,
            target: impl AsRef<Path>,
            name: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let name = crate::types::entry_name(name.as_ref())?;
            rustix::fs::symlinkat(target.as_ref(), &self.fd, name)?;
            Ok(())
        }

        /// Read the target of symbolic link `name`.
        ///
        /// # Errors
        /// * `InvalidInput` for a non-name, or native `readlinkat` failures.
        pub fn read_link_at(&self, name: impl AsRef<Path>) -> std::io::Result<PathBuf> {
            use std::os::unix::ffi::OsStringExt as _;
            let name = crate::types::entry_name(name.as_ref())?;
            let target = rustix::fs::readlinkat(&self.fd, name, Vec::new())?;
            Ok(PathBuf::from(std::ffi::OsString::from_vec(
                target.into_bytes(),
            )))
        }

        /// Change this directory's permission bits (`fchmod`).
        ///
        /// # Errors
        /// * Native failures.
        pub fn set_permissions(&self, permissions: Permissions) -> std::io::Result<()> {
            rustix::fs::fchmod(&self.fd, unix::mode(permissions.mode()))?;
            Ok(())
        }

        /// The entries of this directory (excluding `.` and `..`), sorted by name.
        ///
        /// # Errors
        /// * Native directory reading failures.
        pub fn entries(&self) -> std::io::Result<Vec<DirEntryInfo>> {
            unix::entries(&self.fd)
        }

        /// Make changes to this directory's entries durable (`fsync`).
        ///
        /// # Errors
        /// * Native synchronization failures.
        pub fn sync(&self) -> std::io::Result<()> {
            rustix::fs::fsync(&self.fd)?;
            Ok(())
        }
    }

    #[cfg(not(unix))]
    #[allow(clippy::unused_self, clippy::needless_pass_by_value)]
    impl Dir {
        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            let _ = path;
            Err(dir_unsupported())
        }

        /// The path this directory was opened with.
        #[must_use]
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn open_dir_at(&self, name: impl AsRef<Path>) -> std::io::Result<Self> {
            let _ = name;
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn open_file_at(
            &self,
            name: impl AsRef<Path>,
            options: &OpenOptions,
        ) -> std::io::Result<File> {
            let _ = (name, options);
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn create_dir_at(&self, name: impl AsRef<Path>, mode: u32) -> std::io::Result<()> {
            let _ = (name, mode);
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn metadata(&self) -> std::io::Result<Metadata> {
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn metadata_at(&self, name: impl AsRef<Path>) -> std::io::Result<Metadata> {
            let _ = name;
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn remove_file_at(&self, name: impl AsRef<Path>) -> std::io::Result<()> {
            let _ = name;
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn remove_dir_at(&self, name: impl AsRef<Path>) -> std::io::Result<()> {
            let _ = name;
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn rename_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let _ = (from, to_dir, to);
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn rename_noreplace_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let _ = (from, to_dir, to);
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn hard_link_at(
            &self,
            from: impl AsRef<Path>,
            to_dir: &Self,
            to: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let _ = (from, to_dir, to);
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn symlink_at(
            &self,
            target: impl AsRef<Path>,
            name: impl AsRef<Path>,
        ) -> std::io::Result<()> {
            let _ = (target, name);
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn read_link_at(&self, name: impl AsRef<Path>) -> std::io::Result<PathBuf> {
            let _ = name;
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn set_permissions(&self, permissions: Permissions) -> std::io::Result<()> {
            let _ = permissions;
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn entries(&self) -> std::io::Result<Vec<DirEntryInfo>> {
            Err(dir_unsupported())
        }

        /// Unsupported on this platform.
        ///
        /// # Errors
        /// * Always returns `Unsupported`.
        pub fn sync(&self) -> std::io::Result<()> {
            Err(dir_unsupported())
        }
    }
}

/// Checks if a path exists on the filesystem
///
/// Returns `true` if the path exists, `false` otherwise.
#[must_use]
pub fn exists<P: AsRef<std::path::Path>>(path: P) -> bool {
    path.as_ref().exists()
}

/// Temporary directory functionality for standard filesystem operations
#[cfg(feature = "std")]
pub mod temp_dir {
    use std::{
        ffi::OsStr,
        path::{Path, PathBuf},
    };

    /// A directory in the filesystem that is automatically deleted when it goes out of scope
    pub struct TempDir {
        inner: Option<tempfile::TempDir>,
        path: PathBuf,
    }

    impl TempDir {
        /// Attempts to make a temporary directory inside of the system temp directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created
        pub fn new() -> std::io::Result<Self> {
            let td = tempfile::TempDir::new()?;
            let path = td.path().to_path_buf();
            Ok(Self {
                inner: Some(td),
                path,
            })
        }

        /// Attempts to make a temporary directory inside the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created
        pub fn new_in<P: AsRef<Path>>(dir: P) -> std::io::Result<Self> {
            let td = tempfile::TempDir::new_in(dir)?;
            let path = td.path().to_path_buf();
            Ok(Self {
                inner: Some(td),
                path,
            })
        }

        /// Attempts to make a temporary directory with the specified prefix
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created
        pub fn with_prefix<S: AsRef<OsStr>>(prefix: S) -> std::io::Result<Self> {
            let td = tempfile::TempDir::with_prefix(prefix)?;
            let path = td.path().to_path_buf();
            Ok(Self {
                inner: Some(td),
                path,
            })
        }

        /// Attempts to make a temporary directory with the specified suffix
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created
        pub fn with_suffix<S: AsRef<OsStr>>(suffix: S) -> std::io::Result<Self> {
            let td = tempfile::TempDir::with_suffix(suffix)?;
            let path = td.path().to_path_buf();
            Ok(Self {
                inner: Some(td),
                path,
            })
        }

        /// Attempts to make a temporary directory with the specified prefix in the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created
        pub fn with_prefix_in<S: AsRef<OsStr>, P: AsRef<Path>>(
            prefix: S,
            dir: P,
        ) -> std::io::Result<Self> {
            let td = tempfile::TempDir::with_prefix_in(prefix, dir)?;
            let path = td.path().to_path_buf();
            Ok(Self {
                inner: Some(td),
                path,
            })
        }

        /// Attempts to make a temporary directory with the specified suffix in the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created
        pub fn with_suffix_in<S: AsRef<OsStr>, P: AsRef<Path>>(
            suffix: S,
            dir: P,
        ) -> std::io::Result<Self> {
            let td = tempfile::TempDir::with_suffix_in(suffix, dir)?;
            let path = td.path().to_path_buf();
            Ok(Self {
                inner: Some(td),
                path,
            })
        }

        /// Accesses the Path to the temporary directory
        #[must_use]
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Persist the temporary directory to disk, returning the `PathBuf` where it is located
        #[must_use]
        pub fn keep(mut self) -> PathBuf {
            if let Some(td) = self.inner.take() {
                td.keep()
            } else {
                self.path.clone()
            }
        }

        /// Deprecated alias for `keep()`
        #[deprecated = "use TempDir::keep()"]
        #[must_use]
        pub fn into_path(self) -> PathBuf {
            self.keep()
        }

        /// Disable cleanup of the temporary directory
        pub fn disable_cleanup(&mut self, disable_cleanup: bool) {
            if let Some(ref mut td) = self.inner {
                td.disable_cleanup(disable_cleanup);
            }
        }

        /// Closes and removes the temporary directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be removed
        pub fn close(self) -> std::io::Result<()> {
            self.inner.map_or(Ok(()), tempfile::TempDir::close)
        }
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
                .finish_non_exhaustive()
        }
    }

    /// Re-export Builder from tempfile for full API compatibility
    pub use tempfile::Builder;

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
}
