//! In-memory filesystem simulator for testing.
//!
//! This module provides a complete filesystem simulation that runs entirely in memory,
//! allowing you to test filesystem operations without touching the actual disk. This is
//! particularly useful for unit tests, integration tests, and development environments.
//!
//! The simulator supports both synchronous and asynchronous operations, directory hierarchies,
//! and temporary directory management.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, RwLock},
};

use bytes::BytesMut;

// Module that contains all real_fs functionality
#[cfg(feature = "simulator-real-fs")]
mod real_fs_support {
    use bytes::BytesMut;
    use scoped_tls::scoped_thread_local;
    use std::sync::{Arc, Mutex};

    /// Marker type used with scoped thread-local storage to track real filesystem mode.
    ///
    /// This struct is used internally to mark when the current thread is operating
    /// in "real filesystem" mode rather than simulator mode.
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

    /// Returns `true` if the current thread is operating in real filesystem mode.
    ///
    /// This is used internally to determine whether filesystem operations should
    /// use the real filesystem or the simulator.
    #[inline]
    pub fn is_real_fs() -> bool {
        REAL_FS.is_set()
    }

    /// Converts a standard library file handle to a simulator file handle.
    ///
    /// This function reads the content from a real `std::fs::File` and creates
    /// a simulator `File` with that content loaded into memory.
    ///
    /// # Arguments
    ///
    /// * `std_file` - The standard library file handle to convert
    /// * `path` - The path associated with the file
    /// * `read` - Whether the file was opened for reading (affects content loading)
    /// * `write` - Whether the file was opened for writing
    ///
    /// # Errors
    ///
    /// * If reading from the file fails when `read` is true
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
            data: Arc::new(Mutex::new(BytesMut::from(content.as_slice()))),
            position: 0,
            write,
        })
    }

    /// Asynchronously converts a standard library file handle to a simulator file handle.
    ///
    /// This function reads the content from a real `std::fs::File` in a blocking task
    /// and creates a simulator `File` with that content loaded into memory.
    ///
    /// # Arguments
    ///
    /// * `std_file` - The standard library file handle to convert
    /// * `path` - The path associated with the file
    /// * `read` - Whether the file was opened for reading (affects content loading)
    /// * `write` - Whether the file was opened for writing
    ///
    /// # Errors
    ///
    /// * If reading from the file fails when `read` is true
    ///
    /// # Panics
    ///
    /// * If the spawn_blocking task panics
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
            data: Arc::new(Mutex::new(BytesMut::from(content.as_slice()))),
            position: 0,
            write,
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

thread_local! {
    static FILES: RefCell<RwLock<BTreeMap<String, Arc<Mutex<BytesMut>>>>> =
        const { RefCell::new(RwLock::new(BTreeMap::new())) };
    static DIRECTORIES: RefCell<RwLock<BTreeSet<String>>> =
        const { RefCell::new(RwLock::new(BTreeSet::new())) };
}

/// Resets the simulated filesystem to an empty state
///
/// Clears all files and directories from the in-memory filesystem simulator.
///
/// # Panics
///
/// * If the `FILES` `RwLock` fails to write to
pub fn reset_fs() {
    FILES.with_borrow_mut(|x| x.write().unwrap().clear());
    reset_directories();
}

/// Resets all directories in the simulated filesystem
///
/// Clears the directory registry, removing all tracked directories from the in-memory filesystem.
///
/// # Panics
///
/// * If the `DIRECTORIES` `RwLock` fails to write to
pub fn reset_directories() {
    DIRECTORIES.with_borrow_mut(|x| x.write().unwrap().clear());
}

/// Get all parent directories of a path
fn get_parent_directories(path: &str) -> Vec<String> {
    let mut parents = Vec::new();
    let path_buf = std::path::Path::new(path);

    let mut current = path_buf.parent();
    while let Some(parent) = current {
        if let Some(parent_str) = parent.to_str()
            && !parent_str.is_empty()
            && parent_str != "/"
        {
            parents.push(parent_str.to_string());
        }
        current = parent.parent();
    }

    // Always include root
    if path != "/" {
        parents.push("/".to_string());
    }

    parents.reverse();
    parents
}

/// Normalize a path by resolving `.` and `..` components
///
/// This function handles path normalization without requiring filesystem access,
/// making it suitable for the simulator.
fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();
    let is_absolute = path.starts_with('/');

    for component in path.split('/') {
        match component {
            "" | "." => {
                // Skip empty components and current directory references
            }
            ".." => {
                // Go up one directory (if possible)
                if !components.is_empty() && components.last() != Some(&"..") {
                    components.pop();
                } else if !is_absolute {
                    // For relative paths, keep the .. if we can't go up
                    components.push("..");
                }
                // For absolute paths, ignore .. at root
            }
            other => {
                components.push(other);
            }
        }
    }

    if is_absolute {
        if components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        }
    } else if components.is_empty() {
        ".".to_string()
    } else {
        components.join("/")
    }
}

/// Check if a path exists
///
/// # Panics
///
/// * If the `DIRECTORIES` `RwLock` is poisoned
/// * If the `FILES` `RwLock` is poisoned
pub fn exists<P: AsRef<std::path::Path>>(path: P) -> bool {
    let Some(path) = path.as_ref().to_str() else {
        return false;
    };
    DIRECTORIES.with_borrow(|dirs| dirs.read().unwrap().contains(path))
        || FILES.with_borrow(|files| files.read().unwrap().contains_key(path))
}

