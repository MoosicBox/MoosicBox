//! HTTP adapter for `HyperChad` HTML rendering.
//!
//! This crate provides an HTTP request processor that combines `HyperChad`'s
//! HTML rendering capabilities with routing, optional action handling, and
//! static asset serving. It converts `HyperChad` views into HTTP responses
//! with support for both full page loads and partial updates (htmx).
//!
//! # Features
//!
//! * `actions` - Enable action request handling via channels
//! * `assets` - Enable static asset serving from filesystem or embedded sources
//! * `json` - Enable JSON response content type
//!
//! # Example
//!
//! ```rust,no_run
//! # use hyperchad_renderer_html_http::{HttpApp, http::Response};
//! # use hyperchad_renderer::HtmlTagRenderer;
//! # use hyperchad_router::{Router, RouteRequest, RequestInfo};
//! # async fn example<R: HtmlTagRenderer + Sync>(renderer: R) -> Result<(), Box<dyn std::error::Error>> {
//! let router = Router::new();
//! let app = HttpApp::new(renderer, router)
//!     .with_title("My App")
//!     .with_viewport("width=device-width, initial-scale=1");
//!
//! let request = RouteRequest::from_path("/", RequestInfo::default());
//! let response: Response<Vec<u8>> = app.process(&request).await?;
//! # Ok(())
//! # }
//! ```
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, sync::LazyLock};

use http::Response;
use hyperchad_color::Color;
use hyperchad_renderer::{Content, HtmlTagRenderer};
use hyperchad_renderer_html::html::container_element_to_html;
use hyperchad_router::{RouteRequest, Router};

/// Re-export of the `http` crate for constructing HTTP requests and responses.
///
/// This re-export provides access to HTTP types like [`http::Response`], [`http::Request`],
/// and [`http::StatusCode`] used by this crate's API.
pub use http;

#[cfg(feature = "actions")]
mod actions;

/// Generates the prefix for a directory asset route.
/// Handles the special case where route="/" or "" to avoid producing "//".
#[cfg(feature = "assets")]
fn directory_route_prefix(route: &str) -> String {
    if route == "/" || route.is_empty() {
        "/".to_string()
    } else {
        format!("{route}/")
    }
}

