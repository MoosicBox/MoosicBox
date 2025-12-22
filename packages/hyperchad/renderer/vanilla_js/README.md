# HyperChad Vanilla JS Renderer

Client-side JavaScript renderer for HyperChad with vanilla JavaScript and optional plugins.

## Overview

The HyperChad Vanilla JS Renderer provides:

- **Client-side Rendering**: Dynamic UI updates in the browser using vanilla JavaScript
- **Plugin System**: Modular plugin architecture for extended functionality
- **Action System**: Complete client-side action handling and event processing
- **DOM Manipulation**: Efficient DOM updates and element management
- **Event Handling**: Comprehensive event system with custom events
- **Form Processing**: Advanced form handling and validation
- **Navigation**: Client-side routing and navigation
- **Canvas Support**: 2D canvas rendering and graphics

## Features

### Core Functionality

- **DOM Rendering**: Convert HyperChad components to DOM elements
- **Event System**: Mouse, keyboard, and custom event handling
- **Action Processing**: Client-side action execution and effects
- **State Management**: Component state tracking and updates
- **Element Targeting**: Flexible element selection and manipulation

### Plugin System

- **Modular Architecture**: Enable only needed functionality
- **Navigation Plugin**: Client-side routing and history management
- **Idiomorph Plugin**: Intelligent DOM morphing for smooth updates
- **SSE Plugin**: Server-Sent Events for real-time updates
- **Tauri Plugin**: Integration with Tauri desktop applications
- **UUID Plugin**: Unique identifier generation
- **Canvas Plugin**: 2D graphics and drawing support
- **Form Plugin**: Advanced form handling and validation

### Action Plugins

- **Click Actions** (`plugin-actions-click`): Click event handling and processing
- **Click Outside Actions** (`plugin-actions-click-outside`): Detect clicks outside elements
- **Change Actions** (`plugin-actions-change`): Form input change detection
- **Mouse Down Actions** (`plugin-actions-mouse-down`): Mouse down event handling
- **Mouse Over Actions** (`plugin-actions-mouse-over`): Mouse over event handling
- **Key Down Actions** (`plugin-actions-key-down`): Keyboard key down events
- **Key Up Actions** (`plugin-actions-key-up`): Keyboard key up events
- **Event Key Down Actions** (`plugin-actions-event-key-down`): Custom key down event handling
- **Event Key Up Actions** (`plugin-actions-event-key-up`): Custom key up event handling
- **Resize Actions** (`plugin-actions-resize`): Window and element resize handling
- **Immediate Actions** (`plugin-actions-immediate`): Instant action execution on load
- **Event Actions** (`plugin-actions-event`): Custom event triggering and handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_vanilla_js = { path = "../hyperchad/renderer/vanilla_js" }

# With specific plugins
hyperchad_renderer_vanilla_js = {
    path = "../hyperchad/renderer/vanilla_js",
    features = [
        "plugin-nav",
        "plugin-idiomorph",
        "plugin-sse",
        "plugin-form",
        "plugin-canvas"
    ]
}

# With action plugins
hyperchad_renderer_vanilla_js = {
    path = "../hyperchad/renderer/vanilla_js",
    features = [
        "plugin-actions-click",
        "plugin-actions-change",
        "plugin-actions-resize"
    ]
}
```

## Usage

### Basic HTML Integration

```rust
use hyperchad_renderer_vanilla_js::VanillaJsTagRenderer;
use hyperchad_renderer_html::{HtmlRenderer, stub::StubApp};
use hyperchad_template::container;

// Create vanilla JS tag renderer
let tag_renderer = VanillaJsTagRenderer::default();

// Wrap in StubApp (or use router_to_actix/router_to_lambda for real backends)
let app = StubApp::new(tag_renderer);

// Create HTML renderer with vanilla JS support
let renderer = HtmlRenderer::new(app)
    .with_title(Some("Vanilla JS App".to_string()));

// Create interactive view
let view = container! {
    div class="app" {
        h1 { "Interactive HyperChad App" }

        div class="controls" {
            button
                fx-click=fx { show("message") }
                class="btn btn-primary"
            {
                "Show Message"
            }

            button
                fx-click=fx { hide("message") }
                class="btn btn-secondary"
            {
                "Hide Message"
            }

            button
                fx-click=fx { toggle_visibility("advanced") }
                class="btn btn-info"
            {
                "Toggle Advanced"
            }
        }

        div
            str_id="message"
            class="alert alert-success"
            style="display: none;"
        {
            "Hello from Vanilla JS! This message was shown using client-side actions."
        }

        div
            str_id="advanced"
            class="advanced-panel"
            style="display: none;"
        {
            h3 { "Advanced Features" }
            p { "Additional content can be placed here" }
        }
    }
};

