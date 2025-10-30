# Basic Desktop Application Example

This example demonstrates how to create a simple desktop application using the HyperChad egui renderer.

## Summary

A complete, runnable example showing the core setup pattern for desktop applications with HyperChad and egui, including custom layout calculator implementation, UI rendering, action handling, and window initialization.

## What This Example Demonstrates

- Creating a custom layout calculator implementing both `Calc` and `EguiCalc` traits
- Setting up font metrics with `EguiFontMetrics` for accurate text measurement
- Configuring heading sizes (H1-H6) and margins for proper layout
- Initializing the egui renderer with router, channels, and client info
- Building interactive UI with buttons using HyperChad's template macros
- Handling action events (show/hide visibility toggles)
- Managing window lifecycle and resize events
- Creating async runtime for desktop applications

## Prerequisites

- Rust toolchain (1.70+)
- Basic understanding of async/await in Rust
- Familiarity with HyperChad concepts (router, containers, actions)
- Desktop windowing system (X11, Wayland, or native platform)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/hyperchad/renderer/egui/examples/basic_desktop_app/Cargo.toml
```

Or from the example directory:

```bash
cd packages/hyperchad/renderer/egui/examples/basic_desktop_app
cargo run
```

## Expected Output

When you run this example, you should see:

1. **Console Output**:

    ```
    [INFO] Starting HyperChad egui basic desktop app example
    [INFO] Initializing window...
    [INFO] Window initialized
    [INFO] Navigated to root route
    [INFO] Action handler started
    [INFO] Resize handler started
    [INFO] Creating render runner...
    [INFO] Running application...
    ```

2. **Desktop Window**:
    - A 800x600 pixel window titled "HyperChad Basic Example"
    - A blue heading "Welcome to HyperChad!"
    - Descriptive text in a white card
    - Two buttons: green "Show Message" and red "Hide Message"
    - A hidden yellow message box that appears/disappears when clicking buttons

## Code Walkthrough

### 1. Calculator Setup

The calculator integrates egui's font system with HyperChad's layout engine:

```rust
#[derive(Clone)]
struct MyCalculator {
    inner: Option<Arc<Calculator<EguiFontMetrics>>>,
}

impl EguiCalc for MyCalculator {
    fn with_context(mut self, context: egui::Context) -> Self {
        const DELTA: f32 = 14.0 / 16.0;
        self.inner = Some(Arc::new(Calculator::new(
            EguiFontMetrics::new(context),
            CalculatorDefaults {
                font_size: 16.0 * DELTA,
                h1_font_size: 32.0 * DELTA,
                // ... more heading configurations
            },
        )));
        self
    }
}
```

**Key points**:

- `EguiFontMetrics::new(context)` creates a font metrics implementation using egui's context
- `CalculatorDefaults` defines base font sizes and heading sizes (H1-H6)
- `DELTA` scaling factor adjusts font sizes for desktop rendering
- The calculator is initialized lazily when egui context becomes available

### 2. Async Runtime and Channels

```rust
let runtime = runtime::Builder::new().build()?;
runtime.block_on(async {
    let (action_tx, action_rx) = unbounded();
    let (resize_tx, resize_rx) = unbounded();
    // ...
});
```

**Key points**:

- Creates a single-threaded async runtime for desktop operations
- `action_tx`/`action_rx`: Channel for UI action events (button clicks, etc.)
- `resize_tx`/`resize_rx`: Channel for window resize events
- Channels enable communication between UI and background handlers

### 3. Router and Route Registration

```rust
let router = Router::new();
router.on("GET", "/", Box::new(|_req: RouteRequest| {
    Box::pin(async {
        let view = container! {
            div width=800 height=600 {
                h1 { "Welcome to HyperChad!" }
                // ... more UI elements
            }
        };
        Ok(View::from(view))
    })
})).await;
```

**Key points**:

- Router manages navigation and rendering of different views
- Routes return `View` objects containing HyperChad containers
- The `container!` macro provides a declarative syntax for building UI
- Async route handlers enable data fetching and complex operations

### 4. Renderer Initialization

```rust
let mut renderer = EguiRenderer::new(
    router.clone(),
    action_tx,
    resize_tx,
    client_info,
    calculator,
);

