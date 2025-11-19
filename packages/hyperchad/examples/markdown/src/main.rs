//! `HyperChad` Markdown Example
//!
//! This example demonstrates the `hyperchad_markdown` package, which provides
//! backend-agnostic markdown rendering with full type safety. It showcases:
//!
//! - Converting Markdown to `HyperChad` `Container` structures
//! - Full `CommonMark` and GitHub Flavored Markdown support
//! - Emoji support via shortcodes (`:rocket:`, `:star:`, etc.)
//! - Built-in XSS protection
//! - Tables, task lists, code blocks, and all standard markdown features
//! - Integration with `HyperChad`'s routing and rendering system
//!
//! The example creates a web server that renders a comprehensive markdown demo page,
//! displaying various markdown features in a styled, interactive format.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad::{
    app::AppBuilder,
    renderer::View,
    router::{RouteRequest, Router},
    template::{self as hyperchad_template, Containers, container},
};
use hyperchad_markdown::markdown_to_container;
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

const INTRO_MARKDOWN: &str = r"
# Welcome to HyperChad Markdown! 🚀

This example demonstrates **hyperchad_markdown**, a powerful package that converts
Markdown into HyperChad `Container` structures for backend-agnostic rendering.

## Key Features

- ✅ **Full CommonMark Support** - All standard markdown syntax
- ✅ **GitHub Flavored Markdown** - Tables, strikethrough, task lists, and more
- ✅ **Emoji Support** - Use `:emoji:` shortcodes like `:rocket:`, `:star:`, `:fire:`
- ✅ **XSS Protection** - Built-in filtering of dangerous HTML
- ✅ **Backend Agnostic** - Renders to HTML, egui, or any HyperChad backend
- ✅ **Type Safe** - No HTML strings, just structured Containers

---

> **Note:** All the content you see on this page is rendered from Markdown!
";

const TEXT_FORMATTING: &str = r"
## Text Formatting

You can make text **bold**, *italic*, or ~~strikethrough~~.

You can also combine them: **_bold and italic_** or ***all three together***.

Inline `code` is supported with monospace font and background styling.

> **Blockquotes** are perfect for callouts and important notes.
> They can span multiple lines and include *formatting*.
";

const LISTS_MARKDOWN: &str = r"
## Lists

### Unordered Lists

- First item
- Second item
  - Nested item 1
  - Nested item 2
- Third item

### Ordered Lists

1. First step
2. Second step
3. Third step
   1. Sub-step A
   2. Sub-step B

### Task Lists

- [x] Implement markdown parser
- [x] Add emoji support
- [x] Create example application
- [ ] Add more features
- [ ] Write comprehensive docs
";

const CODE_MARKDOWN: &str = r##"
## Code Examples

### Inline Code

Use the `markdown_to_container()` function to convert markdown strings.

### Code Blocks

```rust
use hyperchad_markdown::markdown_to_container;

fn main() {
    let markdown = "# Hello **World**!";
    let container = markdown_to_container(markdown);

    // Render with any HyperChad backend
    println!("{}", container);
}
```

```javascript
// JavaScript example
const greeting = "Hello, Markdown!";
console.log(greeting);
```

```python
# Python example
def greet(name):
    return f"Hello, {name}!"

print(greet("Markdown"))
```
"##;

const TABLES_MARKDOWN: &str = r"
## Tables

### Feature Comparison

| Feature | hyperchad_markdown | Traditional HTML | Notes |
|---------|-------------------|------------------|-------|
| Type Safety | ✅ | ❌ | Compile-time validation |
| Backend Agnostic | ✅ | ❌ | Works with HTML, egui, etc. |
| GitHub Flavored | ✅ | ⚠️ | Requires additional libraries |
| Emoji Support | ✅ | ⚠️ | Built-in with feature flag |
| XSS Protection | ✅ | ⚠️ | Optional, built-in |
| Zero-cost Abstractions | ✅ | N/A | Rust performance |

### Markdown Elements

| Element | Syntax | Status |
|---------|--------|--------|
| Headings | `# H1` to `###### H6` | ✅ |
| Bold | `**text**` | ✅ |
| Italic | `*text*` | ✅ |
| Links | `[text](url)` | ✅ |
| Images | `![alt](url)` | ✅ |
| Code | `` `code` `` | ✅ |
| Lists | `- item` or `1. item` | ✅ |
| Tables | `\| a \| b \|` | ✅ |
| Blockquotes | `> quote` | ✅ |
| Strikethrough | `~~text~~` | ✅ |
| Task Lists | `- [ ] task` | ✅ |
";

