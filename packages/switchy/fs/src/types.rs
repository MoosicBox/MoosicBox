//! Backend-neutral filesystem value types.
//!
//! These types are shared by the simulator, `std` and `tokio` backends so code written
//! against one backend compiles and behaves the same against another.

use std::{ffi::OsString, path::Path, time::SystemTime};

/// The kind of a filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link (only reported when the link itself is inspected).
    Symlink,
    /// Any other kind of entry (FIFO, socket, device, ...).
    Other,
}

impl FileKind {
    /// Classify a standard library file type.
    #[must_use]
    pub fn from_std(file_type: std::fs::FileType) -> Self {
        if file_type.is_symlink() {
            Self::Symlink
        } else if file_type.is_dir() {
            Self::Dir
        } else if file_type.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

/// A stable identity for an inode: two paths or handles with equal identities refer to
/// the same file.
///
/// Identities are only comparable within one backend; simulator identities never equal
/// identities of files on the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId {
    /// Device (filesystem) identifier.
    pub dev: u64,
    /// Inode number within the device.
    pub ino: u64,
}

/// Unix-style permission bits of a filesystem entry.
///
/// Only the permission bits (`0o7777`) are kept; file type bits are reported through
/// [`Metadata::kind`]. On Windows, modes are synthesized from the read-only attribute
/// (`0o444`/`0o666` for files, `0o555`/`0o777` for directories).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Permissions {
    mode: u32,
}

impl Permissions {
    /// Permissions with the given mode bits (masked to `0o7777`).
    #[must_use]
    pub const fn from_mode(mode: u32) -> Self {
        Self {
            mode: mode & 0o7777,
        }
    }

    /// The permission bits (`0o7777` mask).
    #[must_use]
    pub const fn mode(&self) -> u32 {
        self.mode
    }

    /// Whether no class (owner, group, other) may write.
    #[must_use]
    pub const fn readonly(&self) -> bool {
        self.mode & 0o222 == 0
    }

    /// Clear every write bit, or set every write bit, mirroring
    /// `std::fs::Permissions::set_readonly` on Unix.
    pub const fn set_readonly(&mut self, readonly: bool) {
        if readonly {
            self.mode &= !0o222;
        } else {
            self.mode |= 0o222;
        }
    }

    /// Convert from standard library permissions.
    #[must_use]
    pub fn from_std(permissions: &std::fs::Permissions, is_dir: bool) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = is_dir;
            Self::from_mode(permissions.mode())
        }
        #[cfg(not(unix))]
        {
            let base = if is_dir { 0o777 } else { 0o666 };
            let mut converted = Self::from_mode(base);
            converted.set_readonly(permissions.readonly());
            converted
        }
    }

    /// Apply these permissions to standard library permissions.
    ///
    /// On Windows only the read-only attribute is representable.
    #[must_use]
    pub fn to_std(self, mut base: std::fs::Permissions) -> std::fs::Permissions {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            base.set_mode(self.mode);
            base
        }
        #[cfg(not(unix))]
        {
            base.set_readonly(self.readonly());
            base
        }
    }
}

/// Metadata about a filesystem entry, produced identically by every backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub(crate) len: u64,
    pub(crate) kind: FileKind,
    pub(crate) mode: u32,
    pub(crate) uid: Option<u32>,
    pub(crate) gid: Option<u32>,
    pub(crate) nlink: Option<u64>,
    pub(crate) file_id: Option<FileId>,
    pub(crate) accessed: Option<SystemTime>,
    pub(crate) modified: Option<SystemTime>,
    pub(crate) created: Option<SystemTime>,
}

/// The error for a timestamp the backend or platform does not report, as `std` returns it.
fn time_unsupported(what: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("{what} time is not available on this platform"),
    )
}

impl Metadata {
    /// The size of the entry in bytes (the target length for symbolic links).
    #[must_use]
    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Whether the entry has zero length.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The kind of the entry.
    #[must_use]
    pub const fn kind(&self) -> FileKind {
        self.kind
    }

    /// Whether the entry is a regular file.
    #[must_use]
    pub const fn is_file(&self) -> bool {
        matches!(self.kind, FileKind::File)
    }