renderer.init(
    800.0,                           // width
    600.0,                           // height
    None,                            // x position (centered)
    None,                            // y position (centered)
    None,                            // background color
    Some("HyperChad Basic Example"), // window title
    Some("A basic desktop application..."), // description
    None,                            // viewport
).await?;
```

**Key points**:

- `EguiRenderer::new` creates renderer with all necessary components
- `init()` creates and configures the native window
- Position `None` values center the window on screen
- Returns after window initialization completes

### 5. Action Event Handling

```rust
Handle::current().spawn(async move {
    while let Ok((_action_name, value)) = action_rx.recv_async().await {
        info!("Received action: {:?}", value);
        // Handle action based on type/value
    }
});
```

**Key points**:

- Spawns background task to process UI events
- Actions are triggered by `fx-click` and other event attributes in UI
- Can handle custom actions like navigation, data updates, etc.
- Runs concurrently with main application loop

### 6. Running the Application

```rust
let runner = renderer.to_runner(Handle::current())?;
runner.run()?;
```

**Key points**:

- `to_runner()` converts renderer into executable application
- `run()` starts the event loop and blocks until window closes
- Application handles rendering, input, and system events automatically

## Key Concepts

### Layout Calculator Pattern

The calculator pattern separates layout logic from rendering:

- **Calculator**: Computes element positions and sizes using font metrics
- **Font Metrics**: Provides text measurement capabilities (character widths, heights)
- **Defaults**: Defines standard sizes for headings and base text
- **Two-trait Implementation**: `Calc` for layout logic, `EguiCalc` for egui integration

This separation allows the same HyperChad components to render across different backends (egui, fltk, HTML) with appropriate metrics for each platform.

### Async Architecture

Desktop applications use async patterns for:

- **Non-blocking Operations**: Loading images, fetching data without freezing UI
- **Event Handling**: Processing user actions in background tasks
- **Navigation**: Async route handlers can load data before rendering
- **Concurrency**: Multiple background tasks running simultaneously

### Action System

HyperChad's action system enables declarative event handling:

```rust
fx-click=fx { show("message") }  // Declarative action in template
```

Actions flow from UI → channel → handler, allowing centralized event processing and side effects (navigation, data updates, logging, etc.).

### Immediate Mode GUI

Egui uses immediate mode architecture:

- UI is rebuilt every frame from current state
- No persistent UI object tree
- State is managed externally (in router, containers)
- Simpler mental model but different from retained mode (DOM, native widgets)

## Testing the Example

1. **Click "Show Message"**:

    - Yellow message box should appear with text
    - Console logs: `Received action: ...`

2. **Click "Hide Message"**:

    - Message box should disappear
    - Console logs: `Received action: ...`

3. **Resize Window**:

    - Drag window edges to resize
    - Console logs: `Window resized to: WxH`
    - UI elements should reflow based on flexbox layout

4. **Close Window**:
    - Click close button or press Alt+F4
    - Console logs: `Application closed`
    - Process exits cleanly

## Troubleshooting

### Window Doesn't Appear

- **Check Display Server**: Ensure X11 or Wayland is running on Linux
- **Check Logs**: Look for error messages in console output
- **Graphics Drivers**: Update GPU drivers, try `glow` backend instead of `wgpu`:
    ```bash
    cargo run --no-default-features --features glow
    ```

### Buttons Don't Work

- **Check Action Handler**: Verify action handler task is running (should see "Action handler started")
- **Check Channel**: Ensure `action_tx` is connected to renderer
- **Console Logs**: Look for "Received action" messages when clicking

### Compilation Errors

- **Missing Dependencies**: Run `cargo update` to refresh dependencies
- **Feature Flags**: Ensure `wgpu` feature is enabled (default)
- **Rust Version**: Update to Rust 1.70+ with `rustup update`

### Performance Issues

- **Disable Debug Logging**: Set `RUST_LOG=error` to reduce log overhead
- **Use Release Mode**: Run with `cargo run --release` for optimized build
- **GPU Backend**: Ensure `wgpu` is using hardware acceleration (check logs)

## Related Examples

- `packages/hyperchad/examples/details_summary/` - Advanced web UI components with collapsible sections
- `packages/hyperchad/examples/http_events/` - Server-sent events for live updates
- `packages/hyperchad/renderer/html/web_server/examples/basic_web_server/` - Web-based rendering
- `packages/hyperchad/app/src/renderer.rs` - Complete calculator implementation reference

---

Generated with [Claude Code](https://claude.com/claude-code)
