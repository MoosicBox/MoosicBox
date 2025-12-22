# hyperchad_markdown

Markdown to HyperChad Container conversion with GitHub Flavored Markdown support.

## Features

- **GitHub Flavored Markdown**: Full support for GFM extensions including tables, strikethrough, task lists, footnotes, and smart punctuation
- **Emoji support**: Convert emoji shortcodes (`:rocket:` to 🚀) when the `emoji` feature is enabled
- **XSS protection**: Sanitize dangerous HTML tags and URLs when the `xss-protection` feature is enabled
- **Syntax highlighting**: Highlight code blocks with language-specific coloring when the `syntax-highlighting` feature is enabled
- **Customizable parsing**: Configure which markdown features to enable via `MarkdownOptions`

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_markdown = { workspace = true }
```

## Usage

### Basic Conversion

```rust
use hyperchad_markdown::markdown_to_container;

let markdown = "# Hello World\n\nThis is **bold** and *italic* text.";
let container = markdown_to_container(markdown);
```

### Custom Options

```rust
use hyperchad_markdown::{markdown_to_container_with_options, MarkdownOptions};

let markdown = "| Header |\n|--------|\n| Cell   |";
let options = MarkdownOptions {
    enable_tables: true,
    enable_strikethrough: true,
    enable_tasklists: false,
    enable_footnotes: false,
    enable_smart_punctuation: true,
    emoji_enabled: false,
    xss_protection: true,
    syntax_highlighting: false,
};
let container = markdown_to_container_with_options(markdown, options);
```

## Cargo Features

- `emoji` (default) - Enable emoji shortcode conversion using `gh-emoji`
- `xss-protection` (default) - Enable XSS protection by escaping dangerous HTML tags and filtering dangerous URLs
- `syntax-highlighting` - Enable syntax highlighting for fenced code blocks using `syntect`

## License

See top-level README for licensing details.
