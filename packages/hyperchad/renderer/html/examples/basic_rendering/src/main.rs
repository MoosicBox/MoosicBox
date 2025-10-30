#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic HTML rendering example demonstrating the `HyperChad` HTML renderer.
//!
//! This example shows how to:
//! - Create an HTML tag renderer
//! - Build containers with styled elements using the container! macro
//! - Generate HTML with responsive design
//! - Apply CSS styling and classes

use hyperchad_renderer::Color;
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, html::container_element_to_html_response};
use hyperchad_router::Container;
use hyperchad_template::container;
use hyperchad_transformer::{Number, ResponsiveTrigger};
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("HyperChad HTML Renderer - Basic Rendering Example");
    println!("==================================================\n");

    // Step 1: Create the HTML tag renderer with responsive triggers
    println!("1. Creating HTML tag renderer with responsive breakpoints...");
    let tag_renderer = DefaultHtmlTagRenderer::default()
        .with_responsive_trigger("mobile", ResponsiveTrigger::MaxWidth(Number::Real(768.0)))
        .with_responsive_trigger("tablet", ResponsiveTrigger::MaxWidth(Number::Real(1024.0)));

    println!("   ✓ Renderer created with mobile (768px) and tablet (1024px) breakpoints\n");

    // Step 2: Build a container with styled elements
    println!("2. Building container with styled elements...");
    let container = create_sample_container();
    println!("   ✓ Container created with header, main content, and footer\n");

    // Step 3: Generate HTML from the container
    println!("3. Generating HTML output...");
    let headers = BTreeMap::new();
    let html = container_element_to_html_response(
        &headers,
        &container,
        Some("width=device-width, initial-scale=1"), // viewport
        Some(Color {
            r: 243,
            g: 244,
            b: 246,
            a: Some(255),
        }), // background color (gray-100)
        Some("HyperChad HTML Renderer Example"),     // title
        Some("A demonstration of the HyperChad HTML renderer with responsive design"), // description
        &tag_renderer,
        &[], // CSS URLs
        &[], // CSS paths
        &[], // inline CSS
    )?;

    println!("   ✓ HTML generated successfully\n");

    // Step 4: Display the generated HTML
    println!("4. Generated HTML Output:");
    println!("   {}", "=".repeat(78));
    println!("{html}");
    println!("   {}", "=".repeat(78));

    // Step 5: Show HTML statistics
    println!("\n5. HTML Statistics:");
    println!("   • Total size: {} bytes", html.len());
    println!("   • Contains DOCTYPE: {}", html.contains("<!DOCTYPE"));
    println!("   • Contains viewport meta: {}", html.contains("viewport"));
    println!("   • Contains responsive CSS: {}", html.contains("@media"));
    println!("   • Contains title: {}", html.contains("<title>"));

    println!("\n✓ Example completed successfully!");
    println!("\nThe generated HTML includes:");
    println!("  • Semantic HTML5 structure (header, main, footer)");
    println!("  • Inline CSS styling for layout and colors");
    println!("  • Responsive media queries for mobile/tablet breakpoints");
    println!("  • Proper meta tags for SEO");

    Ok(())
}

/// Creates a sample container demonstrating various `HyperChad` features.
fn create_sample_container() -> Container {
    container! {
        div id="root" class="page" direction=column {
            // Header section
            header
                id="header"
                class="header"
                padding=24
                background=#1f2937
                color=white
                text-align=center
            {
                h1 { "HyperChad HTML Renderer" }
                span font-size=14 {
                    "Server-side HTML generation with responsive design"
                }
            }

            // Main content area
            main
                id="main"
                class="main"
                padding=24
                row-gap=16
                max-width=800
                width=100%
            {
                // Welcome section
                section
                    padding=24
                    background=white
                    border-radius=8
                    row-gap=12
                {
                    h2 { "Welcome to HyperChad" }
                    span {
                        "This example demonstrates basic HTML rendering with the HyperChad framework. "
                        "The renderer converts HyperChad containers into semantic HTML with CSS styling."
                    }
                }

                // Features section
                section
                    padding=24
                    background=#eff6ff
                    border-radius=8
                    row-gap=12
                {
                    h3 { "Key Features" }
                    ul padding-left=20 row-gap=8 {
                        li { "Server-side HTML generation" }
                        li { "Responsive design with media queries" }
                        li { "Semantic HTML5 elements" }
                        li { "CSS styling and classes" }
                        li { "Flexbox and grid layout support" }
                    }
                }
            }

            // Footer section
            footer
                id="footer"
                class="footer"
                padding=24
                background=#f3f4f6
                text-align=center
            {
                span color=#6b7280 {
                    "Built with HyperChad • HTML Renderer"
                }
            }
        }
    }
    .into()
}
