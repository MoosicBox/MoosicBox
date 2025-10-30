# Basic Window Example

A simple demonstration of creating and rendering a desktop window using the HyperChad FLTK renderer.

## Summary

This example shows the minimal setup required to create a native desktop application window with the FLTK renderer. It demonstrates window initialization, basic UI layout, text rendering, and the event loop.

## What This Example Demonstrates

- Creating and initializing an FLTK renderer instance
- Configuring window properties (size, position, background color, title)
- Building a UI layout using HyperChad's template system
- Using flexbox layout with column direction
- Rendering text with different heading levels (h1, h2)
- Applying styling attributes (colors, padding, gaps, margins)
- Running the FLTK event loop
- Basic application lifecycle management

## Prerequisites

Before running this example, you need:

- Rust 1.70 or later
- FLTK system dependencies (see below)

### System Dependencies

**Ubuntu/Debian:**

```bash
sudo apt-get install libfltk1.3-dev libxinerama-dev libxft-dev libxcursor-dev
```

**macOS:**

No additional dependencies needed - FLTK is included in the build.

**Windows:**

No additional dependencies needed - FLTK is statically linked.

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/hyperchad/renderer/fltk/examples/basic_window/Cargo.toml
```

Or from the example directory:

```bash
cd packages/hyperchad/renderer/fltk/examples/basic_window
cargo run
```

## Expected Output

When you run this example:

1. A window titled "HyperChad FLTK - Basic Window Example" will appear
2. The window will be 800x600 pixels and centered on your screen
3. The window will display:
    - A large heading "Welcome to FLTK!"
    - A description of the example
    - A features list showing what's demonstrated
    - An information box explaining FLTK
    - Footer text with instructions
4. The window has a dark background (#181a1b)
5. Closing the window exits the application

## Code Walkthrough

### 1. Setting Up Logging

```rust
env_logger::Builder::from_env(
    env_logger::Env::default().default_filter_or("info")
).init();
```

Initialize logging to see debug and info messages from the renderer.

### 2. Creating the Action Channel

```rust
let (action_tx, _action_rx) = flume::unbounded();
```

Create a communication channel. The UI can send action events through this channel (like button clicks). This example doesn't handle actions, but the channel is required for renderer initialization.

### 3. Initializing the Renderer

```rust
let mut renderer = FltkRenderer::new(action_tx);
renderer.init(
    800.0,                                     // width
    600.0,                                     // height
    None,                                      // x position (centered)
    None,                                      // y position (centered)
    Some(Color::from_hex("#181a1b")),         // background
    Some("HyperChad FLTK - Basic Window Example"), // title
    Some("A simple desktop window example"),   // description
    None,                                      // viewport
).await?;
```

Create the renderer and initialize the window with your desired configuration:

- **width/height**: Window dimensions in pixels
- **x/y position**: None means center the window on screen
- **background**: Optional background color (uses hex color)
- **title**: Window title shown in title bar
- **description**: Optional description metadata
- **viewport**: Optional viewport configuration (for scrolling contexts)

### 4. Building the UI

```rust
let view = container! {
    div
        width=780
        height=580
        direction="column"
        padding=20
        gap=15
    {
        h1 font-size=32 { "Welcome to FLTK!" }
        // ... more elements
    }
};
```

Use the `container!` macro to build a hierarchical UI layout:

- **div**: Container element (like HTML div)
- **direction="column"**: Stack children vertically (flexbox layout)
- **padding**: Space inside the container
- **gap**: Space between child elements
- **h1, h2**: Heading elements with automatic sizing

### 5. Rendering the View

```rust
renderer.render(View::from(view)).await?;
```

Send the view to the renderer to display it in the window.

### 6. Running the Event Loop

```rust
let mut runner = renderer.to_runner(hyperchad_renderer::Handle::current())?;
runner.run()?;
```

Convert the renderer to a runner and start the FLTK event loop. This blocks the main thread and processes GUI events until the window is closed.

## Key Concepts

### FLTK Renderer

The FLTK renderer is a native desktop GUI renderer that uses the Fast Light Toolkit (FLTK) to create cross-platform desktop applications. It's lightweight, fast, and has minimal dependencies.

### Template System

HyperChad uses a template macro system similar to HTML for defining UI layouts. Elements support attributes for styling and layout:

- Layout: `width`, `height`, `padding`, `margin`, `gap`
- Flexbox: `direction`, `justify-content`, `align-items`
- Styling: `background`, `color`, `font-size`

### Event Loop

FLTK uses a traditional retained-mode GUI event loop. The `runner.run()` call blocks and processes events (mouse clicks, keyboard input, window resize) until the application exits.

### Async Runtime

This example uses `switchy::main(tokio)` to provide an async runtime. The renderer's initialization and rendering methods are async, allowing integration with other async operations.

## Testing the Example

1. **Run the example** using the command above
2. **Verify the window appears** centered on your screen
3. **Check the title bar** shows "HyperChad FLTK - Basic Window Example"
4. **Verify the content** displays correctly with proper styling
5. **Try resizing** the window to see how the layout responds
6. **Close the window** to verify clean shutdown

## Troubleshooting

### "error: failed to run custom build command for `fltk-sys`"

You're missing FLTK system dependencies. Install them as described in the Prerequisites section above.

### Window appears but is blank or incorrectly sized

The renderer may have encountered a layout calculation issue. Check the logs (the example enables logging) for error messages.

### Application doesn't exit when closing the window

This shouldn't happen with this example, but if it does, the event loop may not be properly handling the close event. Press Ctrl+C to force exit.

## Related Examples

- For web-based rendering, see the `hyperchad/examples/details_summary` example
- For more complex FLTK features, see the main `hyperchad_renderer_fltk` package README

## Next Steps

After running this example, you can:

1. Modify the layout in `src/main.rs` to experiment with different layouts
2. Try different window sizes, positions, and background colors
3. Add more elements like images (see the package README for image examples)
4. Implement action handling by processing events from `action_rx`
5. Create multiple views and implement navigation between them
