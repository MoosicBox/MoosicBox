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
    use std::path::Path;

    use crate::unsync::OpenOptions;

    pub use tokio::fs::File;

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
    pub async fn read_dir_sorted<P: AsRef<Path>>(
        path: P,
    ) -> std::io::Result<Vec<tokio::fs::DirEntry>> {
        let mut dir = ::tokio::fs::read_dir(path).await?;
        let mut entries = Vec::new();

        while let Some(entry) = dir.next_entry().await? {
            entries.push(entry);
        }

        entries.sort_by_key(tokio::fs::DirEntry::file_name);
        Ok(entries)
    }

    /// Recursively walk directory tree and return all entries sorted by path for deterministic iteration
    ///
    /// # Errors
    ///
    /// * If any directory cannot be read
    /// * If any directory entry cannot be accessed
    pub async fn walk_dir_sorted<P: AsRef<Path>>(
        path: P,
    ) -> std::io::Result<Vec<tokio::fs::DirEntry>> {
        fn walk_recursive<'a>(
            path: &'a Path,
            entries: &'a mut Vec<tokio::fs::DirEntry>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + 'a>>
        {
            Box::pin(async move {
                let mut dir = ::tokio::fs::read_dir(path).await?;
                let mut dir_entries = Vec::new();

                while let Some(entry) = dir.next_entry().await? {
                    dir_entries.push(entry);
                }

                dir_entries.sort_by_key(tokio::fs::DirEntry::file_name);

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
        all_entries.sort_by_key(tokio::fs::DirEntry::path);
        Ok(all_entries)
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

            options.open(path).await
        }
    }
}
