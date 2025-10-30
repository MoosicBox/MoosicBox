# Basic Interactive Example

A comprehensive example demonstrating the core interactive features of the HyperChad Vanilla JS Renderer.

## Summary

This example showcases how to build interactive web applications using the HyperChad Vanilla JS Renderer. It demonstrates client-side actions, event handling, dynamic UI updates, and various interaction patterns—all powered by vanilla JavaScript without any framework dependencies.

## What This Example Demonstrates

- **Visibility actions**: Show and hide elements dynamically
- **Display actions**: Control element display properties for layout management
- **Logging actions**: Debug and trace action execution via browser console
- **Multiple actions**: Execute multiple actions in sequence with a single event
- **Event triggers**: Click event handling with `fx-click`
- **Pure vanilla JavaScript**: No framework overhead, just efficient vanilla JS

## Prerequisites

- Rust 1.70 or later
- Cargo
- Basic understanding of HyperChad's container syntax
- Familiarity with HTML and JavaScript concepts

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/hyperchad/renderer/vanilla_js/examples/basic_interactive/Cargo.toml
```

Or from the example directory:

```bash
cd packages/hyperchad/renderer/vanilla_js/examples/basic_interactive
cargo run
```

The server will start on `http://localhost:8080`. Open this URL in your web browser to interact with the example.

## Expected Output

When you run the example, you'll see:

```
Starting HyperChad Vanilla JS Interactive Example
Server running on http://localhost:8080
Press Ctrl+C to stop
```

In your browser, you'll see an interactive page with four sections:

1. **Show/Hide Actions**: Buttons to show and hide a message box
2. **Display Actions**: Buttons to show/hide a content panel using display properties
3. **Multiple Actions**: Buttons that trigger multiple actions simultaneously
4. **Logging Actions**: Buttons that log messages to the browser console

## Code Walkthrough

### Setting Up the Application

The example starts by initializing the application with the necessary components (packages/hyperchad/renderer/vanilla_js/examples/basic_interactive/src/main.rs:345-357):

```rust
// Initialize logging for development
env_logger::init();

// Create async runtime for handling requests
let runtime = switchy::unsync::runtime::Builder::new().build()?;

// Create router with a single route
let router = create_router();
```

### Serving Static Assets

The JavaScript runtime is served as a static asset (packages/hyperchad/renderer/vanilla_js/examples/basic_interactive/src/main.rs:22-36):

```rust
static ASSETS: LazyLock<Vec<hyperchad::renderer::assets::StaticAssetRoute>> = LazyLock::new(|| {
    vec![
        hyperchad::renderer::assets::StaticAssetRoute {
            route: format!("js/{}", hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()),
            target: hyperchad::renderer::assets::AssetPathTarget::FileContents(
                hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
            ),
        },
    ]
});
```

This ensures the vanilla JS runtime is available to the browser with cache-busting via content hashing.

### Creating Interactive Elements

The example demonstrates various action types using the `fx-*` attributes (packages/hyperchad/renderer/vanilla_js/examples/basic_interactive/src/main.rs:83-106):

```rust
// Show/Hide actions
button
    fx-click=fx { show("message-box") }
    class="btn-primary"
    padding=12
    background="#10b981"
{
    "Show Message"
}

button
    fx-click=fx { hide("message-box") }
    class="btn-secondary"
{
    "Hide Message"
}
```

### Multiple Actions in Sequence

You can chain multiple actions together (packages/hyperchad/renderer/vanilla_js/examples/basic_interactive/src/main.rs:189-202):

```rust
button
    fx-click=fx {
        show("panel-1");
        show("panel-2");
        log("Panels shown!")
    }
{
    "Show All Panels"
}
```

All actions within the `fx { }` block execute in order when the button is clicked.

### Logging Actions

The example shows how to log messages to the browser console (packages/hyperchad/renderer/vanilla_js/examples/basic_interactive/src/main.rs:258-267):

