# HyperChad Actions

Interactive action system for HyperChad UI components with triggers and effects.

## Overview

The HyperChad Actions package provides:

- **Action System**: Comprehensive interactive action framework
- **Trigger Types**: Various event triggers (click, hover, change, etc.)
- **Element Targeting**: Flexible element targeting system
- **Style Actions**: Dynamic styling and visibility control
- **Logic Integration**: Conditional actions and parameterization
- **Multi-Actions**: Composite action sequences

## Features

### Action Triggers

- **Click**: Standard click events
- **ClickOutside**: Click outside element detection
- **MouseDown**: Mouse button down events
- **KeyDown**: Keyboard key down events
- **Hover**: Mouse hover events
- **Change**: Form input change events
- **Resize**: Window/element resize events
- **Custom Events**: User-defined event triggers
- **Immediate**: Execute immediately without trigger
- **HttpBeforeRequest**: Before HTTP request is sent
- **HttpAfterRequest**: After HTTP request completes
- **HttpRequestSuccess**: When HTTP request succeeds
- **HttpRequestError**: When HTTP request fails
- **HttpRequestAbort**: When HTTP request is aborted
- **HttpRequestTimeout**: When HTTP request times out

### Element Targeting

- **String ID**: Target elements by string identifier
- **Numeric ID**: Target elements by numeric ID
- **Class**: Target elements by CSS class
- **Child Class**: Target child elements by class
- **Self Target**: Target the current element
- **Last Child**: Target the last child element

### Action Types

- **Style Actions**: Visibility, display, background, and focus control
- **Input Actions**: Element selection and focus management
- **Navigation**: URL navigation and routing
- **Logging**: Debug and error logging
- **Custom Actions**: User-defined action types
- **Events**: Trigger other actions via events
- **Variable Assignment**: Let bindings for storing values (with DSL)
- **Multi-Actions**: Execute multiple actions sequentially

### Style Control

- **Visibility**: Show/hide elements with visibility property
- **Display**: Show/hide elements with display property
- **Background**: Set/remove background colors
- **Focus**: Set focus state on elements
- **Flexible Targeting**: Apply styles to various element targets

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_actions = { path = "../hyperchad/actions" }

# Enable additional features (default includes handler, logic, serde, and arb)
hyperchad_actions = {
    path = "../hyperchad/actions",
    features = ["handler", "logic", "serde", "arb"]
}
```

## Usage

### Basic Actions

```rust
use hyperchad_actions::{Action, ActionTrigger, ActionType, ElementTarget};

// Simple click action to hide element
let action = Action {
    trigger: ActionTrigger::Click,
    effect: ActionType::hide_str_id("modal").into(),
};

// Show element on hover
let show_action = Action {
    trigger: ActionTrigger::Hover,
    effect: ActionType::show_str_id("tooltip").into(),
};
```

### Element Targeting

```rust
use hyperchad_actions::{ActionType, ElementTarget};
use hyperchad_transformer_models::Visibility;

// Target by string ID
let hide_modal = ActionType::hide_str_id("modal");

// Target by class
let show_menu = ActionType::set_display_class(true, "menu");

// Target self
let hide_self = ActionType::hide_self();

// Target last child
let show_last = ActionType::show_last_child();
```

### Style Actions

```rust
// Visibility control
let hide_action = ActionType::hide_str_id("element");
let show_action = ActionType::show_str_id("element");

// Display control
let display_action = ActionType::display_str_id("element");
let no_display_action = ActionType::no_display_str_id("element");

// Background control
let set_bg = ActionType::set_background_str_id("red", "element");
let remove_bg = ActionType::remove_background_self();
```

### Multi-Actions

```rust
// Combine multiple actions
let multi_action = ActionType::Multi(vec![
    ActionType::hide_str_id("modal"),
    ActionType::show_str_id("success"),
    ActionType::Log {
        message: "Action completed".to_string(),
        level: LogLevel::Info,
    },
]);

// Chain actions with `and`
let chained = ActionType::hide_str_id("modal")
    .and(ActionType::show_str_id("success"));
```

### Action Effects

```rust
// Add throttling to prevent rapid firing
let throttled_action = ActionType::hide_str_id("element")
    .throttle(500); // 500ms throttle

// Add delay before turning off
let delayed_action = ActionType::show_str_id("tooltip")
    .delay_off(2000); // Hide after 2 seconds

// Make action unique (prevent duplicates)
let unique_action = ActionType::display_str_id("notification")
    .unique();
