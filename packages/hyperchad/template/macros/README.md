# HyperChad template macros crate

Procedural macros for the HyperChad template system. This crate provides the `container!` macro for writing HTML-like templates with Rust syntax.

## Features

### Core Macro

The `container!` macro parses HTML-like template syntax and generates `Vec<Container>` structures that can be rendered to HTML or other formats.

### Template Syntax

- **Elements**: Standard HTML-like element syntax

    ```rust
    div { "content" }
    button type="submit" { "Click me" }
    input type="text" name="field";
    ```

- **Attributes**: Static values and dynamic expressions

    ```rust
    // Static attribute values
    input type="text" name="username";

    // Dynamic expressions
    input type="text" value=(username);

    // Mixed static and dynamic
    input type="text" name="field" value=(dynamic_value) placeholder="Static text";
    ```

- **CSS-like Selectors**: Classes and IDs

    ```rust
    div.container #main { }
    button.primary { }
    ```

- **HTMX Attributes**: Support for htmx directives
    ```rust
    div hx-get="/route" hx-trigger="load" { }
    button hx-post="/submit" hx-swap="children" { }
    ```

### Interactive Behaviors with `fx`

The `fx` DSL enables declarative interactive behaviors through special `fx-*` attributes. Use the `fx { ... }` syntax to define actions that execute in response to user events.

#### Event Triggers

- **`fx-click`**: Fires when the element is clicked
- **`fx-hover`**: Fires when the mouse hovers over the element
- **`fx-mouse-down`**: Fires on mouse button press
- **`fx-click-outside`**: Fires when clicking outside the element
- **`fx-resize`**: Fires when the window is resized
- **Custom events**: Any `fx-*` attribute creates a corresponding event trigger

#### Basic Actions

```rust
// Show/hide elements
button fx-click=fx { show("modal") } { "Open Modal" }
button fx-click=fx { hide("modal") } { "Close Modal" }

// Navigate to routes
button fx-click=fx { navigate("/search") } { "Go to Search" }

// Log messages
button fx-click=fx { log("Button clicked") } { "Click me" }

// Custom actions
button fx-click=fx { custom("refresh-data") } { "Refresh" }
```

#### Multiple Actions

Chain multiple actions with semicolons:

```rust
button fx-click=fx {
    hide("modal");
    show("success");
    log("Operation completed");
} {
    "Submit"
}
```

#### Conditional Logic

Use `if/else` expressions for dynamic behavior:

```rust
// Toggle visibility
button fx-click=fx {
    if get_visibility("panel") == visible() {
        hide("panel")
    } else {
        show("panel")
    }
} {
    "Toggle Panel"
}
```

#### Multiple Event Handlers

Elements can have multiple `fx-*` attributes:

```rust
div
    fx-mouse-down=fx { log("Mouse down on slider") }
    fx-hover=fx { show("tooltip") }
{
    "Interactive element"
}
```

#### Advanced Usage

For complex action sequences, you can also use Rust expressions with `ActionType` or `ActionEffect`:

```rust
// Using ActionType directly
div fx-click=(ActionType::hide_by_id("test")) {
    "Click to hide"
}

// Using ActionEffect for timing control
div fx-click=(ActionEffect {
    action: ActionType::hide_by_id(Target::literal("test")),
    delay_off: Some(1000),
    throttle: Some(500),
    unique: Some(true),
}) {
    "Advanced control"
}
```

### Control Flow

- **Conditionals**: `@if`, `@else`, `@if let`

    ```rust
    @if condition {
      div { "true" }
    } @else {
      div { "false" }
    }

    @if let Some(value) = option {
      div { (value) }
    }
    ```

- **Loops**: `@for`, `@while`

    ```rust
    @for item in collection {
      div { (item) }
    }

    @while condition {
      div { "content" }
    }
    ```

- **Pattern Matching**: `@match`

    ```rust
    @match value {
      Some(x) => div { (x) },
      None => div { "empty" },
    }
    ```

- **Variable Bindings**: `@let` (within blocks)
    ```rust
    {
      @let x = compute_value();
      div { (x) }
    }
    ```

### CSS Units and Values

- **Numeric Units**: Viewport units, pixels, ems, percentages

    ```rust
    // Direct numeric syntax
    50vw  100vh  16px  2em  50%

    // Function syntax
    vw(50)  vh(100)  percent(50)
    ```

- **CSS Functions**: calc, min, max, clamp

    ```rust
    calc(100% - 20px)
    min(50vw, 500px)
    max(10em, 100px)
    clamp(1rem, 2vw, 2rem)
    ```

- **Color Functions**: rgb, rgba, hex colors
    ```rust
    rgb(255, 0, 0)
    rgba(0, 255, 0, 0.5)
    #fff  #ff0000  #00ff00ff
    ```

### Expression Interpolation

- **Parenthesized Expressions**: Splice Rust values

    ```rust
    div { (variable) }
    div { (compute_value()) }
    ```

- **Brace Expressions**: Block expressions
    ```rust
    div { {expression} }
    div { {"literal"(arg1)(arg2)} }
    ```

## Implementation Details

### Module Structure

- `src/lib.rs`: Main macro definition and preprocessing logic
- `src/ast.rs`: Abstract syntax tree types and parsing logic
- `src/generate.rs`: Code generation from AST to output tokens

### Key Dependencies

- `syn`: Parsing Rust syntax
- `quote`: Generating Rust code
- `proc-macro2`: Token manipulation
- `proc-macro2-diagnostics`: Error diagnostics
- `hyperchad_transformer`: Container and Element types
- `hyperchad_transformer_models`: Model types (Route, etc.)
- `hyperchad_color`: Color parsing utilities
- `hyperchad_template_actions_dsl`: Template action DSL support

### Error Handling

The macro provides detailed error messages for common syntax issues:

- Invalid element names
- Malformed attributes
- Unbalanced braces
- Missing control flow prefixes
- Invalid hex colors
- Type mismatches

## Usage

```rust
use hyperchad_template_macros::container;

let username = "Alice";
let items = vec!["Apple", "Banana", "Cherry"];

let html = container! {
    div.container {
        h1 { "Welcome, " (username) }

        @if !items.is_empty() {
            ul {
                @for item in items {
                    li { (item) }
                }
            }
        }

        input type="text" name="search" placeholder="Search...";

        button hx-post="/search" hx-trigger="click" {
            "Search"
        }
    }
};
```

## Testing

The crate includes comprehensive tests covering:

- Simple inputs and attributes (tests/simple_input_test.rs)
- Dynamic expressions (tests/expression_test.rs)
- String concatenation (tests/concatenation_test.rs)
- HTMX attributes (tests/htmx_test.rs)
- Pattern matching (tests/match_expr_test.rs)
- Numeric units (tests/number_inference_test.rs)
- Font properties (tests/font_family_test.rs, tests/font_weight_test.rs)
- Srcset attributes (tests/srcset_test.rs)
