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
- **MouseDown/Hover**: Mouse interaction events
- **Change**: Form input change events
- **Resize**: Window/element resize events
- **Custom Events**: User-defined event triggers
- **Immediate**: Execute immediately without trigger

### Element Targeting
- **String ID**: Target elements by string identifier
- **Numeric ID**: Target elements by numeric ID
- **Class**: Target elements by CSS class
- **Child Class**: Target child elements by class
- **Self Target**: Target the current element
- **Last Child**: Target the last child element

### Action Types
- **Style Actions**: Visibility, display, background control
- **Navigation**: URL navigation and routing
- **Logging**: Debug and error logging
- **Custom Actions**: User-defined action types
- **Events**: Trigger other actions via events
- **Multi-Actions**: Execute multiple actions sequentially

### Style Control
- **Visibility**: Show/hide elements with visibility property
- **Display**: Show/hide elements with display property
- **Background**: Set/remove background colors and images
- **Flexible Targeting**: Apply styles to various element targets

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_actions = { path = "../hyperchad/actions" }

# Enable additional features
hyperchad_actions = {
    path = "../hyperchad/actions",
    features = ["logic", "serde", "arb"]
}
```

## Usage

### Basic Actions

```rust
use hyperchad_actions::{Action, ActionTrigger, ActionType, ElementTarget};

// Simple click action to hide element
let action = Action {
    trigger: ActionTrigger::Click,
    action: ActionType::hide_str_id("modal").into(),
};

// Show element on hover
let show_action = Action {
    trigger: ActionTrigger::Hover,
    action: ActionType::show_str_id("tooltip").into(),
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
use hyperchad_actions::logic::{If, Value, Condition};

// Conditional action based on value
let conditional = ActionType::Logic(If {
    condition: Condition::Equals {
        left: Value::Variable("user_role".to_string()),
        right: Value::String("admin".to_string()),
    },
    then_action: Box::new(ActionType::show_str_id("admin-panel")),
    else_action: Some(Box::new(ActionType::hide_str_id("admin-panel"))),
});
```

## Action Structure

### Action
- **trigger**: When the action should be triggered
- **action**: The effect to execute

### ActionEffect
- **action**: The action type to execute
- **delay_off**: Optional delay before deactivation
- **throttle**: Optional throttling to prevent rapid execution
- **unique**: Whether to prevent duplicate executions

### ActionTrigger
- **Click/MouseDown/Hover**: Mouse events
- **Change**: Input change events
- **Resize**: Window resize events
- **Event(String)**: Custom events
- **Immediate**: Execute immediately

## Feature Flags

- **`logic`**: Enable conditional logic and parameterized actions
- **`serde`**: Enable serialization/deserialization
- **`arb`**: Enable arbitrary data generation for testing

## Dependencies

- **HyperChad Transformer Models**: UI model types
- **Serde**: Optional serialization support

## Integration

This package is designed for:
- **Interactive UIs**: Dynamic user interface behaviors
- **Event Handling**: Comprehensive event response system
- **Animation Control**: Show/hide and styling animations
- **Form Interactions**: Form validation and feedback
- **Component Libraries**: Reusable interactive components
