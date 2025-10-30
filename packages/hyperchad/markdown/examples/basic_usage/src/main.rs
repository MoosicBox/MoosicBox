#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic example demonstrating markdown to `HyperChad` `Container` conversion.
//!
//! This example shows how to convert simple markdown text into `HyperChad` `Container`
//! structures using the default options.

use hyperchad_markdown::markdown_to_container;

fn main() {
    println!("=== HyperChad Markdown - Basic Usage Example ===\n");

    // Example 1: Simple text formatting
    println!("Example 1: Text Formatting");
    println!("---------------------------");
    let markdown = "This is **bold** text and this is *italic* text.";
    let container = markdown_to_container(markdown);
    println!("Markdown: {markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 2: Headings
    println!("Example 2: Headings");
    println!("-------------------");
    let markdown = r"# Heading 1
## Heading 2
### Heading 3";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 3: Links
    println!("Example 3: Links");
    println!("----------------");
    let markdown = "Check out [HyperChad](https://github.com/moosicbox/hyperchad) on GitHub!";
    let container = markdown_to_container(markdown);
    println!("Markdown: {markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 4: Lists
    println!("Example 4: Lists");
    println!("----------------");
    let markdown = r"Shopping List:
- Apples
- Bananas
- Oranges";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 5: Code blocks
    println!("Example 5: Code Blocks");
    println!("----------------------");
    let markdown = r#"Here's some Rust code:
```rust
fn main() {
    println!("Hello, world!");
}
```"#;
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 6: Inline code
    println!("Example 6: Inline Code");
    println!("----------------------");
    let markdown = "Use the `markdown_to_container` function to convert markdown.";
    let container = markdown_to_container(markdown);
    println!("Markdown: {markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 7: Blockquotes
    println!("Example 7: Blockquotes");
    println!("----------------------");
    let markdown = r"> This is a blockquote.
> It can span multiple lines.";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 8: Horizontal rules
    println!("Example 8: Horizontal Rules");
    println!("---------------------------");
    let markdown = r"Content above
---
Content below";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}");
    println!("Container children count: {}\n", container.children.len());

    println!("=== All examples completed successfully! ===");
}
