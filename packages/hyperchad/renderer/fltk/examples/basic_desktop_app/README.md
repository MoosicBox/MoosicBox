# Basic Desktop Application Example

A complete desktop GUI application demonstrating the core features of the HyperChad FLTK renderer, including window creation, component rendering, navigation, event handling, and layout management.

## Summary

This example showcases how to build a multi-page desktop application with native GUI widgets using HyperChad's FLTK renderer. It demonstrates navigation between views, interactive elements, flexbox layouts, and event handling.

## What This Example Demonstrates

- **Window initialization** with custom size, position, and styling
- **Multi-page navigation** with anchor elements and route handling
- **Flexbox layout system** for responsive component positioning
- **Interactive elements** including buttons with click handlers
- **Event handling** through action channels
- **Scrollable content areas** with overflow management
- **Component-based architecture** with reusable page functions
- **Async rendering** using Tokio runtime
- **Native FLTK widgets** with platform-specific appearance

## Prerequisites

### System Dependencies

**Ubuntu/Debian:**

```bash
sudo apt-get install libfltk1.3-dev libxinerama-dev libxft-dev libxcursor-dev
```

**macOS:**

```bash
# FLTK is included in the build
# No additional dependencies needed
```

**Windows:**

```bash
# FLTK is statically linked
# No additional dependencies needed
```

### Knowledge Requirements

- Basic understanding of Rust and async/await
- Familiarity with GUI application concepts
- Basic understanding of component-based UI architecture

## Running the Example

From the MoosicBox root directory:

```bash
# Using Nix (recommended for development)
nix develop .#fltk-hyperchad --command bash -c "cd packages/hyperchad/renderer/fltk/examples/basic_desktop_app && cargo run"

# Or directly with cargo (requires system FLTK dependencies)
cargo run --manifest-path packages/hyperchad/renderer/fltk/examples/basic_desktop_app/Cargo.toml

# With verbose logging
RUST_LOG=info cargo run --manifest-path packages/hyperchad/renderer/fltk/examples/basic_desktop_app/Cargo.toml
```

## Expected Output

When you run the example, you should see:

1. **Console Output:**

    ```
    [INFO] Starting HyperChad FLTK Desktop Application Example
    [INFO] Window initialized successfully
    [INFO] Initial view rendered
    [INFO] Starting FLTK event loop
    ```

2. **GUI Window:**
    - A 900x700 pixel window titled "HyperChad FLTK Example"
    - A dark header with navigation links (Home, About, Gallery)
    - A main content area with welcome text and feature cards
    - Interactive buttons for triggering actions
    - A footer with attribution text

3. **Interactive Behavior:**
    - Clicking navigation links changes the displayed page
    - Clicking "Show Message" logs a message to the console
    - Clicking "Count Click" increments a counter (logged to console)
    - The window can be resized, and content adapts accordingly

## Code Walkthrough

### 1. Application Setup

The application starts by initializing logging and creating communication channels:

```rust
env_logger::Builder::from_default_env()
    .filter_level(log::LevelFilter::Info)
    .init();

let (action_tx, action_rx) = unbounded::<(String, Option<Value>)>();
```

### 2. Renderer Initialization

The FLTK renderer is created and initialized with window properties:

```rust
let mut renderer = FltkRenderer::new(action_tx.clone());

renderer
    .init(
        900.0,                                    // width
        700.0,                                    // height
        Some(100),                                // x position
        Some(100),                                // y position
        Some(Color::from_hex("#f5f5f5")),         // background color
        Some("HyperChad FLTK Example"),           // window title
        Some("Basic Desktop Application Demo"),   // description
        None,                                     // viewport
    )
    .await?;
```

### 3. View Creation with `container!` Macro

Views are created using the type-safe `container!` macro, which provides compile-time guarantees:

```rust
fn create_home_page() -> hyperchad_template::Container {
    container! {
        div
            width="100%"
            height="100%"
            direction="column"
            background="#f5f5f5"
        {
            header
                width="100%"
                height=60
                background="#2c3e50"
            {
                h1 { "HyperChad Desktop" }
                // ... more content
            }

            main flex=1 {
                // Main content
            }

            footer {
                // Footer content
            }
        }
    }
    .into()
}
```

### 4. Navigation Handling

Navigation between pages is handled asynchronously:

```rust
tokio::spawn(async move {
    while let Some(href) = renderer_clone.wait_for_navigation().await {
        let new_view = match href.as_str() {
            "/" => create_home_page(),
            "/about" => create_about_page(),
            "/gallery" => create_gallery_page(),
            _ => create_home_page(),
        };

        renderer_clone.render(View::from(new_view)).await?;
    }
});
```

### 5. Action Event Handling

Button clicks and other actions are processed through a channel:

```rust
tokio::spawn(async move {
    while let Ok((action_name, value)) = action_rx.recv_async().await {
        match action_name.as_str() {
            "show_message" => {
                // Handle message display
            }
            "increment_counter" => {
                click_count += 1;
            }
            _ => {}
        }
    }
});
```

