# Basic Actions Example

This example demonstrates the core functionality of the `hyperchad_actions` crate, showing how to create and use actions with triggers, effects, and element targeting.

## What This Example Demonstrates

- Creating actions with different triggers (Click, Hover, Change, HTTP events)
- Element targeting strategies (by ID, class, self, last child)
- Style actions (visibility, display, background, focus)
- Multi-actions and action chaining with `and`
- Action timing modifiers (throttle, delay_off, unique)
- Custom actions and navigation
- Input actions for form elements
- Conditional logic and value calculations (with `logic` feature)
- HTTP lifecycle event handling

## Prerequisites

- Basic understanding of Rust
- Familiarity with UI event handling concepts
- Basic knowledge of HTML/CSS concepts (visibility, display, etc.)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/hyperchad/actions/examples/basic_actions/Cargo.toml
```

Or from the example directory:

```bash
cd packages/hyperchad/actions/examples/basic_actions
cargo run
```

## Expected Output

The example will print structured output demonstrating various action types and their configurations:

```
=== HyperChad Actions - Basic Usage Examples ===

1. Simple Click Action:
   Action: Action { trigger: Click, effect: ActionEffect { ... } }
   When clicked, hides element with ID 'modal'

2. Element Targeting Options:
   By String ID: Style { target: StrId(...), action: SetVisibility(Hidden) }
   By Class: Style { target: Class(...), action: SetVisibility(Visible) }
   ...

[Additional examples demonstrating all action types]
```

## Code Walkthrough

### 1. Simple Click Action

```rust
let click_action = Action {
    trigger: ActionTrigger::Click,
    effect: ActionType::hide_str_id("modal").into(),
};
```

This creates an action that hides an element with ID "modal" when clicked. The `into()` converts the `ActionType` into an `ActionEffect`.

### 2. Element Targeting

The crate provides flexible element targeting:

```rust
// By string ID
let hide_by_id = ActionType::hide_str_id("my-element");

// By class name
let show_by_class = ActionType::show_class("menu");

// Target self (the element with the action)
let hide_self = ActionType::hide_self();

// Target last child element
let show_last_child = ActionType::show_last_child();
```

### 3. Style Actions

Style actions modify element appearance:

```rust
// Visibility control
let set_visibility = ActionType::Style {
    target: ElementTarget::StrId("element".into()),
    action: StyleAction::SetVisibility(Visibility::Hidden),
};

// Display control
let set_display = ActionType::set_display_str_id(false, "element");

// Background color
let set_background = ActionType::set_background_str_id("#ff0000", "element");

// Focus management
let set_focus = ActionType::focus_str_id("input-field");
```

### 4. Multi-Actions

Execute multiple actions sequentially:

```rust
let multi_action = ActionType::Multi(vec![
    ActionType::hide_str_id("loading"),
    ActionType::show_str_id("content"),
    ActionType::Log {
        message: "Content loaded successfully".to_string(),
        level: LogLevel::Info,
    },
]);
```

### 5. Chaining with `and`

Chain actions fluently:

```rust
let chained = ActionType::hide_str_id("modal")
    .and(ActionType::show_str_id("success-message"))
    .and(ActionType::Log {
        message: "Modal closed".to_string(),
        level: LogLevel::Info,
    });
```

### 6. Action Timing

Add timing modifiers to control execution:

```rust
// Throttle to prevent rapid firing (500ms minimum between executions)
let throttled = ActionType::hide_str_id("tooltip").throttle(500);

// Auto-hide after 2 seconds
let delayed = ActionType::show_str_id("notification").delay_off(2000);

// Ensure only one instance runs (prevent duplicates)
let unique = ActionType::display_str_id("alert").unique();
```

### 7. Different Triggers

Actions can respond to various events:

```rust
// Click event
let click_trigger = Action {
    trigger: ActionTrigger::Click,
    effect: ActionType::hide_self().into(),
};

// Hover event
let hover_trigger = Action {
    trigger: ActionTrigger::Hover,
    effect: ActionType::show_str_id("tooltip").into(),
};

// Form input change
let change_trigger = Action {
    trigger: ActionTrigger::Change,
    effect: ActionType::Log {
        message: "Input changed".to_string(),
        level: LogLevel::Debug,
    }.into(),
};

