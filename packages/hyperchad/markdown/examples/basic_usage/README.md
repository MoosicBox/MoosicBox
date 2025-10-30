# Basic Usage Example

This example demonstrates the fundamental usage of `hyperchad_markdown` to convert common markdown syntax into HyperChad Container structures.

## Summary

A comprehensive walkthrough of converting various markdown elements (text formatting, headings, links, lists, code blocks, blockquotes, and horizontal rules) into HyperChad Containers using the default configuration.

## What This Example Demonstrates

- Converting text with **bold** and _italic_ formatting
- Rendering headings (H1, H2, H3)
- Creating links to external resources
- Converting unordered lists
- Rendering fenced code blocks with language specification
- Using inline code snippets
- Creating blockquotes
- Adding horizontal rules for visual separation

## Prerequisites

- Basic understanding of Markdown syntax
- Familiarity with Rust programming
- Understanding that HyperChad Containers are backend-agnostic structures

## Running the Example

```bash
cargo run --manifest-path packages/hyperchad/markdown/examples/basic_usage/Cargo.toml
```

## Expected Output

The example will output each markdown conversion with the input markdown text and the resulting Container's child count:

```
=== HyperChad Markdown - Basic Usage Example ===

Example 1: Text Formatting
---------------------------
Markdown: This is **bold** text and this is *italic* text.
Container children count: 1

Example 2: Headings
-------------------
Markdown:
# Heading 1
## Heading 2
### Heading 3
Container children count: 3

[... additional examples ...]

=== All examples completed successfully! ===
```

## Code Walkthrough

### Basic Conversion

The simplest usage involves calling `markdown_to_container` with a markdown string:

```rust
use hyperchad_markdown::markdown_to_container;

let markdown = "This is **bold** text and this is *italic* text.";
let container = markdown_to_container(markdown);
```

This function uses default options which enable all GitHub Flavored Markdown features, emoji support, and XSS protection.

### Text Formatting

Bold and italic text is converted to appropriate Container structures with styling:

```rust
let markdown = "This is **bold** text and this is *italic* text.";
let container = markdown_to_container(markdown);
```

- `**bold**` creates a Container with `FontWeight::Bold`
- `*italic*` creates a Container with emphasis styling

### Headings

Headings are converted to Container elements with appropriate sizes:

```rust
let markdown = "# Heading 1\n## Heading 2\n### Heading 3";
let container = markdown_to_container(markdown);
```

Each heading level (H1-H6) is rendered with different font sizes and margins.

### Links

Links are converted to Anchor elements with proper href attributes:

```rust
let markdown = "Check out [HyperChad](https://github.com/moosicbox/hyperchad) on GitHub!";
let container = markdown_to_container(markdown);
```

### Lists

Unordered and ordered lists are converted to list Container structures:

```rust
let markdown = "- Apples\n- Bananas\n- Oranges";
let container = markdown_to_container(markdown);
```

### Code Blocks

Fenced code blocks with language specifications are properly converted:

````rust
let markdown = r#"```rust
fn main() {
    println!("Hello, world!");
}
```"#;
let container = markdown_to_container(markdown);
````

The language identifier (`rust`) is stored in the Container's data attributes.

## Key Concepts

### Backend Agnostic

`hyperchad_markdown` generates Container structures, not HTML strings. This means the same markdown can be rendered to:

- HTML via HyperChad's HTML backend
- Native UI via HyperChad's egui backend
- Any other backend that implements HyperChad rendering

### Default Options

The `markdown_to_container` function uses default options that include:

- All GitHub Flavored Markdown features enabled
- Emoji shortcode conversion (if compiled with `emoji` feature)
- XSS protection (if compiled with `xss-protection` feature)

### Container Structure

Each markdown element is converted to a Container with:

- An `element` type (Div, Heading, Anchor, etc.)
- CSS-like styling properties (color, font-size, margin, padding)
- Child containers for nested content
- Data attributes for metadata (like code language)

## Testing the Example

After running the example, verify that:

1. Each example section prints successfully
2. Container child counts match expectations (non-zero for valid markdown)
3. No panics or errors occur during conversion
4. The success message appears at the end

## Troubleshooting

**Issue**: Example doesn't compile

- **Solution**: Ensure you're in the repository root and the workspace is properly configured

**Issue**: Unexpected output

- **Solution**: This example only shows Container child counts, not rendered output. To see actual rendering, integrate with a HyperChad backend (HTML or egui)

## Related Examples

- `gfm_features` - Demonstrates GitHub Flavored Markdown specific features
- `custom_options` - Shows how to customize parsing options
