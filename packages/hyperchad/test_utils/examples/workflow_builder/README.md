# Workflow Builder Example

A comprehensive demonstration of building test workflows using the `hyperchad_test_utils` fluent API.

## Summary

This example showcases how to build declarative test plans for web application testing, including navigation, form interactions, HTTP API testing, keyboard/mouse interactions, and control flow patterns like loops and parallel execution.

## What This Example Demonstrates

- **Simple workflows**: Basic navigation and form filling for login scenarios
- **Complex forms**: Working with multiple field types (text, number, boolean, select, file upload)
- **HTTP API testing**: Building GET, POST, PUT, DELETE requests with headers and JSON bodies
- **Control flow patterns**: Using loops (`repeat()`) and parallel execution (`parallel()`)
- **Keyboard and mouse interactions**: Key sequences, scrolling, hovering, double-clicking
- **Reusable test fragments**: Composing workflows from pre-built patterns like `login_flow()` and `logout_flow()`
- **Complete test configuration**: Adding setup/teardown steps, timeouts, and retry logic

## Prerequisites

- Basic understanding of Rust programming
- Familiarity with web testing concepts (selectors, forms, HTTP methods)
- Knowledge of the builder pattern in Rust

## Running the Example

```bash
cargo run --manifest-path packages/hyperchad/test_utils/examples/workflow_builder/Cargo.toml
```

## Expected Output

The example builds various test workflows and prints information about each one:

```
=== HyperChad Test Utils - Workflow Builder Example ===

1. Building a simple login workflow...
   Created test plan with 5 steps
   Steps: navigate → wait → fill form → click → wait for redirect

2. Building a complex form interaction workflow...
   Created test plan with 5 steps
   Steps: navigate → fill multi-field form → upload file → submit

3. Building an HTTP API test workflow...
   Created test plan with 4 steps
   Steps: GET request → POST with JSON → PUT update → DELETE

4. Building a workflow with control flow...
   Created test plan with 2 steps
   Steps: loop 3x (click → wait) → parallel branches

5. Building keyboard and mouse interaction workflow...
   Created test plan with 9 steps
   Steps: navigation → keyboard shortcuts → scrolling → hover

6. Building workflow using reusable fragments...
   Created test plan with 15 steps
   Steps: login fragment → navigation fragment → logout fragment

7. Building a complete workflow with setup/teardown...
   Setup: true
   Steps: 4
   Teardown: true
   Timeout: Some(30s)
   Retry count: 2

✓ All workflows built successfully!

These test plans can be serialized to JSON for external test runners,
or executed by a test framework that implements the test step execution.
```

## Code Walkthrough

### 1. Simple Login Workflow

```rust
fn build_login_workflow() -> TestPlan {
    TestPlan::new()
        .navigate_to("/login")
        .wait_for_element("#login-form")
        .fill_form(
            FormData::new()
                .text("username", "testuser")
                .text("password", "secure123"),
        )
        .click("#login-button")
        .wait_for_url("/dashboard")
}
```

This demonstrates the basic fluent API pattern. Each method returns `TestPlan`, allowing you to chain operations. The workflow navigates to a login page, waits for the form to appear, fills in credentials, clicks the login button, and waits for redirection.

### 2. Complex Form Interaction

```rust
.fill_form(
    FormData::new()
        .text("name", "John Doe")
        .number("age", 30.0)
        .boolean("newsletter", true)
        .select("country", "US")
        .multi_select("interests", vec!["music".to_string(), "coding".to_string()])
)
```

`FormData` supports multiple field types. The fluent builder pattern makes it easy to construct complex form data with various input types.

### 3. HTTP API Testing

```rust
.send_request(
    HttpRequestStep::post("https://api.example.com/users")
        .json(serde_json::json!({
            "name": "Jane Smith",
            "email": "jane@example.com"
        }))
        .expect_status(201)
)
```

HTTP requests can be built with method-specific constructors (`get()`, `post()`, `put()`, `delete()`), and configured with headers, body content, and expected status codes.

### 4. Control Flow with Loops

```rust
.repeat(3)
    .step(TestStep::Interaction(InteractionStep::Click {
        selector: "#load-more".to_string(),
    }))
    .step(TestStep::Wait(WaitStep::Duration {
        duration: Duration::from_millis(500),
    }))
.end_repeat()
```

The `repeat()` builder creates a loop that executes the specified steps multiple times. Use `end_repeat()` to close the loop and return to the main test plan.

### 5. Parallel Execution

