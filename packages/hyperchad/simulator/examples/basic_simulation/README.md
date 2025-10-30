# Basic Simulation Example

This example demonstrates how to use the HyperChad simulator to test applications across multiple renderer implementations with automated test plans.

## Summary

This example shows how to set up a HyperChad simulator, configure application routes, provide mock data, and execute a test plan that simulates user interactions across multiple renderers (HTML and Vanilla JS).

## What This Example Demonstrates

- Creating a `HyperChadSimulator` instance with custom configuration
- Configuring multiple renderer types (HTML and Vanilla JS)
- Setting up application routes and mock data
- Creating a test plan with navigation, form filling, and button clicks
- Executing simulations and interpreting results
- Handling both test-utils enabled and disabled scenarios

## Prerequisites

- Basic understanding of async Rust programming
- Familiarity with the HyperChad framework concepts
- Understanding of web application testing concepts (navigation, forms, DOM interactions)

## Running the Example

To run this example with full test functionality:

```bash
cargo run --manifest-path packages/hyperchad/simulator/examples/basic_simulation/Cargo.toml --features test-utils
```

To run without test utilities (demonstrates feature gating):

```bash
cargo run --manifest-path packages/hyperchad/simulator/examples/basic_simulation/Cargo.toml
```

## Expected Output

When run with `test-utils` feature enabled, you should see:

```
=== HyperChad Simulator Example ===

✓ Created app config with 3 routes
✓ Prepared simulation data with 1 users
✓ Created simulator with 2 renderers (HTML, Vanilla JS)

✓ Created test plan with 4 steps
  - Navigate to /login
  - Fill login form
  - Click login button
  - Wait for dashboard

Running simulation...

✓ Simulation completed successfully!
  Steps executed: 4
  Execution time: <duration>
```

Without the feature, you'll see a message explaining how to enable test-utils.

## Code Walkthrough

### Step 1: Application Configuration

```rust
let app_config = AppConfig {
    name: "example-app".to_string(),
    routes: vec![
        "/".to_string(),
        "/login".to_string(),
        "/dashboard".to_string(),
    ],
    static_assets: BTreeMap::new(),
    environment: BTreeMap::new(),
};
```

The `AppConfig` defines the structure of your application, including available routes. This tells the simulator what pages exist and can be navigated to.

### Step 2: Simulation Data Setup

```rust
let simulation_data = SimulationData {
    users: vec![serde_json::json!({
        "username": "testuser",
        "password": "password123"
    })],
    api_responses: BTreeMap::new(),
    database_state: BTreeMap::new(),
};
```

`SimulationData` provides mock backend data that your simulated application can use. This includes user credentials, API response mocks, and simulated database state.

### Step 3: Creating the Simulator

```rust
let simulator = HyperChadSimulator::new()
    .with_app_config(app_config)
    .with_renderer(RendererType::Html)
    .with_renderer(RendererType::VanillaJs)
    .with_mock_data(simulation_data);
```

The simulator uses a fluent builder API to configure which renderers to test. Each renderer will be tested independently with the same test plan, ensuring consistent behavior across all rendering backends.

### Step 4: Building a Test Plan

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

The `TestPlan` defines a sequence of user interactions. This example simulates a login workflow:

1. Navigate to the login page
2. Fill in username and password fields
3. Click the login button
4. Wait for redirect to the dashboard

### Step 5: Running the Simulation

```rust
match simulator.run_test_plan(test_plan) {
    Ok(result) => {
        println!("✓ Simulation completed successfully!");
        println!("  Steps executed: {}", result.steps_executed);
        println!("  Execution time: {:?}", result.execution_time);
    }
    Err(e) => {
        eprintln!("✗ Simulation failed: {e}");
    }
}
```

The `run_test_plan` method executes the test across all configured renderers. It returns a `TestResult` with execution statistics, errors, and warnings.

## Key Concepts

### Multiple Renderer Testing

The HyperChad simulator supports testing the same application logic across different rendering backends:

- **HTML**: Server-side rendered HTML
- **Vanilla JS**: Client-side JavaScript rendering
- **Egui**: Native GUI rendering (future support)
- **FLTK**: Native GUI rendering (future support)

This ensures your application behaves consistently regardless of how it's rendered.

### Deterministic Testing with Simvar

The simulator uses the `simvar` crate for deterministic simulation. This means:

- Tests are reproducible across runs
- Network operations are simulated without actual I/O
- Time can be controlled for consistent timing tests
- Multiple renderer tests run in isolated simulation environments

### Test Plan Fluent API

Test plans are built using a fluent API that makes it easy to express user interactions:

- `navigate_to(url)`: Navigate to a URL
- `fill_form(data)`: Fill form fields with data
- `click(selector)`: Click an element matching the selector
- `wait_for_url(url)`: Wait for navigation to a specific URL
- Additional assertions and conditions can be added

### Feature Gating

The example demonstrates proper feature gating with the `test-utils` feature. When disabled, the simulator can still be configured, but test execution requires the feature to be enabled. This allows for flexible compilation in different contexts.

## Testing the Example

Since this is a simulation framework example, the testing involves:

1. **Run with test-utils enabled**: Verify the full simulation executes successfully
2. **Run without test-utils**: Confirm the feature gate message appears
3. **Check logs**: Set `RUST_LOG=debug` to see detailed simulation progress
4. **Modify the test plan**: Try adding more steps or changing the user flow

Example with debug logging:

```bash
RUST_LOG=debug cargo run --manifest-path packages/hyperchad/simulator/examples/basic_simulation/Cargo.toml --features test-utils
```

## Troubleshooting

### "Test utils feature not enabled" message

**Problem**: The example prints a message about enabling test-utils and exits.

**Solution**: Run with the `--features test-utils` flag:

```bash
cargo run --manifest-path packages/hyperchad/simulator/examples/basic_simulation/Cargo.toml --features test-utils
```

### Build errors about missing dependencies

**Problem**: The example fails to compile with missing type errors.

**Solution**: Ensure you're building from the workspace root and that all workspace dependencies are available. The example requires `hyperchad_simulator` with `test-utils` feature and `hyperchad_test_utils`.

### Simulation never completes

**Problem**: The simulation seems to hang or run indefinitely.

**Solution**: This is typically a placeholder implementation issue. The current simulator has placeholder renderer implementations. In production use, each renderer would have full simulation capabilities.

## Related Examples

- `packages/hyperchad/examples/details_summary` - Web component example showing HyperChad UI patterns
- `packages/hyperchad/examples/http_events` - HTTP event handling in HyperChad
- `packages/async/examples/simulated` - Simvar-based async simulation example
