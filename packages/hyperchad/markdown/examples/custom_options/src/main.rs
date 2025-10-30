#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating custom markdown parsing options.
//!
//! This example shows how to use `MarkdownOptions` to selectively enable or disable
//! specific markdown features, allowing fine-grained control over parsing behavior.

use hyperchad_markdown::{
    MarkdownOptions, markdown_to_container, markdown_to_container_with_options,
};

#[allow(clippy::too_many_lines)]
fn main() {
    println!("=== HyperChad Markdown - Custom Options Example ===\n");

    // Example 1: Default Options (all features enabled)
    println!("Example 1: Default Options");
    println!("--------------------------");
    let markdown = "| Table | Support |\n|-------|--------|\n| Yes   | ✓      |";
    let container = markdown_to_container(markdown);
    println!("Markdown: {markdown}");
    println!("With default options, tables are enabled");
    println!("Container children count: {}\n", container.children.len());

    // Example 2: Disable Tables
    println!("Example 2: Tables Disabled");
    println!("--------------------------");
    let markdown = "| Table | Support |\n|-------|--------|\n| Yes   | ✓      |";
    let options = MarkdownOptions {
        enable_tables: false,
        ..Default::default()
    };
    let container = markdown_to_container_with_options(markdown, options);
    println!("Markdown: {markdown}");
    println!("With tables disabled, pipe characters are treated as literal text");
    println!("Container children count: {}\n", container.children.len());

    // Example 3: Disable Strikethrough
    println!("Example 3: Strikethrough Disabled");
    println!("----------------------------------");
    let markdown = "This is ~~incorrect~~ correct text.";
    let options = MarkdownOptions {
        enable_strikethrough: false,
        ..Default::default()
    };
    let container = markdown_to_container_with_options(markdown, options);
    println!("Markdown: {markdown}");
    println!("With strikethrough disabled, tildes are treated as literal text");
    println!("Container children count: {}\n", container.children.len());

    // Example 4: Disable Task Lists
    println!("Example 4: Task Lists Disabled");
    println!("-------------------------------");
    let markdown = "- [x] Completed\n- [ ] Pending";
    let options = MarkdownOptions {
        enable_tasklists: false,
        ..Default::default()
    };
    let container = markdown_to_container_with_options(markdown, options);
    println!("Markdown:\n{markdown}\n");
    println!("With task lists disabled, checkboxes become literal text");
    println!("Container children count: {}\n", container.children.len());

    // Example 5: Minimal Features (only basic markdown)
    println!("Example 5: Minimal Configuration");
    println!("---------------------------------");
    let markdown = r"# Basic Markdown

**Bold** and *italic* still work.

But ~~strikethrough~~ and tables don't:
| A | B |
|---|---|
| 1 | 2 |

And task lists don't work:
- [ ] Task";
    let options = MarkdownOptions {
        enable_tables: false,
        enable_strikethrough: false,
        enable_tasklists: false,
        enable_footnotes: false,
        enable_smart_punctuation: false,
        emoji_enabled: false,
        xss_protection: true,
    };
    let container = markdown_to_container_with_options(markdown, options);
    println!("Markdown:\n{markdown}\n");
    println!("Only basic markdown features are enabled");
    println!("Container children count: {}\n", container.children.len());

    // Example 6: XSS Protection Enabled (default)
    println!("Example 6: XSS Protection Enabled");
    println!("----------------------------------");
    #[cfg(feature = "xss-protection")]
    {
        let markdown = "Safe text <script>alert('xss')</script> more text";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(markdown, options);
        println!("Markdown: {markdown}");
        println!("With XSS protection, dangerous HTML is escaped");
        println!("Container children count: {}\n", container.children.len());
    }
    #[cfg(not(feature = "xss-protection"))]
    {
        println!("Note: XSS protection feature not enabled at compile time\n");
    }

    // Example 7: XSS Protection Disabled
    println!("Example 7: XSS Protection Disabled");
    println!("-----------------------------------");
    let markdown = "Text with <em>HTML</em> tags";
    let options = MarkdownOptions {
        xss_protection: false,
        ..Default::default()
    };
    let container = markdown_to_container_with_options(markdown, options);
    println!("Markdown: {markdown}");
    println!("Without XSS protection, HTML passes through (use with caution!)");
    println!("Container children count: {}\n", container.children.len());

    // Example 8: Smart Punctuation
    println!("Example 8: Smart Punctuation");
    println!("----------------------------");
    let markdown = r#"He said "Hello..." -- then left."#;

    // With smart punctuation
    let options_smart = MarkdownOptions {
        enable_smart_punctuation: true,
        ..Default::default()
    };
    let _container_smart = markdown_to_container_with_options(markdown, options_smart);
    println!("Markdown: {markdown}");
    println!("With smart punctuation: converts ... to …, -- to –, etc.");

    // Without smart punctuation
    let options_no_smart = MarkdownOptions {
        enable_smart_punctuation: false,
        ..Default::default()
    };
    let _container_no_smart = markdown_to_container_with_options(markdown, options_no_smart);
    println!("Without smart punctuation: keeps literal characters\n");

    // Example 9: Emoji Configuration
    println!("Example 9: Emoji Options");
    println!("------------------------");
    #[cfg(feature = "emoji")]
    {
        let markdown = ":rocket: Launch!";

        // With emoji
        let options_emoji = MarkdownOptions {
            emoji_enabled: true,
            ..Default::default()
        };
        let _container = markdown_to_container_with_options(markdown, options_emoji);
        println!("Markdown: {markdown}");
        println!("With emoji enabled: :rocket: becomes 🚀");

        // Without emoji
        let options_no_emoji = MarkdownOptions {
            emoji_enabled: false,
            ..Default::default()
        };
        let _container = markdown_to_container_with_options(markdown, options_no_emoji);
        println!("Without emoji: :rocket: stays as literal text\n");
    }
    #[cfg(not(feature = "emoji"))]
    {
        println!("Note: Emoji feature not enabled at compile time");
        println!("Compile with --features emoji to enable\n");
    }

    // Example 10: Custom Configuration for Different Use Cases
    println!("Example 10: Use Case Configurations");
    println!("------------------------------------");

    // Blog posts: all features enabled
    #[allow(clippy::no_effect_underscore_binding)]
    let _blog_options = MarkdownOptions {
        enable_tables: true,
        enable_strikethrough: true,
        enable_tasklists: true,
        enable_footnotes: true,
        enable_smart_punctuation: true,
        emoji_enabled: cfg!(feature = "emoji"),
        xss_protection: true,
    };
    println!("Blog posts: All features enabled + XSS protection");

    // User comments: restricted features for safety
    #[allow(clippy::no_effect_underscore_binding)]
    let _comment_options = MarkdownOptions {
        enable_tables: false,
        enable_strikethrough: true,
        enable_tasklists: false,
        enable_footnotes: false,
        enable_smart_punctuation: true,
        emoji_enabled: cfg!(feature = "emoji"),
        xss_protection: true, // Critical for user-generated content
    };
    println!("User comments: Limited features + strict XSS protection");

    // Documentation: comprehensive features
    #[allow(clippy::no_effect_underscore_binding)]
    let _doc_options = MarkdownOptions {
        enable_tables: true,
        enable_strikethrough: true,
        enable_tasklists: true,
        enable_footnotes: true,
        enable_smart_punctuation: true,
        emoji_enabled: false,  // Keep docs professional
        xss_protection: false, // Trusted content
    };
    println!("Documentation: Full features, professional tone\n");

    println!("=== All custom options examples completed successfully! ===");
}
