#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating GitHub Flavored Markdown (GFM) specific features.
//!
//! This example showcases features that are specific to GitHub Flavored Markdown,
//! including tables, strikethrough, task lists, and emoji support.

use hyperchad_markdown::markdown_to_container;

fn main() {
    println!("=== HyperChad Markdown - GitHub Flavored Markdown Features ===\n");

    // Example 1: Tables
    println!("Example 1: Tables");
    println!("-----------------");
    let markdown = r"| Feature | Supported |
|---------|-----------|
| Tables  | ✓         |
| GFM     | ✓         |";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}\n");
    println!("Container children count: {}\n", container.children.len());

    // Example 2: Strikethrough
    println!("Example 2: Strikethrough");
    println!("------------------------");
    let markdown = "This is ~~incorrect~~ correct text.";
    let container = markdown_to_container(markdown);
    println!("Markdown: {markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 3: Task Lists
    println!("Example 3: Task Lists");
    println!("---------------------");
    let markdown = r"Project Tasks:
- [x] Set up repository
- [x] Implement markdown parser
- [ ] Add more examples
- [ ] Write documentation";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}\n");
    println!("Container children count: {}\n", container.children.len());

    // Example 4: Complex Table with Alignment
    println!("Example 4: Complex Table");
    println!("------------------------");
    let markdown = r"| Left Aligned | Center Aligned | Right Aligned |
|:-------------|:--------------:|--------------:|
| Row 1        | Data           | 100           |
| Row 2        | More Data      | 200           |
| Row 3        | Even More      | 300           |";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}\n");
    println!("Container children count: {}\n", container.children.len());

    // Example 5: Emoji (if feature enabled)
    #[cfg(feature = "emoji")]
    {
        println!("Example 5: Emoji Support");
        println!("------------------------");
        let markdown = ":rocket: Launch successful! :tada: :smile:";
        let container = markdown_to_container(markdown);
        println!("Markdown: {markdown}");
        println!("Note: Emoji shortcodes are converted to Unicode emoji");
        println!("Container children count: {}\n", container.children.len());
    }

    #[cfg(not(feature = "emoji"))]
    {
        println!("Example 5: Emoji Support");
        println!("------------------------");
        println!("Note: Emoji feature not enabled at compile time");
        println!("To enable, compile with: --features emoji\n");
    }

    // Example 6: Mixed GFM Features
    println!("Example 6: Mixed GFM Features");
    println!("-----------------------------");
    let markdown = r"# Project Status

## Completed Tasks
- [x] ~~Phase 1~~ **Done**
- [x] Phase 2

## Pending Tasks
- [ ] Phase 3
- [ ] Final review

| Phase   | Status      | Priority |
|---------|-------------|----------|
| Phase 1 | ✓ Complete  | High     |
| Phase 2 | ✓ Complete  | High     |
| Phase 3 | In Progress | Medium   |";
    let container = markdown_to_container(markdown);
    println!("Markdown:\n{markdown}\n");
    println!("Container children count: {}\n", container.children.len());

    // Example 7: Autolinks
    println!("Example 7: Autolinks");
    println!("--------------------");
    let markdown = "Visit https://github.com or email test@example.com";
    let container = markdown_to_container(markdown);
    println!("Markdown: {markdown}");
    println!("Container children count: {}\n", container.children.len());

    // Example 8: Smart Punctuation
    println!("Example 8: Smart Punctuation");
    println!("----------------------------");
    let markdown = r#"He said, "Hello..." -- and then -- he left."#;
    let container = markdown_to_container(markdown);
    println!("Markdown: {markdown}");
    println!("Note: Smart punctuation converts quotes, ellipses, and dashes");
    println!("Container children count: {}\n", container.children.len());

    println!("=== All GFM examples completed successfully! ===");
}
