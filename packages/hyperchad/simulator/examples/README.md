# HyperChad Simulator Examples

This directory contains examples demonstrating how to use the HyperChad simulation framework.

## Available Examples

### [basic_simulation](basic_simulation/)

Demonstrates how to use the HyperChad simulator to test applications across multiple renderer implementations with automated test plans. This example shows:

- Creating a `HyperChadSimulator` instance with custom configuration
- Configuring multiple renderer types (HTML and Vanilla JS)
- Setting up application routes and mock data
- Creating a test plan with navigation, form filling, and button clicks
- Executing simulations and interpreting results

Run with:

```bash
cargo run --manifest-path packages/hyperchad/simulator/examples/basic_simulation/Cargo.toml --features test-utils
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
    .fill_form(
        FormData::new()
            .text("username", "testuser")
            .text("password", "password123"),
    )
    .click("#login-button")
    .wait_for_url("/dashboard");
```

## Additional Resources

- [HyperChad Documentation](../../../README.md)
- [Test Utils Package](../../test_utils/README.md)
- [Web Server Simulator](../../../web_server/simulator/README.md)
