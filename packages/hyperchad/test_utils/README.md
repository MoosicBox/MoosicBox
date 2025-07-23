# MoosicBox HyperChad Test Utils

Comprehensive test workflow builders and utilities for HyperChad application testing.

## Features

* **Fluent Test API**: Declarative test scenario building
* **Navigation & Interaction**: URL navigation, clicks, form filling
* **HTTP Testing**: Request/response validation
* **DOM Assertions**: Element existence, content, attributes
* **Wait Conditions**: Synchronization and timing control
* **Conditional Logic**: If/else, loops, parallel execution
* **Data-Driven Testing**: Parameterized test scenarios

## Usage

```rust
use hyperchad_test_utils::{TestPlan, FormData};

let plan = TestPlan::new()
    .navigate_to("/login")
    .fill_form(FormData::new()
        .text("username", "testuser")
        .text("password", "secret123"))
    .click("#login-button")
    .wait_for(WaitCondition::url_contains("/dashboard"))
    .assert_dom("#welcome-message", DomMatcher::visible())
    .assert_dom(".notification", DomMatcher::contains("Welcome"));

// Execute the test plan
let result = simulator.run_test_plan(plan).await?;
```

## Test Step Types

* **Navigation**: `navigate_to()`, `go_back()`, `reload()`
* **Interaction**: `click()`, `hover()`, `key_press()`, `scroll()`
* **Forms**: `fill_form()`, `select_option()`, `upload_file()`
* **HTTP**: `send_request()`, `assert_response()`
* **Timing**: `wait_for()`, `sleep()`
* **Assertions**: `assert_dom()`, `assert_url()`, `assert_state()`