```rust
button
    fx-click=fx { log("Hello from HyperChad!") }
    padding=12
    background="#3b82f6"
{
    "Log Message"
}
```

## Key Concepts

### Client-Side Actions

All actions in this example execute entirely in the browser using the vanilla JavaScript runtime. No server round-trip is required for basic interactions, making the UI feel fast and responsive.

### Action Types

The vanilla JS renderer supports several action categories:

- **Visibility**: `show()`, `hide()`
- **Display**: `display()`, `no_display()`
- **Logging**: `log()`

Each action targets elements by their `id` attribute.

### Event System

The `fx-*` attributes map to JavaScript events:

- `fx-click`: Click events on the element
- `fx-hover`: Mouse over events
- `fx-change`: Change events (for inputs, selects, etc.)
- `fx-resize`: Window or element resize events
- `fx-keydown`: Keyboard key down events

### Element Targeting

Actions target elements using IDs (packages/hyperchad/renderer/vanilla_js/examples/basic_interactive/src/main.rs:108-118):

```rust
div
    id="message-box"  // Target ID for actions
    padding=16
    visibility=hidden
{
    "This element can be targeted by actions"
}
```

The `id` attribute creates an HTML `id` that actions can reference.

### Vanilla JavaScript Runtime

The renderer generates HTML with custom attributes (like `v-onclick`) that are processed by the embedded vanilla JavaScript runtime. This runtime handles:

- Event listener registration
- Action execution
- DOM manipulation
- Element targeting and selection

No external JavaScript frameworks are required—everything runs on pure vanilla JS.

## Testing the Example

### Test Visibility Actions

1. Click "Show Message" → Message box appears
2. Click "Hide Message" → Message box disappears

### Test Display Actions

1. Click "Show Content" → Content panel appears
2. Click "Hide Content" → Content panel disappears (doesn't take up space)

### Test Multiple Actions

1. Click "Show All Panels" → Both panels appear
2. Click "Hide All Panels" → Both panels disappear
3. Check browser console → See logged messages

### Test Logging Actions

1. Open browser DevTools (F12)
2. Click "Log Message" → See "Hello from HyperChad!" in console
3. Click "Log & Show" → See message in console and indicator appears

## Troubleshooting

### Server won't start on port 8080

**Problem**: Port 8080 is already in use by another application.

**Solution**: Either stop the other application or modify the port in the app configuration (this requires code changes as the example uses the default port).

### Actions don't work in the browser

**Problem**: JavaScript is disabled or the script didn't load.

**Solution**:

1. Check browser console (F12) for JavaScript errors
2. Verify that `/js/hyperchad-*.min.js` loads successfully (check Network tab)
3. Ensure the `assets` feature is enabled when running the example

### Elements not responding to clicks

**Problem**: The target element's `id` doesn't match the action's target.

**Solution**: Verify that action targets (e.g., `show("message-box")`) match the `id` attribute of the target element.

### Styles not applying correctly

**Problem**: CSS conflicts or specificity issues.

**Solution**: Use browser DevTools to inspect the element and check which styles are being applied. HyperChad generates inline styles that should have high specificity.

### Console messages not appearing

**Problem**: Browser console is not open or filtering messages.

**Solution**: Press F12 to open DevTools, go to the Console tab, and ensure all log levels are enabled (not filtered).

## Related Examples

- **details_summary** (`packages/hyperchad/examples/details_summary/`): Demonstrates collapsible content without JavaScript using HTML5 details/summary elements
- **http_events** (`packages/hyperchad/examples/http_events/`): Shows HTTP request lifecycle events with the vanilla JS renderer
- **markdown** (`packages/hyperchad/examples/markdown/`): Demonstrates rendering markdown content with HyperChad
- **basic_web_server** (`packages/hyperchad/renderer/html/web_server/examples/basic_web_server/`): Basic web server example without interactive features

This example focuses on client-side interactivity using the most commonly used actions (`show`, `hide`, `display`, `no_display`, `log`), while the others demonstrate different aspects of the HyperChad ecosystem.