```

### Input Actions

```rust
// Select/focus input elements
let select_input = ActionType::select_str_id("email-input");
let focus_button = ActionType::focus_str_id("submit-button");
let focus_by_class = ActionType::focus_class("primary-input");
```

### Custom Actions

```rust
// Custom action type
let custom = ActionType::Custom {
    action: "my-custom-action".to_string(),
};

// Event-based actions
let event_action = ActionType::on_event("user-login",
    ActionType::show_str_id("dashboard")
);

// Navigation
let navigate = ActionType::Navigate {
    url: "/dashboard".to_string(),
};
```

### Conditional Logic (with `logic` feature)

```rust
use hyperchad_actions::logic::{If, Value, Condition, get_visibility_str_id, visible};

// Conditional action based on element state
let conditional = ActionType::Logic(If {
    condition: Condition::Eq(
        get_visibility_str_id("menu").into(),
        visible(),
    ),
    actions: vec![ActionType::hide_str_id("menu").into()],
    else_actions: vec![ActionType::show_str_id("menu").into()],
});

// Toggle visibility using logic
let toggle = ActionType::toggle_visibility_str_id("sidebar");
```

### Value Calculations (with `logic` feature)

```rust
use hyperchad_actions::logic::{get_mouse_x_self, get_width_px_self};

// Calculate values from element properties
let mouse_x = get_mouse_x_self(); // Mouse X relative to element
let width = get_width_px_self();  // Element width in pixels

// Arithmetic operations
let half_width = width.divide(2.0);
let clamped = mouse_x.clamp(0.0, 100.0);

// Use calculated values in actions (requires handler implementation)
```

## Action Structure

### Action

- **trigger**: When the action should be triggered (ActionTrigger)
- **effect**: The effect to execute (ActionEffect)

### ActionEffect

- **action**: The action type to execute (ActionType)
- **delay_off**: Optional delay in milliseconds before deactivation
- **throttle**: Optional throttling in milliseconds to prevent rapid execution
- **unique**: Whether to prevent duplicate executions

### ActionTrigger

- **Click/ClickOutside/MouseDown/KeyDown/Hover**: User interaction events
- **Change**: Input change events
- **Resize**: Window resize events
- **Event(String)**: Custom named events
- **Immediate**: Execute immediately without waiting for a trigger
- **HttpBeforeRequest/HttpAfterRequest/HttpRequestSuccess/HttpRequestError/HttpRequestAbort/HttpRequestTimeout**: HTTP lifecycle events

## Feature Flags

- **`handler`**: Enable action handler implementation (includes `logic` feature, requires `hyperchad_color`)
- **`logic`**: Enable conditional logic, parameterized actions, and value calculations
- **`serde`**: Enable serialization/deserialization support
- **`arb`**: Enable arbitrary data generation for property-based testing
- **`fail-on-warnings`**: Treat compiler warnings as errors (development only)

**Default features**: `handler`, `logic`, `serde`, `arb`

## Dependencies

- **hyperchad_transformer_models**: Core UI model types (Visibility, etc.)
- **hyperchad_color**: Color parsing and manipulation (optional, with `handler` feature)
- **switchy_time**: Cross-platform timing utilities
- **moosicbox_arb**: Arbitrary data generation (optional, with `arb` feature)
- **serde**: Serialization/deserialization (optional, with `serde` feature)
- **log**: Logging facade

## Advanced Features

### DSL Support

The package includes a Domain-Specific Language (DSL) module for representing Rust-like syntax in action definitions. This is primarily for internal use and advanced integration scenarios:

```rust
use hyperchad_actions::dsl::{Expression, Statement, Literal};

// DSL expressions for representing complex action logic
// Note: This is an advanced feature still under development
```

### Action Handler (with `handler` feature)

The `handler` feature provides a complete action execution system with style management, timing controls, and element queries:

```rust
use hyperchad_actions::handler::{
    ActionHandler, StyleManager, ElementFinder, ActionContext
};

// Implement traits for your UI framework to enable action handling
// See handler module documentation for integration examples
```

## Integration

This package is designed for:

- **Interactive UIs**: Dynamic user interface behaviors
- **Event Handling**: Comprehensive event response system
- **Animation Control**: Show/hide and styling animations with timing
- **Form Interactions**: Form validation, input selection, and feedback
- **Component Libraries**: Reusable interactive components
- **Game UIs**: Responsive UI with mouse tracking and element queries
