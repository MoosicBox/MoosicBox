# HyperChad FLTK Renderer

Cross-platform native GUI renderer for HyperChad using the FLTK (Fast Light Toolkit) framework.

## Overview

The HyperChad FLTK Renderer provides:

- **Lightweight Native GUI**: Fast, lightweight native desktop applications
- **Cross-platform**: Works on Windows, macOS, and Linux with native look and feel
- **Low Resource Usage**: Minimal memory and CPU footprint
- **Retained Mode GUI**: Traditional widget-based GUI architecture
- **Layout Engine**: Complete flexbox and positioning layout system
- **Image Support**: Async image loading with caching and format support
- **Event System**: Comprehensive event handling and action system
- **Viewport Management**: Scrolling and viewport-aware rendering

## Features

### Native GUI Capabilities
- **FLTK Widgets**: Complete mapping of HyperChad elements to FLTK widgets
- **Native Styling**: Platform-native appearance and behavior
- **Window Management**: Multi-window support with proper window lifecycle
- **Menu Systems**: Native menu bars and context menus
- **Dialog Boxes**: File dialogs, message boxes, and custom dialogs

### Layout and Styling
- **Flexbox Layout**: Complete CSS flexbox implementation
- **Positioning**: Absolute, relative, and fixed positioning
- **Spacing**: Margins, padding, and gap support
- **Sizing**: Width, height, min/max constraints
- **Typography**: Font families, sizes, and text styling
- **Colors**: Background colors, text colors, and theming

### Interactive Elements
- **Form Controls**: Text inputs, buttons, checkboxes, radio buttons
- **Selection Widgets**: Dropdowns, list boxes, and choice widgets
- **Container Widgets**: Groups, tabs, and scrollable areas
- **Custom Widgets**: Support for custom widget implementations
- **Event Handling**: Mouse, keyboard, and focus events

### Image and Media
- **Image Loading**: Async HTTP image loading with caching
- **Image Formats**: Support for PNG, JPEG, GIF, and other formats
- **Image Scaling**: Automatic scaling and aspect ratio preservation
- **Asset Management**: Local and remote asset loading

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_fltk = { path = "../hyperchad/renderer/fltk" }

# With debug features
hyperchad_renderer_fltk = {
    path = "../hyperchad/renderer/fltk",
    features = ["debug"]
}
```

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

## Usage

### Basic Desktop Application

```rust
use hyperchad_renderer_fltk::FltkRenderer;
use hyperchad_template::container;
use hyperchad_renderer::{View, Renderer, ToRenderRunner};
use hyperchad_actions::logic::Value;
use flume::unbounded;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create communication channels
    let (action_tx, action_rx) = unbounded();

    // Create FLTK renderer
    let mut renderer = FltkRenderer::new(action_tx);

    // Initialize window
    renderer.init(
        800.0,    // width
        600.0,    // height
        Some(100), // x position
        Some(100), // y position
        Some(hyperchad_color::Color::from_hex("#f0f0f0")), // background
        Some("FLTK App"), // title
        Some("My HyperChad FLTK App"), // description
        None,     // viewport
    ).await?;

    // Create HyperChad view
    let view = container! {
        div
            width=780
            height=580
            direction="column"
            padding=10
            gap=10
        {
            h1
                font-size=24
                color="blue"
                text-align="center"
            {
                "Welcome to FLTK!"
            }

            div
                direction="row"
                gap=10
                justify-content="center"
            {
                button
                    width=100
                    height=30
                    background="green"
                    color="white"
                    onclick=show_str_id("message")
                {
                    "Show"
                }

                button
                    width=100
                    height=30
                    background="red"
                    color="white"
                    onclick=hide_str_id("message")
                {
                    "Hide"
                }
            }

            div
                str_id="message"
                width=400
                height=100
                background="yellow"
                padding=10
                align-self="center"
                visibility="hidden"
            {
                "Hello from FLTK! This is a native desktop application."
            }
        }
    };

    // Render the view
    renderer.render(View::from(view)).await?;

    // Convert to runner and start event loop
    let runner = renderer.to_runner(hyperchad_renderer::Handle::current())?;
    runner.run()?;

    Ok(())
}
```

### Form Application

```rust
use hyperchad_template::container;
use hyperchad_actions::ActionType;

