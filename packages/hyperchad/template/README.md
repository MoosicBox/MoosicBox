# HyperChad Template

Template system and DSL for building HyperChad UI components with macros and utilities.

## Overview

The HyperChad Template package provides:

- **Container Macro**: `container!` macro for declarative UI construction
- **Template DSL**: Domain-specific language for UI component definition
- **Rendering System**: Convert templates to HTML strings and containers
- **Extension Traits**: Utility methods for container collections
- **Calculation Functions**: CSS calc() function support with units
- **Color Functions**: RGB/RGBA color creation utilities
- **No-std Support**: Core functionality works without standard library

## Features

### Template Macro
- **`container!` Macro**: Declarative UI component construction
- **Nested Elements**: Support for nested container hierarchies
- **Attribute Syntax**: CSS-like attribute specification
- **Dynamic Content**: Runtime content generation and insertion

### Rendering System
- **HTML Generation**: Convert containers to HTML strings
- **Debug Attributes**: Optional debug information in output
- **Pretty Printing**: Formatted HTML output for debugging
- **String Conversion**: Multiple string conversion utilities

### Extension Traits
- **ContainerVecMethods**: Methods for Vec<Container> collections
- **ContainerVecExt**: Additional utility methods
- **RenderContainer**: Trait for renderable types
- **Conversion Traits**: Type conversion utilities

### Calculation System
- **CSS calc()**: Support for CSS calculation expressions
- **Unit Functions**: vw, vh, dvw, dvh viewport units
- **Math Operations**: Add, subtract, multiply, divide operations
- **Responsive Units**: Percentage and viewport-relative units

### Color System
- **RGB Functions**: rgb() and rgba() color creation
- **Alpha Support**: Transparency and alpha channel handling
- **Type Conversion**: Flexible color value conversion
- **Format Support**: Multiple input formats (float, int, percentage)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_template = { path = "../hyperchad/template" }
```

## Usage

### Basic Template Creation

```rust
use hyperchad_template::{container, to_html};

// Create a simple template
let template = container! {
    div {
        h1 { "Hello World" }
        p { "Welcome to HyperChad!" }
    }
};

// Convert to HTML
let html = to_html(&template);
// Output: <div><h1>Hello World</h1><p>Welcome to HyperChad!</p></div>
```

### Template with Attributes

```rust
use hyperchad_template::container;

let template = container! {
    div
        id="main"
        class="container"
        background="#f0f0f0"
        padding=20
        margin="10px auto"
    {
        h1
            color="blue"
            font-size=24
        {
            "Styled Header"
        }

        p
            text-align="center"
            line-height=1.5
        {
            "Styled paragraph content"
        }
    }
};
```

### Dynamic Content

```rust
use hyperchad_template::{container, RenderContainer};

// Custom type that can be rendered
struct UserCard {
    name: String,
    email: String,
}

impl RenderContainer for UserCard {
    type Error = std::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        let card = container! {
            div class="user-card" {
                h3 { (&self.name) }
                p { (&self.email) }
            }
        };
        containers.extend(card);
        Ok(())
    }
}

// Use in template
let user = UserCard {
    name: "John Doe".to_string(),
    email: "john@example.com".to_string(),
};

let template = container! {
    div class="users" {
        (user) // Renders the UserCard
    }
};
```

### Calculation Functions

```rust
use hyperchad_template::{container, calc::*};

let template = container! {
    div
        width=calc!(100% - 20px)
        height=calc!(min(500px, 80vh))
        margin=calc!(10px + 5%)
    {
        "Calculated dimensions"
    }
};
```

### Unit Functions

```rust
use hyperchad_template::{container, unit_functions::*};

let template = container! {
    div
        width=vw(100)      // 100vw
        height=vh(50)      // 50vh
        font-size=dvw(4)   // 4dvw
    {
        "Viewport-relative sizing"
    }
};
```

### Color Functions

```rust
use hyperchad_template::{container, color_functions::*};

let template = container! {
    div
        background=rgb(255, 0, 0)           // Red
        color=rgba(0, 0, 255, 0.8)         // Blue with transparency
        border-color=rgb_alpha(0, 255, 0, 128) // Green with alpha
    {
        "Colorful content"
    }
};
```

### Container Collection Methods

```rust
use hyperchad_template::{container, ContainerVecMethods};

let template = container! {
    div { "Content" }
};

// Convert to string
let html = template.to_string();

// Pretty print for debugging
let pretty_html = template.display_to_string_pretty(true, true)?;

// Into string (consuming)
let html = template.into_string();
```

### Border Utilities

```rust
use hyperchad_template::{container, IntoBorder};

let template = container! {
    div
        border-top=("red", 2)          // Red 2px border
        border-left=(Color::BLUE, 1)   // Blue 1px border
        border-radius=5
    {
        "Bordered content"
    }
};
```

### Action Integration

```rust
use hyperchad_template::{container, IntoActionEffect};
use hyperchad_actions::ActionType;

let template = container! {
    button
        onclick=ActionType::hide_str_id("modal").throttle(500)
    {
        "Close Modal"
    }
};
```

## Container List Wrapper

```rust
use hyperchad_template::{ContainerList, container};

// Wrap containers in a list
let containers = container! {
    div { "Item 1" }
    div { "Item 2" }
};

let list = ContainerList::new(containers);

// Iterate over containers
for container in &list {
    println!("{}", container);
}

// Convert back to Vec
let vec: Vec<Container> = list.into();
```

## Type Conversions

### Boolean Conversion
```rust
use hyperchad_template::ToBool;

let visible = some_condition.to_bool();
```

### Border Conversion
```rust
use hyperchad_template::IntoBorder;

let border = ("red", 2).into_border(); // (Color, Number)
```

### Action Effect Conversion
```rust
use hyperchad_template::IntoActionEffect;

let effect = ActionType::show_str_id("element").into_action_effect();
```

## No-std Support

The package is `no_std` compatible:

```rust
#![no_std]
extern crate alloc;

use hyperchad_template::{container, ContainerVecMethods};
use alloc::string::String;
```

## Feature Flags

The package supports various feature flags through its dependencies:
- Layout calculation features
- HTML generation features
- Formatting and syntax highlighting

## Dependencies

- **HyperChad Template Macros**: `container!` macro implementation
- **HyperChad Transformer**: Core container and element types
- **HyperChad Actions**: Interactive action system
- **HyperChad Color**: Color handling and conversion
- **Alloc**: For no-std collections support

## Integration

This package is designed for:
- **UI Component Libraries**: Building reusable UI components
- **Template Systems**: Server-side template rendering
- **Static Site Generation**: Pre-rendered HTML generation
- **Desktop Applications**: Native UI component construction
- **Web Applications**: Dynamic HTML generation

## Performance

- **No-std Compatible**: Minimal runtime overhead
- **Zero-cost Abstractions**: Compile-time template processing
- **Efficient String Generation**: Optimized HTML string creation
- **Memory Efficient**: Minimal allocations for container structures
