use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex, RwLock},
};

use bytes::BytesMut;

// Module that contains all real_fs functionality
#[cfg(feature = "simulator-real-fs")]
mod real_fs_support {
    use bytes::BytesMut;
    use scoped_tls::scoped_thread_local;
    use std::sync::{Arc, Mutex};

    pub struct RealFs;

    scoped_thread_local! {
        pub(super) static REAL_FS: RealFs
    }

    pub fn with_real_fs<T>(f: impl FnOnce() -> T) -> T {
        REAL_FS.set(&RealFs, f)
    }

    #[inline]
    pub fn is_real_fs() -> bool {
        REAL_FS.is_set()
    }

    // Simple conversion for std::fs::File to simulator File
    pub fn convert_std_file_to_simulator(
        std_file: std::fs::File,
        path: impl AsRef<std::path::Path>,
        write: bool,
    ) -> std::io::Result<super::sync::File> {
        use std::io::Read;

        let mut std_file = std_file;
        let mut content = Vec::new();
        std_file.read_to_end(&mut content)?;

        Ok(super::sync::File {
            path: path.as_ref().to_path_buf(),
            data: Arc::new(Mutex::new(BytesMut::from(content.as_slice()))),
            position: 0,
            write,
        })
    }

    // Async conversion for std::fs::File to simulator File
    #[cfg(feature = "async")]
    pub async fn convert_std_file_to_simulator_async(
        std_file: std::fs::File,
        path: impl AsRef<std::path::Path>,
        write: bool,
    ) -> std::io::Result<super::unsync::File> {
        let path_buf = path.as_ref().to_path_buf();
        let content = switchy_async::task::spawn_blocking(move || {
            use std::io::Read;
            let mut std_file = std_file;
            let mut content = Vec::new();
            std_file.read_to_end(&mut content)?;
            Ok::<Vec<u8>, std::io::Error>(content)
        })
        .await
        .unwrap()?;

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
}

/// # Panics
///
/// * If the `FILES` `RwLock` fails to write to
pub fn reset_fs() {
    FILES.with_borrow_mut(|x| x.write().unwrap().clear());
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

#[cfg(feature = "sync")]
pub mod sync {
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use bytes::BytesMut;

    use crate::sync::OpenOptions;

    use super::FILES;

    pub struct File {
        pub(crate) path: PathBuf,
        pub(crate) data: Arc<Mutex<BytesMut>>,
        pub(crate) position: u64,
        pub(crate) write: bool,
    }

    impl File {
        /// # Errors
        ///
        /// * If underlying `std::fs::metadata` fails
        pub fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
            std::fs::metadata(&self.path)
        }

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

    impl_file_sync!(File);

    impl OpenOptions {
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
                    std_file, &path, self.write,
                );
            }

            // Original simulator implementation (fallback)
            let location = path_to_str!(path)?;
            let data = if let Some(data) =
                FILES.with_borrow(|x| x.read().unwrap().get(location).cloned())
            {
                data
            } else if self.create {
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

    /// # Errors
    ///
    /// * If the file doesn't exist
    /// * If the file contents cannot be converted to a UTF-8 encoded `String`
    /// * If the file `Path` cannot be converted to a `str`
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

    /// # Errors
    ///
    /// * If underlying `std::fs::create_dir_all` fails (when using real filesystem)
    #[allow(unused_variables)]
    pub fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::create_dir_all(path);
        }
        Ok(())
    }

    /// # Errors
    ///
    /// * If underlying `std::fs::remove_dir_all` fails (when using real filesystem)
    #[allow(unused_variables)]
    pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            return crate::standard::sync::remove_dir_all(path);
        }
        Ok(())
    }

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// Note: In simulator mode, this returns an empty list as the simulator only tracks individual files
    ///
    /// # Errors
    ///
    /// * If underlying `std::fs::read_dir` fails (when using real filesystem)
    /// * If any directory entry cannot be read (when using real filesystem)
    #[allow(unused_variables)]
    pub fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            let std_entries = crate::standard::sync::read_dir_sorted(path)?;
            return std_entries
                .into_iter()
                .map(|x| DirEntry::from_std(&x))
                .collect::<std::io::Result<Vec<_>>>();
        }
        Ok(Vec::new())
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// Note: In simulator mode, this returns an empty list as the simulator only tracks individual files
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read (when using real filesystem)
    /// * If any directory entry cannot be accessed (when using real filesystem)
    #[allow(unused_variables)]
    pub fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "std"))]
        if super::real_fs_support::is_real_fs() {
            let std_entries = crate::standard::sync::walk_dir_sorted(path)?;
            return std_entries
                .into_iter()
                .map(|x| DirEntry::from_std(&x))
                .collect::<std::io::Result<Vec<_>>>();
        }
        Ok(Vec::new())
    }

    /// Directory entry for synchronous filesystem operations
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
        #[must_use]
        pub const fn file_type(&self) -> &super::FileType {
            &self.file_type_info
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
    }
}

#[cfg(feature = "async")]
pub mod unsync {
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        task::Poll,
    };

    use bytes::BytesMut;

    use crate::unsync::OpenOptions;

    pub struct File {
        pub(crate) path: PathBuf,
        pub(crate) data: Arc<Mutex<BytesMut>>,
        pub(crate) position: u64,
        pub(crate) write: bool,
    }

    impl File {
        /// # Errors
        ///
        /// * If underlying `std::fs::metadata` fails
        #[allow(clippy::unused_async)]
        pub async fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
            std::fs::metadata(&self.path)
        }

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
                    std_file, &path, self.write,
                )
                .await;
            }

            // Fallback to sync simulator implementation
            Ok(self.into_sync().open(path)?.into_async())
        }
    }

    /// # Errors
    ///
    /// * If the file doesn't exist
    /// * If the file contents cannot be converted to a UTF-8 encoded `String`
    /// * If the file `Path` cannot be converted to a `str`
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

    /// Directory entry for asynchronous filesystem operations
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
    }

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// Note: In simulator mode, this returns an empty list as the simulator only tracks individual files
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::read_dir` fails (when using real filesystem)
    /// * If any directory entry cannot be read (when using real filesystem)
    /// * If using `real_fs` with simulator runtime (type incompatibility)
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    #[allow(unused_variables)]
    pub async fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || {
                // Reuse existing logic from standard::sync
                let std_entries = crate::standard::sync::read_dir_sorted(path)?;
                // Convert to simulator DirEntry
                std_entries
                    .into_iter()
                    .map(|x| DirEntry::from_std(&x))
                    .collect::<std::io::Result<Vec<_>>>()
            })
            .await
            .unwrap();
        }

        Ok(Vec::new())
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// Note: In simulator mode, this returns an empty list as the simulator only tracks individual files
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read (when using real filesystem)
    /// * If any directory entry cannot be accessed (when using real filesystem)
    /// * If using `real_fs` with simulator runtime (type incompatibility)
    ///
    /// # Panics
    ///
    /// * If the `spawn_blocking` task fails
    #[allow(unused_variables)]
    pub async fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        #[cfg(all(feature = "simulator-real-fs", feature = "async"))]
        if super::real_fs_support::is_real_fs() {
            let path = path.as_ref().to_path_buf();
            return switchy_async::task::spawn_blocking(move || {
                // Reuse existing logic from standard::sync
                let std_entries = crate::standard::sync::walk_dir_sorted(path)?;
                // Convert to simulator DirEntry
                std_entries
                    .into_iter()
                    .map(|x| DirEntry::from_std(&x))
                    .collect::<std::io::Result<Vec<_>>>()
            })
            .await
            .unwrap();
        }

        Ok(Vec::new())
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
}
