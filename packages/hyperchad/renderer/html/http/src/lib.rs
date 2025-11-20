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

            use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};
            use switchy_async::io::AsyncReadExt as _;

            fn content_type_from_path(path: &Path) -> String {
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string()
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
                            target.join(path_match)
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

            for StaticAssetRoute { route, target } in &self.static_asset_routes {
                let route_path = match target {
                    AssetPathTarget::File(..) | AssetPathTarget::FileContents(..) => {
                        hyperchad_router::RoutePath::from(route)
                    }
                    AssetPathTarget::Directory(..) => {
                        hyperchad_router::RoutePath::LiteralPrefix(format!("{route}/"))
                    }
                };

                log::debug!("Checking route {route_path:?} for {req:?}");
                let Some(path_match) = route_path.strip_match(&req.path) else {
                    continue;
                };
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

    #[test]
    fn test_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test file");
        let err = Error::from(io_err);
        assert!(err.to_string().contains("test file"));

        let nav_err = hyperchad_router::NavigateError::InvalidPath;
        let err = Error::from(nav_err);
        assert_eq!(err.to_string(), "Invalid path");
    }

    #[test]
    fn test_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::from(io_err);
        assert!(matches!(err, Error::IO(_)));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_error_from_recv_error() {
        let (_tx, rx) = flume::unbounded::<()>();
        drop(_tx);
        let recv_err = rx.recv().unwrap_err();
        let err = Error::from(recv_err);
        assert!(matches!(err, Error::Recv(_)));
    }

    #[test]
    fn test_error_from_navigate_error() {
        let nav_err = hyperchad_router::NavigateError::InvalidPath;
        let err = Error::from(nav_err);
        assert!(matches!(err, Error::Navigate(_)));
        assert_eq!(err.to_string(), "Invalid path");
    }

    #[cfg(feature = "_json")]
    #[test]
    fn test_error_from_serde_json_error() {
        let json_str = r#"{"invalid": json}"#;
        let json_err: serde_json::Error =
            serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let err = Error::from(json_err);
        assert!(matches!(err, Error::SerdeJson(_)));
    }
}
