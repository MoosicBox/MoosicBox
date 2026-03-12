# `MoosicBox` `HyperChad` Test Utils

Test workflow builders and utilities for `HyperChad` application testing.

## Features

- **Fluent Test API**: Declarative test scenario building
- **Navigation & Interaction**: URL navigation, clicks, form filling, keyboard/mouse events
- **HTTP Testing**: Request building with multiple body formats
- **Wait Conditions**: Element existence, URL patterns, and timing control
- **Control Flow**: Loops, parallel execution, retry logic, try/catch patterns
- **Data-Driven Testing**: Parameterized test scenarios with `ForEach` loops
- **Reusable Fragments**: Pre-built test patterns in `fragments` module (login, logout, navigation, form validation, accessibility tests)

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
```

## Test Step Types

- **Navigation**: `navigate_to()`, `go_back()`, `go_forward()`, `reload()`, `set_hash()`
- **Interaction**: `click()`, `double_click()`, `right_click()`, `hover()`, `focus()`, `blur()`, `key_press()`, `key_sequence()`, `scroll()`
- **Forms**: `fill_form()`, `fill_field()`, `select_option()`, `upload_file()`
- **HTTP**: `send_request()` with support for GET, POST, PUT, DELETE requests and JSON/form/text bodies
- **Timing**: `wait_for_element()`, `wait_for_url()`, `sleep()`
- **Control Flow**: `repeat()`, `parallel()`, try/catch, retry with delay

## HTTP Request Usage

```rust
use hyperchad_test_utils::{HttpRequestStep, TestPlan};

let request = HttpRequestStep::post("/api/login")
    .json(serde_json::json!({"username": "testuser", "password": "secret123"}))
    .expect_status(200);

let plan = TestPlan::new().send_request(request);
```

`HttpRequestStep` is the main API for HTTP test steps. Use `get()`, `post()`, `put()`, or `delete()`, then configure headers/body/validation with `with_header()`, `json()`, `text()`, `form()`, `expect_status()`, and `with_timeout()`.

## Reusable Fragments

```rust
use hyperchad_test_utils::{fragments, TestPlan};

let plan = TestPlan::new()
    .include(fragments::login_flow("testuser", "secret123"))
    .include(fragments::accessibility_test())
    .include(fragments::logout_flow());
```

The `fragments` module provides `login_flow()`, `logout_flow()`, `navigation_test()`, `form_validation_test()`, and `accessibility_test()` for common scenarios.
