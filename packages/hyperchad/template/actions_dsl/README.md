# HyperChad Actions DSL

A procedural macro for writing interactive actions in HyperChad templates.

## Overview

The Actions DSL is a procedural macro (`actions_dsl!`) that provides a Rust-like syntax for defining user interactions in your HyperChad templates. It supports various action types including visibility toggles, navigation, logging, custom actions, and event handling.

## Basic Usage

### Simple Actions

```rust
use hyperchad_template_actions_dsl::actions_dsl;

// Show an element
actions_dsl! { show("panel") }

// Hide an element
actions_dsl! { hide("modal") }

// Navigate to a URL
actions_dsl! { navigate("/dashboard") }

// Log a message
actions_dsl! { log("Button clicked") }

// Custom action
actions_dsl! { custom("my-action") }
```

### Element Reference API

The DSL supports an element reference API for cleaner element manipulation with CSS selectors:

```rust
// Element references support ID and class selectors
actions_dsl! {
    // Using element() with method chaining
    element("#my-id").show();
    element(".my-class").hide();
    element("#panel").set_visibility(Visibility::Visible);
}

// Conditional visibility checks
actions_dsl! {
    if element(".modal").get_visibility() == Visibility::Hidden {
        element(".modal").show();
    }
}
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
actions_dsl! {
    hide("modal");
    show("success-message");
    log("Modal closed successfully");
}
```

### Variables and Reusability

Use variables to make your actions more maintainable:

```rust
actions_dsl! {
    let modal_id = "user-modal";
    let overlay_id = "modal-overlay";

    hide(modal_id);
    hide(overlay_id);
    log("User modal workflow completed");
}
```

### Conditional Expressions

The DSL supports if/else conditionals with visibility checks:

```rust
actions_dsl! {
    if get_visibility("panel") == hidden() {
        show("panel");
    } else {
        hide("panel");
    }
}

// With element references
actions_dsl! {
    if element(".panel").get_visibility() == Visibility::Hidden {
        element(".panel").show();
    } else {
        element(".panel").hide();
    }
}
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
- `remove_background_str_id(id)` - Remove background from element by ID
- `remove_background_class(class)` - Remove background from elements by class
- `set_visibility_child_class(visibility, class)` - Set visibility for child elements by class

### Mathematical Operations
- `clamp(min, value, max)` - Clamp a value between min and max
- Arithmetic operators: `+`, `-`, `*`, `/` (converted to method calls)

### Getters and Queries
- `get_width_px_self()` - Get width of current element in pixels
- `get_height_px_str_id(id)` - Get height of element by ID
- `get_mouse_x_self()` - Get mouse X position relative to current element
- `get_mouse_y_str_id(id)` - Get mouse Y position relative to element
- `get_data_attr_value_self(attr)` - Get data attribute value from current element

## Advanced Features

### Event Handling with Closures

Handle custom events with closures that transform parameter references to `get_event_value()` calls:

```rust
actions_dsl! {
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
}
```

### Complex Parameterized Actions

Combine multiple functions for sophisticated interactions:

```rust
actions_dsl! {
    // Seek to track position based on mouse click
    invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self());

    // Set volume with clamping and throttling
    throttle(30, invoke(Action::SetVolume, clamp(0.0, get_width_px_self(), 1.0)));
}
```

### Arithmetic Expressions

The DSL supports arithmetic operations that are converted to method calls:

```rust
actions_dsl! {
    invoke(
        Action::SetVolume,
        ((get_height_px_str_id("container") - get_mouse_y_str_id("container")) / get_height_px_str_id("container"))
            .clamp(0.0, 1.0)
    );
}
```

## Best Practices

1. **Use element references for selectors**: Prefer `element(".class")` or `element("#id")` for cleaner syntax
2. **Use descriptive element IDs**: Make your actions self-documenting
3. **Group related actions**: Keep logically related actions together
4. **Use variables for reusability**: Define element references as variables when used multiple times
5. **Add logging for debugging**: Include log statements for complex workflows
6. **Throttle frequent actions**: Use `throttle()` for mouse move or scroll handlers

## Integration with HyperChad

The Actions DSL is typically used as a procedural macro within HyperChad templates:

```rust
use hyperchad_template_actions_dsl::actions_dsl;

// Generate actions at compile time
let my_actions = actions_dsl! {
    if get_event_value() == Key::Escape {
        hide("search");
        show("search-button");
    }
};
```

**Note**: This is a procedural macro that generates `ActionType` code at compile time, providing zero-runtime overhead for action definitions.