// The generated HTML will include the vanilla JS script
renderer.render(hyperchad_renderer::View::from(view)).await?;
```

### Form Handling

```rust
use hyperchad_template::container;
use hyperchad_actions::ActionType;

let form_view = container! {
    form class="user-form" {
        h2 { "User Registration" }

        div class="form-group" {
            label for="username" { "Username:" }
            input
                type="text"
                id="username"
                name="username"
                required=true
            {}
        }

        div class="form-group" {
            label for="email" { "Email:" }
            input
                type="email"
                id="email"
                name="email"
                required=true
            {}
        }

        div class="form-group" {
            label for="password" { "Password:" }
            input
                type="password"
                id="password"
                name="password"
                required=true
            {}
        }

        div class="form-group" {
            input
                type="checkbox"
                id="agree"
                name="agree"
                required=true
            {}
            label for="agree" { "I agree to the terms and conditions" }
        }

        button
            type="submit"
            class="btn btn-primary"
        {
            "Register"
        }
    }
};
```

### Navigation and Routing

```rust
// With plugin-nav feature enabled
use hyperchad_template::container;
use hyperchad_actions::ActionType;

let navigation_view = container! {
    div class="app" {
        nav class="navbar" {
            a
                href="/"
                fx-click=fx { navigate("/") }
                class="nav-link"
            {
                "Home"
            }

            a
                href="/about"
                fx-click=fx { navigate("/about") }
                class="nav-link"
            {
                "About"
            }

            a
                href="/contact"
                fx-click=fx { navigate("/contact") }
                class="nav-link"
            {
                "Contact"
            }
        }

        div
            str_id="content"
            class="content"
        {
            // Content will be loaded based on route
        }
    }
};
```

### Working with Background Colors

```rust
use hyperchad_template::container;
use hyperchad_actions::ActionType;

let interactive_view = container! {
    div class="interactive-demo" {
        h2 { "Interactive Background Demo" }

        div class="color-controls" {
            button
                fx-click=fx { set_background_by_id("#ff0000", "display-box") }
            {
                "Red"
            }

            button
                fx-click=fx { set_background_by_id("#00ff00", "display-box") }
            {
                "Green"
            }

            button
                fx-click=fx { set_background_by_id("#0000ff", "display-box") }
            {
                "Blue"
            }

            button
                fx-click=fx { remove_background_by_id("display-box") }
            {
                "Reset"
            }
        }

        div
            str_id="display-box"
            class="color-display"
            style="width: 200px; height: 200px; border: 1px solid #ccc;"
        {
            "Click a color button above"
        }
    }
};
```

## Available Actions

The renderer supports various action types through the `ActionType` enum. Here are the commonly used actions:

### Visibility Actions

- `ActionType::show_by_id(target)` - Show element by ID
- `ActionType::hide_by_id(target)` - Hide element by ID
- `ActionType::toggle_visibility_by_id(target)` - Toggle element visibility (requires `logic` feature)
- `ActionType::show_class(class)` - Show elements by class name
- `ActionType::hide_class(class)` - Hide elements by class name

### Display Actions

- `ActionType::display_by_id(target)` - Set display to initial
- `ActionType::no_display_by_id(target)` - Set display to none
- `ActionType::display_class(class)` - Set display to initial for class
- `ActionType::no_display_class(class)` - Set display to none for class

### Focus Actions

- `ActionType::focus_by_id(target)` - Focus element by ID
- `ActionType::focus_class(class)` - Focus element by class name
- `ActionType::select_by_id(target)` - Select input text by ID

### Style Actions

- `ActionType::set_background_by_id(color, target)` - Set background color
- `ActionType::remove_background_by_id(target)` - Remove background color
- `ActionType::remove_background_class(class)` - Remove background color from class

### Navigation Actions

- `ActionType::Navigate { url }` - Navigate to URL (requires `plugin-nav`)

### Utility Actions

- `ActionType::Log { message, level }` - Log message to console
- `ActionType::Custom { action }` - Custom action string
- `ActionType::NoOp` - No operation

### Combining Actions

```rust
use hyperchad_template::container;

// Chain multiple actions using fx syntax
let view = container! {
    button fx-click=fx {
        show("element1");
        hide("element2");
        focus("input1")
    } {
        "Execute Multiple Actions"
    }
};