```rust
.parallel()
    .branch("check-header")
    .step(/* check header exists */)
    .branch("check-footer")
    .step(/* check footer exists */)
    .branch("check-sidebar")
    .step(/* check sidebar exists */)
.join_all()
```

Parallel execution allows multiple test branches to run concurrently. Each branch has a name and its own sequence of steps. Use `join_all()` to wait for all branches to complete.

### 6. Keyboard Interactions

```rust
.key_sequence(vec![Key::Control, Key::A])  // Ctrl+A
.key_sequence(vec![Key::H, Key::E, Key::L, Key::L, Key::O])  // Type "HELLO"
```

Keyboard interactions support both single key presses and sequences. This is useful for testing shortcuts, form input, and navigation.

### 7. Reusable Test Fragments

```rust
.include(fragments::login_flow("testuser", "password123"))
.include(fragments::navigation_test())
.include(fragments::logout_flow())
```

The `fragments` module provides pre-built test patterns for common scenarios. Use `include()` to compose workflows from these reusable fragments.

### 8. Setup and Teardown

```rust
TestPlan::new()
    .with_setup(SetupStep {
        description: "Clear browser state".to_string(),
        steps: vec![/* setup steps */],
    })
    .navigate_to("/dashboard")
    // ... main test steps ...
    .with_teardown(TeardownStep {
        description: "Logout and clear state".to_string(),
        steps: vec![/* cleanup steps */],
    })
    .with_timeout(Duration::from_secs(30))
    .with_retry_count(2)
```

Complete test plans can include setup and teardown steps that run before and after the main test, along with timeout and retry configuration.

## Key Concepts

### Fluent API Pattern

The fluent API allows you to chain method calls naturally, building up complex workflows in a readable, declarative style. Each method returns `Self` (or a builder type), enabling the chain to continue.

### Builder Types

Some operations use specialized builders:

- **`LoopBuilder`**: Created by `repeat()`, builds loop iterations, completed with `end_repeat()`
- **`ParallelBuilder`**: Created by `parallel()`, manages parallel branches, completed with `join_all()`
- **`BranchBuilder`**: Created by `branch()`, adds steps to a specific parallel branch

### Test Plan Serialization

All test plan types implement `Serialize` and `Deserialize` (via Serde), allowing workflows to be:

- Saved to JSON files
- Transmitted over network protocols
- Loaded by external test runners
- Version-controlled as test specifications

### Form Data Types

The `FormValue` enum supports multiple input types:

- **Text**: String values for text inputs
- **Number**: Numeric values for number inputs
- **Boolean**: Checkbox state
- **Select**: Single-select dropdown value
- **MultiSelect**: Multiple selections from a multi-select dropdown
- **File**: File path for file upload inputs

### Wait Conditions

Three types of wait conditions are available:

- **`ElementExists`**: Wait for a CSS selector to match an element in the DOM
- **`UrlContains`**: Wait for the URL to contain a specific fragment
- **`Duration`**: Wait for a fixed time period (use sparingly, prefer element/URL waits)

## Testing the Example

The example is purely demonstrative - it builds test plans but doesn't execute them. To actually execute these workflows, you would need:

1. A test runner that implements the step execution logic
2. A browser automation backend (e.g., WebDriver, Playwright)
3. An HTTP client for API testing

The test plans can be serialized and passed to such a runner:

```rust
let plan = build_login_workflow();
let json = serde_json::to_string_pretty(&plan)?;
println!("{}", json);
```

## Troubleshooting

### Common Issues

**Issue**: Example doesn't compile

- **Solution**: Ensure you have the latest version of `hyperchad_test_utils` in your workspace
- Check that all workspace dependencies are properly configured

**Issue**: Understanding the builder pattern

- **Solution**: Start with the simple examples first (login workflow)
- Each method call returns the same or a related type, allowing chaining
- Builder types like `LoopBuilder` have terminal methods (`end_repeat()`) that return to the main `TestPlan`

**Issue**: Confusion about when to use which interaction type

- **Solution**:
    - Use `Navigation` for URL-based actions (navigate, back, forward, reload)
    - Use `Interaction` for DOM element manipulation (click, hover, keyboard, mouse)
    - Use `Form` for form-specific operations (fill form, select options, upload files)
    - Use `Http` for API testing without browser involvement

## Related Examples

This is currently the only example for `hyperchad_test_utils`. For related testing patterns, see:

- `packages/hyperchad/examples/details_summary/` - Web component testing
- Other HyperChad examples demonstrating UI components and interactions
