# HyperChad Egui Renderer

Native desktop UI renderer for HyperChad using the egui immediate mode GUI framework.

## Overview

The HyperChad Egui Renderer provides:

- **Native Desktop UI**: Render HyperChad components as native desktop applications
- **Immediate Mode GUI**: Built on egui's immediate mode architecture
- **Hardware Acceleration**: Optional WGPU backend for GPU-accelerated rendering
- **Cross-platform**: Works on Windows, macOS, and Linux
- **Layout Engine**: Complete CSS-like layout system with flexbox support
- **Interactive Elements**: Full support for forms, buttons, and user interactions
- **Image Loading**: Async image loading with caching
- **Viewport Management**: Scrolling and viewport-aware rendering
- **Event System**: Complete action and event handling

## Features

### Rendering Capabilities

- **Container Rendering**: Full HyperChad container hierarchy support
- **Element Types**: All HTML-equivalent elements (div, span, input, button, image, etc.)
- **Layout Systems**: Flexbox, positioning, margins, padding, borders
- **Typography**: Font sizing, text alignment, headings (H1-H6)
- **Styling**: Colors, backgrounds, borders, opacity, visibility
- **Responsive Design**: Conditional styling and responsive breakpoints

### Interactive Features

- **Form Elements**: Text inputs, checkboxes, buttons with validation
- **Event Handling**: Click, hover, focus, resize, and custom events
- **Action System**: Comprehensive action framework with effects
- **State Management**: Component state and data binding
- **Navigation**: Route handling and page navigation
- **Canvas Support**: Canvas rendering for graphics (available in v1 renderer)

### Performance Features

- **Efficient Rendering**: Immediate mode rendering with minimal overhead
- **Layout Caching**: Cached layout calculations for performance
- **Image Caching**: Smart image loading and caching system
- **Viewport Culling**: Only render visible elements
- **Profiling Support**: Optional profiling with puffin and tracing

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_egui = { path = "../hyperchad/renderer/egui" }

# With GPU acceleration
hyperchad_renderer_egui = {
    path = "../hyperchad/renderer/egui",
    features = ["wgpu"]
}

# With profiling
hyperchad_renderer_egui = {
    path = "../hyperchad/renderer/egui",
    features = ["profiling", "profiling-puffin"]
}
```

## Usage

### Basic Desktop Application

```rust
use hyperchad_renderer_egui::EguiRenderer;
use hyperchad_renderer::{ToRenderRunner, Handle};
use hyperchad_router::Router;
use hyperchad_actions::logic::Value;
use flume::unbounded;
use std::sync::Arc;

// You need to implement a custom calculator that implements both
// hyperchad_transformer::layout::Calc and hyperchad_renderer_egui::layout::EguiCalc
// See packages/hyperchad/app/src/renderer.rs for a complete example

#[derive(Clone)]
struct MyCalculator;

impl hyperchad_transformer::layout::Calc for MyCalculator {
    fn calc(&self, container: &mut hyperchad_transformer::Container) -> bool {
        // Your layout calculation logic
        true
    }
}

