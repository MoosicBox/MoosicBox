#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad::{
    app::AppBuilder,
    renderer::View,
    router::{RouteRequest, Router},
    template::{self as hyperchad_template, Containers, container},
};
use log::info;

#[cfg(feature = "assets")]
use std::sync::LazyLock;

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

fn create_router() -> Router {
    Router::new().with_route("/", |_req: RouteRequest| async move {
        View::builder().with_primary(create_main_page()).build()
    })
}

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
