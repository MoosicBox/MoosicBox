//! POSIX error numbers shared by the backends.
//!
//! The simulator reports failures as the same `std::io::Error` values the host operating
//! system produces. On Unix every error is built from the host's raw `errno`, so both
//! [`std::io::Error::kind`] and [`std::io::Error::raw_os_error`] match what a native call
//! returns. Other platforms receive the closest [`std::io::ErrorKind`], except that the
//! Windows-only errors of the simulated Windows personality carry their native Windows
//! error codes on Windows hosts.

/// A POSIX error number reported by a filesystem operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Errno {
    /// `ENOENT`: no such file or directory.
    NoEnt,
    /// `EEXIST`: the entry already exists.
    Exist,
    /// `ENOTDIR`: a path component is not a directory.
    NotDir,
    /// `EISDIR`: the operation is not valid on a directory.
    IsDir,
    /// `ENOTEMPTY`: the directory is not empty.
    NotEmpty,
    /// `ELOOP`: too many symbolic links, or a refused symbolic link (`O_NOFOLLOW`).
    Loop,
    /// `EACCES`: permission bits deny the access.
    Access,
    /// `EPERM`: the operation is not permitted.
    Perm,
    /// `ENOSPC`: no space left on the device.
    NoSpc,
    /// `EIO`: input/output error.
    Io,
    /// `EBADF`: the handle was not opened for this kind of access.
    BadF,
    /// `EINVAL`: invalid argument.
    Inval,
    /// `ESTALE`: the handle outlived its filesystem (after a simulated crash or reset).
    Stale,
    /// `EBUSY`: the entry is in use (for example the root directory).
    Busy,
    /// `EWOULDBLOCK`: a non-blocking lock attempt conflicts with another holder.
    WouldBlock,
    /// `EXDEV`: the operation crosses filesystems (simulated namespaces).
    XDev,
    /// Windows `ERROR_SHARING_VIOLATION`: the file is open (simulated Windows
    /// personality). Reported as `EACCES` on Unix hosts.
    SharingViolation,
    /// Windows `ERROR_PRIVILEGE_NOT_HELD`: creating a symbolic link needs a privilege the
    /// process lacks (simulated Windows personality). Reported as `EPERM` on Unix hosts.
    PrivilegeNotHeld,
}

impl Errno {
    /// The host's raw error number.
    #[cfg(unix)]
    const fn raw(self) -> i32 {
        use rustix::io::Errno as E;
        match self {
            Self::NoEnt => E::NOENT,
            Self::Exist => E::EXIST,
            Self::NotDir => E::NOTDIR,
            Self::IsDir => E::ISDIR,
            Self::NotEmpty => E::NOTEMPTY,
            Self::Loop => E::LOOP,
            Self::Access | Self::SharingViolation => E::ACCESS,
            Self::Perm | Self::PrivilegeNotHeld => E::PERM,
            Self::NoSpc => E::NOSPC,
            Self::Io => E::IO,
            Self::BadF => E::BADF,
            Self::Inval => E::INVAL,
            Self::Stale => E::STALE,
            Self::Busy => E::BUSY,
            Self::WouldBlock => E::WOULDBLOCK,
            Self::XDev => E::XDEV,
        }
        .raw_os_error()
    }

    /// The closest error kind on platforms without POSIX error numbers.
    #[cfg(not(unix))]
    const fn kind(self) -> std::io::ErrorKind {
        use std::io::ErrorKind as K;
        match self {
            Self::NoEnt => K::NotFound,
            Self::Exist => K::AlreadyExists,
            Self::NotDir => K::NotADirectory,
            Self::IsDir => K::IsADirectory,
            Self::NotEmpty => K::DirectoryNotEmpty,
            Self::Loop | Self::Io | Self::BadF => K::Other,
            Self::Access | Self::Perm | Self::SharingViolation | Self::PrivilegeNotHeld => {
                K::PermissionDenied
            }
            Self::NoSpc => K::StorageFull,
            Self::Inval => K::InvalidInput,
            Self::Stale => K::StaleNetworkFileHandle,
            Self::Busy => K::ResourceBusy,
            Self::WouldBlock => K::WouldBlock,
            Self::XDev => K::CrossesDevices,
        }
    }
}

impl From<Errno> for std::io::Error {
    fn from(errno: Errno) -> Self {
        #[cfg(unix)]
        {
            Self::from_raw_os_error(errno.raw())
        }
        #[cfg(windows)]
        {
            /// `ERROR_SHARING_VIOLATION`.
            const SHARING_VIOLATION: i32 = 32;
            /// `ERROR_PRIVILEGE_NOT_HELD`.
            const PRIVILEGE_NOT_HELD: i32 = 1314;
            match errno {
                Errno::SharingViolation => Self::from_raw_os_error(SHARING_VIOLATION),
                Errno::PrivilegeNotHeld => Self::from_raw_os_error(PRIVILEGE_NOT_HELD),
                _ => Self::new(errno.kind(), format!("{errno:?}")),
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            Self::new(errno.kind(), format!("{errno:?}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Errno;

    #[test]
    fn errors_keep_their_kind() {
        let error = std::io::Error::from(Errno::NoEnt);
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        let error = std::io::Error::from(Errno::NotEmpty);
        assert_eq!(error.kind(), std::io::ErrorKind::DirectoryNotEmpty);
        let error = std::io::Error::from(Errno::WouldBlock);
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    }
}
