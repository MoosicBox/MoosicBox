//! Static file serving configuration.
//!
//! This module provides backend-agnostic configuration for serving static files.
//! Each backend (Actix, Simulator, etc.) provides its own implementation for
//! actually serving the files based on this configuration.
//!
//! # Example
//!
//! ```rust
//! use moosicbox_web_server::StaticFiles;
//!
//! let config = StaticFiles::new("/static", "./public")
//!     .index_file("index.html")
//!     .spa_fallback(true);
//! ```

use std::path::PathBuf;

/// Configuration for serving static files from a directory.
///
/// This is a backend-agnostic configuration type. Each backend provides its own
/// implementation for serving static files based on this configuration.
///
/// # Features
///
/// * **Mount path**: URL prefix where files will be served (e.g., `/static`)
/// * **Directory**: Filesystem directory containing the files to serve
/// * **Index file**: Optional index file for directory requests (e.g., `index.html`)
/// * **SPA fallback**: When enabled, unknown routes return the index file instead of 404
///
/// # Example
///
/// ```rust
/// use moosicbox_web_server::StaticFiles;
///
/// // Basic static file serving
/// let config = StaticFiles::new("/assets", "./static");
///
/// // SPA configuration with fallback to index.html
/// let spa_config = StaticFiles::new("/", "./dist")
///     .index_file("index.html")
///     .spa_fallback(true);
/// ```
#[derive(Debug, Clone)]
pub struct StaticFiles {
    /// URL path prefix where files will be served (e.g., "/" or "/static")
    mount_path: String,
    /// Filesystem directory containing the files to serve
    directory: PathBuf,
    /// Optional index file name (e.g., "index.html")
    index_file: Option<String>,
    /// Whether to serve the index file for unknown routes (SPA mode)
    spa_fallback: bool,
}

impl StaticFiles {
    /// Creates a new static file serving configuration.
    ///
    /// # Arguments
    ///
    /// * `mount_path` - URL path prefix where files will be served
    /// * `directory` - Filesystem directory containing the files to serve
    ///
    /// # Example
    ///
    /// ```rust
    /// use moosicbox_web_server::StaticFiles;
    ///
    /// let config = StaticFiles::new("/static", "./public");
    /// ```
    #[must_use]
    pub fn new(mount_path: impl Into<String>, directory: impl Into<PathBuf>) -> Self {
        Self {
            mount_path: mount_path.into(),
            directory: directory.into(),
            index_file: None,
            spa_fallback: false,
        }
    }

    /// Sets the index file to serve for directory requests.
    ///
    /// When a request matches a directory (including the mount path root),
    /// this file will be served automatically.
    ///
    /// # Example
    ///
    /// ```rust
    /// use moosicbox_web_server::StaticFiles;
    ///
    /// let config = StaticFiles::new("/", "./dist")
    ///     .index_file("index.html");
    /// ```
    #[must_use]
    pub fn index_file(mut self, index: impl Into<String>) -> Self {
        self.index_file = Some(index.into());
        self
    }

    /// Enables or disables SPA fallback mode.
    ///
    /// When enabled, requests for non-existent files will return the index file
    /// instead of a 404 error. This is useful for single-page applications that
    /// handle routing on the client side.
    ///
    /// # Note
    ///
    /// This requires an index file to be set via [`Self::index_file`]. If no
    /// index file is configured, this setting uses "index.html" as the default.
    ///
    /// # Example
    ///
    /// ```rust
    /// use moosicbox_web_server::StaticFiles;
    ///
    /// let config = StaticFiles::new("/", "./dist")
    ///     .index_file("index.html")
    ///     .spa_fallback(true);
    /// ```
    #[must_use]
    pub const fn spa_fallback(mut self, enabled: bool) -> Self {
        self.spa_fallback = enabled;
        self
    }

    /// Returns the mount path where files will be served.
    #[must_use]
    pub fn mount_path(&self) -> &str {
        &self.mount_path
    }

    /// Returns the filesystem directory containing the files.
    #[must_use]
    pub const fn directory(&self) -> &PathBuf {
        &self.directory
    }

    /// Returns the configured index file name, if any.
    #[must_use]
    pub fn index_file_name(&self) -> Option<&str> {
        self.index_file.as_deref()
    }

    /// Returns whether SPA fallback mode is enabled.
    #[must_use]
    pub const fn is_spa_fallback(&self) -> bool {
        self.spa_fallback
    }

    /// Returns the index file name, using "index.html" as default if SPA fallback
    /// is enabled but no explicit index file was set.
    #[must_use]
    pub fn effective_index_file(&self) -> Option<&str> {
        if self.index_file.is_some() {
            self.index_file.as_deref()
        } else if self.spa_fallback {
            Some("index.html")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_files_new() {
        let config = StaticFiles::new("/static", "./public");

        assert_eq!(config.mount_path(), "/static");
        assert_eq!(config.directory(), &PathBuf::from("./public"));
        assert!(config.index_file_name().is_none());
        assert!(!config.is_spa_fallback());
    }

    #[test]
    fn test_static_files_index_file() {
        let config = StaticFiles::new("/", "./dist").index_file("index.html");

        assert_eq!(config.index_file_name(), Some("index.html"));
    }

    #[test]
    fn test_static_files_spa_fallback() {
        let config = StaticFiles::new("/", "./dist").spa_fallback(true);

        assert!(config.is_spa_fallback());
        // Default index file when SPA fallback is enabled
        assert_eq!(config.effective_index_file(), Some("index.html"));
    }

    #[test]
    fn test_static_files_full_config() {
        let config = StaticFiles::new("/app", "./build")
            .index_file("app.html")
            .spa_fallback(true);

        assert_eq!(config.mount_path(), "/app");
        assert_eq!(config.directory(), &PathBuf::from("./build"));
        assert_eq!(config.index_file_name(), Some("app.html"));
        assert!(config.is_spa_fallback());
        assert_eq!(config.effective_index_file(), Some("app.html"));
    }

    #[test]
    fn test_static_files_clone() {
        let config = StaticFiles::new("/static", "./public")
            .index_file("index.html")
            .spa_fallback(true);

        let cloned = config.clone();

        assert_eq!(cloned.mount_path(), config.mount_path());
        assert_eq!(cloned.directory(), config.directory());
        assert_eq!(cloned.index_file_name(), config.index_file_name());
        assert_eq!(cloned.is_spa_fallback(), config.is_spa_fallback());
    }
}
