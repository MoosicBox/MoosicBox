# Basic HTML Rendering Example

A demonstration of the HyperChad HTML renderer's core functionality, showing how to generate semantic HTML with CSS styling and responsive design from HyperChad containers.

## What This Example Demonstrates

- Creating an HTML tag renderer with responsive breakpoints
- Building containers with styled elements programmatically
- Generating complete HTML documents from containers
- Applying CSS styling, colors, and layout properties
- Using semantic HTML5 elements (header, main, section, footer)
- Adding responsive media queries for mobile and tablet devices
- Configuring page metadata (title, description, viewport)

## Prerequisites

- Basic understanding of HTML and CSS
- Familiarity with the HyperChad framework and container system
- Knowledge of responsive design concepts (media queries, breakpoints)

## Running the Example

To run this example, execute the following command from the repository root:

```bash
cargo run --manifest-path packages/hyperchad/renderer/html/examples/basic_rendering/Cargo.toml
```

Alternatively, from the example directory:

```bash
cd packages/hyperchad/renderer/html/examples/basic_rendering
cargo run
```

## Expected Output

When you run the example, you will see:

1. **Progress messages** indicating each step of the rendering process
2. **Complete HTML output** showing the generated HTML document
3. **Statistics** about the generated HTML (size, features detected)

The output includes a complete HTML5 document with:

- DOCTYPE declaration and proper HTML structure
- Meta tags for viewport, title, and description
- Background color applied to the body
- Semantic HTML elements (header, main, section, footer)
- Inline CSS styling for layout and appearance
- Responsive CSS media queries for mobile (768px) and tablet (1024px) breakpoints

Example console output:

```
HyperChad HTML Renderer - Basic Rendering Example
==================================================

1. Creating HTML tag renderer with responsive breakpoints...
   ✓ Renderer created with mobile (768px) and tablet (1024px) breakpoints

2. Building container with styled elements...
   ✓ Container created with header, main content, and footer

3. Generating HTML output...
   ✓ HTML generated successfully

4. Generated HTML Output:
   ==============================================================================
<!DOCTYPE html><html style="height:100%" lang="en">
<!-- ... full HTML document ... -->
   ==============================================================================

5. HTML Statistics:
   • Total size: 2847 bytes
   • Contains DOCTYPE: true
   • Contains viewport meta: true
   • Contains responsive CSS: true
   • Contains title: true

✓ Example completed successfully!
```

## Code Walkthrough

### Step 1: Creating the HTML Tag Renderer

```rust
let tag_renderer = DefaultHtmlTagRenderer::default()
    .with_responsive_trigger("mobile", ResponsiveTrigger::MaxWidth(Number::Real(768.0)))
    .with_responsive_trigger("tablet", ResponsiveTrigger::MaxWidth(Number::Real(1024.0)));
```

The `DefaultHtmlTagRenderer` is the core component that converts HyperChad containers into HTML. We configure it with responsive breakpoints that will generate CSS media queries. The "mobile" trigger activates for screens 768px and smaller, while "tablet" activates for screens 1024px and smaller.

### Step 2: Building the Container Structure

```rust
let container = create_sample_container();
```

The `create_sample_container()` function builds a container hierarchy programmatically:

- **Root container**: Uses `LayoutDirection::Column` for vertical stacking
- **Header section**: Dark background with centered white text
- **Main content**: Contains welcome and features sections with padding and styling
- **Footer section**: Light background with attribution text

Each container can have:

- `str_id`: Unique identifier for CSS targeting
- `tag`: Semantic HTML element type (Header, Main, Section, Footer, etc.)
- `classes`: CSS class names
- Style properties: `padding`, `background`, `color`, `text_align`, etc.
- `children`: Nested containers forming the document structure

### Step 3: Generating HTML

