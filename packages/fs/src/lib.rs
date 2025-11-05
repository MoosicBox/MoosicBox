//! Switchy filesystem abstraction layer with support for real and simulated filesystems.
//!
//! This crate provides a unified filesystem API that can switch between different backends:
//!
//! * **Real filesystem** - Standard filesystem operations using `std::fs` or `tokio::fs`
//! * **Simulated filesystem** - In-memory filesystem for testing without touching the disk
//!
//! # Features
//!
//! * `simulator` - Enables in-memory filesystem simulator (enabled by default)
//! * `simulator-real-fs` - Allows temporarily using real filesystem within simulator mode
//! * `std` - Standard library filesystem support (enabled by default)
//! * `tokio` - Async filesystem operations using tokio (enabled by default)
//! * `sync` - Synchronous filesystem operations (enabled by default)
//! * `async` - Asynchronous filesystem operations (enabled by default)
//!
//! # Examples
//!
//! Basic file operations:
//!
//! ```rust
//! # #[cfg(all(feature = "sync", any(feature = "simulator", feature = "std")))]
//! # {
//! use switchy_fs::sync::{OpenOptions, read_to_string, create_dir_all};
//! # #[cfg(feature = "simulator")]
//! # {
//! # use switchy_fs::simulator::reset_fs;
//! # reset_fs();
//! # create_dir_all("/tmp").unwrap();
//!
//! // Create a directory and write to a file
//! let mut file = OpenOptions::new()
//!     .create(true)
//!     .write(true)
//!     .open("/tmp/example.txt")
//!     .unwrap();
//!
//! use std::io::Write;
//! file.write_all(b"Hello, world!").unwrap();
//! drop(file);
//!
//! // Read the file back
//! let content = read_to_string("/tmp/example.txt").unwrap();
//! assert_eq!(content, "Hello, world!");
//! # }
//! # }
//! ```
//!
//! Using temporary directories:
//!
//! ```rust
//! # #[cfg(any(feature = "simulator", feature = "std"))]
//! # {
//! use switchy_fs::tempdir;
//!
//! // Create a temporary directory that will be cleaned up when dropped
//! let temp_dir = tempdir().unwrap();
//! let temp_path = temp_dir.path();
//!
//! // Use the temporary directory
//! println!("Temp directory: {}", temp_path.display());
//!
//! // Directory is automatically deleted when temp_dir goes out of scope
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// In-memory filesystem simulator for testing
///
/// Provides a complete filesystem simulation that runs entirely in memory, useful for testing
/// filesystem operations without touching the actual disk.
#[cfg(feature = "simulator")]
pub mod simulator;

/// Standard library filesystem operations
///
/// Thin wrappers around `std::fs` operations for consistency with the simulator API.
#[cfg(feature = "std")]
pub mod standard;

/// Tokio async filesystem operations
///
/// Async filesystem operations using tokio runtime.
#[cfg(feature = "tokio")]
pub mod tokio;

/// Trait for synchronous file operations
///
/// This trait combines standard I/O traits for use with generic file handling code.
#[cfg(all(feature = "sync", feature = "std"))]
pub trait GenericSyncFile:
    Send + Sync + ::std::io::Read + ::std::io::Write + ::std::io::Seek
{
}

/// Trait for asynchronous file operations
///
/// This trait combines async I/O traits for use with generic async file handling code.
#[cfg(all(feature = "async", feature = "tokio"))]
pub trait GenericAsyncFile:
    Send
    + Sync
    + switchy_async::io::AsyncRead
    + switchy_async::io::AsyncWrite
    + switchy_async::io::AsyncSeek
{
}