let form_view = container! {
    div
        width=400
        height=300
        direction="column"
        padding=20
        gap=15
        background="white"
    {
        h2
            text-align="center"
            margin-bottom=20
        {
            "User Registration"
        }

        div
            direction="column"
            gap=10
        {
            div
                direction="row"
                align-items="center"
                gap=10
            {
                span
                    width=80
                    text-align="right"
                {
                    "Username:"
                }

                input
                    type="text"
                    name="username"
                    width=200
                    height=25
                    onchange=set_data_attr("username", event_value())
                {}
            }

            div
                direction="row"
                align-items="center"
                gap=10
            {
                span
                    width=80
                    text-align="right"
                {
                    "Password:"
                }

                input
                    type="password"
                    name="password"
                    width=200
                    height=25
                    onchange=set_data_attr("password", event_value())
                {}
            }

            div
                direction="row"
                align-items="center"
                gap=10
            {
                input
                    type="checkbox"
                    name="agree"
                    onchange=set_data_attr("agreed", event_value())
                {}

                span { "I agree to the terms and conditions" }
            }
        }

        div
            direction="row"
            justify-content="center"
            margin-top=20
        {
            button
                width=100
                height=35
                background="blue"
                color="white"
                onclick=request_action("submit_form", data_attrs())
            {
                "Register"
            }
        }
    }
};

renderer.render(View::from(form_view)).await?;
```

### Image Gallery

```rust
use hyperchad_template::container;

let gallery_view = container! {
    div
        width=600
        height=400
        direction="column"
        padding=20
        gap=15
    {
        h2
            text-align="center"
        {
            "Image Gallery"
        }

        div
            direction="row"
            gap=10
            justify-content="center"
            flex-wrap="wrap"
        {
            img
                src="https://picsum.photos/150/150?random=1"
                width=150
                height=150
                fit="cover"
                onclick=set_attr("main-image", "src", "https://picsum.photos/400/300?random=1")
            {}

            img
                src="https://picsum.photos/150/150?random=2"
                width=150
                height=150
                fit="cover"
                onclick=set_attr("main-image", "src", "https://picsum.photos/400/300?random=2")
            {}

            img
                src="https://picsum.photos/150/150?random=3"
                width=150
                height=150
                fit="cover"
                onclick=set_attr("main-image", "src", "https://picsum.photos/400/300?random=3")
            {}
        }

        div
            align-self="center"
            border="1px solid #ccc"
        {
            img
                str_id="main-image"
                src="https://picsum.photos/400/300?random=1"
                width=400
                height=300
                fit="contain"
            {}
        }
    }
};

renderer.render(View::from(gallery_view)).await?;
```

### Scrollable Content

```rust
use hyperchad_template::container;

let scrollable_view = container! {
    div
        width=400
        height=300
        direction="column"
    {
        h2
            padding=10
            background="lightgray"
        {
            "Scrollable List"
        }

        div
            flex=1
            overflow-y="scroll"
            padding=10
            gap=5
            direction="column"
        {
            // Generate many items
            @for i in 0..50 {
                div
                    padding=10
                    background=if i % 2 == 0 { "lightblue" } else { "white" }
                    border="1px solid #ccc"
                {
                    format!("Item {}", i + 1)
                }
            }
        }
    }
};

renderer.render(View::from(scrollable_view)).await?;
```

### Event Handling

```rust
use hyperchad_actions::logic::Value;

// Handle action events
tokio::spawn(async move {
    while let Ok((action_name, value)) = action_rx.recv_async().await {
        match action_name.as_str() {
            "submit_form" => {
                if let Some(Value::Object(data)) = value {
                    println!("Form data: {:?}", data);

                    // Show success message
                    // You could update the UI here
                }
            }
            "file_open" => {
                // Handle file operations
                use fltk::dialog;
                if let Some(filename) = dialog::file_chooser("Open File", "*.txt", ".", false) {
                    println!("Selected file: {}", filename);
                }
            }
            "app_exit" => {
                std::process::exit(0);
            }
            _ => {
                println!("Unknown action: {}", action_name);
            }
        }
    }
});
```

### Multi-Window Application

```rust
use hyperchad_template::container;

