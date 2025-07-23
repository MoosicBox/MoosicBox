# HyperChad Simulator Examples

This directory contains examples demonstrating how to use the HyperChad simulation framework.

## Basic Simulation

Run the basic simulation example:

```bash
# Without test utils (basic simulator setup)
cargo run --example basic_simulation

# With test utils (full test plan execution)
cargo run --example basic_simulation --features test-utils
```

## Features

The simulator supports:

- **Multiple Renderers**: Test HTML, Vanilla JS, egui, and FLTK renderers
- **Deterministic Testing**: Reproducible results using simvar
- **Test Workflows**: Fluent API for building complex test scenarios
- **Mock Data**: Simulate API responses and database state
- **Performance Testing**: Measure rendering performance

## Example Test Plan

```rust
let test_plan = TestPlan::new()
    .navigate_to("/login")
    .fill_form(FormData::new()
        .text("username", "testuser")
        .text("password", "password123"))
    .click("#login-button")
    .wait_for(WaitCondition::url_contains("/dashboard"))
    .assert_dom("#welcome-message", DomMatcher::visible())
    .assert_response(ResponseMatcher::status_ok());
```