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

## Supported Elements

### Fully Supported

- **Containers**: `div`, `aside`, `header`, `footer`, `main`, `section`, `form`, `span`, `details`, `summary`
- **Lists**: `ul` (unordered list), `ol` (ordered list), `li` (list item)
- **Tables**: `table`, `thead`, `th`, `tbody`, `tr`, `td`
- **Text**: `h1`, `h2`, `h3`, `h4`, `h5`, `h6` (headings), raw text
- **Images**: `img` (with async loading, HTTP support, and local file support)
- **Links**: `a` (anchor with navigation support)
- **Buttons**: `button` (rendered as clickable containers)
- **Dropdowns**: `select` with `option` children (rendered as FLTK Choice widget)

### Not Rendered

- **Form Inputs**: `input` (text, password, checkbox, radio, etc.)
- **Canvas**: `canvas` (element exists but no rendering implementation)

## Features

### Native GUI Capabilities

- **FLTK Widgets**: Rendering of HyperChad elements using FLTK widgets
- **Native Styling**: Platform-native appearance and behavior
- **Window Management**: Single-window support with resize handling
- **Planned**: Menu systems, dialog boxes, and multi-window support

### Layout and Styling

- **Flexbox Layout**: Complete CSS flexbox implementation
- **Positioning**: Support for layout positioning
- **Spacing**: Margins, padding, and gap support
- **Sizing**: Width, height, min/max constraints
- **Typography**: Font families, sizes, and text styling
- **Colors**: Background colors, text colors, and theming

### Interactive Elements

- **Containers**: Divs, sections, headers, footers, and other semantic elements
- **Clickable Elements**: Buttons (rendered as containers) and anchors with navigation
- **Scrollable Areas**: Horizontal and vertical scrolling with overflow support
- **Event Handling**: Click events and navigation
- **Dropdowns**: Select elements with option children
- **Planned**: Form inputs (text, checkbox, radio)

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
                    fx-click=fx { show("message") }
                {
                    "Show"
                }

                button
                    width=100
                    height=30
                    background="red"
                    color="white"
                    fx-click=fx { hide("message") }
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

### Interactive Button Application

```rust
use hyperchad_template::container;

let button_view = container! {
    div
        width=400
        height=200
        direction="column"
        padding=20
        gap=15
        background="white"
    {
        h2
            text-align="center"
            margin-bottom=20
        {
            "Interactive Buttons"
        }

        div
            direction="row"
            justify-content="center"
            gap=10
        {
            button
                width=120
                height=40
                background="blue"
                color="white"
                padding=10
                fx-click=fx { request_action("button_clicked", "action1") }
            {
                "Action 1"
            }

            button
                width=120
                height=40
                background="green"
                color="white"
                padding=10
                fx-click=fx { request_action("button_clicked", "action2") }
            {
                "Action 2"
            }

            button
                width=120
                height=40
                background="red"
                color="white"
                padding=10
                fx-click=fx { request_action("button_clicked", "action3") }
            {
                "Action 3"
            }
        }

        div
            str_id="status"
            padding=10
            text-align="center"
        {
            "Click a button above"
        }
    }
};

renderer.render(View::from(button_view)).await?;
```

**Note**: Form inputs (text, password, checkbox) are not yet implemented. The `input` element exists but is not currently rendered by the FLTK renderer. Additionally, `fx-click` and other action handlers shown in the examples above demonstrate the HyperChad template syntax but are not yet processed by the FLTK renderer.

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
                fx-click=fx { set_attr("main-image", "src", "https://picsum.photos/400/300?random=1") }
            {}

            img
                src="https://picsum.photos/150/150?random=2"
                width=150
                height=150
                fit="cover"
                fx-click=fx { set_attr("main-image", "src", "https://picsum.photos/400/300?random=2") }
            {}

            img
                src="https://picsum.photos/150/150?random=3"
                width=150
                height=150
                fit="cover"
                fx-click=fx { set_attr("main-image", "src", "https://picsum.photos/400/300?random=3") }
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

// Handle action events from button clicks and other interactions
tokio::spawn(async move {
    while let Ok((action_name, value)) = action_rx.recv_async().await {
        match action_name.as_str() {
            "button_clicked" => {
                if let Some(Value::String(button_id)) = value {
                    println!("Button clicked: {}", button_id);
                    // Update UI or perform action based on button_id
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

### Navigation Between Views

```rust
use hyperchad_template::container;

// Main menu view
let menu_view = container! {
    div
        width=600
        height=400
        direction="column"
        padding=20
        gap=15
    {
        h1
            text-align="center"
        {
            "Main Menu"
        }

        div
            direction="column"
            gap=10
            align-items="center"
        {
            a
                href="/gallery"
                width=200
                height=50
                background="blue"
                color="white"
                padding=10
                text-align="center"
            {
                div { "View Gallery" }
            }

            a
                href="/about"
                width=200
                height=50
                background="green"
                color="white"
                padding=10
                text-align="center"
            {
                div { "About" }
            }
        }
    }
};

// Render and handle navigation
renderer.render(View::from(menu_view)).await?;

// Wait for navigation event
if let Some(href) = renderer.wait_for_navigation().await {
    println!("Navigating to: {}", href);
    // Render the new view based on href
}
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

- **`debug`**: Enable debug rendering and logging (default: enabled)
- **`format`**: Enable formatter support for templates (default: enabled)
- **`unsafe`**: Enable unsafe optimizations (default: enabled)

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

### Core Dependencies

- **fltk**: Fast Light Toolkit GUI library (with ninja build support)
- **hyperchad_renderer**: Core renderer traits and utilities (with canvas and viewport-retained features)
- **hyperchad_transformer**: Template transformation and layout engine (with html and layout features)
- **hyperchad_actions**: Action system and logic (with logic feature)
- **moosicbox_app_native_image**: Native image asset handling

### Runtime Dependencies

- **image**: Image processing and format support
- **switchy_async**: Async runtime and task utilities (with sync and tokio features)
- **switchy_http**: HTTP client for remote image loading (reqwest backend)
- **flume**: Multi-producer, multi-consumer channels
- **bytes**: Byte buffer utilities

## Integration

This renderer is designed for:

- **Desktop Applications**: Traditional desktop GUI applications
- **Utility Tools**: System utilities and development tools
- **Embedded Systems**: Applications for embedded devices
- **Legacy Systems**: Integration with existing FLTK applications
- **Cross-platform Tools**: Applications targeting multiple desktop platforms

## Limitations

### Not Yet Implemented

- **Form Inputs**: Text inputs, checkboxes, radio buttons are not rendered
- **Action Handlers**: `fx-click`, `fx-change`, and other action event handlers are not processed
- **Multi-Window Support**: Currently limited to single window applications
- **Native Menus**: Menu bars and context menus not implemented
- **Dialog Boxes**: File choosers and message boxes not integrated

### Design Constraints

- **Theming**: Basic theming capabilities compared to web renderers
- **Animations**: No animation support
- **Advanced CSS**: Some advanced CSS features not supported
- **Web Technologies**: No HTML/CSS/JavaScript integration
- **Buttons**: Rendered as containers rather than native FLTK buttons
