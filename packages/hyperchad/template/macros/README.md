# HyperChad Template Macros

Procedural macros for the HyperChad template system.

## Overview

This crate provides the `container!` macro for writing HTML-like templates with Rust syntax. The macro generates `Vec<Container>` structures that can be rendered to HTML or other formats through the HyperChad rendering system.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_template_macros = { version = "0.1.4" }
```

## Usage

### Basic Syntax

```rust
use hyperchad_template_macros::container;

let html = container! {
    div.container {
        h1 { "Hello, World!" }
        button hx-post="/submit" { "Click me" }
    }
};
```

### Dynamic Expressions

Use parentheses to embed Rust expressions:

```rust
use hyperchad_template_macros::container;

let username = "Alice";
let items = vec!["Apple", "Banana", "Cherry"];

let html = container! {
    div.container {
        h1 { "Welcome, " (username) }
        ul {
            @for item in items {
                li { (item) }
            }
        }
    }
};
```

### Control Flow

The macro supports control flow constructs prefixed with `@`:

- `@if` / `@else` - Conditional rendering
- `@for` - Iteration over collections
- `@while` - While loops
- `@match` - Pattern matching
- `@let` - Local variable bindings

```rust
use hyperchad_template_macros::container;

let show_message = true;
let items = vec!["one", "two"];

let html = container! {
    @if show_message {
        p { "Message is visible" }
    } @else {
        p { "Message is hidden" }
    }

    @for item in items {
        span { (item) }
    }
};
```

### Attributes and CSS Selectors

```rust
use hyperchad_template_macros::container;

let html = container! {
    // CSS-like class and ID selectors
    div.container #main {
        // Standard HTML attributes
        input type="text" name="search" placeholder="Search...";

        // HTMX attributes
        button hx-post="/search" hx-trigger="click" {
            "Search"
        }
    }
};
```

### CSS Units

Numeric values followed by CSS units are automatically converted:

```rust
use hyperchad_template_macros::container;

let html = container! {
    div width=100% height=50vh {
        span font-size=1.5em { "Styled text" }
    }
};
```

Supported units: `%`, `vw`, `vh`, `dvw`, `dvh`, `px`, `em`, `rem`, `ch`, `ex`, `pt`, `pc`, `in`, `cm`, `mm`

## License

See the [LICENSE](../../../../LICENSE) file for details.