/// Errors that can occur during HTTP request processing.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP response construction error.
    #[error(transparent)]
    Http(#[from] http::Error),
    /// File I/O error during asset serving.
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// HTML rendering error from the renderer.
    #[error(transparent)]
    Renderer(Box<dyn std::error::Error + Send>),
    /// Channel receive error when handling actions.
    #[error(transparent)]
    Recv(#[from] flume::RecvError),
    /// Router navigation error.
    #[error(transparent)]
    Navigate(#[from] hyperchad_router::NavigateError),
    /// JSON serialization error (requires `json` feature).
    #[cfg(feature = "_json")]
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

/// HTTP application wrapper for `HyperChad` rendering.
///
/// Combines a renderer, router, and optional action/asset handling into an HTTP response processor.
#[derive(Clone)]
pub struct HttpApp<R: HtmlTagRenderer + Sync> {
    /// HTML tag renderer for converting views to HTML.
    pub renderer: R,
    /// Router for handling navigation and route matching.
    pub router: Router,
    /// Action sender channel for handling action requests.
    #[cfg(feature = "actions")]
    pub action_tx: Option<
        flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    >,
    /// Static asset routes for serving files.
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    /// Custom handlers for resolving static asset paths.
    #[cfg(feature = "assets")]
    #[allow(clippy::type_complexity)]
    pub static_asset_route_handlers: Vec<
        std::sync::Arc<
            Box<
                dyn Fn(&RouteRequest) -> Option<hyperchad_renderer::assets::AssetPathTarget>
                    + Send
                    + Sync,
            >,
        >,
    >,
    /// Default behavior when a requested asset file is not found.
    #[cfg(feature = "assets")]
    pub asset_not_found_behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    background: Option<Color>,
    title: Option<String>,
    description: Option<String>,
    viewport: Option<String>,
    css_urls: Vec<String>,
    css_paths: Vec<String>,
    inline_css: Vec<String>,
}

impl<R: HtmlTagRenderer + Sync> std::fmt::Debug for HttpApp<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct("HttpApp");

        dbg.field("router", &self.router)
            .field("background", &self.background)
            .field("title", &self.title)
            .field("description", &self.description)
            .field("viewport", &self.viewport)
            .field("css_urls", &self.css_urls)
            .field("css_paths", &self.css_paths)
            .field("inline_css", &self.inline_css);

        #[cfg(feature = "actions")]
        dbg.field("action_tx", &self.action_tx);

        #[cfg(feature = "assets")]
        dbg.field("static_asset_routes", &self.static_asset_routes);

        dbg.finish_non_exhaustive()
    }
}

impl<R: HtmlTagRenderer + Sync> HttpApp<R> {
    /// Processes an HTTP request and returns an HTTP response.
    ///
    /// # Errors
    ///
    /// * `Error::Http` - If HTTP response construction fails
    /// * `Error::IO` - If file I/O operations fail (asset serving)
    /// * `Error::Navigate` - If routing fails
    /// * `Error::SerdeJson` - If JSON serialization fails (with `json` feature)
    ///
    /// # Panics
    ///
    /// * If action channel send fails (with `actions` feature)
    /// * If path string conversion fails during asset serving (with `assets` feature)
    #[allow(clippy::too_many_lines)]
    pub async fn process(&self, req: &RouteRequest) -> Result<Response<Vec<u8>>, Error> {
        static HEADERS: LazyLock<BTreeMap<String, String>> = LazyLock::new(BTreeMap::new);

        log::debug!("process: req={req:?}");

        #[cfg(feature = "actions")]
        {
            let route = hyperchad_router::RoutePath::from("/$action");

            if route.matches(&req.path) {
                let Some(tx) = &self.action_tx else {
                    return Ok(Response::builder().status(204).body(vec![])?);
                };

                return actions::handle_action(tx, req);
            }
        }

        #[cfg(feature = "assets")]
        {
            use std::{
                path::{Path, PathBuf},
                str::FromStr as _,
            };

            use hyperchad_renderer::assets::{
                AssetNotFoundBehavior, AssetPathTarget, StaticAssetRoute,
            };
            use switchy_async::io::AsyncReadExt as _;

            fn content_type_from_path(path: &Path) -> String {
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string()
            }

            /// Safely join a path within a directory, preventing directory traversal attacks.
            /// Returns None if:
            /// - The directory doesn't exist
            /// - The resolved path escapes the directory
            /// - The resolved path doesn't exist
            fn safe_join_path(dir: &Path, path_match: &str) -> Option<PathBuf> {
                // Get canonical path of the base directory
                let canonical_dir = dir.canonicalize().ok()?;

                // Join and canonicalize the full path
                let file_path = dir.join(path_match);
                let canonical_file = file_path.canonicalize().ok()?;

                // Ensure the resolved path is within the directory
                if !canonical_file.starts_with(&canonical_dir) {
                    log::warn!(
                        "Directory traversal attempt blocked: {path_match:?} resolved to {}",
                        canonical_file.display()
                    );
                    return None;
                }

                Some(canonical_file)
            }

            async fn asset_to_response(
                path: &str,
                target: &AssetPathTarget,
                path_match: &str,
            ) -> Result<Response<Vec<u8>>, Error> {
                let is_directory = matches!(target, AssetPathTarget::Directory(..));

                match target {
                    AssetPathTarget::FileContents(target) => {
                        let content_type =
                            content_type_from_path(&PathBuf::from_str(path).unwrap());

                        let response = Response::builder()
                            .status(200)
                            .header("Content-Type", content_type)
                            .body(target.to_vec())?;

                        Ok::<_, Error>(response)
                    }
                    AssetPathTarget::File(target) | AssetPathTarget::Directory(target) => {
                        let target = if is_directory {
                            // Use safe path joining to prevent directory traversal
                            match safe_join_path(target, path_match) {
                                Some(path) => path,
                                None => {
                                    return Ok(Response::builder().status(404).body(vec![])?);
                                }
                            }
                        } else {
                            target.clone()
                        };

                        let content_type = content_type_from_path(&target);

                        log::debug!(
                            "Serving asset target={} is_directory={is_directory} content_type={content_type}",
                            target.display()
                        );

                        let mut file = switchy_fs::unsync::OpenOptions::new()
                            .read(true)
                            .open(target)
                            .await?;

                        let mut buf = vec![];
                        file.read_to_end(&mut buf).await?;

                        let response = Response::builder()
                            .status(200)
                            .header("Content-Type", content_type)
                            .body(buf)?;

                        Ok::<_, Error>(response)
                    }
                }
            }

            for handler in &self.static_asset_route_handlers {
                let Some(target) = handler(req) else {
                    continue;
                };

                return asset_to_response(&req.path, &target, "").await;
            }

            for StaticAssetRoute {
                route,
                target,
                not_found_behavior,
            } in &self.static_asset_routes
            {
                // Determine the effective behavior: per-route override or global default
                let behavior = not_found_behavior.unwrap_or(self.asset_not_found_behavior);

                let route_path = match target {
                    AssetPathTarget::File(..) | AssetPathTarget::FileContents(..) => {
                        hyperchad_router::RoutePath::from(route)
                    }
                    AssetPathTarget::Directory(..) => {
                        hyperchad_router::RoutePath::LiteralPrefix(directory_route_prefix(route))
                    }
                };

                log::debug!("Checking route {route_path:?} for {req:?}");
                let Some(path_match) = route_path.strip_match(&req.path) else {
                    continue;
                };

                // Skip empty path matches for directories (e.g., "/" matching "/" exactly)
                // to allow the router to handle the root path
                if matches!(target, AssetPathTarget::Directory(..)) && path_match.is_empty() {
                    continue;
                }

                // For directories, check if the target file exists before serving
                if let AssetPathTarget::Directory(dir) = target {
                    match safe_join_path(dir, path_match) {
                        Some(file_path) if file_path.is_file() => {
                            // File exists and is safe to serve
                        }
                        _ => {
                            // File doesn't exist - handle according to behavior
                            match behavior {
                                AssetNotFoundBehavior::Fallthrough => {
                                    log::debug!(
                                        "Skipping directory asset route {route_path:?} - file does not exist or path is invalid: {path_match:?}"
                                    );
                                    continue;
                                }
                                AssetNotFoundBehavior::NotFound => {
                                    log::debug!(
                                        "Returning 404 for directory asset route {route_path:?} - file does not exist: {path_match:?}"
                                    );
                                    return Ok(Response::builder().status(404).body(vec![])?);
                                }
                                AssetNotFoundBehavior::InternalServerError => {
                                    log::debug!(
                                        "Returning 500 for directory asset route {route_path:?} - file does not exist: {path_match:?}"
                                    );
                                    return Ok(Response::builder().status(500).body(vec![])?);
                                }
                            }
                        }
                    }
                }

                log::debug!("Matched route {route_path:?} for {req:?}");

                return asset_to_response(route, target, path_match).await;
            }
        }

        let Some(content) = self.router.navigate(req.clone()).await? else {
            return Ok(Response::builder().status(204).body(vec![])?);
        };

        let has_fragments = matches!(&content, Content::View(v) if !v.fragments.is_empty());
        let delete_selectors = if let Content::View(v) = &content {
            if v.delete_selectors.is_empty() {
                None
            } else {
                serde_json::to_string(
                    &v.delete_selectors
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                )
                .ok()
            }
        } else {
            None
        };

        #[allow(unreachable_patterns)]
        let html = match content {
            Content::View(boxed_view) => {
                let view = boxed_view.primary.as_ref();

                if let Some(view) = view {
                    let content = container_element_to_html(view, &self.renderer)?;

                    if req.headers.contains_key("hx-request") {
                        self.renderer.partial_html(
                            &HEADERS,
                            view,
                            content,
                            self.viewport.as_deref(),
                            self.background,
                        )
                    } else {
                        self.renderer.root_html(
                            &HEADERS,
                            view,
                            content,
                            self.viewport.as_deref(),
                            self.background,
                            self.title.as_deref(),
                            self.description.as_deref(),
                            &self.css_urls,
                            &self.css_paths,
                            &self.inline_css,
                        )
                    }
                } else {
                    // Fragments-only response
                    String::new()
                }
            }
            Content::Raw { data, content_type } => {
                return Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", content_type)
                    .body(data.to_vec())?);
            }
            #[cfg(feature = "json")]
            Content::Json(json) => {
                let mut bytes: Vec<u8> = Vec::new();
                serde_json::to_writer(&mut bytes, &json)?;
                return Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", "application/json")
                    .body(bytes)?);
            }
            #[cfg(not(feature = "json"))]
            _ => {
                unimplemented!("JSON serialization is not enabled");
            }
        };

        let mut response = Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8");

        if has_fragments {
            response = response.header("X-HyperChad-Fragments", "true");
        }

        if let Some(selectors) = delete_selectors {
            response = response.header("X-HyperChad-Delete-Selectors", selectors);
        }

        Ok(response.body(html.into_bytes())?)
    }
}

