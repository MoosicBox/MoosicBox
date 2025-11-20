#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # `HyperChad` Details/Summary Example
//!
//! This example demonstrates collapsible content using HTML `<details>` and `<summary>` elements
//! in [`HyperChad`](https://github.com/MoosicBox/MoosicBox/tree/master/packages/hyperchad).
//!
//! ## Features Demonstrated
//!
//! * Basic details/summary usage
//! * Default open state with `open` attribute
//! * FAQ accordion pattern with multiple collapsible sections
//! * Nested details elements
//! * Styled details with `HyperChad` attributes
//! * Details without summary (browser default triangle)
//! * Practical use cases: settings panels and debug information
//!
//! ## Running the Example
//!
//! ```bash
//! cd packages/hyperchad/examples/details_summary
//! PORT=3132 cargo run -- serve
//! ```
//!
//! Then open your browser to: <http://localhost:3132>
//!
//! ## Key Points
//!
//! * No JavaScript required - native HTML functionality
//! * `<summary>` must be first child of `<details>` if present
//! * Only one `<summary>` allowed per `<details>`
//! * Can be styled with standard CSS/HyperChad attributes
//! * Fully accessible by default
//! * Works in all modern browsers
//!
//! ## Example Usage
//!
//! Basic collapsible section:
//! ```rust,ignore
//! details {
//!     summary { "Click me" }
//!     div { "Hidden content" }
//! }
//! ```
//!
//! Default open state:
//! ```rust,ignore
//! details open {
//!     summary { "Already expanded" }
//!     div { "Visible content" }
//! }
//! ```
//!
//! Nested details:
//! ```rust,ignore
//! details {
//!     summary { "Parent" }
//!     div {
//!         "Parent content"
//!         details {
//!             summary { "Nested Child" }
//!             div { "Nested content" }
//!         }
//!     }
//! }
//! ```

use hyperchad::{
    app::AppBuilder,
    renderer::View,
    router::{RouteRequest, Router},
    template::{self as hyperchad_template, Containers, container},
};
use log::info;

#[cfg(feature = "assets")]
use std::sync::LazyLock;

/// Static assets served by the application.
///
/// Contains the vanilla JavaScript runtime required for `HyperChad`'s client-side interactivity.
/// This includes the hashed JavaScript bundle that provides dynamic behavior without requiring
/// a full framework.
#[cfg(feature = "assets")]
static ASSETS: LazyLock<Vec<hyperchad::renderer::assets::StaticAssetRoute>> = LazyLock::new(|| {
    vec![
        #[cfg(feature = "vanilla-js")]
        hyperchad::renderer::assets::StaticAssetRoute {
            route: format!(
                "js/{}",
                hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
            ),
            target: hyperchad::renderer::assets::AssetPathTarget::FileContents(
                hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
            ),
        },
    ]
});

