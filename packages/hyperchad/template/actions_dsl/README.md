# HyperChad Actions DSL

A domain-specific language (DSL) for writing interactive actions in HyperChad templates.

## Overview

The Actions DSL provides a simple, intuitive syntax for defining user interactions in your HyperChad templates. It supports various action types including visibility toggles, navigation, logging, and custom actions.

## Basic Usage

### Simple Actions

```rust
// Show an element
fx-click=fx { show("panel") }

// Hide an element
fx-click=fx { hide("modal") }

// Toggle visibility
fx-click=fx { toggle("sidebar") }

// Navigate to a URL
fx-click=fx { navigate("/dashboard") }

// Log a message
fx-click=fx { log("Button clicked") }

// Custom action
fx-click=fx { custom("my-action") }
```

### Multiple Actions

Chain multiple actions together:

```rust
fx-click=fx {
    hide("modal");
    show("success-message");
    log("Modal closed successfully");
}
```

### Variables and Reusability

Use variables to make your actions more maintainable:

```rust
fx-click=fx {
    let modal_id = "user-modal";
    let overlay_id = "modal-overlay";
    
    hide(modal_id);
    hide(overlay_id);
    log("User modal workflow completed");
}
```

### Conditional Expressions

Use if/else expressions for clear conditional logic:

```rust
fx-click=fx {
    if get_visibility("panel") == hidden() {
        show("panel")
    } else {
        hide("panel")
    }
}
```

### Real-world Examples

#### Modal Toggle with Feedback
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

#### Multi-step Navigation
```rust
button fx-click=fx {
    hide("current-page");
    show("loading-spinner");
    navigate("/next-page");
} {
    "Next Page"
}
```

#### Single Action (Clean Syntax)
```rust
button fx-click=fx { hide("search") } {
    "Close Search"
}
```

## Action Types

### Visibility Actions
- `show(id)` - Show an element
- `hide(id)` - Hide an element  
- `toggle(id)` - Toggle element visibility

### Navigation Actions
- `navigate(url)` - Navigate to a URL

### Logging Actions
- `log(message)` - Log an info message
- `log_debug(message)` - Log a debug message
- `log_warn(message)` - Log a warning message
- `log_error(message)` - Log an error message

### Custom Actions
- `custom(action_name)` - Execute a custom action

### Conditional Functions
- `get_visibility(id)` - Get element visibility state
- `visible()` - Visibility state constant
- `hidden()` - Hidden state constant

## Comparison Operators

The DSL supports standard comparison operators:
- `==` - Equal
- `!=` - Not equal
- `<` - Less than
- `>` - Greater than
- `<=` - Less than or equal
- `>=` - Greater than or equal

## Control Flow

### If/Else Expressions
```rust
fx-click=fx {
    if condition {
        action1()
    } else {
        action2()
    }
}
```

### Complex Conditions
```rust
fx-click=fx {
    if get_visibility("panel") == visible() && user_logged_in() {
        show("admin-panel")
    } else {
        navigate("/login")
    }
}
```

## Best Practices

1. **Use descriptive element IDs**: Make your actions self-documenting
2. **Group related actions**: Keep logically related actions together
3. **Use variables for reusability**: Define IDs as variables when used multiple times
4. **Add logging for debugging**: Include log statements for complex workflows
5. **Keep actions focused**: Each action should have a single responsibility

## Integration with HyperChad

The Actions DSL integrates seamlessly with HyperChad's template system:

```rust
use hyperchad_template::container;

let ui = container! {
    div {
        button fx-click=fx {
            toggle("search-panel");
            log("Search panel toggled");
        } {
            "Toggle Search"
        }
        
        div id="search-panel" {
            "Search content here..."
        }
    }
};
```

This DSL provides a powerful yet simple way to add interactivity to your HyperChad applications while maintaining clean, readable code.
