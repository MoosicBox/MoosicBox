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

    /// File handle wrapping `std::fs::File`
    ///
    /// Provides convenience methods for opening files that match the simulator API.
    pub struct File(std::fs::File);

    impl File {
        /// Opens a file in read-only mode
        ///
        /// This is a convenience method equivalent to `OpenOptions::new().read(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the file does not exist
        /// * If permission is denied
        pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            Ok(Self(std::fs::File::open(path)?))
        }

        /// Creates a new file for writing, truncating any existing file
        ///
        /// This is a convenience method equivalent to
        /// `OpenOptions::new().create(true).write(true).truncate(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the parent directory does not exist
        /// * If permission is denied
        pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
            Ok(Self(std::fs::File::create(path)?))
        }

        /// Returns a new `OpenOptions` builder for configuring how a file is opened
        #[must_use]
        pub const fn options() -> OpenOptions {
            OpenOptions::new()
        }

        /// Retrieves metadata about the file
        ///
        /// # Errors
        ///
        /// * If the file metadata cannot be retrieved
        pub fn metadata(&self) -> std::io::Result<Metadata> {
            Ok(Metadata(self.0.metadata()?))
        }

        /// Returns a reference to the inner `std::fs::File`
        #[must_use]
        pub const fn inner(&self) -> &std::fs::File {
            &self.0
        }

        /// Returns the inner `std::fs::File`, consuming this wrapper
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

    /// Metadata information about a file
    ///
    /// Wrapper around `std::fs::Metadata` providing information about file size and type.
    #[derive(Debug, Clone)]
    pub struct Metadata(std::fs::Metadata);

    impl Metadata {
        /// Returns the size of the file in bytes
        #[must_use]
        pub fn len(&self) -> u64 {
            self.0.len()
        }

        /// Returns `true` if the file has zero length
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }

        /// Returns `true` if this metadata is for a regular file
        #[must_use]
        pub fn is_file(&self) -> bool {
            self.0.is_file()
        }

        /// Returns `true` if this metadata is for a directory
        #[must_use]
        pub fn is_dir(&self) -> bool {
            self.0.is_dir()
        }

        /// Returns `true` if this metadata is for a symbolic link
        #[must_use]
        pub fn is_symlink(&self) -> bool {
            self.0.is_symlink()
        }

        /// Returns a reference to the inner `std::fs::Metadata`
        #[must_use]
        pub const fn inner(&self) -> &std::fs::Metadata {
            &self.0
        }
    }

    impl From<std::fs::Metadata> for Metadata {
        fn from(metadata: std::fs::Metadata) -> Self {
            Self(metadata)
        }
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

    /// Creates a directory
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

    /// Canonicalizes a path
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

            Ok(File(options.open(path)?))
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