/// Creates the main page demonstrating details/summary elements.
///
/// Builds a comprehensive demonstration page showcasing various use cases for HTML
/// `<details>` and `<summary>` elements in `HyperChad`, including:
///
/// * Basic collapsible sections
/// * Default open state
/// * FAQ accordion pattern
/// * Nested details elements
/// * Styled details with custom appearance
/// * Settings panels
/// * Debug information panels
/// * Details without summary elements
///
/// Returns a [`Containers`] instance containing the complete page structure ready for rendering.
#[must_use]
#[allow(clippy::too_many_lines, clippy::large_stack_frames)]
fn create_main_page() -> Containers {
    container! {
        div class="page" {
            header
                class="header"
                padding=24
                background=#1f2937
                color=white
                text-align=center
            {
                h1 { "HyperChad Details/Summary Demo" }
                span { "Demonstrating collapsible content with <details> and <summary>" }
            }

            div
                direction=row
                justify-content=center
                width=100%
            {
                main
                    class="main"
                    padding=24
                    max-width=800
                    width=100%
                    gap=32
                {
                    // Section 1: Basic Usage
                    section
                        class="basic-usage"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Basic Details/Summary" }
                        span color=#6b7280 { "Click the summary to toggle visibility" }

                        details
                            margin-top=12
                            border-radius=6
                            padding=12
                            background=#f9fafb
                        {
                            summary
                                font-weight=bold
                                cursor=pointer
                                padding=8
                            {
                                "Click to expand"
                            }
                            div padding-top=12 {
                                "This is the hidden content that appears when expanded. "
                                "It can contain any HTML elements."
                            }
                        }
                    }

                    // Section 2: Default Open
                    section
                        class="default-open"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Default Open State" }
                        span color=#6b7280 { "Using `open` to start expanded" }

                        details
                            open
                            margin-top=12
                            border-radius=6
                            padding=12
                            background="#eff6ff"
                        {
                            summary
                                font-weight=bold
                                cursor=pointer
                                padding=8
                                color="#1e40af"
                            {
                                "This is open by default"
                            }
                            div
                                padding-top=12
                                color="#1e3a8a"
                            {
                                "Content visible immediately on page load. "
                                "You can still click the summary to collapse it."
                            }
                        }
                    }

                    // Section 3: FAQ Accordion
                    section
                        class="faq-section"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Frequently Asked Questions" }
                        span color=#6b7280 { "Multiple independent collapsible sections" }

                        div gap=12 margin-top=12 {
                            details
                                border-radius=6
                                padding=12
                                background="#fef3c7"
                            {
                                summary
                                    font-weight=bold
                                    cursor=pointer
                                    padding=8
                                    color="#92400e"
                                {
                                    "What is HyperChad?"
                                }
                                div
                                    padding-top=12
                                    color=#78350f
                                {
                                    "HyperChad is a Rust-based UI framework that provides a "
                                    "declarative syntax for building web interfaces with strong "
                                    "type safety and excellent performance."
                                }
                            }

                            details
                                border-radius=6
                                padding=12
                                background="#fef3c7"
                            {
                                summary
                                    font-weight=bold
                                    cursor=pointer
                                    padding=8
                                    color="#92400e"
                                {
                                    "How do I use details/summary?"
                                }
                                div
                                    padding-top=12
                                    color=#78350f
                                {
                                    "Simply use the details element with a summary child as the first element. "
                                    "The summary provides the clickable heading, and any other children are the "
                                    "collapsible content."
                                }
                            }

                            details
                                border-radius=6
                                padding=12
                                background="#fef3c7"
                            {
                                summary
                                    font-weight=bold
                                    cursor=pointer
                                    padding=8
                                    color="#92400e"
                                {
                                    "Can I nest details elements?"
                                }
                                div
                                    padding-top=12
                                    color=#78350f
                                {
                                    "Yes! See the nested example section below for a demonstration "
                                    "of details elements within other details elements."
                                }
                            }
                        }
                    }

                    // Section 4: Nested Details
                    section
                        class="nested-section"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Nested Details" }
                        span color=#6b7280 { "Details within details, independently collapsible" }

                        details
                            margin-top=12
                            border-radius=6
                            padding=12
                            background="#dbeafe"
                        {
                            summary
                                font-weight=bold
                                cursor=pointer
                                padding=8
                                color="#1e40af"
                            {
                                "📁 Parent Section"
                            }
                            div padding-top=12 gap=12 {
                                div color="#1e3a8a" {
                                    "This is the parent content. Below are nested collapsible sections:"
                                }

                                details
                                    border-radius=6
                                    padding=12
                                    background="#bfdbfe"
                                    margin-left=16
                                {
                                    summary
                                        font-weight=bold
                                        cursor=pointer
                                        padding=8
                                        color="#1e40af"
                                    {
                                        "📄 Nested Child Section 1"
                                    }
                                    div
                                        padding-top=12
                                        color="#1e3a8a"
                                    {
                                        "This is nested content that can be independently collapsed. "
                                        "It doesn't affect the parent or sibling sections."
                                    }
                                }

                                details
                                    border-radius=6
                                    padding=12
                                    background="#bfdbfe"
                                    margin-left=16
                                {
                                    summary
                                        font-weight=bold
                                        cursor=pointer
                                        padding=8
                                        color="#1e40af"
                                    {
                                        "📄 Nested Child Section 2"
                                    }
                                    div
                                        padding-top=12
                                        color="#1e3a8a"
                                    {
                                        "Another nested section with its own independent state."
                                    }
                                }
                            }
                        }
                    }

                    // Section 5: Styled Details (Settings Panel)
                    section
                        class="styled-section"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Styled Details" }
                        span color=#6b7280 { "Custom appearance with HyperChad attributes" }

                        details
                            margin-top=12
                            border-radius=8
                            padding=16
                            background=#f3f4f6
                        {
                            summary
                                font-weight=bold
                                color=#4338ca
                                cursor=pointer
                                padding=8
                                font-size=18
                            {
                                "⚙️ Advanced Settings"
                            }

                            div
                                padding-top=16
                                gap=12
                            {
                                div
                                    padding=12
                                    background=white
                                    border-radius=6
                                {
                                    span font-weight=bold color=#374151 { "Option 1:" }
                                    span color=#6b7280 { " Enable auto-save functionality" }
                                }
                                div
                                    padding=12
                                    background=white
                                    border-radius=6
                                {
                                    span font-weight=bold color=#374151 { "Option 2:" }
                                    span color=#6b7280 { " Show advanced developer tools" }
                                }
                                div
                                    padding=12
                                    background=white
                                    border-radius=6
                                {
                                    span font-weight=bold color=#374151 { "Option 3:" }
                                    span color=#6b7280 { " Enable experimental features" }
                                }
                            }
                        }
                    }

                    // Section 6: Debug/Developer Info Panel
                    section
                        class="debug-section"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Developer Info Panel" }
                        span color=#6b7280 { "Collapsible debug information" }

                        details
                            margin-top=12
                            border-radius=6
                            overflow-x=hidden
                        {
                            summary
                                font-weight=bold
                                cursor=pointer
                                padding=12
                                background=#1f2937
                                color=white
                                border-radius=6
                            {
                                "🐛 Debug Information"
                            }
                            div
                                background=#111827
                                color=#10b981
                                padding=16
                                border-radius=6
                                font-family=monospace
                                font-size=14
                                gap=6
                            {
                                div { "Build: Release v1.0.0" }
                                div { "Renderer: HTML" }
                                div { "Backend: Actix Web" }
                                div { "Features: vanilla-js, router" }
                                div { "Status: ✓ All systems operational" }
                            }
                        }
                    }

                    // Section 7: Details without Summary
                    section
                        class="no-summary-section"
                        padding=24
                        background=white
                        border-radius=8
                        gap=16
                    {
                        h2 { "Details Without Summary" }
                        span color=#6b7280 { "Uses browser's default disclosure triangle" }

                        details
                            margin-top=12
                            padding=12
                            background="#fef2f2"
                            border-radius=6
                        {
                            div color=#991b1b {
                                "This details element has no summary element. "
                                "The browser provides a default disclosure triangle. "
                                "This is valid HTML and works correctly."
                            }
                        }
                    }

                    // Info section at bottom
                    section
                        class="info-section"
                        padding=24
                        background="#eff6ff"
                        border-radius=8
                        gap=16
                    {
                        h3 { "Element Info" }
                        ul padding-left=20 gap=8 {
                            li {
                                "<details> creates collapsible sections"
                            }
                            li {
                                "<summary> provides the clickable heading (optional)"
                            }
                            li {
                                "`open` makes details expanded by default"
                            }
                            li {
                                "Native browser support - no JavaScript required"
                            }
                            li {
                                "Fully styleable with HyperChad attributes"
                            }
                            li {
                                "<summary> must be the first child of <details> if present"
                            }
                            li {
                                "Only one <summary> allowed per <details>"
                            }
                        }
                    }
                }
            }

            footer
                class="footer"
                padding=24
                text-align=center
                background=#f3f4f6
            {
                span { "Built with HyperChad • Details/Summary Demo" }
            }
        }
    }
}

