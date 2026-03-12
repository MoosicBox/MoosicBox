# MoosicBox HyperChad Simulator

Simulation framework for HyperChad applications using simvar for deterministic testing.

## Features

- **Simulation Framework**: Provides infrastructure for simulating HyperChad applications
- **Multi-Renderer Support**: Supports HTML, Vanilla JS, egui, and FLTK renderer types
- **Test Plan Execution**: Integrates with `hyperchad_test_utils` for test automation
- **Web Server Simulation**: Uses `switchy_web_server_simulator` for backend mocking
- **Configuration**: Supports app configuration, mock data, and multiple renderers

**Note**: Renderer simulation implementations are currently placeholders. Full renderer testing capabilities are planned.

## Core API

- `HyperChadSimulator::new()` creates a simulator with default `AppConfig` and empty `SimulationData`
- `with_app_config`, `with_renderer`/`with_renderers`, `with_mock_data`, and `with_web_server` configure simulation inputs
- `run_test_plan` executes a `TestPlan` and is available only with the `test-utils` feature
- `start_simulation_server` starts the configured `SimulationWebServer`
- `AppConfig`, `SimulationData`, and `RendererType` are the primary configuration types

## Usage

```rust
use hyperchad_simulator::{AppConfig, HyperChadSimulator, RendererType, SimulationData};
use hyperchad_test_utils::{TestPlan, FormData};

// Create simulation data
let simulation_data = SimulationData {
    users: vec![serde_json::json!({"username": "testuser"})],
    api_responses: std::collections::BTreeMap::new(),
    database_state: std::collections::BTreeMap::new(),
};

// Create simulator
let simulator = HyperChadSimulator::new()
    .with_app_config(AppConfig {
        name: "my-app".to_string(),
        routes: vec!["/login".to_string(), "/dashboard".to_string()],
        static_assets: std::collections::BTreeMap::new(),
        environment: std::collections::BTreeMap::new(),
    })
    .with_renderers(vec![RendererType::Html, RendererType::VanillaJs])
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

Start a simulation web server:

```rust
use std::sync::Arc;

use hyperchad_simulator::{web_server::SimulationWebServer, HyperChadSimulator};

# async fn example() -> Result<(), hyperchad_simulator::SimulatorError> {
let web_server = Arc::new(SimulationWebServer::new());
let simulator = HyperChadSimulator::new().with_web_server(web_server);
simulator.start_simulation_server().await?;
# Ok(())
# }
```

## Features

Enable the `test-utils` feature to use `TestPlan` execution:

```toml
[dependencies]
hyperchad_simulator = { version = "0.1.0", features = ["test-utils"] }
```
