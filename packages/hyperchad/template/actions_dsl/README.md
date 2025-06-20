# hyperchad_template_actions_dsl

HyperChad template actions DSL macros package

## Features

This package provides a domain-specific language (DSL) for defining UI actions in HyperChad templates. The DSL supports both simple function calls and complex patterns used in real-world applications like MoosicBox.

### Basic DSL Functions

#### Element Visibility
- `show(id)` - Show an element by ID
- `hide(id)` - Hide an element by ID
- `toggle(id)` - Toggle element visibility
- `set_visibility(id, visibility)` - Set specific visibility state

#### Self-targeting Functions
- `show_self()` - Show the current element
- `hide_self()` - Hide the current element
- `show_last_child()` - Show the last child element

#### Display Control
- `set_display(id, display)` - Set element display property

#### Background Control
- `set_background(id, background)` - Set element background

#### Navigation
- `navigate(url)` - Navigate to a URL

#### Logging and Custom Actions
- `log(message)` - Log a message
- `custom(action_name)` - Execute a custom action by name

#### Visibility State Functions
- `get_visibility(id)` - Get visibility state of an element
- `get_visibility_self()` - Get visibility state of current element
- `visible()` - Returns visible state for comparison
- `hidden()` - Returns hidden state for comparison

### Multiple Actions

The DSL supports multiple actions in a single expression using block syntax:

**New Curly Brace Syntax (Recommended):**
```rust
fx-click=fx {
    hide("modal");
    show("success-message");
    log("Action completed");
}
```

**Legacy Parentheses Syntax (Still Supported):**
```rust
fx-click=(fx({
    hide("modal");
    show("success-message");
    log("Action completed");
}))
```

When multiple actions are used, they are automatically wrapped in `ActionType::Multi` to ensure all actions are executed.

### Method Chaining

The DSL supports method chaining for complex logic:

```rust
fx-click=(fx(
    get_visibility("modal")
        .eq(hidden())
        .then(show("modal"))
        .or_else(hide("modal"))
))
```

### Conditional Expressions

Use if/else expressions for clear conditional logic:

**New Curly Brace Syntax (Recommended):**
```rust
fx-click=fx {
    if get_visibility("panel") == hidden() {
        show("panel")
    } else {
        hide("panel")
    }
}
```

**Legacy Parentheses Syntax (Still Supported):**
```rust
fx-click=(fx(
    if get_visibility("panel") == hidden() {
        show("panel")
    } else {
        hide("panel")
    }
))
```

### Real-world Examples

#### Modal Toggle with Feedback
**New Curly Brace Syntax:**
```rust
button fx-click=fx {
    if get_visibility("modal") == hidden() {
        show("modal");
        log("Modal opened");
    } else {
        hide("modal");
        log("Modal closed");
    }
} {
    "Toggle Modal"
}
```

**Legacy Parentheses Syntax:**
```rust
button fx-click=(fx({
    if get_visibility("modal") == hidden() {
        show("modal");
        log("Modal opened");
    } else {
        hide("modal");
        log("Modal closed");
    }
})) {
    "Toggle Modal"
}
```

#### Multi-step Navigation
**New Curly Brace Syntax:**
```rust
button fx-click=fx {
    hide("current-page");
    show("loading-spinner");
    navigate("/next-page");
} {
    "Next Page"
}
```

**Legacy Parentheses Syntax:**
```rust
button fx-click=(fx({
    hide("current-page");
    show("loading-spinner");
    navigate("/next-page");
})) {
    "Next Page"
}
```

#### Single Action (Clean Syntax)
**New Curly Brace Syntax:**
```rust
button fx-click=fx { hide("search") } {
    "Close Search"
}
```

**Legacy Parentheses Syntax:**
```rust
button fx-click=(fx(hide("search"))) {
    "Close Search"
}
```

## Backwards Compatibility

The DSL is fully backwards compatible with existing HyperChad action syntax:

```rust
// DSL syntax
fx-click=(fx(toggle("menu")))

// Traditional syntax (still works)
fx-click=(ActionType::toggle_str_id("menu"))

// Complex expressions (still work)
fx-click=(
    get_visibility_str_id("menu")
        .eq(Visibility::Hidden)
        .then(ActionType::show_str_id("menu"))
        .or_else(ActionType::hide_str_id("menu"))
)
```

## Implementation

The DSL is implemented using procedural macros that parse Rust-like syntax and generate appropriate action code at compile time. This ensures type safety and performance while providing a more expressive syntax for UI actions.
