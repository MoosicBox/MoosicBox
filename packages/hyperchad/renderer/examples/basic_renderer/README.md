# Basic Renderer Example

A comprehensive example demonstrating how to implement the core `Renderer` trait from the `hyperchad_renderer` package to create a custom console-based renderer.

## Summary

This example shows how to build a complete renderer implementation that outputs HTML to the console. It demonstrates initialization, view rendering, event handling, and responsive trigger registration.

## What This Example Demonstrates

- **Renderer Trait Implementation**: Complete implementation of the async `Renderer` trait
- **View Composition**: Building views with primary content and fragments
- **Content Creation**: Using the `Content` builder API
- **Event System**: Emitting and handling custom events
- **Responsive Triggers**: Registering responsive breakpoints
- **Renderer Events**: Processing different event types
- **RenderRunner**: Converting a renderer to a runnable application
- **ToRenderRunner Trait**: Implementation for runtime execution

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with trait implementation
- Knowledge of HTML structure (helpful but not required)

## Running the Example

```bash
# From the repository root
cargo run --manifest-path packages/hyperchad/renderer/examples/basic_renderer/Cargo.toml

# Or from within the example directory
cd packages/hyperchad/renderer/examples/basic_renderer
cargo run

# With logging enabled (recommended)
RUST_LOG=info cargo run --manifest-path packages/hyperchad/renderer/examples/basic_renderer/Cargo.toml
```

## Expected Output

The example will output several sections demonstrating different rendering scenarios:

```
=== HyperChad Basic Renderer Example ===

[INFO] Starting Basic Renderer Example
[INFO] Renderer initialized:
[INFO]   Size: 1024x768
[INFO]   Title: Basic Renderer Demo
[INFO]   Description: Demonstrates implementing the Renderer trait
[INFO]   Background: Color { ... }
[INFO]   Viewport: width=device-width, initial-scale=1.0

--- Example 1: Simple Primary Content ---
================================================================================
RENDERED OUTPUT:
================================================================================
<div id="primary-content">
Hello from HyperChad!
</div>
================================================================================

--- Example 2: View with Fragments ---
[Shows rendered output with fragments]

--- Example 3: Using Content Builder ---
[Shows content built with Content API]

--- Example 4: Custom Events ---
[INFO] Event emitted: user_action = "button_clicked"
[INFO] Event emitted: page_loaded = "(no value)"

--- Example 5: Responsive Triggers ---
[INFO] Added responsive trigger 'mobile': MaxWidth(768.0)
[INFO] Added responsive trigger 'desktop': MinWidth(1024.0)

--- Example 6: Renderer Events ---
[Shows event processing]

=== Example completed successfully ===
```

## Code Walkthrough

### 1. Defining the Renderer Struct

The `ConsoleRenderer` struct stores renderer state:

```rust
struct ConsoleRenderer {
    width: f32,
    height: f32,
    title: String,
    description: String,
    background: Option<Color>,
    viewport: Option<String>,
}
```

### 2. Implementing the Renderer Trait

The `Renderer` trait requires implementing several async methods:

#### Initialization (`init`)

```rust
async fn init(
    &mut self,
    width: f32,
    height: f32,
    x: Option<i32>,
    y: Option<i32>,
    background: Option<Color>,
    title: Option<&str>,
    description: Option<&str>,
    viewport: Option<&str>,
) -> Result<(), Box<dyn Error + Send + 'static>> {
    self.width = width;
    self.height = height;
    self.background = background;
    // ... store other properties
    Ok(())
}
```

#### Rendering Views (`render`)

```rust
async fn render(&self, view: View) -> Result<(), Box<dyn Error + Send + 'static>> {
    // Render primary content
    if let Some(primary) = &view.primary {
        let html = self.render_container(primary);
        println!("{}", html);
    }

    // Render fragments for targeted updates
    for fragment in &view.fragments {
        let html = self.render_container(&fragment.container);
        println!("Fragment: {}", html);
    }

    Ok(())
}
```

#### Event Emission (`emit_event`)

```rust
async fn emit_event(
    &self,
    event_name: String,
    event_value: Option<String>,
) -> Result<(), Box<dyn Error + Send + 'static>> {
    info!("Event: {} = {:?}", event_name, event_value);
    Ok(())
}
```

#### Responsive Triggers (`add_responsive_trigger`)

```rust
fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
    info!("Responsive trigger '{}': {:?}", name, trigger);
}
```

### 3. Implementing ToRenderRunner and RenderRunner

These traits convert the renderer into a runnable application:

