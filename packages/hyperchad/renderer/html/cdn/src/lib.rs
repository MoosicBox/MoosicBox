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

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_router::RouteRequest;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_setup_cdn_optimization_with_no_root_route() {
        // Router with no root route should be returned unchanged
        let router = Router::new();
        let result = setup_cdn_optimization(router, None, None);

        // Should not have static route since no root route existed
        assert!(!result.has_static_route("/"));
        // Should not have dynamic root endpoint
        assert!(!result.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_with_static_root_route() {
        // Router with static root route should be returned unchanged
        let router = Router::new().with_static_route("/", |_req| async { "Static content" });

        let result = setup_cdn_optimization(router, None, None);

        // Should still have the original static route
        assert!(result.has_static_route("/"));
        // Should NOT add the dynamic root endpoint
        assert!(!result.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_setup_cdn_optimization_with_dynamic_root_route() {
        // Router with dynamic root route should be transformed
        let router = Router::new().with_route("/", |_req| async { "Dynamic content" });

        let result = setup_cdn_optimization(router, None, None);

        // Should now have a static route for root
        assert!(result.has_static_route("/"));
        // Should have the dynamic root endpoint
        assert!(result.has_route("/__hyperchad_dynamic_root__"));
        // Note: The original dynamic "/" route still exists and takes precedence over the static route
        // This is the current behavior - navigate() checks dynamic routes first
        assert!(result.has_route("/"));
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_adds_static_route_with_title() {
        let router = Router::new().with_route("/", |_req| async { "Dynamic content" });

        let result = setup_cdn_optimization(router, Some("My App Title"), None);

        // Should have a static route for root (for static file generation)
        assert!(result.has_static_route("/"));
        // Should have the dynamic endpoint
        assert!(result.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_adds_static_route_with_viewport() {
        let router = Router::new().with_route("/", |_req| async { "Dynamic content" });

        let result =
            setup_cdn_optimization(router, None, Some("width=device-width, initial-scale=1"));

        // Should have a static route for root (for static file generation)
        assert!(result.has_static_route("/"));
        // Should have the dynamic endpoint
        assert!(result.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_adds_static_route_with_both_title_and_viewport() {
        let router = Router::new().with_route("/", |_req| async { "Dynamic content" });

        let result = setup_cdn_optimization(
            router,
            Some("Test App"),
            Some("width=device-width, initial-scale=1.0"),
        );

        // Should have a static route for root (for static file generation)
        assert!(result.has_static_route("/"));
        // Should have the dynamic endpoint
        assert!(result.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_setup_cdn_optimization_preserves_dynamic_root() {
        use hyperchad_router::RequestInfo;

        // Create a router with a dynamic root that returns specific content
        let expected_content = "Original dynamic content from handler";
        let router =
            Router::new().with_route("/", move |_req| async move { expected_content.to_string() });

        let result = setup_cdn_optimization(router, None, None);

        // Navigate to the dynamic endpoint
        let req = RouteRequest::from_path(
            "/__hyperchad_dynamic_root__",
            RequestInfo {
                client: hyperchad_router::DEFAULT_CLIENT_INFO.clone(),
            },
        );
        let content = result
            .navigate(req)
            .await
            .expect("Navigation should succeed")
            .expect("Should return content");

        // Verify the original handler's content is preserved
        #[allow(clippy::match_wildcard_for_single_variants)]
        let text = match content {
            Content::Raw { data, .. } => String::from_utf8(data.to_vec()).unwrap(),
            _ => panic!("Expected Raw content"),
        };

        assert_eq!(text, expected_content);
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_return_value_is_router() {
        let router = Router::new().with_route("/", |_req| async { "content" });
        let result = setup_cdn_optimization(router, None, None);

        // Should return a Router instance
        assert!(result.has_route("/__hyperchad_dynamic_root__"));
        assert!(result.has_static_route("/"));
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_with_empty_title() {
        let router = Router::new().with_route("/", |_req| async { "content" });
        // Empty string title should still work
        let result = setup_cdn_optimization(router, Some(""), None);

        assert!(result.has_static_route("/"));
        assert!(result.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_with_empty_viewport() {
        let router = Router::new().with_route("/", |_req| async { "content" });
        // Empty string viewport should still work
        let result = setup_cdn_optimization(router, None, Some(""));

        assert!(result.has_static_route("/"));
        assert!(result.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test]
    fn test_setup_cdn_optimization_idempotency() {
        // Calling setup_cdn_optimization twice should be safe
        let router = Router::new().with_route("/", |_req| async { "content" });
        let result1 = setup_cdn_optimization(router, Some("App"), None);
        // Second call should see the static route and return early
        let result2 = setup_cdn_optimization(result1, Some("App2"), None);

        // Should still have both routes
        assert!(result2.has_static_route("/"));
        assert!(result2.has_route("/__hyperchad_dynamic_root__"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_skeleton_html_content_structure() {
        use hyperchad_router::RequestInfo;

        // Setup CDN optimization with title and viewport
        let router = Router::new().with_route("/", |_req| async { "Dynamic content" });
        let result = setup_cdn_optimization(
            router,
            Some("My Test App"),
            Some("width=device-width, initial-scale=1"),
        );

        // Access the static route handler directly and clone before dropping the lock
        let handler = {
            let static_routes = result.static_routes.read().unwrap();
            static_routes
                .iter()
                .find(|(route, _)| route.matches("/"))
                .expect("Static route for / should exist")
                .1
                .clone()
        };

        let req = RouteRequest::from_path(
            "/",
            RequestInfo {
                client: hyperchad_router::DEFAULT_CLIENT_INFO.clone(),
            },
        );

        let content = handler(req)
            .await
            .expect("Handler should succeed")
            .expect("Handler should return content");

        // Extract HTML string from content
        #[allow(clippy::match_wildcard_for_single_variants)]
        let html = match content {
            Content::Raw { data, .. } => String::from_utf8(data.to_vec()).unwrap(),
            _ => panic!("Expected Raw content"),
        };

        // Verify skeleton HTML structure
        assert!(html.contains("<!DOCTYPE html>"), "Should have DOCTYPE");
        assert!(
            html.contains("<html lang=\"en\">"),
            "Should have html lang attribute"
        );
        assert!(
            html.contains("<meta charset=\"utf-8\">"),
            "Should have charset meta"
        );
        assert!(
            html.contains("<title>My Test App</title>"),
            "Should have title element with correct content"
        );
        assert!(
            html.contains(
                "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
            ),
            "Should have viewport meta with correct content"
        );
        assert!(
            html.contains("fetch('/__hyperchad_dynamic_root__')"),
            "Should fetch from dynamic root endpoint"
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_skeleton_html_without_title_or_viewport() {
        use hyperchad_router::RequestInfo;

        // Setup CDN optimization without title or viewport
        let router = Router::new().with_route("/", |_req| async { "Dynamic content" });
        let result = setup_cdn_optimization(router, None, None);

        // Access the static route handler and clone before dropping the lock
        let handler = {
            let static_routes = result.static_routes.read().unwrap();
            static_routes
                .iter()
                .find(|(route, _)| route.matches("/"))
                .expect("Static route for / should exist")
                .1
                .clone()
        };

        let req = RouteRequest::from_path(
            "/",
            RequestInfo {
                client: hyperchad_router::DEFAULT_CLIENT_INFO.clone(),
            },
        );

        let content = handler(req)
            .await
            .expect("Handler should succeed")
            .expect("Handler should return content");

        #[allow(clippy::match_wildcard_for_single_variants)]
        let html = match content {
            Content::Raw { data, .. } => String::from_utf8(data.to_vec()).unwrap(),
            _ => panic!("Expected Raw content"),
        };

        // Verify HTML structure without title/viewport
        assert!(html.contains("<!DOCTYPE html>"), "Should have DOCTYPE");
        assert!(
            html.contains("<meta charset=\"utf-8\">"),
            "Should have charset meta"
        );
        assert!(
            !html.contains("<title>"),
            "Should not have title element when not provided"
        );
        assert!(
            !html.contains("name=\"viewport\""),
            "Should not have viewport meta when not provided"
        );
        assert!(
            html.contains("fetch('/__hyperchad_dynamic_root__')"),
            "Should fetch from dynamic root endpoint"
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_dynamic_root_propagates_handler_errors() {
        use hyperchad_router::RequestInfo;
        use std::io;

        // Create a router with a dynamic root that always returns an error
        let router = Router::new()
            .with_route_result::<Content, Option<Content>, _, _>("/", |_req| async move {
                Err::<Option<Content>, _>(io::Error::other("Simulated handler error"))
            });

        let result = setup_cdn_optimization(router, None, None);

        // Navigate to the dynamic endpoint
        let req = RouteRequest::from_path(
            "/__hyperchad_dynamic_root__",
            RequestInfo {
                client: hyperchad_router::DEFAULT_CLIENT_INFO.clone(),
            },
        );

        // The error should propagate through the dynamic root endpoint
        let navigation_result = result.navigate(req).await;
        assert!(
            navigation_result.is_err(),
            "Handler errors should propagate through dynamic root endpoint"
        );

        let err = navigation_result.unwrap_err();
        assert!(
            err.to_string().contains("Simulated handler error"),
            "Error message should be preserved"
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_dynamic_root_preserves_none_response() {
        use hyperchad_router::RequestInfo;

        // Create a router with a dynamic root that returns None (no content)
        let router =
            Router::new().with_route::<Content, _, _>("/", |_req| async move { None::<Content> });

        let result = setup_cdn_optimization(router, None, None);

        // Navigate to the dynamic endpoint
        let req = RouteRequest::from_path(
            "/__hyperchad_dynamic_root__",
            RequestInfo {
                client: hyperchad_router::DEFAULT_CLIENT_INFO.clone(),
            },
        );

        let navigation_result = result
            .navigate(req)
            .await
            .expect("Navigation should succeed");

        // None response should be preserved
        assert!(
            navigation_result.is_none(),
            "None response from handler should be preserved"
        );
    }
}