// Main window
let main_view = container! {
    div
        width=600
        height=400
        direction="column"
        padding=20
    {
        h1 { "Main Window" }

        div
            direction="row"
            gap=10
        {
            button
                onclick=request_action("open_settings", null)
            {
                "Open Settings"
            }

            button
                onclick=request_action("open_about", null)
            {
                "About"
            }
        }
    }
};

// Settings window (created when needed)
let settings_view = container! {
    div
        width=400
        height=300
        direction="column"
        padding=20
        gap=15
    {
        h2 { "Settings" }

        div
            direction="row"
            align-items="center"
            gap=10
        {
            span { "Theme:" }

            select
                name="theme"
                onchange=set_data_attr("theme", event_value())
            {
                option value="light" { "Light" }
                option value="dark" { "Dark" }
            }
        }

        div
            direction="row"
            justify-content="space-between"
            margin-top=20
        {
            button
                onclick=request_action("save_settings", data_attrs())
            {
                "Save"
            }

            button
                onclick=request_action("close_settings", null)
            {
                "Cancel"
            }
        }
    }
};
```

## Layout System

### Flexbox Support
- **Direction**: row, column, row-reverse, column-reverse
- **Justify Content**: start, center, end, space-between, space-around
- **Align Items**: start, center, end, stretch
- **Flex Properties**: flex-grow, flex-shrink, flex-basis
- **Gap**: Space between flex items

### Positioning
- **Static**: Normal document flow
- **Relative**: Positioned relative to normal position
- **Absolute**: Positioned relative to parent container
- **Fixed**: Positioned relative to window

### Sizing
- **Fixed Sizes**: Pixel values for width and height
- **Percentage**: Relative to parent container
- **Constraints**: min-width, max-width, min-height, max-height
- **Flex**: Flexible sizing based on available space

## Image Loading

### Supported Formats
- **PNG**: Portable Network Graphics
- **JPEG**: Joint Photographic Experts Group
- **GIF**: Graphics Interchange Format
- **BMP**: Windows Bitmap
- **TIFF**: Tagged Image File Format

### Loading Features
- **Async Loading**: Non-blocking image loading
- **HTTP Support**: Load images from URLs
- **Caching**: Automatic image caching
- **Scaling**: Automatic scaling to fit containers
- **Error Handling**: Graceful handling of load failures

## Feature Flags

- **`debug`**: Enable debug rendering and logging

## Performance Characteristics

### Advantages
- **Low Memory**: Minimal memory footprint
- **Fast Startup**: Quick application startup time
- **Native Performance**: Native widget performance
- **Small Binary**: Compact executable size

### Considerations
- **Retained Mode**: Widgets persist between updates
- **Layout Calculation**: Efficient layout algorithms
- **Image Caching**: Smart caching to reduce memory usage
- **Event Handling**: Efficient event propagation

## Dependencies

- **FLTK**: Fast Light Toolkit GUI library
- **HyperChad Core**: Template, transformer, and action systems
- **Image**: Image processing and format support
- **HTTP Client**: For remote image loading
- **Tokio**: Async runtime for image loading

## Integration

This renderer is designed for:
- **Desktop Applications**: Traditional desktop GUI applications
- **Utility Tools**: System utilities and development tools
- **Embedded Systems**: Applications for embedded devices
- **Legacy Systems**: Integration with existing FLTK applications
- **Cross-platform Tools**: Applications targeting multiple desktop platforms

## Limitations

- **Modern UI**: Limited support for modern UI patterns
- **Theming**: Basic theming capabilities compared to web
- **Animations**: Limited animation support
- **Complex Layouts**: Some complex CSS layouts not supported
- **Web Technologies**: No HTML/CSS/JavaScript integration
