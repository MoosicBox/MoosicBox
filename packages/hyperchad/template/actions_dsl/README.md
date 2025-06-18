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

#### Display Control
- `set_display(id, display)` - Set element display property

#### Background Control
- `set_background(id, background)` - Set element background

#### Getter Functions
- `get_visibility(id)` - Get element visibility state
- `get_display(id)` - Get element display state
- `get_width(id)` - Get element width
- `get_height(id)` - Get element height
- `get_mouse_x()` / `get_mouse_x(id)` - Get mouse X position
- `get_mouse_y()` / `get_mouse_y(id)` - Get mouse Y position
- `get_mouse_x_self()` - Get mouse X position relative to current element
- `get_width_px_self()` - Get current element width
- `get_height_px_str_id(id)` - Get element height by string ID
- `get_mouse_y_str_id(id)` - Get mouse Y position relative to element

#### Utility Functions
- `log(message)` - Log a message
- `navigate(url)` - Navigate to URL
- `custom(action)` - Custom action
- `noop()` - No-operation

#### Logic Functions
- `visible()` - Visible state constant
- `hidden()` - Hidden state constant

### Method Chaining

The DSL supports method chaining for complex logic expressions:

```rust
get_visibility("modal")
    .eq(hidden())
    .then(show("modal"))
    .or_else(hide("modal"))
```

#### Supported Methods
- `.eq(value)` - Equality comparison
- `.then(action)` - Execute action if condition is true
- `.or_else(action)` - Execute action if condition is false
- `.and(action)` - Combine actions
- `.divide(value)` - Mathematical division
- `.minus(value)` - Mathematical subtraction
- `.plus(value)` - Mathematical addition
- `.multiply(value)` - Mathematical multiplication
- `.clamp(min, max)` - Clamp value between min and max
- `.then_pass_to(action)` - Pass calculated value to action
- `.delay_off(ms)` - Delay action execution
- `.throttle(ms)` - Throttle action execution

### Enum Variants

The DSL supports enum variant syntax:

```rust
// Visibility enum
Visibility::Hidden
Visibility::Visible

// ActionType enum
ActionType::show_str_id("id")
ActionType::hide_str_id("id")
ActionType::Navigate { url: "/path" }
```

### Control Flow

#### If Statements
```rust
if get_visibility("modal") == hidden() {
    show("modal")
} else {
    hide("modal")
}
```

#### Match Expressions
```rust
match get_visibility("modal") {
    visible() => hide("modal"),
    hidden() => show("modal"),
}
```

#### Variables
```rust
let modal_id = "main-modal";
show(modal_id);
```

#### Loops
```rust
for item in items {
    show(item)
}

while condition {
    log("Processing...")
}
```

### MoosicBox Integration

The DSL is designed to support complex patterns used in MoosicBox UI:

#### Visibility Toggle Pattern
```rust
fx-click=(fx(
    if get_visibility("audio-zones") == hidden() {
        show("audio-zones")
    } else {
        hide("audio-zones")
    }
))
```

#### Mouse Interaction Pattern
```rust
fx-mouse-down=(fx(
    get_mouse_x_self()
        .divide(get_width_px_self())
        .clamp(0.0, 1.0)
        .then_pass_to(Action::SeekCurrentTrackPercent)
))
```

#### Volume Control Pattern
```rust
fx-mouse-down=(fx(
    get_height_px_str_id("volume-container")
        .minus(get_mouse_y_str_id("volume-container"))
        .divide(get_height_px_str_id("volume-container"))
        .clamp(0.0, 1.0)
        .then_pass_to(Action::SetVolume)
        .throttle(30)
))
```

### Usage in Templates

The DSL can be used in HyperChad templates with the `fx()` wrapper:

```rust
container! {
    div {
        button fx-click=(fx(show("modal"))) {
            "Show Modal"
        }

        button fx-click=(fx(
            if get_visibility("modal") == visible() {
                hide("modal")
            } else {
                show("modal")
            }
        )) {
            "Toggle Modal"
        }
    }
}
```

### Backwards Compatibility

The DSL maintains full backwards compatibility with existing action expressions:

```rust
// Old syntax still works
fx-click=(ActionType::show_str_id("modal"))

// New DSL syntax also works
fx-click=(fx(show("modal")))

// Complex existing patterns still work
fx-click=(get_visibility_str_id(ID).eq(Visibility::Hidden).then(ActionType::show_str_id(ID)))
```

## Implementation

The DSL is implemented using procedural macros that parse Rust-like syntax and generate appropriate action code at compile time. This ensures type safety and performance while providing a more expressive syntax for UI actions.