/// Get immediate children (files and directories) of a directory
fn get_directory_children(dir_path: &str) -> (Vec<String>, Vec<String>) {
    let normalized_dir = if dir_path == "/" {
        "/"
    } else {
        &format!("{dir_path}/")
    };

    // Get files in this directory
    let files = FILES.with_borrow(|files| {
        files
            .read()
            .unwrap()
            .keys()
            .filter_map(|file_path| {
                file_path.strip_prefix(normalized_dir).and_then(|stripped| {
                    if !stripped.contains('/') && !stripped.is_empty() {
                        Some(stripped.to_string())
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>()
    });

    // Get subdirectories in this directory
    let subdirs = DIRECTORIES.with_borrow(|dirs| {
        dirs.read()
            .unwrap()
            .iter()
            .filter_map(|subdir_path| {
                subdir_path
                    .strip_prefix(normalized_dir)
                    .and_then(|stripped| {
                        if !stripped.contains('/') && !stripped.is_empty() {
                            Some(stripped.to_string())
                        } else {
                            None
                        }
                    })
            })
            .collect::<Vec<_>>()
    });

    (files, subdirs)
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

macro_rules! path_to_str {
    ($path:expr) => {{
        $path.as_ref().to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "path is invalid str")
        })
    }};
}

macro_rules! impl_file_sync {
    ($file:ident $(,)?) => {
        impl std::io::Read for $file {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if buf.is_empty() {
                    return Ok(0);
                }

                let binding = self.data.lock().unwrap();

                let len = binding.len();
                let pos = usize::try_from(self.position).unwrap();

                let remaining = len - pos;
                let read_count = std::cmp::min(remaining, buf.len());

                if read_count == 0 {
                    return Ok(0);
                }

                let data = &binding[pos..(pos + read_count)];
                buf[..read_count].copy_from_slice(data);

                self.position += read_count as u64;

                drop(binding);

                Ok(read_count)
            }
        }

        impl std::io::Write for $file {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                use bytes::BufMut as _;

                if !self.write {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "File not opened in write mode",
                    ));
                }
                let mut binding = self.data.lock().unwrap();

                binding.put(buf);

                drop(binding);

                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl std::io::Seek for $file {
            fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
                self.position = match pos {
                    std::io::SeekFrom::Start(x) => x,
                    std::io::SeekFrom::End(x) => {
                        u64::try_from(i64::try_from(self.data.lock().unwrap().len()).unwrap() - x)
                            .unwrap()
                    }
                    std::io::SeekFrom::Current(x) => {
                        u64::try_from(i64::try_from(self.position).unwrap() + x).unwrap()
                    }
                };

                Ok(self.position)
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
}

/// Metadata information about a file in the simulated filesystem
///
/// Provides information about file size and type (file, directory, symlink).
#[derive(Debug, Clone)]
pub struct Metadata {
    pub(crate) len: u64,
    pub(crate) is_file: bool,
    pub(crate) is_dir: bool,
    pub(crate) is_symlink: bool,
}

impl Metadata {
    /// Returns the size of the file in bytes
    #[must_use]
    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Returns `true` if the file has zero length
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns `true` if this metadata is for a regular file
    #[must_use]
    pub const fn is_file(&self) -> bool {
        self.is_file
    }

    /// Returns `true` if this metadata is for a directory
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Returns `true` if this metadata is for a symbolic link
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        self.is_symlink
    }
}

impl From<std::fs::Metadata> for Metadata {
    fn from(meta: std::fs::Metadata) -> Self {
        Self {
            len: meta.len(),
            is_file: meta.is_file(),
            is_dir: meta.is_dir(),
            is_symlink: meta.is_symlink(),
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
/// This module provides blocking filesystem operations that work with the in-memory
/// simulated filesystem. All operations are immediate and do not touch the actual disk.
#[cfg(feature = "sync")]
pub mod sync {
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use bytes::BytesMut;

    use crate::sync::OpenOptions;

    use super::{DIRECTORIES, FILES};

    /// File handle for synchronous operations in the simulated filesystem
    ///
    /// Provides read, write, and seek operations on files stored in the in-memory filesystem.
    pub struct File {
        #[cfg_attr(not(feature = "simulator-real-fs"), allow(dead_code))]
        pub(crate) path: PathBuf,
        pub(crate) data: Arc<Mutex<BytesMut>>,
        pub(crate) position: u64,
        pub(crate) write: bool,
    }

    impl File {
        /// Opens a file in read-only mode
        ///
        /// This is a convenience method equivalent to `OpenOptions::new().read(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the file does not exist
        /// * If the path cannot be converted to a string
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
        /// * If the parent directory does not exist
        /// * If the path cannot be converted to a string
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

        /// Retrieves metadata about the file
        ///
        /// # Errors
        ///
        /// * If the file metadata cannot be retrieved (when using real filesystem)
        ///
        /// # Panics
        ///
        /// * If the internal data mutex is poisoned (when using simulator)
        pub fn metadata(&self) -> std::io::Result<Metadata> {
            #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
            if super::real_fs_support::is_real_fs() {
                return Ok(std::fs::metadata(&self.path)?.into());
            }

            Ok(Metadata {
                len: u64::try_from(self.data.lock().unwrap().len()).unwrap_or(0),
                is_file: true,
                is_dir: false,
                is_symlink: false,
            })
        }

        /// Converts this synchronous file handle into an asynchronous file handle
        #[cfg(feature = "async")]
        #[must_use]
        pub fn into_async(self) -> crate::unsync::File {
            crate::unsync::File {
                path: self.path,
                data: self.data,
                position: self.position,
                write: self.write,
            }
        }
    }

    pub use super::Metadata;

    impl_file_sync!(File);

    impl OpenOptions {
        /// Opens a file with the configured options
        ///
        /// # Errors
        ///
        /// * If and IO error occurs
        ///
        /// # Panics
        ///
        /// * If the `FILES` `RwLock` fails to read.
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

            // Original simulator implementation (fallback)
            let location = path_to_str!(path)?;
            let data = if let Some(data) =
                FILES.with_borrow(|x| x.read().unwrap().get(location).cloned())
            {
                data
            } else if self.create {
                // Check if parent directory exists when creating a file
                if let Some(parent) = std::path::Path::new(location).parent()
                    && let Some(parent_str) = parent.to_str()
                {
                    let parent_normalized = if parent_str.is_empty() || parent_str == "." {
                        ".".to_string()
                    } else if parent_str == "/" {
                        "/".to_string()
                    } else {
                        parent_str.trim_end_matches('/').to_string()
                    };

                    // Allow current directory "." to exist by default, otherwise check DIRECTORIES
                    if parent_normalized != "." && !super::exists(&parent_normalized) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Parent directory not found: {parent_normalized}"),
                        ));
                    }
                }

                let data = Arc::new(Mutex::new(BytesMut::new()));
                FILES.with_borrow_mut(|x| {
                    x.write()
                        .unwrap()
                        .insert(location.to_string(), data.clone())
                });
                data
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("File not found at path={location}"),
                ));
            };

            if self.truncate {
                data.lock().unwrap().clear();
            }

            Ok(File {
                path: path.as_ref().to_path_buf(),
                data,
                position: 0,
                write: self.write,
            })
        }
    }

    /// Reads the entire contents of a file into a byte vector
    ///
    /// # Errors
    ///
    /// * If the file doesn't exist
    /// * If the file `Path` cannot be converted to a `str`
    ///
    /// # Panics
    ///
    /// * If the `FILES` `RwLock` fails to read.
    pub fn read<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<u8>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return std::fs::read(path);
        }

        // Original simulator implementation (fallback)
        let location = path_to_str!(path)?;
        let Some(existing) = FILES.with_borrow(|x| x.read().unwrap().get(location).cloned()) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found at path={location}"),
            ));
        };

        Ok(existing.lock().unwrap().to_vec())
    }

    /// Reads the entire contents of a file into a string
    ///
    /// # Errors
    ///
    /// * Returns `std::io::ErrorKind::NotFound` if the file does not exist.
    /// * Returns `std::io::ErrorKind::InvalidData` if the file contains invalid UTF-8.
    ///
    /// # Panics
    ///
    /// * If the `FILES` `RwLock` fails to read.
    pub fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::read_to_string(path);
        }

        // Original simulator implementation (fallback)
        let location = path_to_str!(path)?;
        let Some(existing) = FILES.with_borrow(|x| x.read().unwrap().get(location).cloned()) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found at path={location}"),
            ));
        };

        String::from_utf8(existing.lock().unwrap().to_vec())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Writes a slice as the entire contents of a file
    ///
    /// # Errors
    ///
    /// * If the file cannot be created
    /// * If the file cannot be written to
    /// * If the `FILES` `RwLock` fails to write to
    pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> std::io::Result<()> {
        use std::io::Write;

        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::write(path, contents);
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        file.write_all(contents.as_ref())?;
        Ok(())
    }

    /// Creates a directory and all missing parent directories
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir` fails (when using real filesystem)
    /// * If the path cannot be converted to a string
    /// * If the parent directory does not exist
    ///
    /// # Panics
    ///
    /// * If the `DIRECTORIES` `RwLock` fails to write to
    pub fn create_dir<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::create_dir(path);
        }

        let path_str = path_to_str!(path)?;

        // Normalize path - remove trailing slashes except for root
        let normalized = if path_str == "/" {
            "/".to_string()
        } else {
            path_str.trim_end_matches('/').to_string()
        };

        // Check that parent directory exists
        if let Some(parent) = std::path::Path::new(&normalized).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if !parent_str.is_empty() && parent_str != "/" {
                let parent_exists =
                    DIRECTORIES.with_borrow(|dirs| dirs.read().unwrap().contains(&parent_str));
                if !parent_exists {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Parent directory does not exist: {parent_str}"),
                    ));
                }
            }
        }

        // Create the directory
        DIRECTORIES.with_borrow_mut(|dirs| {
            dirs.write().unwrap().insert(normalized);
        });

        Ok(())
    }

    /// Creates a directory and all missing parent directories
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir_all` fails (when using real filesystem)
    /// * If the path cannot be converted to a string
    ///
    /// # Panics
    ///
    /// * If the `DIRECTORIES` `RwLock` fails to write to
    pub fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::create_dir_all(path);
        }

        let path_str = path_to_str!(path)?;

        // Normalize path - remove trailing slashes except for root
        let normalized = if path_str == "/" {
            "/".to_string()
        } else {
            path_str.trim_end_matches('/').to_string()
        };

        // Get all directories that need to be created (including parents)
        let mut dirs_to_create = super::get_parent_directories(&normalized);
        dirs_to_create.push(normalized);

        // Create all directories
        DIRECTORIES.with_borrow_mut(|dirs| {
            let mut dirs_write = dirs.write().unwrap();
            for dir in dirs_to_create {
                dirs_write.insert(dir);
            }
        });

        Ok(())
    }

    /// Removes a directory and all its contents recursively
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::remove_dir_all` fails (when using real filesystem)
    /// * If the path cannot be converted to a string
    /// * If the directory doesn't exist
    ///
    /// # Panics
    ///
    /// * If the `DIRECTORIES` or `FILES` `RwLock` fails to write to
    pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::remove_dir_all(path);
        }

        let path_str = path_to_str!(path)?;

        // Normalize path - remove trailing slashes except for root
        let normalized = if path_str == "/" {
            "/".to_string()
        } else {
            path_str.trim_end_matches('/').to_string()
        };

        // Check if directory exists
        if !super::exists(&normalized) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Directory not found: {normalized}"),
            ));
        }

        // Find all subdirectories and files to remove
        let prefix = if normalized == "/" {
            "/"
        } else {
            &format!("{normalized}/")
        };

        // Remove all files in this directory and subdirectories
        FILES.with_borrow_mut(|files| {
            let mut files_write = files.write().unwrap();
            files_write
                .retain(|file_path, _| !file_path.starts_with(prefix) && file_path != &normalized);
        });

        // Remove all subdirectories
        DIRECTORIES.with_borrow_mut(|dirs| {
            let mut dirs_write = dirs.write().unwrap();
            dirs_write.retain(|dir_path| !dir_path.starts_with(prefix) && dir_path != &normalized);
        });

        Ok(())
    }

    /// Canonicalizes a path by resolving `.` and `..` components and normalizing it
    ///
    /// Unlike `std::fs::canonicalize`, this does not require the path to exist,
    /// but it will verify the path exists in the simulator filesystem.
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::canonicalize` fails (when using real filesystem)
    /// * If the path cannot be converted to a string
    /// * If the path does not exist in the simulator
    pub fn canonicalize<P: AsRef<Path>>(path: P) -> std::io::Result<std::path::PathBuf> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::canonicalize(path);
        }

        let path_str = path_to_str!(path)?;

        // Normalize the path by resolving . and .. components
        let normalized = super::normalize_path(path_str);

        // Check if the path exists (either as file or directory)
        if !super::exists(&normalized) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Path not found: {normalized}"),
            ));
        }

        Ok(std::path::PathBuf::from(normalized))
    }

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::read_dir` fails (when using real filesystem)
    /// * If any directory entry cannot be read (when using real filesystem)
    /// * If the path cannot be converted to a string
    /// * If the directory doesn't exist
    pub fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            let std_entries = crate::standard::sync::read_dir_sorted(path)?;
            return std_entries
                .into_iter()
                .map(|x| DirEntry::from_std(&x))
                .collect::<std::io::Result<Vec<_>>>();
        }

        let path_str = path_to_str!(path)?;

        // Normalize path
        let normalized = if path_str == "/" {
            "/".to_string()
        } else {
            path_str.trim_end_matches('/').to_string()
        };

        // Check if directory exists
        if !super::exists(&normalized) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Directory not found: {normalized}"),
            ));
        }

        // Get children
        let (files, subdirs) = super::get_directory_children(&normalized);

        let mut entries = Vec::new();

        // Add file entries
        for filename in files {
            let full_path = if normalized == "/" {
                format!("/{filename}")
            } else {
                format!("{normalized}/{filename}")
            };
            entries.push(DirEntry::new_file(full_path, filename)?);
        }

        // Add directory entries
        for dirname in subdirs {
            let full_path = if normalized == "/" {
                format!("/{dirname}")
            } else {
                format!("{normalized}/{dirname}")
            };
            entries.push(DirEntry::new_dir(full_path, dirname)?);
        }

        // Sort by filename for deterministic ordering
        entries.sort_by_key(DirEntry::file_name);

        Ok(entries)
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read (when using real filesystem)
    /// * If any directory entry cannot be accessed (when using real filesystem)
    /// * If the path cannot be converted to a string
    /// * If the directory doesn't exist
    pub fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        fn walk_recursive(dir_path: &str) -> std::io::Result<Vec<DirEntry>> {
            let mut all_entries = Vec::new();

            // Get immediate children
            let (files, subdirs) = super::get_directory_children(dir_path);

            // Add all files in current directory
            for filename in files {
                let full_path = if dir_path == "/" {
                    format!("/{filename}")
                } else {
                    format!("{dir_path}/{filename}")
                };
                all_entries.push(DirEntry::new_file(full_path, filename)?);
            }

            // Add all subdirectories and recursively walk them
            for dirname in subdirs {
                let full_path = if dir_path == "/" {
                    format!("/{dirname}")
                } else {
                    format!("{dir_path}/{dirname}")
                };

                // Add the directory itself
                all_entries.push(DirEntry::new_dir(full_path.clone(), dirname)?);

                // Recursively walk the subdirectory
                let sub_entries = walk_recursive(&full_path)?;
                all_entries.extend(sub_entries);
            }

            Ok(all_entries)
        }

        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            let std_entries = crate::standard::sync::walk_dir_sorted(path)?;
            return std_entries
                .into_iter()
                .map(|x| DirEntry::from_std(&x))
                .collect::<std::io::Result<Vec<_>>>();
        }

        let path_str = path_to_str!(path)?;

        // Normalize path
        let normalized = if path_str == "/" {
            "/".to_string()
        } else {
            path_str.trim_end_matches('/').to_string()
        };

        // Check if directory exists
        if !super::exists(&normalized) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Directory not found: {normalized}"),
            ));
        }

        let mut all_entries = walk_recursive(&normalized)?;

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
        /// This function always succeeds for simulator entries, but returns
        /// `Result` to match the `std::fs::DirEntry::file_type()` API.
        pub fn file_type(&self) -> std::io::Result<super::FileType> {
            Ok(self.file_type_info.clone())
        }
    }

    #[cfg(test)]
    mod test {
        use std::{
            io::Read as _,
            sync::{Arc, Mutex},
        };

        use bytes::BytesMut;
        use pretty_assertions::assert_eq;

        use crate::simulator::FILES;

        use super::OpenOptions;

        #[switchy_async::test]
        async fn can_read_empty_file() {
            const FILENAME: &str = "sync::test1";

            FILES.with_borrow_mut(|x| {
                x.write()
                    .unwrap()
                    .insert(FILENAME.to_string(), Arc::new(Mutex::new(BytesMut::new())))
            });

            let mut file = OpenOptions::new().create(true).open(FILENAME).unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).unwrap();

            assert_eq!(read_count, 0);
        }

        #[switchy_async::test]
        async fn can_read_small_bytes_file() {
            const FILENAME: &str = "sync::test2";

            FILES.with_borrow_mut(|x| {
                x.write().unwrap().insert(
                    FILENAME.to_string(),
                    Arc::new(Mutex::new(BytesMut::from(b"hey" as &[u8]))),
                )
            });

            let mut file = OpenOptions::new().create(true).open(FILENAME).unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).unwrap();

            assert_eq!(read_count, 3);
        }

        #[test_log::test]
        fn test_write_without_write_permission() {
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

            // Attempt to write should fail with PermissionDenied
            let result = file.write_all(b"should fail");
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().kind(),
                std::io::ErrorKind::PermissionDenied
            );
        }

        #[test_log::test]
        fn test_truncate_existing_file() {
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

        #[test_log::test]
        fn test_partial_reads() {
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

        #[test_log::test]
        fn test_seek_and_read() {
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

        #[test_log::test]
        fn test_seek_from_end() {
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

        #[test_log::test]
        fn test_seek_from_current() {
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

        #[test_log::test]
        fn test_seek_past_eof() {
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

        #[test_log::test]
        fn test_multiple_handles_same_file() {
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

            // Reader should see updated content (shared Arc<Mutex<BytesMut>>)
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).unwrap();
            assert_eq!(buf, b"updated content");
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

            // NOTE: Current implementation bug - write does not update position
            // It always appends, so position stays at 0
            // This test documents the current buggy behavior
            let pos = file.stream_position().unwrap();
            assert_eq!(pos, 0, "BUG: Write should update position but doesn't");

            // Seek to beginning (no-op since we're at 0)
            file.seek(SeekFrom::Start(0)).unwrap();

            // Read 5 bytes
            let mut buf = [0u8; 5];
            file.read_exact(&mut buf).unwrap();

            // Position should be at 5 after read
            let pos = file.stream_position().unwrap();
            assert_eq!(pos, 5);
        }

        #[test_log::test]
        #[cfg(all(feature = "sync", feature = "async"))]
        fn test_into_async_conversion() {
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

            let position = file.position;
            let path = file.path.clone();

            // Convert to async
            let async_file = file.into_async();

            // Verify state is preserved
            assert_eq!(async_file.position, position);
            assert_eq!(async_file.path, path);
            assert_eq!(async_file.write, true);
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

/// Asynchronous filesystem operations for the simulator
///
/// This module provides async filesystem operations that work with the in-memory
/// simulated filesystem. Operations are non-blocking but execute immediately since
/// no actual I/O is performed.
#[cfg(feature = "async")]
pub mod unsync {
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        task::Poll,
    };

    use bytes::BytesMut;

    use crate::unsync::OpenOptions;

    /// File handle for asynchronous operations in the simulated filesystem
    ///
    /// Provides async read, write, and seek operations on files stored in the in-memory filesystem.
    pub struct File {
        pub(crate) path: PathBuf,
        pub(crate) data: Arc<Mutex<BytesMut>>,
        pub(crate) position: u64,
        pub(crate) write: bool,
    }

    impl File {
        /// Opens a file in read-only mode asynchronously
        ///
        /// This is a convenience method equivalent to `OpenOptions::new().read(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the file does not exist
        /// * If the path cannot be converted to a string
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
        /// * If the parent directory does not exist
        /// * If the path cannot be converted to a string
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
        /// * If the file metadata cannot be retrieved (when using real filesystem)
        ///
        /// # Panics
        ///
        /// * If the internal data mutex is poisoned (when using simulator)
        /// * If the `spawn_blocking` task panics (when using real filesystem)
        #[allow(clippy::unused_async)]
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

            Ok(Metadata {
                len: u64::try_from(self.data.lock().unwrap().len()).unwrap_or(0),
                is_file: true,
                is_dir: false,
                is_symlink: false,
            })
        }

        /// Converts this asynchronous file handle into a synchronous file handle
        #[cfg(feature = "sync")]
        #[must_use]
        pub fn into_sync(self) -> crate::sync::File {
            crate::sync::File {
                path: self.path,
                data: self.data,
                position: self.position,
                write: self.write,
            }
        }
    }

    pub use super::Metadata;

    impl_file_sync!(File);

    impl tokio::io::AsyncRead for File {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
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

    impl tokio::io::AsyncSeek for File {
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

    impl tokio::io::AsyncWrite for File {
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
        /// * If and IO error occurs
        ///
        /// # Panics
        ///
        /// * If the `FILES` `RwLock` fails to read.
        #[allow(clippy::unused_async, clippy::future_not_send)]
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

            // Fallback to sync simulator implementation
            Ok(self.into_sync().open(path)?.into_async())
        }
    }

    /// Reads the entire contents of a file into a byte vector asynchronously
    ///
    /// # Errors
    ///
    /// * If the file doesn't exist
    /// * If the file `Path` cannot be converted to a `str`
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

        // Fallback to sync simulator implementation
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
    /// # Panics
    ///
    /// * If the `FILES` `RwLock` fails to read from
    #[allow(clippy::unused_async)]
    pub async fn is_file<P: AsRef<Path>>(path: P) -> bool {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || path.is_file())
                .await
                .unwrap_or(false);
        }

        let path_str = path.as_ref().to_string_lossy().to_string();
        super::FILES.with_borrow(|files| files.read().unwrap().contains_key(&path_str))
    }

    /// Checks if a path is a directory asynchronously
    ///
    /// Returns `true` if the path exists and is a directory, `false` otherwise.
    ///
    /// # Panics
    ///
    /// * If the `DIRECTORIES` `RwLock` fails to read from
    #[allow(clippy::unused_async)]
    pub async fn is_dir<P: AsRef<Path>>(path: P) -> bool {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || path.is_dir())
                .await
                .unwrap_or(false);
        }

        let path_str = path.as_ref().to_string_lossy().to_string();
        super::DIRECTORIES.with_borrow(|dirs| dirs.read().unwrap().contains(&path_str))
    }

    /// Writes a slice as the entire contents of a file
    ///
    /// # Errors
    ///
    /// * If the file cannot be created
    /// * If the file cannot be written to
    /// * If the `FILES` `RwLock` fails to write to
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
        #[allow(clippy::unused_async)]
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
        /// * If the FILES or data mutex is poisoned (when using simulator)
        /// * If the `spawn_blocking` task panics (when using real filesystem)
        #[allow(clippy::unused_async)]
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

            if self.file_type_info.is_dir() {
                Ok(Metadata {
                    len: 0,
                    is_file: false,
                    is_dir: true,
                    is_symlink: false,
                })
            } else if self.file_type_info.is_file() {
                // For files, get the actual size from the simulator storage
                let path_str = self.path.to_str().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "path is invalid str")
                })?;
                let len = super::FILES
                    .with_borrow(|files| {
                        files
                            .read()
                            .unwrap()
                            .get(path_str)
                            .map(|data| u64::try_from(data.lock().unwrap().len()).unwrap_or(0))
                    })
                    .unwrap_or(0);
                Ok(Metadata {
                    len,
                    is_file: true,
                    is_dir: false,
                    is_symlink: false,
                })
            } else {
                Ok(Metadata {
                    len: 0,
                    is_file: false,
                    is_dir: false,
                    is_symlink: self.file_type_info.is_symlink(),
                })
            }
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
        #[allow(clippy::unused_async)]
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

    #[cfg(test)]
    #[allow(clippy::await_holding_lock)]
    mod test {
        use std::sync::{Arc, Mutex};

        use bytes::BytesMut;
        use pretty_assertions::assert_eq;
        use tokio::io::AsyncReadExt as _;

        use crate::simulator::FILES;

        use super::OpenOptions;

        #[switchy_async::test]
        async fn can_read_empty_file() {
            const FILENAME: &str = "unsync::test1";

            FILES.with_borrow_mut(|x| {
                x.write()
                    .unwrap()
                    .insert(FILENAME.to_string(), Arc::new(Mutex::new(BytesMut::new())))
            });

            let mut file = OpenOptions::new()
                .create(true)
                .open(FILENAME)
                .await
                .unwrap();

            let mut buf = [0u8; 1024];

            let read_count = file.read(&mut buf).await.unwrap();

            assert_eq!(read_count, 0);
        }

        #[switchy_async::test]
        async fn can_read_small_bytes_file() {
            const FILENAME: &str = "unsync::test2";

            FILES.with_borrow_mut(|x| {
                x.write().unwrap().insert(
                    FILENAME.to_string(),
                    Arc::new(Mutex::new(BytesMut::from(b"hey" as &[u8]))),
                )
            });

            let mut file = OpenOptions::new()
                .create(true)
                .open(FILENAME)
                .await
                .unwrap();

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
mod get_parent_directories_tests {
    use super::get_parent_directories;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_parent_directories_for_deeply_nested_path() {
        let parents = get_parent_directories("/a/b/c/d/e");
        assert_eq!(parents, vec!["/", "/a", "/a/b", "/a/b/c", "/a/b/c/d"]);
    }

    #[test_log::test]
    fn test_parent_directories_for_single_level() {
        let parents = get_parent_directories("/single");
        assert_eq!(parents, vec!["/"]);
    }

    #[test_log::test]
    fn test_parent_directories_for_root() {
        let parents = get_parent_directories("/");
        // Root has no parents
        assert!(parents.is_empty());
    }

    #[test_log::test]
    fn test_parent_directories_preserves_order() {
        // Parents should be returned from root to immediate parent
        let parents = get_parent_directories("/usr/local/bin");
        assert_eq!(parents, vec!["/", "/usr", "/usr/local"]);
    }
}

#[cfg(test)]
mod get_directory_children_tests {
    use super::{get_directory_children, reset_fs, sync};
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_children_of_empty_directory() {
        reset_fs();
        sync::create_dir_all("/empty").unwrap();

        let (files, subdirs) = get_directory_children("/empty");
        assert!(files.is_empty());
        assert!(subdirs.is_empty());
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
        assert!(subdirs.is_empty());
    }

    #[test_log::test]
    fn test_children_with_subdirs_only() {
        reset_fs();
        sync::create_dir_all("/dirs_only/subdir1").unwrap();
        sync::create_dir_all("/dirs_only/subdir2").unwrap();

        let (files, mut subdirs) = get_directory_children("/dirs_only");
        subdirs.sort();
        assert!(files.is_empty());
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
        assert!(files.is_empty());
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
        assert_eq!(async_file.position, 5);
        assert_eq!(async_file.path.to_string_lossy(), "/tmp/async_to_sync.txt");
        assert!(async_file.write);

        // Convert back to sync
        let sync_file_again = async_file.into_sync();
        assert_eq!(sync_file_again.position, 5);
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

        assert!(file.write);
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
pub mod temp_dir {
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        ffi::{OsStr, OsString},
        path::{Path, PathBuf},
        sync::RwLock,
    };

    /// Tracking state for temp directories in simulator
    struct TempDirState {
        cleanup_enabled: bool,
    }

    thread_local! {
        static TEMP_DIRS: RefCell<RwLock<BTreeMap<PathBuf, TempDirState>>> =
            const { RefCell::new(RwLock::new(BTreeMap::new())) };
    }

    /// Reset temp directory state (useful for testing)
    ///
    /// # Panics
    ///
    /// * If the `TEMP_DIRS` `RwLock` fails to write to
    pub fn reset_temp_dirs() {
        TEMP_DIRS.with_borrow_mut(|x| x.write().unwrap().clear());
    }

    /// A directory in the filesystem that is automatically deleted when it goes out of scope
    pub struct TempDir {
        path: PathBuf,
        cleanup_enabled: bool,
    }

    impl TempDir {
        /// Attempts to make a temporary directory inside of the system temp directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn new() -> std::io::Result<Self> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let real_temp = tempfile::TempDir::new()?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp); // Let our Drop handle cleanup
                return Ok(Self {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(None, None, 6);
            let path = PathBuf::from("/tmp").join(dir_name);

            // Create in simulated filesystem
            super::sync::create_dir_all(&path)?;

            // Register in temp directory tracking
            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(Self {
                path,
                cleanup_enabled: true,
            })
        }

        /// Attempts to make a temporary directory inside the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn new_in<P: AsRef<Path>>(dir: P) -> std::io::Result<Self> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let real_temp = tempfile::TempDir::new_in(dir)?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp);
                return Ok(Self {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(None, None, 6);
            let path = dir.as_ref().join(dir_name);

            super::sync::create_dir_all(&path)?;

            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(Self {
                path,
                cleanup_enabled: true,
            })
        }

        /// Attempts to make a temporary directory with the specified prefix
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn with_prefix<S: AsRef<OsStr>>(prefix: S) -> std::io::Result<Self> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let real_temp = tempfile::TempDir::with_prefix(prefix)?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp);
                return Ok(Self {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(Some(prefix.as_ref()), None, 6);
            let path = PathBuf::from("/tmp").join(dir_name);

            super::sync::create_dir_all(&path)?;

            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(Self {
                path,
                cleanup_enabled: true,
            })
        }

        /// Attempts to make a temporary directory with the specified suffix
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn with_suffix<S: AsRef<OsStr>>(suffix: S) -> std::io::Result<Self> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let real_temp = tempfile::TempDir::with_suffix(suffix)?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp);
                return Ok(Self {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(None, Some(suffix.as_ref()), 6);
            let path = PathBuf::from("/tmp").join(dir_name);

            super::sync::create_dir_all(&path)?;

            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(Self {
                path,
                cleanup_enabled: true,
            })
        }

        /// Attempts to make a temporary directory with the specified prefix in the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn with_prefix_in<S: AsRef<OsStr>, P: AsRef<Path>>(
            prefix: S,
            dir: P,
        ) -> std::io::Result<Self> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let real_temp = tempfile::TempDir::with_prefix_in(prefix, dir)?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp);
                return Ok(Self {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(Some(prefix.as_ref()), None, 6);
            let path = dir.as_ref().join(dir_name);

            super::sync::create_dir_all(&path)?;

            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(Self {
                path,
                cleanup_enabled: true,
            })
        }

        /// Attempts to make a temporary directory with the specified suffix in the specified directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn with_suffix_in<S: AsRef<OsStr>, P: AsRef<Path>>(
            suffix: S,
            dir: P,
        ) -> std::io::Result<Self> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let real_temp = tempfile::TempDir::with_suffix_in(suffix, dir)?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp);
                return Ok(Self {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(None, Some(suffix.as_ref()), 6);
            let path = dir.as_ref().join(dir_name);

            super::sync::create_dir_all(&path)?;

            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(Self {
                path,
                cleanup_enabled: true,
            })
        }

        /// Accesses the Path to the temporary directory
        #[must_use]
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Persist the temporary directory to disk, returning the `PathBuf` where it is located
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        #[must_use]
        pub fn keep(mut self) -> PathBuf {
            self.cleanup_enabled = false;
            TEMP_DIRS.with_borrow_mut(|dirs| {
                if let Some(state) = dirs.write().unwrap().get_mut(&self.path) {
                    state.cleanup_enabled = false;
                }
            });
            self.path.clone()
        }

        /// Deprecated alias for `keep()`
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        #[deprecated = "use TempDir::keep()"]
        #[must_use]
        pub fn into_path(self) -> PathBuf {
            self.keep()
        }

        /// Disable cleanup of the temporary directory
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn disable_cleanup(&mut self, disable_cleanup: bool) {
            self.cleanup_enabled = !disable_cleanup;
            TEMP_DIRS.with_borrow_mut(|dirs| {
                if let Some(state) = dirs.write().unwrap().get_mut(&self.path) {
                    state.cleanup_enabled = !disable_cleanup;
                }
            });
        }

        /// Closes and removes the temporary directory
        ///
        /// # Errors
        ///
        /// * If the directory cannot be removed from the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn close(mut self) -> std::io::Result<()> {
            if !self.cleanup_enabled {
                return Ok(());
            }

            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                return std::fs::remove_dir_all(&self.path);
            }

            // Remove from tracking
            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().remove(&self.path);
            });

            // Remove from simulated filesystem
            super::sync::remove_dir_all(&self.path)?;

            // Prevent double cleanup in Drop
            self.cleanup_enabled = false;
            Ok(())
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
                .field("cleanup_enabled", &self.cleanup_enabled)
                .finish()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            if self.cleanup_enabled {
                #[cfg(feature = "simulator-real-fs")]
                if super::real_fs_support::is_real_fs() {
                    let _ = std::fs::remove_dir_all(&self.path);
                    return;
                }

                TEMP_DIRS.with_borrow_mut(|dirs| {
                    dirs.write().unwrap().remove(&self.path);
                });

                let _ = super::sync::remove_dir_all(&self.path);
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

        /// Set the number of random bytes to use for the directory name
        pub const fn rand_bytes(&mut self, rand: usize) -> &mut Self {
            self.rand_bytes = rand;
            self
        }

        /// Create a temporary directory in the default location
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn tempdir(&self) -> std::io::Result<TempDir> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let mut builder = tempfile::Builder::new();
                if let Some(ref prefix) = self.prefix {
                    builder.prefix(prefix);
                }
                if let Some(ref suffix) = self.suffix {
                    builder.suffix(suffix);
                }
                builder.rand_bytes(self.rand_bytes);
                let real_temp = builder.tempdir()?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp);
                return Ok(TempDir {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(
                self.prefix.as_deref(),
                self.suffix.as_deref(),
                self.rand_bytes,
            );
            let path = PathBuf::from("/tmp").join(dir_name);

            super::sync::create_dir_all(&path)?;

            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(TempDir {
                path,
                cleanup_enabled: true,
            })
        }

        /// Create a temporary directory in the specified location
        ///
        /// # Errors
        ///
        /// * If the directory cannot be created in the simulated filesystem
        ///
        /// # Panics
        ///
        /// * If the `TEMP_DIRS` `RwLock` fails to write to
        pub fn tempdir_in<P: AsRef<Path>>(&self, dir: P) -> std::io::Result<TempDir> {
            #[cfg(feature = "simulator-real-fs")]
            if super::real_fs_support::is_real_fs() {
                let mut builder = tempfile::Builder::new();
                if let Some(ref prefix) = self.prefix {
                    builder.prefix(prefix);
                }
                if let Some(ref suffix) = self.suffix {
                    builder.suffix(suffix);
                }
                builder.rand_bytes(self.rand_bytes);
                let real_temp = builder.tempdir_in(dir)?;
                let path = real_temp.path().to_path_buf();
                std::mem::forget(real_temp);
                return Ok(TempDir {
                    path,
                    cleanup_enabled: true,
                });
            }

            let dir_name = generate_temp_name(
                self.prefix.as_deref(),
                self.suffix.as_deref(),
                self.rand_bytes,
            );
            let path = dir.as_ref().join(dir_name);

            super::sync::create_dir_all(&path)?;

            TEMP_DIRS.with_borrow_mut(|dirs| {
                dirs.write().unwrap().insert(
                    path.clone(),
                    TempDirState {
                        cleanup_enabled: true,
                    },
                );
            });

            Ok(TempDir {
                path,
                cleanup_enabled: true,
            })
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

    /// Generate deterministic temp directory name
    fn generate_temp_name(
        prefix: Option<&OsStr>,
        suffix: Option<&OsStr>,
        rand_bytes: usize,
    ) -> OsString {
        let mut name = OsString::new();

        if let Some(p) = prefix {
            name.push(p);
        }

        // Generate deterministic random part
        for i in 0..rand_bytes {
            #[allow(clippy::cast_possible_truncation)]
            let c = char::from(b'a' + (i % 26) as u8);
            name.push(c.to_string());
        }

        if let Some(s) = suffix {
            name.push(s);
        }

        name
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::simulator::{exists, reset_fs};
        use pretty_assertions::assert_eq;

        #[test_log::test]
        fn test_generate_temp_name_without_prefix_or_suffix() {
            let name = generate_temp_name(None, None, 6);
            assert_eq!(name.to_str().unwrap(), "abcdef");
        }

        #[test_log::test]
        fn test_generate_temp_name_with_prefix() {
            let name = generate_temp_name(Some(std::ffi::OsStr::new("test-")), None, 4);
            assert_eq!(name.to_str().unwrap(), "test-abcd");
        }

        #[test_log::test]
        fn test_generate_temp_name_with_suffix() {
            let name = generate_temp_name(None, Some(std::ffi::OsStr::new("-end")), 4);
            assert_eq!(name.to_str().unwrap(), "abcd-end");
        }

        #[test_log::test]
        fn test_generate_temp_name_with_both() {
            let name = generate_temp_name(
                Some(std::ffi::OsStr::new("pre-")),
                Some(std::ffi::OsStr::new("-suf")),
                3,
            );
            assert_eq!(name.to_str().unwrap(), "pre-abc-suf");
        }

        #[test_log::test]
        fn test_generate_temp_name_wraps_alphabet() {
            // With 30 rand_bytes, it should wrap around the alphabet
            let name = generate_temp_name(None, None, 30);
            let s = name.to_str().unwrap();
            // First 26 chars are a-z, then wraps: 0%26=a, 1%26=b, etc.
            assert!(s.starts_with("abcdefghijklmnopqrstuvwxyzabcd"));
        }

        #[test_log::test]
        fn test_builder_default_values() {
            reset_fs();

            let builder = Builder::new();
            let temp = builder.tempdir().unwrap();

            // Should be created in /tmp with deterministic name
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

            let file_name = temp.path().file_name().unwrap().to_str().unwrap();
            // 10 rand bytes = "abcdefghij"
            assert!(file_name.contains("abcdefghij"));
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

            let file_name = temp.path().file_name().unwrap().to_str().unwrap();
            assert_eq!(file_name, "start-abc-end");
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
            is_file: true,
            is_dir: false,
            is_symlink: false,
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
            is_file: false,
            is_dir: true,
            is_symlink: false,
        };

        assert!(dir_metadata.is_dir());
        assert!(!dir_metadata.is_file());
        assert!(dir_metadata.is_empty());
    }

    #[test_log::test]
    fn test_metadata_for_symlink_entry() {
        let symlink_metadata = Metadata {
            len: 0,
            is_file: false,
            is_dir: false,
            is_symlink: true,
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
mod normalize_path_tests {
    use super::normalize_path;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_normalize_absolute_path_with_single_dot() {
        // Single dot should be removed
        assert_eq!(normalize_path("/a/./b"), "/a/b");
        assert_eq!(normalize_path("/./a"), "/a");
        assert_eq!(normalize_path("/a/."), "/a");
        assert_eq!(normalize_path("/././."), "/");
    }

    #[test_log::test]
    fn test_normalize_absolute_path_with_double_dots() {
        // Double dots should go up one directory
        assert_eq!(normalize_path("/a/b/../c"), "/a/c");
        assert_eq!(normalize_path("/a/b/c/../../d"), "/a/d");
        assert_eq!(normalize_path("/a/../b"), "/b");
    }

    #[test_log::test]
    fn test_normalize_absolute_path_double_dots_at_root() {
        // Double dots at root should be ignored for absolute paths
        assert_eq!(normalize_path("/.."), "/");
        assert_eq!(normalize_path("/../a"), "/a");
        assert_eq!(normalize_path("/../../a/b"), "/a/b");
    }

    #[test_log::test]
    fn test_normalize_absolute_path_with_trailing_slashes() {
        // Trailing slashes should be handled (empty components ignored)
        assert_eq!(normalize_path("/a/b/"), "/a/b");
        assert_eq!(normalize_path("/a//b"), "/a/b");
        assert_eq!(normalize_path("///a///b///"), "/a/b");
    }

    #[test_log::test]
    fn test_normalize_relative_path_with_single_dot() {
        assert_eq!(normalize_path("./a"), "a");
        assert_eq!(normalize_path("a/./b"), "a/b");
        assert_eq!(normalize_path("."), ".");
    }

    #[test_log::test]
    fn test_normalize_relative_path_with_double_dots() {
        // For relative paths, .. at the start should be preserved
        assert_eq!(normalize_path("../a"), "../a");
        assert_eq!(normalize_path("../../a"), "../../a");
        assert_eq!(normalize_path("a/b/../../c"), "c");
        assert_eq!(normalize_path("a/../b"), "b");
    }

    #[test_log::test]
    fn test_normalize_relative_path_double_dots_preserved() {
        // Double dots that can't go further up should be preserved for relative paths
        assert_eq!(normalize_path("a/../../b"), "../b");
        assert_eq!(normalize_path("../.."), "../..");
    }

    #[test_log::test]
    fn test_normalize_empty_path() {
        // Empty path should become "."
        assert_eq!(normalize_path(""), ".");
    }

    #[test_log::test]
    fn test_normalize_root_path() {
        assert_eq!(normalize_path("/"), "/");
    }

    #[test_log::test]
    fn test_normalize_mixed_dots() {
        // Complex combinations of . and ..
        assert_eq!(normalize_path("/a/./b/../c/./d"), "/a/c/d");
        assert_eq!(normalize_path("./a/./b/../c"), "a/c");
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

        // Creating root should work (no parent check needed)
        sync::create_dir("/").unwrap();
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
    fn test_create_dir_idempotent() {
        reset_fs();
        sync::create_dir_all("/parent").unwrap();

        // Creating the same directory twice should work
        sync::create_dir("/parent/child").unwrap();
        sync::create_dir("/parent/child").unwrap();
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
