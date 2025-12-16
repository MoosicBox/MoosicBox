# HyperChad Actions DSL

A domain-specific language for writing interactive actions in HyperChad templates.

## Overview

The Actions DSL provides a Rust-like syntax for defining user interactions in your HyperChad templates through the convenient `fx { ... }` syntax. It supports various action types including visibility toggles, navigation, logging, custom actions, and event handling.

### How It Works

The `fx { ... }` syntax is syntactic sugar that uses the `actions_dsl!` procedural macro behind the scenes:

- **In templates**: Use the readable `fx { ... }` syntax (e.g., `fx-click=fx { show("panel") }`)
- **Behind the scenes**: The template macro system intercepts `fx` calls and processes them through the `actions_dsl!` procedural macro at compile time
- **Result**: Zero-runtime overhead action definitions with clean, intuitive syntax

The `fx` function itself is just a marker - all the real work happens at compile time through macro expansion.

## Basic Usage

### Simple Actions

```rust
// Show an element
button fx-click=fx { show("panel") } { "Show Panel" }

// Hide an element
button fx-click=fx { hide("modal") } { "Close Modal" }

// Navigate to a URL
button fx-click=fx { navigate("/dashboard") } { "Go to Dashboard" }

// Log a message
button fx-click=fx { log("Button clicked") } { "Click Me" }

// Custom action
button fx-click=fx { custom("my-action") } { "Custom Action" }
```

### Element Reference API

The DSL supports an element reference API for cleaner element manipulation with CSS selectors:

```rust
// Element references support ID and class selectors
button fx-click=fx {
    // Using element() with method chaining
    element("#my-id").show();
    element(".my-class").hide();
    element("#panel").set_visibility(Visibility::Visible);
} { "Toggle Elements" }

// Conditional visibility checks
button fx-click=fx {
    if element(".modal").get_visibility() == Visibility::Hidden {
        element(".modal").show();
    }
} { "Show Modal If Hidden" }
```

**Available element methods:**

- `element(selector).show()` - Show the element
- `element(selector).hide()` - Hide the element
- `element(selector).toggle_visibility()` - Toggle element visibility
- `element(selector).get_visibility()` - Get current visibility state
- `element(selector).set_visibility(visibility)` - Set visibility state

### Multiple Actions

Chain multiple actions together:

```rust
button fx-click=fx {
    hide("modal");
    show("success-message");
    log("Modal closed successfully");
} { "Close and Confirm" }
```

### Variables and Reusability

Use variables to make your actions more maintainable:

```rust
button fx-click=fx {
    let modal_id = "user-modal";
    let overlay_id = "modal-overlay";

    hide(modal_id);
    hide(overlay_id);
    log("User modal workflow completed");
} { "Close User Modal" }
```

### Conditional Expressions

The DSL supports if/else conditionals with visibility checks:

```rust
button fx-click=fx {
    if get_visibility("panel") == hidden() {
        show("panel");
    } else {
        hide("panel");
    }
} { "Toggle Panel" }

// With element references
button fx-click=fx {
    if element(".panel").get_visibility() == Visibility::Hidden {
        element(".panel").show();
    } else {
        element(".panel").hide();
    }
} { "Toggle Panel" }
```

## Action Types

### Visibility Actions

- `show(id)` - Show an element by ID
- `hide(id)` - Hide an element by ID
- `set_visibility(id, visibility)` - Set element visibility state
- `show_self()` - Show the current element
- `hide_self()` - Hide the current element

### Navigation Actions

- `navigate(url)` - Navigate to a URL

### Logging Actions

- `log(message)` - Log an info message
- `custom(action_name)` - Execute a custom action

### Element Reference Functions

- `element(selector)` - Get an element reference (supports `#id` and `.class` selectors)
- `element(selector).show()` - Show the element
- `element(selector).hide()` - Hide the element
- `element(selector).toggle_visibility()` - Toggle element visibility
- `element(selector).get_visibility()` - Get current visibility state
- `element(selector).set_visibility(visibility)` - Set visibility state

