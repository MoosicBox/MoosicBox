//! Tokio async filesystem operations.
//!
//! This module provides asynchronous filesystem operations using the tokio runtime.
//! All operations are non-blocking and can be used in async contexts.

/// Asynchronous filesystem operations using tokio
///
/// This module provides async filesystem operations using `tokio::fs` for non-blocking
/// disk I/O. All operations return futures that can be awaited in async contexts.
#[cfg(feature = "async")]
pub mod unsync {
    use std::path::{Path, PathBuf};

    use crate::unsync::OpenOptions;

    /// Async file handle wrapping `tokio::fs::File`
    ///
    /// Provides convenience methods for opening files that match the simulator API.
    pub struct File(tokio::fs::File);

    impl File {
        /// Opens a file in read-only mode asynchronously
        ///
        /// This is a convenience method equivalent to `OpenOptions::new().read(true).open(path)`.
        ///
        /// # Errors
        ///
        /// * If the file does not exist
        /// * If permission is denied
        pub async fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
            Ok(Self(tokio::fs::File::open(path).await?))
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
        pub async fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
            Ok(Self(tokio::fs::File::create(path).await?))
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
        pub async fn metadata(&self) -> std::io::Result<Metadata> {
            Ok(Metadata(self.0.metadata().await?))
        }

        /// Returns a reference to the inner `tokio::fs::File`
        #[must_use]
        pub const fn inner(&self) -> &tokio::fs::File {
            &self.0
        }

        /// Returns the inner `tokio::fs::File`, consuming this wrapper
        #[must_use]
        pub fn into_inner(self) -> tokio::fs::File {
            self.0
        }
    }

    impl From<tokio::fs::File> for File {
        fn from(file: tokio::fs::File) -> Self {
            Self(file)
        }
    }

    impl From<File> for tokio::fs::File {
        fn from(file: File) -> Self {
            file.0
        }
    }