```rust
impl ToRenderRunner for ConsoleRenderer {
    fn to_runner(self, _handle: Handle) -> Result<Box<dyn RenderRunner>, Box<dyn Error + Send>> {
        Ok(Box::new(ConsoleRenderRunner { renderer: self }))
    }
}

impl RenderRunner for ConsoleRenderRunner {
    fn run(&mut self) -> Result<(), Box<dyn Error + Send + 'static>> {
        info!("Renderer is running");
        Ok(())
    }
}
```

### 4. Building Views

#### Simple View with Primary Content

```rust
let simple_view = View::builder()
    .with_primary(Container::from("Hello from HyperChad!"))
    .build();
```

#### View with Fragments (Targeted Updates)

```rust
let mut header = Container::from("Header Content");
header.str_id = Some("header".to_string());

let view = View::builder()
    .with_primary(Container::from("Main Content"))
    .with_fragment(ReplaceContainer::from(header))
    .build();
```

### 5. Using the Content Builder

```rust
let content = Content::builder()
    .with_primary(Container::from("Content"))
    .build();

if let Content::View(view) = content {
    renderer.render(*view).await?;
}
```

### 6. Processing Renderer Events

```rust
match event {
    RendererEvent::View(view) => {
        renderer.render(*view).await?;
    }
    RendererEvent::Event { name, value } => {
        renderer.emit_event(name, value).await?;
    }
}
```

## Key Concepts

### Renderer Trait

The `Renderer` trait is the core abstraction in `hyperchad_renderer`. It defines the interface that all rendering backends must implement, whether they output to:

- HTML (for web browsers)
- Native UI frameworks (desktop applications)
- Console output (like this example)
- Terminal UI frameworks
- Or any other display target

### View Structure

A `View` consists of:

- **primary**: Optional main content that replaces the triggering element
- **fragments**: Additional containers for targeted DOM updates (each identified by a selector)
- **delete_selectors**: CSS selectors for elements to remove from the DOM

This structure supports both full-page renders and partial updates (AJAX-style).

### Content Types

The `Content` enum represents different response types:

- `Content::View`: HTML view with containers
- `Content::Json`: JSON response (with `json` feature)
- `Content::Raw`: Raw bytes with custom content type

### Async Rendering

All rendering operations are async to support:

- Network operations (fetching assets, API calls)
- File I/O (loading templates, reading data)
- Concurrent rendering tasks
- Non-blocking UI updates

### Fragments and Targeted Updates

Fragments enable efficient partial page updates without full page reloads. Each fragment has a selector that identifies which DOM element to replace, enabling HTMX-style interactivity.

## Testing the Example

Run the example and observe:

1. **Initialization logs** showing renderer setup with window properties
2. **Rendered HTML output** in the console for each example
3. **Event logs** showing custom events being emitted
4. **Responsive trigger registration** for mobile and desktop breakpoints
5. **Event processing** demonstrating how to handle different event types

Try modifying the example:

- Change the window dimensions in the `init()` call
- Add more containers to the views
- Create views with delete selectors
- Add different responsive triggers
- Implement canvas rendering (with `canvas` feature)

## Troubleshooting

### Example doesn't compile

Ensure you have the workspace dependencies available:

```bash
# From repository root
cargo check --manifest-path packages/hyperchad/renderer/examples/basic_renderer/Cargo.toml
```

### No log output visible

Enable logging with the `RUST_LOG` environment variable:

```bash
RUST_LOG=info cargo run --manifest-path packages/hyperchad/renderer/examples/basic_renderer/Cargo.toml
```

### Async runtime errors

The example uses `switchy_async::main` which provides a runtime. If you see runtime-related errors, ensure the `switchy_async` dependency is configured correctly in `Cargo.toml`.

## Related Examples

- `packages/hyperchad/examples/details_summary/` - Full web application using HyperChad with renderer
- `packages/hyperchad/examples/http_events/` - Event handling in web applications
- `packages/web_server/examples/simple_get/` - HTTP server implementation patterns

## Next Steps

After understanding this basic example, you can:

1. **Implement a real renderer** - Build a renderer for your target platform (web, desktop, mobile)
2. **Add HTML rendering** - Implement the `HtmlTagRenderer` trait for full HTML generation
3. **Integrate with a web server** - Connect your renderer to Actix or another web framework
4. **Add canvas support** - Implement `render_canvas()` for drawing operations
5. **Build interactive applications** - Use the HyperChad app framework with your custom renderer

## Architecture Notes

This example demonstrates the **separation of concerns** in HyperChad:

- **hyperchad_renderer**: Core abstractions and traits (this package)
- **hyperchad_transformer**: Container model and HTML generation
- **hyperchad_color**: Color handling
- **switchy_async**: Runtime abstraction for async execution

Your renderer implementation can target any platform by implementing the `Renderer` trait and providing appropriate rendering logic for your target environment.