### Conditional Functions

- `get_visibility(id)` - Get element visibility state
- `visible()` - Visibility state constant
- `hidden()` - Hidden state constant

### Parameterized Actions

- `invoke(action, value)` - Execute a parameterized action
- `throttle(duration, action)` - Throttle action execution
- `delay_off(duration, action)` - Delay action deactivation
- `unique(action)` - Ensure action uniqueness

### Event Handling

- `on_event(event_name, closure)` - Handle custom events
- `get_event_value()` - Get the current event value (use within event handlers)

### Background Styling

- `set_background_self(color)` - Set background color of current element
- `remove_background_self()` - Remove background from current element
- `remove_background_by_id(id)` - Remove background from element by ID
- `remove_background_class(class)` - Remove background from elements by class
- `set_visibility_child_class(visibility, class)` - Set visibility for child elements by class

### Mathematical Operations

- `clamp(min, value, max)` - Clamp a value between min and max
- Arithmetic operators: `+`, `-`, `*`, `/` (converted to method calls)

### Getters and Queries

- `get_width_px_self()` - Get width of current element in pixels
- `get_height_px_by_id(id)` - Get height of element by ID
- `get_mouse_x_self()` - Get mouse X position relative to current element
- `get_mouse_y_by_id(id)` - Get mouse Y position relative to element
- `get_data_attr_value_self(attr)` - Get data attribute value from current element

## Advanced Features

### Event Handling with Closures

Handle custom events with closures that transform parameter references to `get_event_value()` calls:

```rust
div fx-mounted=fx {
    on_event("play-track", |value| {
        if value == get_data_attr_value_self("track-id") {
            set_background_self("#333");
            set_visibility_child_class(Visibility::Hidden, "track-number");
            set_visibility_child_class(Visibility::Visible, "track-playing");
        } else {
            remove_background_self();
            set_visibility_child_class(Visibility::Visible, "track-number");
        }
    });
} { /* track content */ }
```

### Complex Parameterized Actions

Combine multiple functions for sophisticated interactions:

```rust
// Seek bar - click to jump to position
div fx-click=fx {
    invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());
} { /* seek bar content */ }

// Volume slider - drag to adjust
div fx-mousemove=fx {
    throttle(30, invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0)));
} { /* volume slider */ }
```

### Arithmetic Expressions

The DSL supports arithmetic operations that are converted to method calls:

```rust
div fx-click=fx {
    invoke(
        Action::SetVolume,
        ((get_height_px_by_id("container") - get_mouse_y_by_id("container")) / get_height_px_by_id("container"))
            .clamp(0.0, 1.0)
    );
} id="container" { /* vertical volume control */ }
```

## Best Practices

1. **Use element references for selectors**: Prefer `element(".class")` or `element("#id")` for cleaner syntax
2. **Use descriptive element IDs**: Make your actions self-documenting
3. **Group related actions**: Keep logically related actions together
4. **Use variables for reusability**: Define element references as variables when used multiple times
5. **Add logging for debugging**: Include log statements for complex workflows
6. **Throttle frequent actions**: Use `throttle()` for mouse move or scroll handlers

## Integration with HyperChad

The Actions DSL is typically used within HyperChad templates using the `fx { ... }` syntax:

```rust
use hyperchad_template::container;

// Use in template event handlers
let my_template = container! {
    div fx-key-down=fx {
        if get_event_value() == Key::Escape {
            hide("search");
            show("search-button");
        }
    } {
        "Press Escape to toggle search"
    }
};
```

**Note**: The `fx` syntax is processed by the template macro system, which uses the `actions_dsl!` procedural macro behind the scenes. This generates `ActionType` code at compile time, providing zero-runtime overhead for action definitions.

### Using `actions_dsl!` Directly

While the `fx { ... }` syntax is more readable and recommended for templates, you can also use the `actions_dsl!` macro directly:

```rust
use hyperchad_template_actions_dsl::actions_dsl;

let my_actions = actions_dsl! {
    show("panel");
    hide("modal");
};
```