### 6. Event Loop Execution

The renderer is converted to a runner and the FLTK event loop is started:

```rust
let runner = renderer.to_runner(hyperchad_renderer::Handle::current())?;
runner.run()?;
```

## Key Concepts

### Flexbox Layout System

The example uses a flexbox-based layout system similar to CSS Flexbox:

- **`direction`**: Controls the flow direction (`row` or `column`)
- **`flex`**: Allows elements to grow and fill available space
- **`gap`**: Spacing between flex items
- **`padding`**: Inner spacing within elements
- **`align-items`**: Cross-axis alignment
- **`justify-content`**: Main-axis alignment
- **`overflow-y`**: Controls scrolling behavior

### Component Hierarchy

The application follows a structured component hierarchy:

```
Window
└── Page Container (div)
    ├── Header
    │   ├── Title (h1)
    │   └── Navigation (anchors)
    ├── Main Content
    │   ├── Hero Section
    │   ├── Features Section
    │   └── Interactive Section
    └── Footer
```

### Event-Driven Architecture

The application uses an event-driven architecture with three main event streams:

1. **Navigation Events**: Triggered by anchor clicks, handled to change views
2. **Action Events**: Triggered by button clicks, handled to perform operations
3. **FLTK Events**: Window events (resize, close, etc.) handled by the renderer

### Async Image Loading

While this example uses placeholder divs for images, the FLTK renderer supports async image loading:

```rust
// Example of actual image usage (not in this demo)
img
    src="https://example.com/image.png"
    width=200
    height=200
    fit="cover"
{}
```

Images are:

- Loaded asynchronously without blocking the UI
- Cached automatically to reduce network requests
- Scaled to fit specified dimensions
- Support HTTP URLs and local file paths

### Type Safety

The `container!` macro provides compile-time type safety:

- Invalid element combinations are caught at compile time
- Attribute types are validated
- Ensures valid HTML-like structure

## Testing the Example

### Test Navigation

1. **Click "About" link**: Should navigate to the about page with framework information
2. **Click "Gallery" link**: Should navigate to the gallery page with image placeholders
3. **Click "Home" link**: Should return to the home page

### Test Interactive Elements

1. **Click "Show Message"**: Check console for logged message
2. **Click "Count Click"**: Check console for incrementing counter
3. **Resize window**: Content should reflow according to flexbox rules

### Test Scrolling

1. Resize window to be very small vertically
2. Scroll content should appear in the main area
3. Header and footer should remain fixed

## Troubleshooting

### Issue: Window doesn't appear

**Solution**: Check that FLTK system dependencies are installed correctly. On Linux, ensure X11 or Wayland display server is running.

### Issue: Compilation errors related to FLTK

**Solution**: Install FLTK development libraries:

- Ubuntu/Debian: `sudo apt-get install libfltk1.3-dev`
- macOS: Use the Nix development environment
- Windows: Ensure Visual Studio Build Tools are installed

### Issue: "Cannot connect to display" error on Linux

**Solution**: Ensure X11 or Wayland is running. If using SSH, enable X11 forwarding or use a virtual display:

```bash
export DISPLAY=:0
```

### Issue: Navigation not working

**Solution**: Check console logs for navigation events. Ensure the navigation task is spawned and the renderer's `wait_for_navigation()` method is being called.

### Issue: Actions not being handled

**Solution**: Verify the action receiver task is running. Check console logs for action events. Ensure action channel is not dropped prematurely.

## Related Examples

- **`packages/hyperchad/examples/details_summary/`** - Demonstrates details/summary components
- **`packages/hyperchad/renderer/html/web_server/examples/basic_web_server/`** - Web-based equivalent using HTML renderer
- **`packages/hyperchad/examples/http_events/`** - Advanced event handling patterns

## Next Steps

After understanding this example, you can explore:

1. **Form inputs**: Add text fields, checkboxes (when implemented)
2. **Real images**: Replace placeholders with actual image URLs
3. **Persistent state**: Add state management between view changes
4. **Custom styling**: Experiment with colors, fonts, and layouts
5. **Multiple windows**: Create dialog boxes or tool palettes (when multi-window support is added)
6. **File operations**: Add file open/save dialogs
7. **System integration**: Add system tray icons, notifications

## Performance Notes

- **Fast startup**: FLTK applications start quickly with minimal overhead
- **Low memory**: Native widgets use less memory than web-based UIs
- **Efficient rendering**: Only changed elements are redrawn
- **Image caching**: Images are cached to reduce memory and network usage
- **Layout calculation**: Flexbox layout is calculated efficiently on resize

## Architecture Patterns

This example demonstrates several architectural patterns:

- **Separation of concerns**: Views, navigation, and actions are handled separately
- **Component composition**: Pages are composed from smaller reusable elements
- **Event-driven design**: UI updates driven by events rather than polling
- **Async/await**: Non-blocking operations using Tokio
- **Type safety**: Compile-time guarantees through the `container!` macro