    /// Whether the entry is a directory.
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        matches!(self.kind, FileKind::Dir)
    }

    /// Whether the entry is a symbolic link.
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        matches!(self.kind, FileKind::Symlink)
    }

    /// The permission bits of the entry (`0o7777` mask).
    #[must_use]
    pub const fn mode(&self) -> u32 {
        self.mode
    }

    /// The permissions of the entry.
    #[must_use]
    pub const fn permissions(&self) -> Permissions {
        Permissions::from_mode(self.mode)
    }

    /// The owning user id, or `None` on platforms without Unix ownership.
    #[must_use]
    pub const fn uid(&self) -> Option<u32> {
        self.uid
    }

    /// The owning group id, or `None` on platforms without Unix ownership.
    #[must_use]
    pub const fn gid(&self) -> Option<u32> {
        self.gid
    }

    /// The number of hard links, or `None` when the platform does not report it.
    #[must_use]
    pub const fn nlink(&self) -> Option<u64> {
        self.nlink
    }

    /// The same-file identity, or `None` when the platform does not report it.
    #[must_use]
    pub const fn file_id(&self) -> Option<FileId> {
        self.file_id
    }

    /// The time of the last data access (POSIX `atime`).
    ///
    /// The simulator takes timestamps from the simulated clock (`switchy_time`).
    ///
    /// # Errors
    /// * `Unsupported` when the platform does not report it.
    pub fn accessed(&self) -> std::io::Result<SystemTime> {
        self.accessed.ok_or_else(|| time_unsupported("access"))
    }

    /// The time of the last data modification (POSIX `mtime`). For a directory, the time
    /// an entry was last created, removed or renamed.
    ///
    /// # Errors
    /// * `Unsupported` when the platform does not report it.
    pub fn modified(&self) -> std::io::Result<SystemTime> {
        self.modified.ok_or_else(|| time_unsupported("modification"))
    }

    /// The creation (birth) time.
    ///
    /// # Errors
    /// * `Unsupported` when the platform or call does not report it (for example
    ///   `Dir`-relative metadata on Unix systems other than Apple platforms).
    pub fn created(&self) -> std::io::Result<SystemTime> {
        self.created.ok_or_else(|| time_unsupported("creation"))
    }

    /// Whether `self` and `other` describe the same file (both identities known and equal).
    #[must_use]
    pub fn is_same_file(&self, other: &Self) -> bool {
        self.file_id.is_some() && self.file_id == other.file_id
    }
}

impl From<std::fs::Metadata> for Metadata {
    fn from(metadata: std::fs::Metadata) -> Self {
        Self::from(&metadata)
    }
}

impl From<&std::fs::Metadata> for Metadata {
    fn from(metadata: &std::fs::Metadata) -> Self {
        let kind = FileKind::from_std(metadata.file_type());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Self {
                len: metadata.len(),
                kind,
                mode: metadata.mode() & 0o7777,
                uid: Some(metadata.uid()),
                gid: Some(metadata.gid()),
                nlink: Some(metadata.nlink()),
                file_id: Some(FileId {
                    dev: metadata.dev(),
                    ino: metadata.ino(),
                }),
                accessed: metadata.accessed().ok(),
                modified: metadata.modified().ok(),
                created: metadata.created().ok(),
            }
        }
        #[cfg(not(unix))]
        {
            Self {
                len: metadata.len(),
                kind,
                mode: Permissions::from_std(&metadata.permissions(), metadata.is_dir()).mode(),
                uid: None,
                gid: None,
                nlink: None,
                file_id: None,
                accessed: metadata.accessed().ok(),
                modified: metadata.modified().ok(),
                created: metadata.created().ok(),
            }
        }
    }
}

/// One entry of a directory listing produced by `Dir::entries`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirEntryInfo {
    pub(crate) name: OsString,
    pub(crate) kind: FileKind,
}

impl DirEntryInfo {
    /// The entry's name within its directory.
    #[must_use]
    pub fn name(&self) -> &std::ffi::OsStr {
        &self.name
    }

    /// The kind of the entry itself (symbolic links are not followed).
    #[must_use]
    pub const fn kind(&self) -> FileKind {
        self.kind
    }
}