const LINKS_IMAGES: &str = r"
## Links and Images

### Links

Check out these resources:

- [HyperChad GitHub Repository](https://github.com/MoosicBox/MoosicBox)
- [Markdown Guide](https://www.markdownguide.org/)
- [CommonMark Spec](https://commonmark.org/)

### Images

Images are supported (showing placeholder example):

![Markdown Logo](https://via.placeholder.com/150x50/4a90e2/ffffff?text=Markdown)
";

const EMOJIS_MARKDOWN: &str = r"
## Emoji Support 😎

Emojis make your content more expressive! You can use Unicode emojis directly
or use GitHub shortcodes:

- :rocket: `:rocket:` - For launches and deployments
- :star: `:star:` - For favorites and highlights
- :fire: `:fire:` - For hot topics
- :zap: `:zap:` - For fast performance
- :tada: `:tada:` - For celebrations
- :bug: `:bug:` - For bug reports
- :white_check_mark: `:white_check_mark:` - For completed tasks
- :x: `:x:` - For errors or failures
- :warning: `:warning:` - For warnings
- :heart: `:heart:` - For favorites

### More Emojis

🎨 🎯 🎮 🎲 🎸 🎹 🎤 🎧 🎬 🎭 🎪 🎨
📚 📝 📊 📈 📉 📌 📍 📎 📏 📐 📑 📒
💻 💾 💿 📀 🖥️ ⌨️ 🖱️ 🖨️ 📱 📲 ☎️ 📞
";

const ADVANCED_MARKDOWN: &str = r"
## Advanced Features

### Nested Blockquotes

> First level quote
> > Nested quote
> > > Even deeper nesting!

### Complex Lists

1. First item with **bold** and *italic*
   - Sub-item with `code`
   - Another sub-item with [link](https://example.com)
2. Second item with ~~strikethrough~~
   1. Numbered sub-item
   2. Another numbered sub-item
3. Third item with emoji :sparkles:

### Horizontal Rules

You can use horizontal rules to separate sections:

---

Content continues below the separator...

### Mixed Content

Combine **bold** with *italic*, add `code`, include :rocket: emojis,
create [links](https://github.com), and even ~~cross things out~~.
All in one sentence!
";

const USAGE_EXAMPLE: &str = r##"
## Usage Example

Here's how to use `hyperchad_markdown` in your own projects:

### Basic Usage

```rust
use hyperchad_markdown::markdown_to_container;
use hyperchad::template::container;

fn render_page() -> hyperchad::template::Containers {
    let markdown = r#"
# My Page Title

This is **markdown** content that will be
converted to HyperChad Containers!
    "#;

    let markdown_container = markdown_to_container(markdown);

    container! {
        div class="page" {
            header { "My Website" }
            main {
                // Insert the markdown content
                (markdown_container)
            }
            footer { "© 2024" }
        }
    }
}
```

### With Custom Options

```rust
use hyperchad_markdown::{markdown_to_container_with_options, MarkdownOptions};

let options = MarkdownOptions {
    enable_tables: true,
    enable_strikethrough: true,
    enable_tasklists: true,
    enable_footnotes: true,
    enable_smart_punctuation: true,
    emoji_enabled: true,
    xss_protection: true,
};

let container = markdown_to_container_with_options(markdown, options);
```

### Integration with HyperChad

```rust
use hyperchad::{
    router::{RouteRequest, Router},
    renderer::View,
};
use hyperchad_markdown::markdown_to_container;

fn create_router() -> Router {
    Router::new()
        .with_route("/blog/:slug", |req: RouteRequest| async move {
            // Load markdown content from file or database
            let markdown = load_blog_post(&req.params["slug"]).await;

            // Convert to container
            let content = markdown_to_container(&markdown);

            // Render in view
            View::builder()
                .with_primary(content)
                .build()
        })
}
```

### Cargo.toml Configuration

Add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_markdown = { version = "0.1", features = ["emoji", "xss-protection"] }
```

Or use the main hyperchad package:

```toml
[dependencies]
hyperchad = { version = "0.1", features = ["markdown", "markdown-emoji"] }
```
"##;

#[allow(clippy::too_many_lines)]
fn create_main_page() -> Containers {
    container! {
        div class="page" {
            header
                class="header"
                padding=32
                background=#1e293b
                color=white
                text-align=center
            {
                h1 font-size=36 margin-bottom=8 { "HyperChad Markdown Demo" }
                span font-size=18 color=#cbd5e1 {
                    "Backend-Agnostic Markdown Rendering with Type Safety"
                }
            }

            div
                direction=row
                justify-content=center
                width=100%
                background=#f8fafc
            {
                main
                    class="main"
                    padding=32
                    max-width=900
                    width=100%
                    gap=24
                {
                    div
                        padding=20
                        background=#dbeafe
                        border-left="4, #3b82f6"
                        border-radius=8
                        gap=8
                    {
                        div font-weight=bold color=#1e40af font-size=18 {
                            "ℹ️ About This Demo"
                        }
                        div color=#1e3a8a {
                            "Everything on this page is rendered from Markdown using "
                            span font-weight=bold { "hyperchad_markdown" }
                            ". The markdown content is converted to HyperChad Containers "
                            "and rendered with full type safety."
                        }
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(INTRO_MARKDOWN))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(TEXT_FORMATTING))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(LISTS_MARKDOWN))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(CODE_MARKDOWN))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(TABLES_MARKDOWN))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(LINKS_IMAGES))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(EMOJIS_MARKDOWN))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(ADVANCED_MARKDOWN))
                    }

                    div
                        background=white
                        padding=32
                        border-radius=12
                        gap=16
                    {
                        (markdown_to_container(USAGE_EXAMPLE))
                    }

                    div
                        padding=24
                        background=#f0fdf4
                        border-radius=12
                        gap=16
                    {
                        h2 color=#15803d { "Why hyperchad_markdown?" }

                        ul padding-left=20 gap=10 {
                            li {
                                span font-weight=bold { "Type Safe: " }
                                "No raw HTML strings, everything is structured data"
                            }
                            li {
                                span font-weight=bold { "Backend Agnostic: " }
                                "Works with HTML renderer, egui, or any future backend"
                            }
                            li {
                                span font-weight=bold { "Performance: " }
                                "Zero-cost abstractions, compiled Rust performance"
                            }
                            li {
                                span font-weight=bold { "Security: " }
                                "Built-in XSS protection (optional feature)"
                            }
                            li {
                                span font-weight=bold { "GitHub Flavored: " }
                                "Tables, task lists, strikethrough, and more"
                            }
                            li {
                                span font-weight=bold { "Emoji Support: " }
                                "GitHub-style :emoji: shortcodes"
                            }
                            li {
                                span font-weight=bold { "Easy Integration: " }
                                "Simple API, works seamlessly with HyperChad"
                            }
                        }
                    }
                }
            }

            footer
                class="footer"
                padding=24
                text-align=center
                background=#1e293b
                color=#cbd5e1
            {
                div gap=8 {
                    span { "Built with " }
                    span font-weight=bold color=white { "HyperChad" }
                    span { " & " }
                    span font-weight=bold color=white { "hyperchad_markdown" }
                }
                div margin-top=8 font-size=14 {
                    anchor
                        href="https://github.com/MoosicBox/MoosicBox"
                        color=#60a5fa
                    {
                        "View on GitHub"
                    }
                }
            }
        }
    }
}

fn create_router() -> Router {
    Router::new().with_route("/", |_req: RouteRequest| async move {
        View::builder().with_primary(create_main_page()).build()
    })
}

/// Runs the `HyperChad` Markdown example web server.
///
/// This function initializes the server and demonstrates markdown rendering
/// by creating a demo page with various markdown features. The server runs
/// on `http://localhost:8080` and serves a single route (`/`) that displays
/// the markdown demo.
///
/// # Errors
///
/// Returns an error if:
/// * The runtime cannot be created
/// * The server fails to bind to the specified port
/// * The application fails to start
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting HyperChad Markdown Example");

    let runtime = switchy::unsync::runtime::Builder::new().build()?;
    let router = create_router();

    info!("Server running on http://localhost:8080");
    info!("Press Ctrl+C to stop");

    #[allow(unused_mut)]
    let mut app = AppBuilder::new()
        .with_router(router)
        .with_runtime_handle(runtime.handle())
        .with_title("HyperChad Markdown Demo".to_string())
        .with_description(
            "Demonstrating backend-agnostic markdown rendering with hyperchad_markdown".to_string(),
        );

    #[cfg(feature = "assets")]
    for asset in ASSETS.iter().cloned() {
        app.static_asset_route_result(asset).unwrap();
    }

    app.build_default()?.run()?;

    Ok(())
}