// Execute immediately (no user interaction required)
let immediate_trigger = Action {
    trigger: ActionTrigger::Immediate,
    effect: ActionType::show_str_id("welcome-message").into(),
};
```

### 8. HTTP Event Triggers

Handle HTTP request lifecycle events:

```rust
// Before HTTP request starts
let before_request = Action {
    trigger: ActionTrigger::HttpBeforeRequest,
    effect: ActionType::display_str_id("loading-spinner").into(),
};

// After request completes (success or error)
let after_request = Action {
    trigger: ActionTrigger::HttpAfterRequest,
    effect: ActionType::no_display_str_id("loading-spinner").into(),
};

// On successful response
let on_success = Action {
    trigger: ActionTrigger::HttpRequestSuccess,
    effect: ActionType::show_str_id("success-banner").into(),
};

// On error response
let on_error = Action {
    trigger: ActionTrigger::HttpRequestError,
    effect: ActionType::show_str_id("error-banner").into(),
};
```

### 9. Conditional Logic (with `logic` feature)

Create dynamic actions based on runtime state:

```rust
use hyperchad_actions::logic::{get_visibility_str_id, Condition, If};

// Toggle visibility based on current state
let conditional = ActionType::Logic(If {
    condition: Condition::Eq(
        get_visibility_str_id("menu").into(),
        Visibility::Visible.into(),
    ),
    actions: vec![ActionType::hide_str_id("menu").into()],
    else_actions: vec![ActionType::show_str_id("menu").into()],
});

// Or use the convenience helper
let toggle = ActionType::toggle_visibility_str_id("sidebar");
```

### 10. Value Calculations (with `logic` feature)

Calculate values from element state:

```rust
use hyperchad_actions::logic::{get_mouse_x_self, get_width_px_self};

// Get mouse position relative to element
let mouse_x = get_mouse_x_self();

// Get element width
let width = get_width_px_self();

// Perform calculations
let half_width = width.divide(2.0);
let clamped = mouse_x.clamp(0.0, 100.0);
```

## Key Concepts

### Actions

An `Action` combines a trigger event with an effect to execute. This is the fundamental building block of the action system.

### Triggers

`ActionTrigger` defines when an action should execute:

- **User Events**: Click, Hover, MouseDown, KeyDown, Change
- **System Events**: Resize, Immediate
- **HTTP Events**: HttpBeforeRequest, HttpAfterRequest, HttpRequestSuccess, HttpRequestError, HttpRequestAbort, HttpRequestTimeout
- **Custom Events**: Event(String) for user-defined events

### Effects

`ActionEffect` wraps an action with timing and execution modifiers:

- **throttle**: Minimum time between executions (milliseconds)
- **delay_off**: Auto-deactivate after specified time (milliseconds)
- **unique**: Prevent duplicate simultaneous executions

### Element Targeting

`ElementTarget` provides flexible ways to identify elements:

- **StrId**: Target by string identifier
- **Id**: Target by numeric ID
- **Class**: Target by CSS class name
- **ChildClass**: Target child by class name
- **SelfTarget**: Target the current element
- **LastChild**: Target the last child element

### Style Actions

`StyleAction` modifies element appearance:

- **SetVisibility**: Show/hide with visibility property
- **SetDisplay**: Show/hide with display property
- **SetBackground**: Set/remove background color
- **SetFocus**: Control focus state

## Testing the Example

This is a CLI example that demonstrates the action system API. To see actions in action with a real UI, check out:

- `packages/hyperchad/examples/http_events` - HTTP event handling in a web UI
- `packages/hyperchad/examples/details_summary` - Interactive collapsible content

## Troubleshooting

### Example doesn't compile

Ensure you're running from the correct directory and have all dependencies:

```bash
cargo clean
cargo build --manifest-path packages/hyperchad/actions/examples/basic_actions/Cargo.toml
```

### Missing features

Some functionality requires feature flags. The example enables `logic` and `serde` by default. To disable:

```bash
cargo run --manifest-path packages/hyperchad/actions/examples/basic_actions/Cargo.toml --no-default-features
```

## Related Examples

- **packages/hyperchad/examples/http_events** - Demonstrates HTTP event triggers in a real web application
- **packages/hyperchad/examples/details_summary** - Shows actions with interactive UI components
- **packages/hyperchad/examples/markdown** - Uses actions for navigation and content loading

## Further Reading

- See the `hyperchad_actions` package README for comprehensive API documentation
- Check the `logic` module documentation for advanced conditional logic
- Review the `handler` module for implementing action handlers in your UI framework