impl<R: HtmlTagRenderer + Sync> HttpApp<R> {
    /// Creates a new HTTP application with the given renderer and router.
    #[must_use]
    pub const fn new(renderer: R, router: Router) -> Self {
        Self {
            renderer,
            router,
            #[cfg(feature = "actions")]
            action_tx: None,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            #[cfg(feature = "assets")]
            static_asset_route_handlers: vec![],
            #[cfg(feature = "assets")]
            asset_not_found_behavior: hyperchad_renderer::assets::AssetNotFoundBehavior::NotFound,
            background: None,
            title: None,
            description: None,
            viewport: None,
            css_urls: vec![],
            css_paths: vec![],
            inline_css: vec![],
        }
    }

    /// Sets the viewport meta tag content for HTML responses.
    #[must_use]
    pub fn with_viewport(mut self, content: impl Into<String>) -> Self {
        self.viewport.replace(content.into());
        self
    }

    /// Sets the background color for HTML responses.
    #[must_use]
    pub fn with_background(mut self, color: impl Into<Color>) -> Self {
        self.background.replace(color.into());
        self
    }

    /// Sets the page title for HTML responses.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title.replace(title.into());
        self
    }

    /// Sets the page description meta tag for HTML responses.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description.replace(description.into());
        self
    }

    /// Adds a CSS URL and returns the modified app.
    #[must_use]
    pub fn with_css_url(mut self, url: impl Into<String>) -> Self {
        self.css_urls.push(url.into());
        self
    }

    /// Adds multiple CSS URLs and returns the modified app.
    #[must_use]
    pub fn with_css_urls(mut self, urls: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.css_urls.extend(urls.into_iter().map(Into::into));
        self
    }

    /// Adds a CSS path and returns the modified app.
    #[must_use]
    pub fn with_css_path(mut self, path: impl Into<String>) -> Self {
        self.css_paths.push(path.into());
        self
    }

    /// Adds multiple CSS paths and returns the modified app.
    #[must_use]
    pub fn with_css_paths(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.css_paths.extend(paths.into_iter().map(Into::into));
        self
    }

    /// Adds inline CSS and returns the modified app.
    #[must_use]
    pub fn with_inline_css(mut self, css: impl Into<String>) -> Self {
        self.inline_css.push(css.into());
        self
    }

    /// Adds multiple inline CSS blocks and returns the modified app.
    #[must_use]
    pub fn with_inline_css_blocks(
        mut self,
        css_blocks: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.inline_css
            .extend(css_blocks.into_iter().map(Into::into));
        self
    }

    /// Sets the action sender channel for handling action requests.
    #[cfg(feature = "actions")]
    #[must_use]
    pub fn with_action_tx(
        mut self,
        tx: flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    ) -> Self {
        self.action_tx = Some(tx);
        self
    }

    /// Adds a custom handler for resolving static asset routes.
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_route_handler(
        mut self,
        handler: impl Fn(&RouteRequest) -> Option<hyperchad_renderer::assets::AssetPathTarget>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.static_asset_route_handlers
            .push(std::sync::Arc::new(Box::new(handler)));
        self
    }

    /// Sets the default behavior when a requested asset file is not found.
    #[cfg(feature = "assets")]
    #[must_use]
    pub const fn with_asset_not_found_behavior(
        mut self,
        behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) -> Self {
        self.asset_not_found_behavior = behavior;
        self
    }

    /// Sets the default behavior when a requested asset file is not found (in place).
    #[cfg(feature = "assets")]
    pub const fn set_asset_not_found_behavior(
        &mut self,
        behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) {
        self.asset_not_found_behavior = behavior;
    }

    /// Sets the action sender channel by mutable reference.
    #[cfg(feature = "actions")]
    pub fn set_action_tx(
        &mut self,
        tx: flume::Sender<(
            String,
            Option<hyperchad_renderer::transformer::actions::logic::Value>,
        )>,
    ) {
        self.action_tx = Some(tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test file");
        let err = Error::from(io_err);
        assert!(err.to_string().contains("test file"));

        let nav_err = hyperchad_router::NavigateError::InvalidPath;
        let err = Error::from(nav_err);
        assert_eq!(err.to_string(), "Invalid path");
    }

    #[test_log::test]
    fn test_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::from(io_err);
        assert!(matches!(err, Error::IO(_)));
        assert!(err.to_string().contains("file not found"));
    }

    #[test_log::test]
    fn test_error_from_recv_error() {
        let (tx, rx) = flume::unbounded::<()>();
        drop(tx);
        let recv_err = rx.recv().unwrap_err();
        let err = Error::from(recv_err);
        assert!(matches!(err, Error::Recv(_)));
    }

    #[test_log::test]
    fn test_error_from_navigate_error() {
        let nav_err = hyperchad_router::NavigateError::InvalidPath;
        let err = Error::from(nav_err);
        assert!(matches!(err, Error::Navigate(_)));
        assert_eq!(err.to_string(), "Invalid path");
    }

    #[cfg(feature = "_json")]
    #[test_log::test]
    fn test_error_from_serde_json_error() {
        let json_str = r#"{"invalid": json}"#;
        let json_err: serde_json::Error =
            serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let err = Error::from(json_err);
        assert!(matches!(err, Error::SerdeJson(_)));
    }

    #[cfg(feature = "assets")]
    #[test_log::test]
    fn test_directory_route_prefix() {
        use super::directory_route_prefix;

        assert_eq!(directory_route_prefix("/"), "/");
        assert_eq!(directory_route_prefix(""), "/");
        assert_eq!(directory_route_prefix("/assets"), "/assets/");
        assert_eq!(directory_route_prefix("/static/files"), "/static/files/");
    }

    #[cfg(feature = "assets")]
    mod safe_join_path_tests {
        use std::fs::{self, File};
        use std::path::PathBuf;
        use switchy_fs::tempdir;

        // Import safe_join_path - it's defined inside the process() method's #[cfg(feature = "assets")] block
        // We need to test it by recreating the same function here since it's not accessible
        fn safe_join_path(dir: &std::path::Path, path_match: &str) -> Option<std::path::PathBuf> {
            // Get canonical path of the base directory
            let canonical_dir = dir.canonicalize().ok()?;

            // Join and canonicalize the full path
            let file_path = dir.join(path_match);
            let canonical_file = file_path.canonicalize().ok()?;

            // Ensure the resolved path is within the directory
            if !canonical_file.starts_with(&canonical_dir) {
                return None;
            }

            Some(canonical_file)
        }

        #[test_log::test]
        fn test_safe_join_path_valid_file() {
            let temp_dir = tempdir().unwrap();
            let file_path = temp_dir.path().join("test.txt");
            File::create(&file_path).unwrap();

            let result = safe_join_path(temp_dir.path(), "test.txt");
            assert!(result.is_some());
            assert_eq!(result.unwrap(), file_path.canonicalize().unwrap());
        }

        #[test_log::test]
        fn test_safe_join_path_nonexistent_file() {
            let temp_dir = tempdir().unwrap();

            let result = safe_join_path(temp_dir.path(), "nonexistent.txt");
            assert!(result.is_none());
        }

        #[test_log::test]
        fn test_safe_join_path_blocks_parent_directory_traversal() {
            let temp_dir = tempdir().unwrap();

            // Create a file outside the temp_dir to try to access
            let parent_file = temp_dir.path().parent().unwrap().join("outside.txt");
            File::create(&parent_file).unwrap();

            // Attempt directory traversal
            let result = safe_join_path(temp_dir.path(), "../outside.txt");
            assert!(result.is_none(), "Should block parent directory traversal");

            // Cleanup
            let _ = fs::remove_file(&parent_file);
        }

        #[test_log::test]
        fn test_safe_join_path_blocks_absolute_path_traversal() {
            let temp_dir = tempdir().unwrap();

            // Attempt to escape using absolute-looking path
            let result = safe_join_path(temp_dir.path(), "/../../../etc/passwd");
            assert!(result.is_none(), "Should block traversal attempts");
        }

        #[test_log::test]
        fn test_safe_join_path_blocks_various_traversal_patterns() {
            let temp_dir = tempdir().unwrap();

            // Various traversal attempts
            let attempts = vec![
                "../../../etc/passwd",
                "foo/../../bar/../../../etc/passwd",
                "./../../etc/passwd",
            ];

            for attempt in attempts {
                let result = safe_join_path(temp_dir.path(), attempt);
                assert!(
                    result.is_none(),
                    "Should block traversal attempt: {attempt}"
                );
            }
        }

        #[test_log::test]
        fn test_safe_join_path_allows_nested_directories() {
            let temp_dir = tempdir().unwrap();

            // Create nested directory structure
            let nested_dir = temp_dir.path().join("subdir").join("nested");
            fs::create_dir_all(&nested_dir).unwrap();
            let nested_file = nested_dir.join("file.txt");
            File::create(&nested_file).unwrap();

            let result = safe_join_path(temp_dir.path(), "subdir/nested/file.txt");
            assert!(result.is_some());
            assert_eq!(result.unwrap(), nested_file.canonicalize().unwrap());
        }

        #[test_log::test]
        fn test_safe_join_path_nonexistent_directory() {
            let nonexistent_dir = PathBuf::from("/nonexistent/directory/that/does/not/exist");

            let result = safe_join_path(&nonexistent_dir, "file.txt");
            assert!(result.is_none());
        }
    }

    mod process_tests {
        use super::*;
        use bytes::Bytes;
        use hyperchad_renderer::Content;
        use hyperchad_renderer_html::DefaultHtmlTagRenderer;
        use hyperchad_router::{RequestInfo, RouteRequest, Router};
        use switchy::http::models::Method;

        fn create_route_request(path: &str) -> RouteRequest {
            RouteRequest {
                path: path.to_string(),
                method: Method::Get,
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                cookies: BTreeMap::new(),
                info: RequestInfo::default(),
                body: None,
            }
        }

        fn create_route_request_with_headers(
            path: &str,
            headers: BTreeMap<String, String>,
        ) -> RouteRequest {
            RouteRequest {
                path: path.to_string(),
                method: Method::Get,
                query: BTreeMap::new(),
                headers,
                cookies: BTreeMap::new(),
                info: RequestInfo::default(),
                body: None,
            }
        }

        #[cfg(feature = "actions")]
        #[test_log::test(switchy_async::test)]
        async fn test_process_action_route_without_action_tx_returns_204() {
            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new();
            let app = HttpApp::new(renderer, router);
            // action_tx is None by default

            let req = create_route_request("/$action");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 204);
            assert!(response.body().is_empty());
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_no_matching_route_returns_navigate_error() {
            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new();
            // No routes added
            let app = HttpApp::new(renderer, router);

            let req = create_route_request("/nonexistent");
            let result = app.process(&req).await;

            // When no route matches, the router returns an InvalidPath error
            assert!(matches!(result, Err(Error::Navigate(_))));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_raw_content_response() {
            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/raw", |_req| async {
                Content::Raw {
                    data: Bytes::from_static(b"raw content"),
                    content_type: "text/plain".to_string(),
                }
            });
            let app = HttpApp::new(renderer, router);

            let req = create_route_request("/raw");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            assert_eq!(
                response.headers().get("Content-Type").unwrap(),
                "text/plain"
            );
            assert_eq!(response.body(), b"raw content");
        }

        #[cfg(feature = "json")]
        #[test_log::test(switchy_async::test)]
        async fn test_process_json_content_response() {
            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/json", |_req| async {
                Content::Json(serde_json::json!({"key": "value"}))
            });
            let app = HttpApp::new(renderer, router);

            let req = create_route_request("/json");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            assert_eq!(
                response.headers().get("Content-Type").unwrap(),
                "application/json"
            );
            let body_str = std::str::from_utf8(response.body()).unwrap();
            assert!(body_str.contains("key"));
            assert!(body_str.contains("value"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_view_content_returns_html() {
            use hyperchad_router::{Container, Element};

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/page", |_req| async {
                Container {
                    element: Element::Div,
                    ..Default::default()
                }
            });
            let app = HttpApp::new(renderer, router).with_title("Test Page");

            let req = create_route_request("/page");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            assert_eq!(
                response.headers().get("Content-Type").unwrap(),
                "text/html; charset=utf-8"
            );
            let body_str = std::str::from_utf8(response.body()).unwrap();
            assert!(body_str.contains("<!DOCTYPE html>"));
            assert!(body_str.contains("Test Page"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_htmx_request_returns_partial_html() {
            use hyperchad_router::{Container, Element};

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/partial", |_req| async {
                Container {
                    element: Element::Div,
                    ..Default::default()
                }
            });
            let app = HttpApp::new(renderer, router).with_title("Test Page");

            let mut headers = BTreeMap::new();
            headers.insert("hx-request".to_string(), "true".to_string());
            let req = create_route_request_with_headers("/partial", headers);
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            let body_str = std::str::from_utf8(response.body()).unwrap();
            // Partial responses don't include full HTML document structure
            assert!(!body_str.contains("<!DOCTYPE html>"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_view_with_fragments_sets_header() {
            use hyperchad_renderer::View;
            use hyperchad_router::{Container, Element};

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/fragments", |_req| async {
                let fragment = Container {
                    element: Element::Span,
                    str_id: Some("target".to_string()),
                    ..Default::default()
                };
                let view = View::builder().with_fragment(fragment).build();
                Content::View(Box::new(view))
            });
            let app = HttpApp::new(renderer, router);

            let req = create_route_request("/fragments");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            assert_eq!(
                response.headers().get("X-HyperChad-Fragments").unwrap(),
                "true"
            );
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_view_with_delete_selectors_sets_header() {
            use hyperchad_renderer::{View, transformer::models::Selector};
            use hyperchad_router::{Container, Element};

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/delete", |_req| async {
                let view = View::builder()
                    .with_primary(Container {
                        element: Element::Div,
                        ..Default::default()
                    })
                    .with_delete_selector(Selector::Class("old-element".to_string()))
                    .build();
                Content::View(Box::new(view))
            });
            let app = HttpApp::new(renderer, router);

            let req = create_route_request("/delete");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            let delete_header = response
                .headers()
                .get("X-HyperChad-Delete-Selectors")
                .unwrap()
                .to_str()
                .unwrap();
            assert!(delete_header.contains(".old-element"));
        }

        #[cfg(feature = "assets")]
        #[test_log::test(switchy_async::test)]
        async fn test_process_static_asset_file_contents() {
            use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new();
            let mut app = HttpApp::new(renderer, router);
            app.static_asset_routes.push(StaticAssetRoute {
                route: "/style.css".to_string(),
                target: AssetPathTarget::FileContents(Bytes::from_static(b"body { color: red; }")),
                not_found_behavior: None,
            });

            let req = create_route_request("/style.css");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            assert_eq!(response.headers().get("Content-Type").unwrap(), "text/css");
            assert_eq!(response.body(), b"body { color: red; }");
        }

        #[cfg(feature = "assets")]
        #[test_log::test(switchy_async::test)]
        async fn test_process_static_asset_handler() {
            use hyperchad_renderer::assets::AssetPathTarget;

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new();
            let app = HttpApp::new(renderer, router).with_static_asset_route_handler(
                |req: &RouteRequest| {
                    if req.path == "/custom.js" {
                        Some(AssetPathTarget::FileContents(Bytes::from_static(
                            b"console.log('test');",
                        )))
                    } else {
                        None
                    }
                },
            );

            let req = create_route_request("/custom.js");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            // mime_guess returns "text/javascript" for .js files
            assert_eq!(
                response.headers().get("Content-Type").unwrap(),
                "text/javascript"
            );
            assert_eq!(response.body(), b"console.log('test');");
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_view_without_primary_returns_empty_html() {
            use hyperchad_renderer::View;

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/empty", |_req| async {
                let view = View::builder().build();
                Content::View(Box::new(view))
            });
            let app = HttpApp::new(renderer, router);

            let req = create_route_request("/empty");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            assert_eq!(
                response.headers().get("Content-Type").unwrap(),
                "text/html; charset=utf-8"
            );
            // View without primary returns empty string for html content
            let body_str = std::str::from_utf8(response.body()).unwrap();
            assert!(body_str.is_empty());
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_route_returning_none_returns_204() {
            let renderer = DefaultHtmlTagRenderer::default();
            // Use with_no_content_result to create a route that returns None on success
            let router = Router::new().with_no_content_result("/no-content", |_req| async {
                Ok::<_, Box<dyn std::error::Error>>(())
            });
            let app = HttpApp::new(renderer, router);

            let req = create_route_request("/no-content");
            let response = app.process(&req).await.unwrap();

            // Router returning None should produce a 204 No Content response
            assert_eq!(response.status(), 204);
            assert!(response.body().is_empty());
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_includes_css_urls_in_response() {
            use hyperchad_router::{Container, Element};

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/styled", |_req| async {
                Container {
                    element: Element::Div,
                    ..Default::default()
                }
            });
            let app = HttpApp::new(renderer, router)
                .with_css_url("https://example.com/style.css")
                .with_css_urls(vec!["https://example.com/another.css"]);

            let req = create_route_request("/styled");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            let body_str = std::str::from_utf8(response.body()).unwrap();
            assert!(body_str.contains("https://example.com/style.css"));
            assert!(body_str.contains("https://example.com/another.css"));
        }

        #[test_log::test(switchy_async::test)]
        async fn test_process_with_viewport_meta() {
            use hyperchad_router::{Container, Element};

            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new().with_route("/mobile", |_req| async {
                Container {
                    element: Element::Div,
                    ..Default::default()
                }
            });
            let app =
                HttpApp::new(renderer, router).with_viewport("width=device-width, initial-scale=1");

            let req = create_route_request("/mobile");
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 200);
            let body_str = std::str::from_utf8(response.body()).unwrap();
            assert!(body_str.contains("width=device-width"));
        }

        #[cfg(feature = "actions")]
        #[test_log::test(switchy_async::test)]
        async fn test_process_action_route_with_tx_delegates_to_handler() {
            let renderer = DefaultHtmlTagRenderer::default();
            let router = Router::new();
            let (tx, rx) = flume::unbounded();
            let app = HttpApp::new(renderer, router).with_action_tx(tx);

            // Create a request with action payload
            let req = RouteRequest {
                path: "/$action".to_string(),
                method: Method::Post,
                query: BTreeMap::new(),
                headers: BTreeMap::new(),
                cookies: BTreeMap::new(),
                info: RequestInfo::default(),
                body: Some(std::sync::Arc::new(Bytes::from_static(
                    br#"{"action":"test"}"#,
                ))),
            };
            let response = app.process(&req).await.unwrap();

            assert_eq!(response.status(), 204);

            // Verify action was sent through channel
            let (action_name, value) = rx.try_recv().unwrap();
            assert_eq!(action_name, "test");
            assert!(value.is_none());
        }
    }
}