    impl tokio::io::AsyncRead for File {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.0).poll_read(cx, buf)
        }
    }

    impl tokio::io::AsyncWrite for File {
        fn poll_write(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::pin::Pin::new(&mut self.0).poll_write(cx, buf)
        }

        fn poll_flush(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.0).poll_flush(cx)
        }

        fn poll_shutdown(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.0).poll_shutdown(cx)
        }
    }

    impl tokio::io::AsyncSeek for File {
        fn start_seek(
            mut self: std::pin::Pin<&mut Self>,
            position: std::io::SeekFrom,
        ) -> std::io::Result<()> {
            std::pin::Pin::new(&mut self.0).start_seek(position)
        }

        fn poll_complete(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<u64>> {
            std::pin::Pin::new(&mut self.0).poll_complete(cx)
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

    /// Async directory reader that yields directory entries
    ///
    /// Wrapper around `tokio::fs::ReadDir`.
    pub struct ReadDir(tokio::fs::ReadDir);

    impl ReadDir {
        /// Returns the next entry in the directory
        ///
        /// Returns `Ok(None)` when there are no more entries.
        ///
        /// # Errors
        ///
        /// * If the directory entry cannot be read
        pub async fn next_entry(&mut self) -> std::io::Result<Option<DirEntry>> {
            Ok(self.0.next_entry().await?.map(DirEntry))
        }
    }

    impl From<tokio::fs::ReadDir> for ReadDir {
        fn from(read_dir: tokio::fs::ReadDir) -> Self {
            Self(read_dir)
        }
    }

    /// Directory entry wrapper
    ///
    /// Wrapper around `tokio::fs::DirEntry` providing consistent API.
    pub struct DirEntry(tokio::fs::DirEntry);

    impl DirEntry {
        /// Returns the full path to this entry
        #[must_use]
        pub fn path(&self) -> PathBuf {
            self.0.path()
        }

        /// Returns the file name of this entry
        #[must_use]
        pub fn file_name(&self) -> std::ffi::OsString {
            self.0.file_name()
        }

        /// Returns the file type of this entry
        ///
        /// # Errors
        ///
        /// * If the file type cannot be determined
        pub async fn file_type(&self) -> std::io::Result<std::fs::FileType> {
            self.0.file_type().await
        }

        /// Returns metadata for this entry
        ///
        /// # Errors
        ///
        /// * If the metadata cannot be retrieved
        pub async fn metadata(&self) -> std::io::Result<Metadata> {
            Ok(Metadata(self.0.metadata().await?))
        }

        /// Returns a reference to the inner `tokio::fs::DirEntry`
        #[must_use]
        pub const fn inner(&self) -> &tokio::fs::DirEntry {
            &self.0
        }
    }

    impl From<tokio::fs::DirEntry> for DirEntry {
        fn from(entry: tokio::fs::DirEntry) -> Self {
            Self(entry)
        }
    }

    /// Returns an async iterator over the entries in a directory
    ///
    /// # Errors
    ///
    /// * If the directory does not exist
    /// * If permission is denied
    pub async fn read_dir<P: AsRef<Path>>(path: P) -> std::io::Result<ReadDir> {
        Ok(ReadDir(tokio::fs::read_dir(path).await?))
    }

    /// Reads the entire contents of a file into a byte vector asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::read` fails
    pub async fn read<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<u8>> {
        ::tokio::fs::read(path).await
    }

    /// Reads the entire contents of a file into a string asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::read_to_string` fails
    pub async fn read_to_string<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
        ::tokio::fs::read_to_string(path).await
    }

    /// Writes a slice as the entire contents of a file
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::write` fails
    pub async fn write<P: AsRef<Path>, C: AsRef<[u8]>>(
        path: P,
        contents: C,
    ) -> std::io::Result<()> {
        ::tokio::fs::write(path, contents).await
    }

    /// Creates a directory asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::create_dir` fails
    pub async fn create_dir<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        tokio::fs::create_dir(path).await
    }

    /// Creates a directory and all missing parent directories asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::create_dir_all` fails
    pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        tokio::fs::create_dir_all(path).await
    }

    /// Removes a directory and all its contents recursively asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::remove_dir_all` fails
    pub async fn remove_dir_all<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
        tokio::fs::remove_dir_all(path).await
    }

    /// Read directory entries and return them sorted by filename for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::read_dir` fails
    /// * If any directory entry cannot be read
    pub async fn read_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        let mut dir = ::tokio::fs::read_dir(path).await?;
        let mut entries = Vec::new();

        while let Some(entry) = dir.next_entry().await? {
            entries.push(DirEntry(entry));
        }

        entries.sort_by_key(DirEntry::file_name);
        Ok(entries)
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read
    /// * If any directory entry cannot be accessed
    pub async fn walk_dir_sorted<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<DirEntry>> {
        fn walk_recursive<'a>(
            path: &'a Path,
            entries: &'a mut Vec<DirEntry>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + 'a>>
        {
            Box::pin(async move {
                let mut dir = ::tokio::fs::read_dir(path).await?;
                let mut dir_entries = Vec::new();

                while let Some(entry) = dir.next_entry().await? {
                    dir_entries.push(DirEntry(entry));
                }

                dir_entries.sort_by_key(DirEntry::file_name);

                for entry in dir_entries {
                    let path = entry.path();
                    entries.push(entry);

                    if path.is_dir() {
                        walk_recursive(&path, entries).await?;
                    }
                }
                Ok(())
            })
        }

        let mut all_entries = Vec::new();
        walk_recursive(path.as_ref(), &mut all_entries).await?;

        // Sort all entries by full path for deterministic order
        all_entries.sort_by_key(DirEntry::path);
        Ok(all_entries)
    }

    /// Checks if a path exists asynchronously
    ///
    /// Returns `true` if the path exists, `false` otherwise.
    pub async fn exists<P: AsRef<Path>>(path: P) -> bool {
        ::tokio::fs::try_exists(path).await.unwrap_or(false)
    }

    /// Checks if a path is a file asynchronously
    ///
    /// Returns `true` if the path exists and is a file, `false` otherwise.
    pub async fn is_file<P: AsRef<Path>>(path: P) -> bool {
        ::tokio::fs::metadata(path).await.is_ok_and(|m| m.is_file())
    }

    /// Checks if a path is a directory asynchronously
    ///
    /// Returns `true` if the path exists and is a directory, `false` otherwise.
    pub async fn is_dir<P: AsRef<Path>>(path: P) -> bool {
        ::tokio::fs::metadata(path).await.is_ok_and(|m| m.is_dir())
    }

    /// Canonicalizes a path asynchronously
    ///
    /// # Errors
    ///
    /// * If underlying `tokio::fs::canonicalize` fails
    pub async fn canonicalize<P: AsRef<Path>>(path: P) -> std::io::Result<std::path::PathBuf> {
        ::tokio::fs::canonicalize(path).await
    }

    impl From<OpenOptions> for tokio::fs::OpenOptions {
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
        /// Opens a file asynchronously with the configured options
        ///
        /// # Errors
        ///
        /// * If and IO error occurs
        pub async fn open(self, path: impl AsRef<::std::path::Path>) -> ::std::io::Result<File> {
            let options: tokio::fs::OpenOptions = self.into();

            Ok(File(options.open(path).await?))
        }
    }
}