#[allow(unused)]
macro_rules! impl_open_options {
    ($(,)?) => {
        /// Options for configuring how a file is opened
        ///
        /// This builder exposes the ability to configure how a file is opened and what operations are permitted on the resulting file.
        #[derive(Clone)]
        pub struct OpenOptions {
            pub(crate) create: bool,
            pub(crate) append: bool,
            pub(crate) read: bool,
            pub(crate) write: bool,
            pub(crate) truncate: bool,
        }

        impl Default for OpenOptions {
            fn default() -> Self {
                Self::new()
            }
        }

        impl OpenOptions {
            /// Creates a new set of options with default values
            ///
            /// All options are initially set to `false`.
            #[must_use]
            pub const fn new() -> Self {
                Self {
                    create: false,
                    append: false,
                    read: false,
                    write: false,
                    truncate: false,
                }
            }

            /// Sets the option to create a new file if it doesn't exist
            #[must_use]
            pub const fn create(mut self, create: bool) -> Self {
                self.create = create;
                self
            }

            /// Sets the option to append to a file
            #[must_use]
            pub const fn append(mut self, append: bool) -> Self {
                self.append = append;
                self
            }

            /// Sets the option to read from a file
            #[must_use]
            pub const fn read(mut self, read: bool) -> Self {
                self.read = read;
                self
            }

            /// Sets the option to write to a file
            #[must_use]
            pub const fn write(mut self, write: bool) -> Self {
                self.write = write;
                self
            }

            /// Sets the option to truncate the file to 0 length if it exists
            #[must_use]
            pub const fn truncate(mut self, truncate: bool) -> Self {
                self.truncate = truncate;
                self
            }
        }
    };
}

#[allow(unused)]
macro_rules! impl_sync_fs {
    ($module:ident $(,)?) => {
        /// Synchronous filesystem operations
        ///
        /// This module provides blocking filesystem operations that can be used in synchronous contexts.
        #[cfg(feature = "sync")]
        pub mod sync {
            pub use $crate::$module::sync::{
                File, create_dir_all, read_dir_sorted, read_to_string, remove_dir_all,
                walk_dir_sorted, write,
            };

            impl_open_options!();
        }

        pub use $crate::$module::exists;
    };
}

#[allow(unused)]
macro_rules! impl_async_fs {
    ($module:ident $(,)?) => {
        /// Asynchronous filesystem operations
        ///
        /// This module provides non-blocking async filesystem operations for use in async contexts.
        #[cfg(feature = "async")]
        pub mod unsync {
            pub use $crate::$module::unsync::{
                File, create_dir_all, read_dir_sorted, read_to_string, remove_dir_all,
                walk_dir_sorted, write,
            };

            impl_open_options!();

            #[cfg(feature = "sync")]
            impl OpenOptions {
                /// Converts async `OpenOptions` into sync `OpenOptions`
                #[must_use]
                pub const fn into_sync(self) -> crate::sync::OpenOptions {
                    crate::sync::OpenOptions {
                        create: self.create,
                        append: self.append,
                        read: self.read,
                        write: self.write,
                        truncate: self.truncate,
                    }
                }
            }

            #[cfg(feature = "sync")]
            impl From<OpenOptions> for crate::sync::OpenOptions {
                fn from(value: OpenOptions) -> Self {
                    value.into_sync()
                }
            }
        }
    };
}

#[cfg(feature = "simulator")]
impl_sync_fs!(simulator);
#[cfg(feature = "simulator")]
impl_async_fs!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "std"))]
impl_sync_fs!(standard);

#[cfg(all(not(feature = "simulator"), feature = "tokio"))]
impl_async_fs!(tokio);

// Conditional compilation for temp_dir module
#[cfg(feature = "simulator")]
pub use simulator::temp_dir;

#[cfg(all(feature = "std", not(feature = "simulator")))]
pub use standard::temp_dir;

// Re-export key types at crate root for convenience
#[cfg(any(feature = "simulator", feature = "std"))]
pub use temp_dir::{TempDir, tempdir, tempdir_in};

#[cfg(all(feature = "std", not(feature = "simulator")))]
pub use temp_dir::Builder;

#[cfg(all(feature = "simulator-real-fs", feature = "simulator"))]
pub use simulator::with_real_fs;

/// Executes a function, always using the actual filesystem (no-op when simulator feature is disabled)
///
/// When the `simulator` feature is disabled, this simply executes the provided closure.
#[cfg(all(feature = "simulator-real-fs", not(feature = "simulator")))]
pub fn with_real_fs<T>(f: impl FnOnce() -> T) -> T {
    f()
}

