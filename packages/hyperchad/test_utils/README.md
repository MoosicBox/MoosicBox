# MoosicBox HyperChad Test Utils

Test workflow builders and utilities for HyperChad application testing.

## Features

- **Fluent Test API**: Declarative test scenario building
- **Navigation & Interaction**: URL navigation, clicks, form filling, keyboard/mouse events
- **HTTP Testing**: Request building with multiple body formats
- **Wait Conditions**: Element existence, URL patterns, and timing control
- **Control Flow**: Loops, parallel execution, retry logic, try/catch patterns
- **Data-Driven Testing**: Parameterized test scenarios with `ForEach` loops
- **Reusable Fragments**: Pre-built test patterns for common scenarios

## Usage

```rust
use hyperchad_test_utils::{TestPlan, FormData};

let plan = TestPlan::new()
    .navigate_to("/login")
    .fill_form(FormData::new()
        .text("username", "testuser")
        .text("password", "secret123"))
    .click("#login-button")
    .wait_for_url("/dashboard")
    .wait_for_element("#welcome-message");

// Execute the test plan with your test executor
let result = executor.run_test_plan(plan).await?;
```

## Test Step Types

- **Navigation**: `navigate_to()`, `go_back()`, `go_forward()`, `reload()`, `set_hash()`
- **Interaction**: `click()`, `double_click()`, `right_click()`, `hover()`, `focus()`, `blur()`, `key_press()`, `key_sequence()`, `scroll()`
- **Forms**: `fill_form()`, `fill_field()`, `select_option()`, `upload_file()`
- **HTTP**: `send_request()` with support for GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS requests and JSON/form/text/binary bodies
- **Timing**: `wait_for_element()`, `wait_for_url()`, `sleep()`
- **Control Flow**: `repeat()`, `parallel()`, `ForEach`, try/catch, retry with delay
- **Test Lifecycle**: `with_setup()`, `with_teardown()`, `with_timeout()`, `with_retry_count()`

## Reusable Test Fragments

Pre-built test patterns available in `workflow::fragments`:

```rust
use hyperchad_test_utils::workflow::fragments;

// Common authentication flow
let login = fragments::login_flow("user@example.com", "password123");

// Test navigation across sections
let nav_test = fragments::navigation_test();

// Form validation testing
let form_test = fragments::form_validation_test("#signup-form");

// Keyboard accessibility test
let a11y_test = fragments::accessibility_test();
```

## Advanced Features

### Test Lifecycle Hooks

```rust
use hyperchad_test_utils::{TestPlan, SetupStep, TeardownStep};
use std::time::Duration;

let plan = TestPlan::new()
    .with_setup(SetupStep {
        description: "Initialize test data".to_string(),
        steps: vec![/* setup steps */],
    })
    .with_teardown(TeardownStep {
        description: "Clean up test data".to_string(),
        steps: vec![/* cleanup steps */],
    })
    .with_timeout(Duration::from_secs(30))
    .with_retry_count(3);
```

### Data-Driven Testing

```rust
use hyperchad_test_utils::{TestPlan, TestStep, ControlStep, NavigationStep};
use serde_json::json;

let test_data = vec![
    json!({"username": "user1", "email": "user1@example.com"}),
    json!({"username": "user2", "email": "user2@example.com"}),
];

let plan = TestPlan::new().add_step(TestStep::Control(
    ControlStep::for_each(test_data, vec![
        TestStep::Navigation(NavigationStep::GoTo { url: "/register".to_string() }),
        // Use data in steps...
    ])
));
```

### HTTP Request Testing

```rust
use hyperchad_test_utils::{TestPlan, HttpRequestStep};
use serde_json::json;
use std::time::Duration;

let plan = TestPlan::new()
    .send_request(
        HttpRequestStep::post("/api/users")
            .json(json!({"name": "Test User", "email": "test@example.com"}))
            .with_header("Authorization", "Bearer token123")
            .expect_status(201)
            .with_timeout(Duration::from_secs(10))
    );
```
