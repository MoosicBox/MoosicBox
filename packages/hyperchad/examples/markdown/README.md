# HyperChad Markdown Example

This example demonstrates the `hyperchad_markdown` package, which converts Markdown content into HyperChad `Container` structures for backend-agnostic rendering.

## Features Demonstrated

✅ **Text Formatting** - Bold, italic, strikethrough, inline code
✅ **Headings** - H1 through H6 with proper hierarchy
✅ **Lists** - Ordered, unordered, and nested lists
✅ **Task Lists** - GitHub-style checkboxes
✅ **Code Blocks** - With language-specific syntax
✅ **Tables** - Full table support with headers
✅ **Links** - Internal and external hyperlinks
✅ **Images** - Image embedding with alt text
✅ **Blockquotes** - Nested blockquotes supported
✅ **Emoji** - GitHub-style `:emoji:` shortcodes
✅ **Horizontal Rules** - Section separators
✅ **XSS Protection** - Built-in security filtering

## Running the Example

```bash
cargo run -p hyperchad_markdown_example
```

Then open your browser to: **http://localhost:8080**

## Cargo Features

- `actix` - Enables Actix web server backend
- `assets` - Enables static asset serving
- `dev` - Enables embedded assets for local development (enables `assets`)
- `vanilla-js` - Enables vanilla JavaScript renderer with client-side interactivity plugins

Default features: `actix`, `dev`, `vanilla-js`

## What's Special About hyperchad_markdown?

### 1. **Type Safety**

Unlike traditional markdown-to-HTML converters that produce strings, `hyperchad_markdown` generates structured `Container` objects. This provides:

- Compile-time validation
- No risk of malformed HTML
- Type-safe composition with other UI elements

### 2. **Backend Agnostic**

The same markdown content can be rendered to:

- HTML (web)
- egui (native desktop apps)
- Any future HyperChad backend

No code changes needed!

### 3. **Zero-Cost Abstractions**

Built in Rust with zero-cost abstractions:

- No runtime overhead
- Compile-time optimization
- Native performance

### 4. **Security Built-In**

Optional XSS protection feature:

- Filters dangerous HTML tags
- Sanitizes JavaScript URLs
- Escapes malicious content

### 5. **GitHub Flavored Markdown**

Full GFM support:

- Tables
- Strikethrough
- Task lists
- Footnotes
- Smart punctuation

### 6. **Easy Integration**

Simple API that works seamlessly with HyperChad:

```rust
use hyperchad_markdown::markdown_to_container;

let markdown = "# Hello **World**!";
let container = markdown_to_container(markdown);

// Use in any HyperChad context
container! {
    div {
        (container)
    }
}
```

## Code Highlights

### Basic Markdown Rendering

```rust
use hyperchad_markdown::markdown_to_container;

let markdown = r#"
# Welcome

This is **bold** and this is *italic*.
"#;

let container = markdown_to_container(markdown);
```

### Custom Options

```rust
use hyperchad_markdown::{markdown_to_container_with_options, MarkdownOptions};

let options = MarkdownOptions {
    enable_tables: true,
    enable_strikethrough: true,
    enable_tasklists: true,
    emoji_enabled: true,
    xss_protection: true,
    ..Default::default()
};

let container = markdown_to_container_with_options(markdown, options);
```

### Integration with Router

```rust
Router::new()
    .with_route("/blog/:slug", |req: RouteRequest| async move {
        let markdown = load_blog_post(&req.params["slug"]).await;
        let content = markdown_to_container(&markdown);

        View::builder()
            .with_primary(content)
            .build()
    })
```

## Markdown Elements Supported

### Text Formatting

- **Bold**: `**text**` or `__text__`
- _Italic_: `*text*` or `_text_`
- ~~Strikethrough~~: `~~text~~`
- `Inline code`: `` `code` ``

### Headings

```markdown
# H1 Heading

## H2 Heading

### H3 Heading

#### H4 Heading

##### H5 Heading

###### H6 Heading
```

### Lists

```markdown
- Unordered list item
- Another item
    - Nested item

1. Ordered list item
2. Another item
    1. Nested item

- [ ] Task list item
- [x] Completed task
```

### Links and Images

```markdown
[Link text](https://example.com)
![Alt text](https://example.com/image.png)
```

### Code Blocks

````markdown
```rust
fn main() {
    println!("Hello, world!");
}
```
````

### Tables

```markdown
| Header 1 | Header 2 |
| -------- | -------- |
| Cell 1   | Cell 2   |
```

### Blockquotes

```markdown
> This is a blockquote
>
> > Nested blockquote
```

### Emoji

```markdown
:rocket: :star: :fire: :zap: :tada:
```

## Architecture

This example uses:

- **hyperchad_markdown** - Core markdown-to-Container conversion
- **HyperChad router** - Page routing
- **Actix web server** - HTTP backend
- **Vanilla JS renderer** - Client-side interactivity
- **HyperChad template syntax** - Declarative UI composition

## Performance

All markdown parsing happens:

- At compile time (for const strings)
- Once per request (for dynamic content)
- No re-parsing on client side
- Efficient Container tree generation

## Comparison

### Traditional Approach (HTML Strings)

```rust
let html = markdown_to_html(markdown);  // String
// ❌ Type unsafe
// ❌ Backend specific
// ❌ Runtime validation needed
// ❌ Easy to create malformed HTML
```

### hyperchad_markdown Approach

```rust
let container = markdown_to_container(markdown);  // Container
// ✅ Type safe
// ✅ Backend agnostic
// ✅ Compile-time validation
// ✅ Cannot create malformed HTML
```

## Use Cases

Perfect for:

- 📝 Blog platforms
- 📚 Documentation sites
- 💬 Comment systems
- 📖 Content management
- 📄 Static site generators
- 🔧 Admin panels
- 📱 Native desktop apps with markdown

## Future Enhancements

Potential improvements (not yet implemented):

- Math equation rendering (KaTeX/MathJax)
- Mermaid diagram support
- Custom markdown extensions
- Incremental rendering
- Streaming parsing

## Browser Compatibility

All features work in modern browsers:

- Chrome/Edge: ✅
- Firefox: ✅
- Safari: ✅
- Opera: ✅

## License

This example is part of the MoosicBox/HyperChad project.
