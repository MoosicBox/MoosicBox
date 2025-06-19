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
- **Canvas Support**: Optional canvas rendering for graphics

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
use hyperchad_renderer_egui::{EguiRenderer, layout::EguiCalc};
use hyperchad_router::Router;
use hyperchad_actions::logic::Value;
use flume::unbounded;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create communication channels
    let (action_tx, action_rx) = unbounded();
    let (resize_tx, resize_rx) = unbounded();

    // Create router
    let router = Router::new();

    // Create client info
    let client_info = Arc::new(hyperchad_router::ClientInfo::default());

    // Create layout calculator
    let calculator = EguiCalc::default();

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
    let runner = renderer.to_runner(hyperchad_renderer::Handle::current())?;
    runner.run()?;

    Ok(())
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
                onclick=show_str_id("message")
            {
                "Show Message"
            }

            button
                background="red"
                color="white"
                padding=10
                onclick=hide_str_id("message")
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

### Form Handling

```rust
use hyperchad_template::container;
use hyperchad_actions::ActionType;

let form_view = container! {
    div
        width=400
        background="white"
        padding=20
        border="1px solid #ccc"
        direction="column"
        gap=15
    {
        h2 { "User Registration" }

        input
            type="text"
            name="username"
            placeholder="Enter username"
            onchange=set_data_attr("username", event_value())
        {}

        input
            type="password"
            name="password"
            placeholder="Enter password"
            onchange=set_data_attr("password", event_value())
        {}

        input
            type="checkbox"
            name="agree"
            onchange=set_data_attr("agreed", event_value())
        {}

        span { "I agree to the terms" }

        button
            background="blue"
            color="white"
            padding="10px 20px"
            onclick=request_action("submit_form", data_attrs())
        {
            "Register"
        }
    }
};

renderer.render(View::from(form_view)).await?;
```

### Image Display

```rust
use hyperchad_template::container;

let image_view = container! {
    div
        width=600
        height=400
        direction="column"
        align-items="center"
        gap=20
    {
        h2 { "Image Gallery" }

        img
            src="https://example.com/image.jpg"
            alt="Example Image"
            width=400
            height=300
            fit="cover"
            loading="lazy"
        {}

        div
            direction="row"
            gap=10
        {
            img
                src="/assets/thumb1.jpg"
                width=100
                height=100
                fit="cover"
                onclick=set_attr("main-image", "src", "/assets/image1.jpg")
            {}

            img
                src="/assets/thumb2.jpg"
                width=100
                height=100
                fit="cover"
                onclick=set_attr("main-image", "src", "/assets/image2.jpg")
            {}
        }
    }
};

renderer.render(View::from(image_view)).await?;
```

### Custom Layout Calculator

```rust
use hyperchad_renderer_egui::layout::{EguiCalc, EguiCalcDefaults};
use hyperchad_transformer::layout::calc::{Calculator, CalculatorDefaults};

// Create custom calculator with different defaults
let calculator = Calculator::new(
    EguiCalc::default(),
    CalculatorDefaults {
        font_size: 18.0,
        font_margin_top: 2.0,
        font_margin_bottom: 2.0,
        h1_font_size: 36.0,
        h1_font_margin_top: 24.0,
        h1_font_margin_bottom: 24.0,
        // ... other heading sizes
        ..Default::default()
    },
);

let renderer = EguiRenderer::new(
    router,
    action_tx,
    resize_tx,
    client_info,
    calculator,
);
```

### Event Handling

```rust
use hyperchad_actions::{ActionType, ActionEffect};

// Handle action events
tokio::spawn(async move {
    while let Ok((action_name, value)) = action_rx.recv_async().await {
        match action_name.as_str() {
            "submit_form" => {
                if let Some(Value::Object(data)) = value {
                    println!("Form submitted: {:?}", data);
                    // Process form data
                }
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
tokio::spawn(async move {
    while let Ok((width, height)) = resize_rx.recv_async().await {
        println!("Window resized: {}x{}", width, height);
        // Handle window resize
    }
});
```

### Partial Rendering

```rust
use hyperchad_renderer::PartialView;

// Update specific parts of the UI
let partial_update = PartialView {
    target: "message".to_string(),
    content: container! {
        div
            background="green"
            color="white"
            padding=10
        {
            "Updated message!"
        }
    },
    swap: hyperchad_transformer_models::SwapTarget::InnerHtml,
};

renderer.render_partial(partial_update).await?;
```

### Canvas Rendering

```rust
use hyperchad_renderer::canvas::{CanvasUpdate, CanvasAction};

let canvas_update = CanvasUpdate {
    id: "my-canvas".to_string(),
    actions: vec![
        CanvasAction::Clear,
        CanvasAction::SetFillStyle("#ff0000".to_string()),
        CanvasAction::FillRect { x: 10.0, y: 10.0, width: 100.0, height: 50.0 },
        CanvasAction::SetStrokeStyle("#000000".to_string()),
        CanvasAction::StrokeRect { x: 10.0, y: 10.0, width: 100.0, height: 50.0 },
    ],
};

renderer.render_canvas(canvas_update).await?;
```

## Feature Flags

- **`wgpu`**: Enable WGPU backend for GPU acceleration
- **`profiling`**: Enable performance profiling support
- **`profiling-puffin`**: Enable puffin profiler integration
- **`profiling-tracing`**: Enable tracing profiler integration
- **`debug`**: Enable debug rendering features

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
- **Tokio**: Async runtime for image loading and events
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