#[cfg(any(feature = "simulator", feature = "std"))]
#[cfg(test)]
mod temp_dir_tests {
    use super::*;

    #[test]
    fn test_temp_dir_creation() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let path = temp_dir.path();

        // Path should be accessible and not empty
        assert!(!path.to_string_lossy().is_empty());

        // Path should be under /tmp in simulator mode
        #[cfg(feature = "simulator")]
        assert!(path.starts_with("/tmp"));

        // In real mode, directory should exist on filesystem
        #[cfg(all(feature = "std", not(feature = "simulator")))]
        {
            assert!(path.exists());
            assert!(path.is_dir());
        }
    }

    #[test]
    fn test_temp_dir_with_prefix() {
        let temp_dir =
            TempDir::with_prefix("test-prefix-").expect("Failed to create temp directory");
        let path = temp_dir.path();

        // Path should be accessible
        assert!(!path.to_string_lossy().is_empty());

        // In simulator mode, check prefix is used
        #[cfg(feature = "simulator")]
        {
            let file_name = path.file_name().unwrap().to_string_lossy();
            assert!(file_name.starts_with("test-prefix-"));
        }

        // In real mode, directory should exist on filesystem
        #[cfg(all(feature = "std", not(feature = "simulator")))]
        {
            assert!(path.exists());
            assert!(path.is_dir());
        }
    }

    #[test]
    fn test_temp_dir_with_suffix() {
        let temp_dir =
            TempDir::with_suffix("-test-suffix").expect("Failed to create temp directory");
        let path = temp_dir.path();

        // Path should be accessible
        assert!(!path.to_string_lossy().is_empty());

        // In simulator mode, check suffix is used
        #[cfg(feature = "simulator")]
        {
            let file_name = path.file_name().unwrap().to_string_lossy();
            assert!(file_name.ends_with("-test-suffix"));
        }

        // In real mode, directory should exist on filesystem
        #[cfg(all(feature = "std", not(feature = "simulator")))]
        {
            assert!(path.exists());
            assert!(path.is_dir());
        }
    }

    #[test]
    fn test_temp_dir_keep() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let path = temp_dir.path().to_path_buf();

        // Path should be accessible
        assert!(!path.to_string_lossy().is_empty());

        // Keep the directory
        let kept_path = temp_dir.keep();
        assert_eq!(path, kept_path);

        // In real mode, directory should still exist after keep
        #[cfg(all(feature = "std", not(feature = "simulator")))]
        assert!(kept_path.exists());
    }

    #[test]
    fn test_temp_dir_close() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let path = temp_dir.path().to_path_buf();

        // Path should be accessible
        assert!(!path.to_string_lossy().is_empty());

        // Close the directory
        temp_dir.close().expect("Failed to close temp directory");

        // Test passes if close() doesn't error
    }

    #[test]
    fn test_temp_dir_in_custom_location() {
        // First create a temp directory to use as parent
        let parent_temp = tempdir().expect("Failed to create parent temp directory");
        let parent_path = parent_temp.path();

        // Create temp directory inside it
        let temp_dir = tempdir_in(parent_path).expect("Failed to create temp directory");
        let path = temp_dir.path();

        // Path should be accessible and be inside parent
        assert!(!path.to_string_lossy().is_empty());
        assert!(path.starts_with(parent_path));

        // In real mode, directory should exist on filesystem
        #[cfg(all(feature = "std", not(feature = "simulator")))]
        {
            assert!(path.exists());
            assert!(path.is_dir());
        }
    }

    #[cfg(feature = "simulator")]
    #[test]
    fn test_temp_dir_simulator_reset() {
        use crate::simulator::temp_dir::reset_temp_dirs;

        // Reset the simulator state
        reset_temp_dirs();

        // Create a temp directory
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let path = temp_dir.path().to_path_buf();

        // Path should be accessible
        assert!(!path.to_string_lossy().is_empty());

        // Drop the temp directory (should trigger cleanup)
        drop(temp_dir);

        // Test passes - cleanup is handled internally
    }
}
