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

### Element Reference API (Object-Oriented)

The DSL supports a modern, object-oriented API for element manipulation:

```rust
// Get an element reference
fx-click=fx {
    let queue = element("#play-queue");
    if queue.visibility() == hidden() {
        queue.show();
    } else {
        queue.hide();
    }
}

// Simple method calls
fx-click=fx {
    let button = element("#my-button");
    button.show();
}

// Set properties
fx-click=fx {
    let panel = element("#info-panel");
    panel.set_visibility(visible());
}
```

**Available element methods:**
- `element.show()` - Show the element
- `element.hide()` - Hide the element
- `element.toggle()` - Toggle element visibility
- `element.visibility()` - Get current visibility state
- `element.set_visibility(visibility)` - Set visibility state

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

Compare traditional vs. element reference syntax:

**Traditional syntax:**
```rust
fx-click=fx {
    if get_visibility("panel") == hidden() {
        show("panel")
    } else {
        hide("panel")
    }
}
```

**Element reference syntax (recommended):**
```rust
fx-click=fx {
    let panel = element("panel");
    if panel.visibility() == hidden() {
        panel.show();
    } else {
        panel.hide();
    }
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
- `custom(action_name)` - Execute a custom action

### Element Reference Functions
- `element(selector)` - Get an element reference
- `element.show()` - Show the element
- `element.hide()` - Hide the element
- `element.toggle()` - Toggle element visibility
- `element.visibility()` - Get current visibility state
- `element.set_visibility(visibility)` - Set visibility state

### Conditional Functions
- `get_visibility(id)` - Get element visibility state (traditional)
- `visible()` - Visibility state constant
- `hidden()` - Hidden state constant

## Best Practices

1. **Prefer element references**: Use the element reference API for cleaner, more readable code
2. **Use descriptive element IDs**: Make your actions self-documenting
3. **Group related actions**: Keep logically related actions together
4. **Use variables for reusability**: Define element references as variables when used multiple times
5. **Add logging for debugging**: Include log statements for complex workflows

## Integration with HyperChad

The Actions DSL integrates seamlessly with HyperChad's template system:

```rust
use hyperchad_template::container;

let ui = container! {
    div {
        button fx-click=fx {
            let search = element("search-panel");
            search.toggle();
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