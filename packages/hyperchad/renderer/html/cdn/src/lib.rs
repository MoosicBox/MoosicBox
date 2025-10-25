//! CDN optimization utilities for Hyperchad HTML renderers.
//!
//! This crate provides functionality to configure Hyperchad routers for CDN-optimized
//! deployments. It creates a static skeleton HTML file that can be cached by CDNs,
//! while the actual dynamic content is loaded via JavaScript.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad_renderer::Content;
use hyperchad_router::Router;

/// Setup CDN optimization for HTML applications
///
/// This function configures the router to support CDN-optimized deployments by:
/// 1. Adding a static index.html route with skeleton HTML that fetches dynamic content
/// 2. Registering a dynamic endpoint that serves the full application content
///
/// Only activates if the root route ("/") is dynamic (not static).
///
/// # Parameters
///
/// * `router` - The router to configure
/// * `title` - Optional page title for the skeleton HTML
/// * `viewport` - Optional viewport meta tag content for the skeleton HTML
///
/// # Example
///
/// ```rust
/// use hyperchad_router::Router;
/// use hyperchad_renderer_html_cdn::setup_cdn_optimization;
///
/// let router = Router::new()
///     .with_route("/", |_req| async move { "Hello, World!" });
///
/// // Setup CDN optimization with custom title and viewport
/// let router = setup_cdn_optimization(
///     router,
///     Some("My App"),
///     Some("width=device-width, initial-scale=1")
/// );
/// ```
#[must_use]
pub fn setup_cdn_optimization(
    mut router: Router,
    title: Option<&str>,
    viewport: Option<&str>,
) -> Router {
    // Only setup if root route is dynamic
    if router.has_static_route("/") {
        log::debug!("CDN optimization configured - root route is static");
        return router;
    }

    let Some(original_handler) = router.get_route_func("/") else {
        log::debug!("CDN optimization configured - root route is not dynamic");
        return router;
    };

    // 1. Add skeleton index.html as a static asset route
    let title_element = title
        .map(|x| format!("<title>{x}</title>"))
        .unwrap_or_default();
    let viewport_element = viewport
        .map(|x| format!("<meta name=\"viewport\" content=\"{x}\">"))
        .unwrap_or_default();

    let skeleton_html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    {viewport_element}{title_element}
</head>
<body>
    <script>
        fetch('/__hyperchad_dynamic_root__')
            .then(response => response.text())
            .then(html => {{
                document.open();
                document.write(html);
                document.close();
            }})
            .catch(error => {{
                document.write(`<div style="color: red;">Failed to load content: ${{error.message}}</div>`);
            }});
    </script>
</body>
</html>"#
    );

    router = router
        // Add as static route so it gets generated automatically
        .with_static_route("/", move |_req| {
            let content = skeleton_html.clone();
            async move { content }
        })
        // 2. Register dynamic route for actual content
        .with_route_result::<Content, Option<Content>, _, _>(
            "/__hyperchad_dynamic_root__",
            move |req| {
                let handler = original_handler.clone();
                async move { handler(req).await }
            },
        );

    log::debug!("Auto-registered /__hyperchad_dynamic_root__ for CDN optimization");
    log::debug!(
        "CDN optimization configured - skeleton index.html will be generated as static asset"
    );

    router
}