// Add throttling (300ms) and delay_off (2000ms) using action modifiers
let view_with_effects = container! {
    button fx-click=fx { show("modal").throttle(300) } {
        "Show Modal (Throttled)"
    }

    button fx-click=fx { show("notification").delay_off(2000) } {
        "Show Notification (Auto-hide)"
    }
};
```

## Plugin Features

### Navigation Plugin (`plugin-nav`)

Provides client-side navigation capabilities:

- **Navigation Actions**: Use `ActionType::Navigate { url }` for programmatic navigation
- **Browser History**: Integration with browser history API
- **SPA Support**: Single-page application routing

### Idiomorph Plugin (`plugin-idiomorph`)

Enables intelligent DOM updates:

- **Smart DOM Morphing**: Minimal DOM manipulation for updates
- **State Preservation**: Maintains form state and element focus during updates
- **Performance**: Efficient diffing algorithm

### SSE Plugin (`plugin-sse`)

Server-Sent Events support for real-time updates:

- **Real-time Updates**: Receive server-side events
- **Event Handling**: Process server-sent data streams
- **UUID Generation**: Requires `plugin-uuid` for event tracking

### Form Plugin (`plugin-form`)

Enhanced form handling capabilities:

- **Form Processing**: Client-side form handling
- **Data Serialization**: Form data collection and serialization

### Canvas Plugin (`plugin-canvas`)

Canvas rendering support:

- **2D Graphics**: Canvas 2D rendering context
- **Drawing Operations**: Basic rendering support (lines, rectangles, fills, strokes) through `CanvasUpdate` API

## Script Inclusion

The renderer automatically includes the appropriate JavaScript:

```html
<!-- Development -->
<script src="/js/hyperchad.js"></script>

<!-- Production (minified) -->
<script src="/js/hyperchad.min.js"></script>

<!-- With hash (cache busting) -->
<script src="/js/hyperchad-a1b2c3d4e5.min.js"></script>
```

## Feature Flags

### Core Features

- **`script`**: Include JavaScript code in the binary
- **`hash`**: Generate content-based script hashes

### Plugins

- **`plugin-nav`**: Navigation and routing
- **`plugin-idiomorph`**: DOM morphing
- **`plugin-sse`**: Server-Sent Events
- **`plugin-tauri-event`**: Tauri integration
- **`plugin-uuid`**: UUID generation
- **`plugin-uuid-insecure`**: Insecure UUID (development only)
- **`plugin-routing`**: Advanced routing
- **`plugin-event`**: Custom events
- **`plugin-canvas`**: Canvas support
- **`plugin-form`**: Form handling
- **`plugin-http-events`**: HTTP request lifecycle events

### Action Plugins

- **`plugin-actions-change`**: Change event actions
- **`plugin-actions-click`**: Click event actions
- **`plugin-actions-click-outside`**: Click outside detection
- **`plugin-actions-event`**: Custom event actions
- **`plugin-actions-event-key-down`**: Custom key down event handling
- **`plugin-actions-event-key-up`**: Custom key up event handling
- **`plugin-actions-immediate`**: Immediate actions
- **`plugin-actions-key-down`**: Keyboard key down events
- **`plugin-actions-key-up`**: Keyboard key up events
- **`plugin-actions-mouse-down`**: Mouse down actions
- **`plugin-actions-mouse-over`**: Mouse over actions
- **`plugin-actions-resize`**: Resize actions

## Dependencies

- **hyperchad_renderer**: Core renderer with canvas and HTML features
- **hyperchad_renderer_html**: HTML generation and rendering with assets and extension support
- **hyperchad_transformer**: Transformer with HTML and logic features
- **async-trait**: Async trait support
- **const_format**: Compile-time string formatting
- **convert_case**: String case conversion for JavaScript generation
- **html-escape**: HTML attribute escaping
- **log**: Logging support
- **maud**: HTML template generation
- **md5** (optional): Content hashing for cache busting (with `hash` feature)

## Integration

This renderer is designed for:

- **Web Applications**: Interactive client-side web apps
- **Progressive Enhancement**: Add interactivity to server-rendered pages
- **Single Page Applications**: Full SPA functionality
- **Desktop Apps**: Tauri-based desktop applications
- **Real-time Applications**: Apps with live updates and notifications

## Performance Considerations

- **Vanilla JavaScript**: No framework overhead
- **Modular Loading**: Load only needed plugins
- **Efficient DOM Updates**: Minimal DOM manipulation
- **Event Delegation**: Efficient event handling
- **Caching**: Script caching with content hashing