/// Validated `OpenOptions` flags, shared by backends that interpret them themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // mirrors the independent POSIX open flags
pub(crate) struct OpenSpec {
    pub(crate) read: bool,
    pub(crate) write: bool,
    pub(crate) append: bool,
    pub(crate) truncate: bool,
    pub(crate) create: bool,
    pub(crate) create_new: bool,
    pub(crate) nofollow: bool,
    pub(crate) directory: bool,
    pub(crate) mode: u32,
}

impl OpenSpec {
    /// Validate the flag combination exactly as `std::fs::OpenOptions` does on Unix,
    /// with the same `InvalidInput` errors and messages.
    ///
    /// # Errors
    /// * `InvalidInput` when no access mode is set, or creation/truncation is requested
    ///   without write access, or truncation is combined with append.
    #[cfg_attr(not(any(feature = "simulator", feature = "std")), allow(dead_code))]
    pub(crate) fn validate(&self) -> std::io::Result<()> {
        const NO_ACCESS: &str = "must specify at least one of read, write, or append access";
        const NEEDS_WRITE: &str = "creating or truncating a file requires write or append access";
        let creating = self.create || self.create_new || self.truncate;
        let message = match (self.read, self.write, self.append) {
            (false, false, false) if creating => Some(NEEDS_WRITE),
            (false, false, false) => Some(NO_ACCESS),
            (_, false, false) if creating => Some(NEEDS_WRITE),
            (_, _, true) if self.truncate && !self.create_new => Some(NEEDS_WRITE),
            _ => None,
        };
        message.map_or(Ok(()), |message| {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                message,
            ))
        })
    }

    /// Whether [`OpenSpec::validate`] accepts the flags.
    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub(crate) fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }

    /// Whether the handle may write.
    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub(crate) const fn writable(&self) -> bool {
        self.write || self.append
    }
}

/// Require `name` to be exactly one normal path component (no separators, `.` or `..`).
///
/// Directory-relative (`*_at`) operations take entry names, never paths, so the same
/// validation applies to every backend.
///
/// # Errors
/// * Returns `InvalidInput` when `name` is empty or not a single normal component.
#[cfg_attr(not(any(feature = "simulator", feature = "std")), allow(dead_code))]
pub(crate) fn entry_name(name: &Path) -> std::io::Result<&std::ffi::OsStr> {
    let mut components = name.components();
    let bytes = name.as_os_str().as_encoded_bytes();
    let trailing_separator = bytes
        .last()
        .is_some_and(|byte| std::path::is_separator(char::from(*byte)));
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(component)), None)
            if !trailing_separator && component.len() == name.as_os_str().len() =>
        {
            Ok(component)
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected a single directory entry name",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_are_single_components() {
        assert!(entry_name(Path::new("file")).is_ok());
        assert!(entry_name(Path::new("a/b")).is_err());
        assert!(entry_name(Path::new("a/")).is_err());
        assert!(entry_name(Path::new("..")).is_err());
        assert!(entry_name(Path::new(".")).is_err());
        assert!(entry_name(Path::new("")).is_err());
        assert!(entry_name(Path::new("/a")).is_err());
        assert!(entry_name(Path::new("./a")).is_err());
    }

    #[test]
    fn permissions_readonly_round_trip() {
        let mut permissions = Permissions::from_mode(0o100_644);
        assert_eq!(permissions.mode(), 0o644);
        assert!(!permissions.readonly());
        permissions.set_readonly(true);
        assert_eq!(permissions.mode(), 0o444);
        assert!(permissions.readonly());
    }

    #[test]
    fn open_spec_rejects_std_invalid_combinations() {
        let base = OpenSpec {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            nofollow: false,
            directory: false,
            mode: 0o666,
        };
        assert!(!base.is_valid());
        assert!(OpenSpec { read: true, ..base }.is_valid());
        assert!(
            !OpenSpec {
                read: true,
                create: true,
                ..base
            }
            .is_valid()
        );
        assert!(
            !OpenSpec {
                append: true,
                truncate: true,
                ..base
            }
            .is_valid()
        );
        assert!(
            OpenSpec {
                write: true,
                create_new: true,
                ..base
            }
            .is_valid()
        );
    }
}