impl hyperchad_renderer_egui::layout::EguiCalc for MyCalculator {
    fn with_context(self, context: eframe::egui::Context) -> Self {
        // Initialize with egui context
        self
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create async runtime
    let runtime = switchy::unsync::runtime::Builder::new().build()?;

    runtime.block_on(async {
        // Create communication channels
        let (action_tx, action_rx) = unbounded();
        let (resize_tx, resize_rx) = unbounded();

        // Create router
        let router = Router::new();

        // Create client info
        let client_info = Arc::new(hyperchad_router::ClientInfo::default());

        // Create layout calculator
        let calculator = MyCalculator;

        // Create renderer
        let mut renderer = EguiRenderer::new(
            router,
            action_tx,
            resize_tx,
            client_info,
            calculator,
        );

        // Initialize window
        renderer.init(
            800.0,    // width
            600.0,    // height
            None,     // x position
            None,     // y position
            None,     // background color
            Some("My App"), // title
            Some("My HyperChad App"), // description
            None,     // viewport
        ).await?;

        // Create and run the application
        let runner = renderer.to_runner(Handle::current())?;
        runner.run()?;

        Ok(())
    })
}
```

### Rendering HyperChad Components

```rust
use hyperchad_template::container;
use hyperchad_renderer::{View, Renderer};

// Create HyperChad components
let view = container! {
    div
        width=800
        height=600
        background="#f0f0f0"
        direction="column"
        padding=20
    {
        h1
            color="blue"
            margin-bottom=20
        {
            "Welcome to HyperChad!"
        }

        div
            direction="row"
            gap=10
        {
            button
                background="green"
                color="white"
                padding=10
                fx-click=fx { show("message") }
            {
                "Show Message"
            }

            button
                background="red"
                color="white"
                padding=10
                fx-click=fx { hide("message") }
            {
                "Hide Message"
            }
        }

        div
            str_id="message"
            background="yellow"
            padding=15
            margin-top=20
            visibility="hidden"
        {
            "Hello from HyperChad!"
        }
    }
};

// Render the view
renderer.render(View::from(view)).await?;
```

### Custom Layout Calculator

For a complete working example of implementing a custom layout calculator with font metrics
and default sizing, see `packages/hyperchad/app/src/renderer.rs` which demonstrates:

- Creating a calculator that implements both `Calc` and `EguiCalc` traits
- Using `Calculator` with `CalculatorDefaults` for font sizes and margins
- Integrating `EguiFontMetrics` for accurate text measurement
- Setting up H1-H6 heading sizes and margins

The calculator is initialized with the egui context via the `with_context` method
when the renderer starts.

### Event Handling

```rust
use hyperchad_actions::logic::Value;
use switchy_async::runtime::Handle;

// Handle action events
Handle::current().spawn(async move {
    while let Ok((action_name, value)) = action_rx.recv_async().await {
        match action_name.as_str() {
            "submit_form" => {
                println!("Form submitted: {:?}", value);
                // Process form data
            }
            "navigate" => {
                if let Some(Value::String(url)) = value {
                    println!("Navigate to: {}", url);
                    // Handle navigation
                }
            }
            _ => {
                println!("Unknown action: {}", action_name);
            }
        }
    }
});

// Handle resize events
Handle::current().spawn(async move {
    while let Ok((width, height)) = resize_rx.recv_async().await {
        println!("Window resized: {}x{}", width, height);
        // Handle window resize
    }
});
```

### Canvas Rendering

**Note**: Canvas rendering is implemented in the v1 renderer only. The v2 renderer has canvas
rendering stubbed but not yet implemented.

```rust
use hyperchad_renderer::canvas::{CanvasUpdate, CanvasAction, Pos};
use hyperchad_renderer::Color;

let canvas_update = CanvasUpdate {
    target: "my-canvas".to_string(),
    canvas_actions: vec![
        CanvasAction::Clear,
        CanvasAction::StrokeColor(Color { r: 255, g: 0, b: 0, a: Some(255) }),
        CanvasAction::StrokeSize(2.0),
        CanvasAction::Line(Pos(10.0, 10.0), Pos(110.0, 10.0)),
        CanvasAction::FillRect(Pos(10.0, 20.0), Pos(110.0, 70.0)),
    ],
};

renderer.render_canvas(canvas_update).await?;
```

## Feature Flags

- **`wgpu`**: Enable WGPU backend for GPU acceleration (enabled by default)
- **`glow`**: Enable OpenGL backend
- **`wayland`**: Enable Wayland support on Linux
- **`x11`**: Enable X11 support on Linux
- **`profiling`**: Enable performance profiling support
- **`profiling-puffin`**: Enable puffin profiler integration
- **`profiling-tracing`**: Enable tracing profiler integration
- **`profiling-tracy`**: Enable Tracy profiler integration
- **`debug`**: Enable debug rendering features (enabled by default)
- **`v1`**: Use v1 renderer implementation (enabled by default) - Full-featured renderer with complete canvas support, comprehensive action handling, and mature viewport management. This is the production-ready implementation (~3800 lines).
- **`v2`**: Use v2 renderer implementation (enabled by default) - Refactored renderer with modular action handling using the new `ActionHandler` API from `hyperchad_actions`. This version uses simplified, more maintainable code architecture (~1000 lines). Canvas rendering and some advanced features are stubbed but not yet fully implemented. When both features are enabled, v2 takes precedence.

## Performance Considerations

- **Immediate Mode**: UI is rebuilt every frame for maximum flexibility
- **Layout Caching**: Layout calculations are cached when possible
- **Image Caching**: Images are cached to avoid repeated loading
- **Viewport Culling**: Only visible elements are processed
- **GPU Acceleration**: Use WGPU backend for better performance

## Dependencies

- **eframe**: egui application framework
- **egui**: Immediate mode GUI library
- **HyperChad Core**: Template, transformer, and action systems
- **switchy_async**: Runtime abstraction for async operations
- **Image**: Image processing and loading

## Integration

This renderer is designed for:

- **Desktop Applications**: Native desktop apps with rich UI
- **Development Tools**: IDE-like applications and editors
- **Games**: Game UI and tools with immediate mode benefits
- **Prototyping**: Rapid UI prototyping and development
- **Cross-platform Apps**: Applications targeting multiple desktop platforms

## Limitations

- **Mobile Support**: Not designed for mobile platforms
- **Web Deployment**: Cannot run in web browsers
- **Immediate Mode**: UI state must be managed externally
- **Memory Usage**: Higher memory usage due to immediate mode architecture