```rust
let html = container_element_to_html_response(
    &headers,
    &container,
    Some("width=device-width, initial-scale=1"), // viewport
    Some(Color { r: 243, g: 244, b: 246 }),      // background
    Some("HyperChad HTML Renderer Example"),      // title
    Some("A demonstration of the HyperChad HTML renderer"), // description
    &tag_renderer,
    &[], // CSS URLs
    &[], // CSS paths
    &[], // inline CSS
)?;
```

The `container_element_to_html_response` function is the main entry point for HTML generation. It:

1. Traverses the container tree recursively
2. Converts each container to appropriate HTML tags
3. Generates inline CSS from container style properties
4. Creates CSS media queries for responsive overrides
5. Wraps everything in a complete HTML document with proper DOCTYPE, head, and body

The function returns a complete, ready-to-serve HTML string.

## Key Concepts

### HTML Tag Renderer

The `DefaultHtmlTagRenderer` implements the `HtmlTagRenderer` trait, which defines how containers are converted to HTML. It handles:

- Converting container properties to CSS inline styles
- Generating CSS classes
- Creating responsive media queries based on triggers
- Building complete HTML documents with proper structure

### Containers and Elements

Containers are the building blocks of HyperChad UIs. Each container represents a visual element with:

- **Tag**: The HTML element type (div, header, section, span, etc.)
- **Styling**: Layout, colors, spacing, typography
- **Content**: Text or child containers
- **Behavior**: IDs, classes, and data attributes

### Responsive Design

Responsive triggers define breakpoints that generate CSS media queries:

```css
@media (max-width: 768px) {
    /* Mobile styles */
}

@media (max-width: 1024px) {
    /* Tablet styles */
}
```

Containers can specify responsive overrides that apply at these breakpoints, enabling adaptive layouts.

### Semantic HTML

The renderer generates semantic HTML5 elements:

- `<header>`: Page or section headers
- `<main>`: Primary content
- `<section>`: Thematic groupings
- `<footer>`: Page or section footers
- `<h1>`, `<h2>`, `<h3>`: Heading hierarchy
- `<p>`, `<span>`: Text content
- `<ul>`, `<li>`: Lists

This improves accessibility, SEO, and document structure.

## Testing the Example

After running the example:

1. **Examine the HTML output** - Look for proper DOCTYPE, semantic elements, and CSS styling
2. **Check the statistics** - Verify that responsive CSS and meta tags are present
3. **Save the HTML** - Copy the output to a `.html` file and open in a browser
4. **Test responsiveness** - Resize the browser window to see media queries in action
5. **Inspect with DevTools** - Use browser developer tools to examine the generated markup

To save the output to a file:

```bash
cargo run --manifest-path packages/hyperchad/renderer/html/examples/basic_rendering/Cargo.toml > output.html
```

Note: You'll need to extract just the HTML portion from the console output.

## Troubleshooting

### Issue: "error: could not find `Cargo.toml`"

**Solution**: Ensure you're running the command from the repository root, or use the full path to the Cargo.toml file as shown in the "Running the Example" section.

### Issue: "failed to compile" or dependency errors

**Solution**: Run `cargo clean` and `cargo update` in the repository root, then try again. Ensure you're using a compatible Rust version (check the repository's rust-toolchain file).

### Issue: Output is too verbose or cluttered

**Solution**: Adjust the logging level by setting the `RUST_LOG` environment variable:

```bash
RUST_LOG=warn cargo run --manifest-path packages/hyperchad/renderer/html/examples/basic_rendering/Cargo.toml
```

### Issue: Want to see the HTML in a browser

**Solution**: The example outputs to console. To view in a browser, redirect the HTML portion to a file, then open it:

1. Run the example and copy the HTML output between the separator lines
2. Save it to `output.html`
3. Open `output.html` in your web browser

## Related Examples

- `packages/hyperchad/examples/details_summary/` - Comprehensive web component example with interactive elements
- `packages/hyperchad/renderer/html/web_server/examples/basic_web_server/` - Example of serving HTML through a web server
- `packages/web_server/examples/simple_get/` - Simple HTTP server example

For more information about the HyperChad HTML renderer, see the [main package README](../../README.md).
