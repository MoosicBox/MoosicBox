# MoosicBox HyperChad Simulator

Simulation framework for HyperChad applications using simvar for deterministic testing.

## Features

- **Simulation Framework**: Provides infrastructure for simulating HyperChad applications
- **Multi-Renderer Support**: Supports HTML, Vanilla JS, egui, and FLTK renderer types
- **Test Plan Execution**: Integrates with `hyperchad_test_utils` for test automation
- **Web Server Simulation**: Uses `switchy_web_server_simulator` for backend mocking
- **Configuration**: Supports app configuration, mock data, and multiple renderers

**Note**: Renderer simulation implementations are currently placeholders. Full renderer testing capabilities are planned.

## Usage

```rust
use hyperchad_simulator::{HyperChadSimulator, RendererType, SimulationData};
use hyperchad_test_utils::{TestPlan, FormData};

// Create simulation data
let simulation_data = SimulationData {
    users: vec![serde_json::json!({"username": "testuser"})],
    api_responses: std::collections::BTreeMap::new(),
    database_state: std::collections::BTreeMap::new(),
};

// Create simulator
let simulator = HyperChadSimulator::new()
    .with_renderer(RendererType::VanillaJs)
    .with_mock_data(simulation_data);

// Create test plan
let plan = TestPlan::new()
    .navigate_to("/login")
    .fill_form(FormData::new().text("username", "testuser"))
    .click("#submit")
    .wait_for_url("/dashboard");

// Run simulation (requires "test-utils" feature)
let result = simulator.run_test_plan(plan)?;
```

## Features

Enable the `test-utils` feature to use `TestPlan` execution:

```toml
[dependencies]
hyperchad_simulator = { version = "0.1", features = ["test-utils"] }
```
