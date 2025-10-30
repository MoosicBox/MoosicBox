#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::too_many_lines)]

//! Basic usage example for `hyperchad_transformer`
//!
//! This example demonstrates:
//! - Creating containers with styling
//! - Building a component hierarchy
//! - Generating HTML output
//! - Tree traversal and element finding
//! - Using different element types

use hyperchad_color::Color;
use hyperchad_transformer::models::LayoutDirection;
use hyperchad_transformer::{Container, Element, HeaderSize, Number};

fn main() {
    println!("=== HyperChad Transformer Basic Usage Example ===\n");

    // Step 1: Create a simple container
    println!("1. Creating a simple container with styling...");
    let _simple_container = Container {
        element: Element::Div,
        width: Some(Number::from(300)),
        height: Some(Number::from(200)),
        background: Some(Color::from_hex("#f0f0f0")),
        padding_left: Some(Number::from(20)),
        padding_right: Some(Number::from(20)),
        padding_top: Some(Number::from(20)),
        padding_bottom: Some(Number::from(20)),
        ..Default::default()
    };

    println!("   Container created with 300x200px dimensions and #f0f0f0 background\n");

    // Step 2: Build a component hierarchy
    println!("2. Building a component hierarchy (header, content, footer)...");
    let mut page = Container {
        element: Element::Div,
        direction: LayoutDirection::Column,
        width: Some(Number::from(800)),
        row_gap: Some(Number::from(20)),
        ..Default::default()
    };

    // Add header
    let mut header = Container {
        element: Element::Header,
        background: Some(Color::from_hex("#2c3e50")),
        color: Some(Color::from_hex("#ecf0f1")),
        padding_left: Some(Number::from(30)),
        padding_right: Some(Number::from(30)),
        padding_top: Some(Number::from(20)),
        padding_bottom: Some(Number::from(20)),
        ..Default::default()
    };

    let mut title = Container {
        element: Element::Heading {
            size: HeaderSize::H1,
        },
        font_size: Some(Number::from(32)),
        ..Default::default()
    };
    title.children.push(Container {
        element: Element::Raw {
            value: "Welcome to HyperChad".to_string(),
        },
        ..Default::default()
    });
    header.children.push(title);

    // Add main content section
    let mut main_section = Container {
        element: Element::Main,
        background: Some(Color::from_hex("#ffffff")),
        padding_left: Some(Number::from(30)),
        padding_right: Some(Number::from(30)),
        padding_top: Some(Number::from(40)),
        padding_bottom: Some(Number::from(40)),
        direction: LayoutDirection::Column,
        row_gap: Some(Number::from(15)),
        ..Default::default()
    };

    let mut intro = Container {
        element: Element::Span,
        font_size: Some(Number::from(18)),
        ..Default::default()
    };
    intro.children.push(Container {
        element: Element::Raw {
            value:
                "This example demonstrates the core features of the HyperChad Transformer package."
                    .to_string(),
        },
        ..Default::default()
    });

    let mut description = Container {
        element: Element::Span,
        font_size: Some(Number::from(16)),
        color: Some(Color::from_hex("#7f8c8d")),
        ..Default::default()
    };
    description.children.push(Container {
        element: Element::Raw {
            value: "HyperChad provides a flexible container model for building modern UIs with styling, layout, and HTML generation capabilities.".to_string(),
        },
        ..Default::default()
    });

    main_section.children.push(intro);
    main_section.children.push(description);

    // Add footer
    let mut footer = Container {
        element: Element::Footer,
        background: Some(Color::from_hex("#34495e")),
        color: Some(Color::from_hex("#bdc3c7")),
        padding_left: Some(Number::from(30)),
        padding_right: Some(Number::from(30)),
        padding_top: Some(Number::from(15)),
        padding_bottom: Some(Number::from(15)),
        font_size: Some(Number::from(14)),
        ..Default::default()
    };
    footer.children.push(Container {
        element: Element::Raw {
            value: "HyperChad Transformer © 2024".to_string(),
        },
        ..Default::default()
    });

    page.children.push(header);
    page.children.push(main_section);
    page.children.push(footer);

    println!("   Built page with header, main content, and footer sections\n");

    // Step 3: Generate HTML output
    println!("3. Generating HTML output...");
    #[cfg(feature = "html")]
    {
        let html = page
            .display_to_string_default_pretty(false, true)
            .expect("Failed to generate HTML");
        let html_len = html.len();
        println!("   Generated HTML ({html_len} bytes):\n");
        println!("--- HTML Output ---");
        println!("{html}");
        println!("--- End HTML Output ---\n");
    }

    #[cfg(not(feature = "html"))]
    {
        println!("   (HTML feature not enabled, using Display trait)");
        println!("   Basic output: {page}\n");
    }

    // Step 4: Tree traversal
    println!("4. Demonstrating tree traversal...");
    let paths = page.bfs();
    let mut container_count = 0;
    paths.traverse(&page, |container| {
        container_count += 1;
        println!("   - Found {:?} element", container.element);
    });
    println!("   Total containers traversed: {container_count}\n");

    // Step 5: Working with different element types
    println!("5. Creating different element types...");

    // Button element
    let mut button = Container {
        element: Element::Button { r#type: None },
        background: Some(Color::from_hex("#3498db")),
        color: Some(Color::from_hex("#ffffff")),
        padding_left: Some(Number::from(20)),
        padding_right: Some(Number::from(20)),
        padding_top: Some(Number::from(10)),
        padding_bottom: Some(Number::from(10)),
        border_top_left_radius: Some(Number::from(5)),
        border_top_right_radius: Some(Number::from(5)),
        border_bottom_left_radius: Some(Number::from(5)),
        border_bottom_right_radius: Some(Number::from(5)),
        ..Default::default()
    };
    button.children.push(Container {
        element: Element::Raw {
            value: "Click Me".to_string(),
        },
        ..Default::default()
    });
    println!("   - Created Button element with styling");

    // Image element
    let _image = Container {
        element: Element::Image {
            source: Some("/images/example.jpg".to_string()),
            alt: Some("Example image".to_string()),
            fit: None,
            loading: None,
            sizes: None,
            source_set: None,
        },
        width: Some(Number::from(400)),
        height: Some(Number::from(300)),
        ..Default::default()
    };
    println!("   - Created Image element (400x300px)");

    // Anchor (link) element
    let mut link = Container {
        element: Element::Anchor {
            href: Some("https://example.com".to_string()),
            target: None,
        },
        color: Some(Color::from_hex("#2980b9")),
        ..Default::default()
    };
    link.children.push(Container {
        element: Element::Raw {
            value: "Visit Example".to_string(),
        },
        ..Default::default()
    });
    println!("   - Created Anchor (link) element\n");

    // Step 6: Demonstrate number system with different units
    println!("6. Number system examples...");
    let _ = Number::from(100);
    println!("   - Pixels: 100px");

    let _ = Number::RealPercent(50.0);
    println!("   - Percentage: 50%");

    let _ = Number::RealVw(80.0);
    println!("   - Viewport width: 80vw");

    let _ = Number::RealVh(60.0);
    println!("   - Viewport height: 60vh\n");

    println!("=== Example Complete ===");
    println!("\nThis example demonstrated:");
    println!("  ✓ Creating containers with styling properties");
    println!("  ✓ Building hierarchical component structures");
    println!("  ✓ Generating HTML output from containers");
    println!("  ✓ Traversing the container tree");
    println!("  ✓ Using various element types (div, header, main, footer, button, image, anchor)");
    println!("  ✓ Working with the number system (px, %, vw, vh)");
}
