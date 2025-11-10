# hyperchad_markdown

Markdown to HyperChad Container conversion with GitHub Flavored Markdown support.

This crate provides a bridge between Markdown content and HyperChad's Container model, allowing Markdown to be rendered in any HyperChad backend (HTML, egui, etc.) without being tied to HTML string generation.

## Features

- **Full CommonMark Support**: Parse standard Markdown syntax
- **GitHub Flavored Markdown**: Tables, strikethrough, task lists, and more
- **Backend Agnostic**: Generates HyperChad Containers, not HTML strings
- **Emoji Support**: Convert `:emoji:` shortcodes to Unicode emojis (optional)
- **XSS Protection**: Filters dangerous HTML tags and attributes (optional)

## Usage

```rust
use hyperchad_markdown::markdown_to_container;

let markdown = "# Hello World\n\nThis is **bold** text.";
let container = markdown_to_container(markdown);

// Now render with any HyperChad backend
println!("{}", container); // HTML output
```

## Features

- `default` - Includes `emoji` and `xss-protection`
- `emoji` - Enable emoji shortcode replacement (e.g., `:rocket:` → 🚀)
- `xss-protection` - Filter dangerous HTML tags and attributes