/// Creates and configures the application router.
///
/// Sets up a single route at the root path (`/`) that serves the main demonstration page.
/// The router handles incoming HTTP requests and returns the appropriate [`View`] response.
///
/// Returns a configured [`Router`] instance ready to handle requests.
#[must_use]
fn create_router() -> Router {
    Router::new().with_route("/", |_req: RouteRequest| async move {
        View::builder().with_primary(create_main_page()).build()
    })
}

/// Application entry point.
///
/// Initializes logging, creates the async runtime, sets up the router with the demonstration
/// page, configures static assets (if enabled), and starts the web server.
///
/// The server listens on the port specified by the `PORT` environment variable, or defaults
/// to port 8080 if not set.
///
/// # Errors
///
/// Returns an error if:
/// * The async runtime fails to initialize
/// * Static asset routes are invalid or fail to register
/// * The web server fails to bind to the specified port
/// * The server encounters a fatal error during execution
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting HyperChad Details/Summary Example");

    let runtime = switchy::unsync::runtime::Builder::new().build()?;

    let router = create_router();

    info!("Server running on http://localhost:8080");
    info!("Press Ctrl+C to stop");

    #[allow(unused_mut)]
    let mut app = AppBuilder::new()
        .with_router(router)
        .with_runtime_handle(runtime.handle())
        .with_title("HyperChad Details/Summary Demo".to_string())
        .with_description(
            "Demonstrating collapsible content with details/summary elements in HyperChad"
                .to_string(),
        );

    #[cfg(feature = "assets")]
    for asset in ASSETS.iter().cloned() {
        app.static_asset_route_result(asset).unwrap();
    }

    app.build_default()?.run()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod page_structure_tests {
        use super::*;

        #[test]
        fn test_create_main_page_returns_valid_structure() {
            let page = create_main_page();

            // Verify the page structure is created successfully
            // Containers is a Vec<Container>
            assert!(!page.is_empty(), "Page should contain containers");
        }

        #[test]
        fn test_create_main_page_contains_header_section() {
            let page = create_main_page();

            // Convert to string representation to verify content
            let page_str = format!("{page:?}");

            // Verify header content is present
            assert!(
                page_str.contains("HyperChad Details/Summary Demo"),
                "Page should contain the main title"
            );
        }

        #[test]
        fn test_create_main_page_contains_basic_usage_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify basic usage section exists
            assert!(
                page_str.contains("Basic Details/Summary"),
                "Page should contain Basic Details/Summary section"
            );
            assert!(
                page_str.contains("Click to expand"),
                "Page should contain collapsible content"
            );
        }

        #[test]
        fn test_create_main_page_contains_default_open_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify default open section exists
            assert!(
                page_str.contains("Default Open State"),
                "Page should contain Default Open State section"
            );
            assert!(
                page_str.contains("This is open by default"),
                "Page should contain default open content"
            );
        }

        #[test]
        fn test_create_main_page_contains_faq_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify FAQ section with multiple questions
            assert!(
                page_str.contains("Frequently Asked Questions"),
                "Page should contain FAQ section"
            );
            assert!(
                page_str.contains("What is HyperChad?"),
                "Page should contain first FAQ question"
            );
            assert!(
                page_str.contains("How do I use details/summary?"),
                "Page should contain second FAQ question"
            );
            assert!(
                page_str.contains("Can I nest details elements?"),
                "Page should contain third FAQ question"
            );
        }

        #[test]
        fn test_create_main_page_contains_nested_details_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify nested details section
            assert!(
                page_str.contains("Nested Details"),
                "Page should contain Nested Details section"
            );
            assert!(
                page_str.contains("Parent Section"),
                "Page should contain parent section"
            );
            assert!(
                page_str.contains("Nested Child Section"),
                "Page should contain nested child sections"
            );
        }

        #[test]
        fn test_create_main_page_contains_styled_details_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify styled details section (settings panel)
            assert!(
                page_str.contains("Styled Details"),
                "Page should contain Styled Details section"
            );
            assert!(
                page_str.contains("Advanced Settings"),
                "Page should contain settings panel"
            );
        }

        #[test]
        fn test_create_main_page_contains_debug_info_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify debug info section
            assert!(
                page_str.contains("Developer Info Panel"),
                "Page should contain Developer Info Panel section"
            );
            assert!(
                page_str.contains("Debug Information"),
                "Page should contain debug information"
            );
        }

        #[test]
        fn test_create_main_page_contains_no_summary_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify section demonstrating details without summary
            assert!(
                page_str.contains("Details Without Summary"),
                "Page should contain Details Without Summary section"
            );
            assert!(
                page_str.contains("default disclosure triangle"),
                "Page should explain default disclosure triangle"
            );
        }

        #[test]
        fn test_create_main_page_contains_info_section() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify informational section at bottom
            assert!(
                page_str.contains("Element Info"),
                "Page should contain Element Info section"
            );
            assert!(
                page_str.contains("creates collapsible sections"),
                "Page should contain element usage information"
            );
        }

        #[test]
        fn test_create_main_page_contains_footer() {
            let page = create_main_page();
            let page_str = format!("{page:?}");

            // Verify footer is present
            assert!(
                page_str.contains("Built with HyperChad"),
                "Page should contain footer"
            );
        }
    }

    mod router_tests {
        use super::*;

        #[test]
        fn test_create_router_returns_valid_router() {
            let router = create_router();

            // Verify router is created successfully
            // The router should have the root route registered
            assert!(
                router.has_route("/"),
                "Router should have root route registered"
            );
        }

        #[test]
        fn test_create_router_has_only_root_route() {
            let router = create_router();

            // Verify only the root route is registered
            assert!(router.has_route("/"), "Router should have root route");
            assert!(
                !router.has_route("/other"),
                "Router should not have other routes"
            );
        }
    }

    #[cfg(feature = "assets")]
    mod asset_tests {
        use super::*;

        #[test]
        fn test_assets_contains_vanilla_js_route() {
            // Verify static assets are initialized
            assert!(
                !ASSETS.is_empty(),
                "ASSETS should contain at least one asset"
            );

            // Check for vanilla-js asset
            #[cfg(feature = "vanilla-js")]
            {
                let has_js_asset = ASSETS.iter().any(|asset| asset.route.contains("js/"));
                assert!(has_js_asset, "ASSETS should contain JavaScript asset");
            }
        }

        #[test]
        #[cfg(feature = "vanilla-js")]
        fn test_assets_vanilla_js_uses_hashed_filename() {
            // Verify that the JavaScript asset uses a hashed filename
            let js_assets: Vec<_> = ASSETS
                .iter()
                .filter(|asset| asset.route.starts_with("js/"))
                .collect();

            assert!(!js_assets.is_empty(), "Should have at least one JS asset");

            for asset in js_assets {
                // Verify the route contains the hashed script name
                assert!(
                    asset
                        .route
                        .contains(hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()),
                    "JS asset route should use hashed filename for cache busting"
                );
            }
        }

        #[test]
        #[cfg(feature = "vanilla-js")]
        fn test_assets_vanilla_js_has_content() {
            // Verify that the JavaScript asset has actual content
            let js_assets: Vec<_> = ASSETS
                .iter()
                .filter(|asset| asset.route.starts_with("js/"))
                .collect();

            for asset in &js_assets {
                match &asset.target {
                    hyperchad::renderer::assets::AssetPathTarget::FileContents(content) => {
                        assert!(
                            !content.is_empty(),
                            "JS asset should have non-empty content"
                        );
                    }
                    _ => panic!("JS asset should use FileContents variant"),
                }
            }
        }
    }
}
