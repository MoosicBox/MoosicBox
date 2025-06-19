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
- **Click Actions**: Click event handling and processing
- **Change Actions**: Form input change detection
- **Mouse Events**: Mouse over, mouse down event handling
- **Resize Actions**: Window and element resize handling
- **Immediate Actions**: Instant action execution
- **Event Actions**: Custom event triggering and handling

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
use hyperchad_renderer_html::{HtmlRenderer, HtmlApp};
use hyperchad_template::container;

// Create vanilla JS tag renderer
let tag_renderer = VanillaJsTagRenderer::default();

// Create HTML renderer with vanilla JS support
let mut renderer = HtmlRenderer::new(tag_renderer)
    .with_title(Some("Vanilla JS App".to_string()));

// Create interactive view
let view = container! {
    div class="app" {
        h1 { "Interactive HyperChad App" }

        div class="controls" {
            button
                onclick=show_str_id("message")
                class="btn btn-primary"
            {
                "Show Message"
            }

            button
                onclick=hide_str_id("message")
                class="btn btn-secondary"
            {
                "Hide Message"
            }

            button
                onclick=toggle_str_id("advanced")
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

            input
                type="text"
                placeholder="Enter some text"
                onchange=set_data_attr("user-input", event_value())
            {}

            button
                onclick=request_action("process_input", data_attr("user-input"))
            {
                "Process Input"
            }
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
    form
        class="user-form"
        onsubmit=request_action("submit_form", form_data())
    {
        h2 { "User Registration" }

        div class="form-group" {
            label for="username" { "Username:" }
            input
                type="text"
                id="username"
                name="username"
                required=true
                onchange=validate_field("username", event_value())
            {}
            span
                str_id="username-error"
                class="error-message"
                style="display: none;"
            {}
        }

        div class="form-group" {
            label for="email" { "Email:" }
            input
                type="email"
                id="email"
                name="email"
                required=true
                onchange=validate_field("email", event_value())
            {}
            span
                str_id="email-error"
                class="error-message"
                style="display: none;"
            {}
        }

        div class="form-group" {
            label for="password" { "Password:" }
            input
                type="password"
                id="password"
                name="password"
                required=true
                onchange=validate_field("password", event_value())
            {}
            span
                str_id="password-error"
                class="error-message"
                style="display: none;"
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

let navigation_view = container! {
    div class="app" {
        nav class="navbar" {
            a
                href="/"
                onclick=navigate("/")
                class="nav-link"
            {
                "Home"
            }

            a
                href="/about"
                onclick=navigate("/about")
                class="nav-link"
            {
                "About"
            }

            a
                href="/contact"
                onclick=navigate("/contact")
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

// Handle route changes
let route_handler = container! {
    div
        data-route="/"
        class="page"
    {
        h1 { "Home Page" }
        p { "Welcome to our website!" }
    }

    div
        data-route="/about"
        class="page"
        style="display: none;"
    {
        h1 { "About Us" }
        p { "Learn more about our company." }
    }

    div
        data-route="/contact"
        class="page"
        style="display: none;"
    {
        h1 { "Contact" }
        p { "Get in touch with us." }
    }
};
```

### Real-time Updates with SSE

```rust
// With plugin-sse feature enabled
use hyperchad_template::container;

let realtime_view = container! {
    div class="dashboard" {
        h1 { "Real-time Dashboard" }

        div
            str_id="status"
            class="status-panel"
            data-sse-endpoint="/api/status"
            data-sse-event="status-update"
        {
            "Connecting..."
        }

        div
            str_id="notifications"
            class="notifications"
            data-sse-endpoint="/api/notifications"
            data-sse-event="notification"
        {
            // Notifications will be added here
        }

        div class="metrics" {
            div
                str_id="user-count"
                class="metric"
                data-sse-endpoint="/api/metrics"
                data-sse-event="user-count"
            {
                "Users: 0"
            }

            div
                str_id="cpu-usage"
                class="metric"
                data-sse-endpoint="/api/metrics"
                data-sse-event="cpu-usage"
            {
                "CPU: 0%"
            }
        }
    }
};
```

### Canvas Graphics

```rust
// With plugin-canvas feature enabled
use hyperchad_template::container;
use hyperchad_renderer::canvas::{CanvasAction, CanvasUpdate};

let canvas_view = container! {
    div class="graphics-app" {
        h2 { "Canvas Graphics" }

        div class="canvas-controls" {
            button
                onclick=canvas_action("draw-canvas", "clear")
            {
                "Clear"
            }

            button
                onclick=canvas_action("draw-canvas", "draw-rect")
            {
                "Draw Rectangle"
            }

            button
                onclick=canvas_action("draw-canvas", "draw-circle")
            {
                "Draw Circle"
            }
        }

        canvas
            str_id="draw-canvas"
            width=600
            height=400
            style="border: 1px solid #ccc;"
        {}
    }
};

// Canvas actions would be handled client-side
let canvas_update = CanvasUpdate {
    id: "draw-canvas".to_string(),
    actions: vec![
        CanvasAction::SetFillStyle("#ff0000".to_string()),
        CanvasAction::FillRect { x: 10.0, y: 10.0, width: 100.0, height: 50.0 },
        CanvasAction::SetStrokeStyle("#000000".to_string()),
        CanvasAction::StrokeRect { x: 10.0, y: 10.0, width: 100.0, height: 50.0 },
    ],
};
```

### Custom Events and Actions

```rust
use hyperchad_template::container;
use hyperchad_actions::{ActionType, ActionEffect};

let interactive_view = container! {
    div class="interactive-demo" {
        h2 { "Custom Events Demo" }

        div class="color-picker" {
            button
                data-color="red"
                onclick=trigger_event("color-selected", data_attr("color"))
                class="color-btn red"
            {}

            button
                data-color="green"
                onclick=trigger_event("color-selected", data_attr("color"))
                class="color-btn green"
            {}

            button
                data-color="blue"
                onclick=trigger_event("color-selected", data_attr("color"))
                class="color-btn blue"
            {}
        }

        div
            str_id="color-display"
            class="color-display"
            on_event="color-selected" => set_style("background-color", event_value())
        {
            "Selected color will appear here"
        }

        div class="counter" {
            button
                onclick=increment_counter("counter-value")
            {
                "+"
            }

            span
                str_id="counter-value"
                data-count="0"
            {
                "0"
            }

            button
                onclick=decrement_counter("counter-value")
            {
                "-"
            }
        }
    }
};
```

### Tauri Integration

```rust
// With plugin-tauri-event feature enabled
use hyperchad_template::container;

let tauri_view = container! {
    div class="tauri-app" {
        h1 { "Tauri Desktop App" }

        div class="file-operations" {
            button
                onclick=tauri_invoke("open_file_dialog", null)
            {
                "Open File"
            }

            button
                onclick=tauri_invoke("save_file", data_attr("content"))
            {
                "Save File"
            }

            button
                onclick=tauri_invoke("show_notification", "Hello from Tauri!")
            {
                "Show Notification"
            }
        }

        textarea
            str_id="file-content"
            data-content=""
            placeholder="File content will appear here"
            onchange=set_data_attr("content", event_value())
        {}

        div
            str_id="status"
            class="status-bar"
        {
            "Ready"
        }
    }
};
```

## Plugin Features

### Navigation Plugin (`plugin-nav`)
- **Client-side Routing**: Browser history management
- **Route Matching**: Pattern-based route matching
- **Navigation Actions**: Programmatic navigation
- **History API**: Browser back/forward support

### Idiomorph Plugin (`plugin-idiomorph`)
- **Smart DOM Updates**: Intelligent DOM diffing and morphing
- **Animation Support**: Smooth transitions between states
- **Preserve State**: Maintain form state during updates
- **Performance**: Minimal DOM manipulation

### SSE Plugin (`plugin-sse`)
- **Server-Sent Events**: Real-time server updates
- **Automatic Reconnection**: Handle connection drops
- **Event Filtering**: Subscribe to specific events
- **Error Handling**: Graceful error recovery

### Form Plugin (`plugin-form`)
- **Form Validation**: Client-side validation rules
- **Data Binding**: Two-way data binding
- **Serialization**: Form data serialization
- **Submit Handling**: Form submission processing

### Canvas Plugin (`plugin-canvas`)
- **2D Graphics**: Canvas 2D rendering context
- **Drawing Operations**: Shapes, paths, and images
- **Event Handling**: Canvas mouse and touch events
- **Animation**: Frame-based animation support

## Script Inclusion

The renderer automatically includes the appropriate JavaScript:

```html
<!-- Development -->
<script src="/hyperchad.js"></script>

<!-- Production (minified) -->
<script src="/hyperchad.min.js"></script>

<!-- With hash (cache busting) -->
<script src="/hyperchad-a1b2c3d4e5.min.js"></script>
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

### Action Plugins
- **`plugin-actions-change`**: Change event actions
- **`plugin-actions-click`**: Click event actions
- **`plugin-actions-click-outside`**: Click outside detection
- **`plugin-actions-event`**: Custom event actions
- **`plugin-actions-immediate`**: Immediate actions
- **`plugin-actions-mouse-down`**: Mouse down actions
- **`plugin-actions-mouse-over`**: Mouse over actions
- **`plugin-actions-resize`**: Resize actions

## Dependencies

- **HyperChad Core**: Template, transformer, and action systems
- **HyperChad HTML Renderer**: HTML generation and rendering
- **Maud**: HTML template generation
- **Convert Case**: String case conversion
- **MD5**: Content hashing for cache busting

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
