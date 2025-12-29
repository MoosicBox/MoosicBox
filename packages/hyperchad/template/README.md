# HyperChad Template

Template system and DSL for building HyperChad UI components with macros and utilities.

## Overview

The HyperChad Template package provides:

- **Container Macro**: `container!` macro for declarative UI construction
- **Template DSL**: Domain-specific language for UI component definition with CSS-like syntax
- **Control Flow**: `@if`, `@else`, `@for`, `@while`, `@match`, `@let` for dynamic templates
- **Rendering System**: Convert containers to HTML strings
- **Extension Traits**: Utility methods for container collections (`ContainerVecMethods`, `ContainerVecExt`)
- **Calculation Functions**: CSS `calc()`, `min()`, `max()`, `clamp()` with viewport units
- **Color Functions**: `rgb()` and `rgba()` with multiple alpha formats, hex colors, named colors
- **Action System**: `fx` DSL for interactive behaviors
- **No-std Support**: Core functionality works without standard library (uses `alloc`)

## Features

### Template Macro

- **`container!` Macro**: Declarative UI component construction
- **Nested Elements**: Support for nested container hierarchies
- **Attribute Syntax**: CSS-like attribute specification
- **Dynamic Content**: Runtime content generation and insertion

### Control Flow

- **Conditionals**: `@if`, `@else if`, `@else` for conditional rendering
- **If-Let**: `@if let` for pattern matching with Option/Result types
- **Loops**: `@for` for iterating over collections
- **While**: `@while` for condition-based loops
- **Match**: `@match` for pattern matching expressions
- **Let**: `@let` for local variable bindings within templates

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
- **CSS Math Functions**: `min()`, `max()`, `clamp()` for responsive sizing
- **Unit Functions**: `vw()`, `vh()`, `dvw()`, `dvh()` viewport units, `percent()` for percentages
- **Math Operations**: Add, subtract, multiply, divide operations
- **Responsive Units**: Percentage and viewport-relative units
- **Raw Percent Values**: Direct percentage notation (e.g., `100%`, `50%`)

### Color System

- **RGB Functions**: `rgb()` color creation (3 or 4 arguments)
- **RGBA Functions**: `rgba()` as alias for 4-argument `rgb()`
- **Alpha Support**: Float (0.0-1.0), integer (0-255), or percentage ("50%")
- **Hex Colors**: Support for 3, 4, 6, and 8-digit hex colors (#fff, #ffff, #ffffff, #ffffffff)
- **Named Colors**: Built-in color constants (`Color::BLACK`, `Color::WHITE`)

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
use hyperchad_template::{container, Container, RenderContainer};

// Custom type that can be rendered
struct UserCard {
    name: String,
    email: String,
}

impl RenderContainer for UserCard {
    type Error = core::fmt::Error;

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

### Control Flow

```rust
use hyperchad_template::container;

let show_header = true;
let items = vec!["Apple", "Banana", "Cherry"];

let template = container! {
    div {
        // Conditional rendering
        @if show_header {
            h1 { "Shopping List" }
        }

        // Iterating over collections
        ul {
            @for item in &items {
                li { (item) }
            }
        }

        // Pattern matching
        @match items.len() {
            0 => p { "No items" },
            1 => p { "One item" },
            n => p { (n) " items" },
        }
    }
};
```

### Calculation Functions

```rust
use hyperchad_template::container;

let template = container! {
    div
        width=calc(100% - 20)
        height=min(500, 80vh)
        margin=calc(10 + 5%)
    {
        "Calculated dimensions"
    }
};
```

### Unit Functions

```rust
use hyperchad_template::container;

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
use hyperchad_template::container;

let template = container! {
    div
        background=rgb(255, 0, 0)        // Red (3-arg RGB)
        color=rgb(0, 0, 255, 0.8)       // Blue with transparency (4-arg RGB)
        border-color=rgba(0, 255, 0, 128) // Green with alpha (rgba alias)
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
use hyperchad_color::Color;

let template = container! {
    div
        border-top=("#ff0000", 2)      // Red 2px border
        border-left=(Color::BLACK, 1)  // Black 1px border
        border-radius=5
    {
        "Bordered content"
    }
};
```

### Action Integration

```rust
use hyperchad_template::container;

let template = container! {
    button
        fx-click=fx { hide("modal") }
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

let border = ("#ff0000", 2).into_border(); // (Color, Number)
```

### Action Effect Conversion

```rust
use hyperchad_template::IntoActionEffect;
use hyperchad_template::actions::ActionType;

let effect = ActionType::show_by_id("element").into_action_effect();
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

The package supports the following feature flags:

- **`default`**: Enables the `logic` feature
- **`logic`**: Enables logic features in actions and transformer (conditional rendering, responsive values)
- **`fail-on-warnings`**: Enables strict compilation mode across all dependencies

## Dependencies

- **hyperchad_template_macros**: `container!` macro implementation
- **hyperchad_transformer**: Core container and element types
- **hyperchad_actions**: Interactive action system
- **hyperchad_color**: Color handling and conversion
- **hyperchad_template_actions_dsl**: Actions DSL for `fx` syntax
- **hyperchad_transformer_models**: Model types re-exported from transformer
- **itoa**: Fast integer to string conversion
- **ryu**: Fast float to string conversion

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
