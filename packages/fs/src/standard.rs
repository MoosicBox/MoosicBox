//! Standard library filesystem operations.
//!
//! This module provides thin wrappers around `std::fs` operations for consistency
//! with the simulator API. It allows you to use the same API whether you're working
//! with the real filesystem or the simulator.

/// Synchronous filesystem operations using standard library
///
/// This module provides blocking filesystem operations that use `std::fs` for actual
/// disk operations. Operations are simple wrappers around standard library functions.
#[cfg(feature = "sync")]
pub mod sync {
    use std::path::Path;

    use crate::sync::OpenOptions;

    pub use std::fs::File;

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
                .truncate(value.truncate);

            options
        }
    }

    #[cfg(not(feature = "simulator"))]
    impl OpenOptions {
        /// Opens a file with the configured options
        ///
        /// # Errors
        ///
        /// * If and IO error occurs
        pub fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            let options: std::fs::OpenOptions = self.into();

            options.open(path)
        }
    }
}

/// Checks if a path exists on the filesystem
///
/// Returns `true` if the path exists, `false` otherwise.
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
