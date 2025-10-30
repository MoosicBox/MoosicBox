#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic simulation example demonstrating `HyperChad` simulator usage.
//!
//! This example shows how to set up a `HyperChad` simulator with multiple renderers,
//! configure application routes, provide mock data, and execute a test plan.

#[cfg(feature = "test-utils")]
use std::collections::BTreeMap;

#[cfg(feature = "test-utils")]
use hyperchad_simulator::{AppConfig, HyperChadSimulator, RendererType, SimulationData};
#[cfg(feature = "test-utils")]
use hyperchad_test_utils::{FormData, TestPlan};

#[cfg(feature = "test-utils")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging so we can see simulation progress
    env_logger::init();

    println!("=== HyperChad Simulator Example ===\n");

    // Step 1: Create application configuration
    // Define the routes that our simulated app will have
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

    println!(
        "✓ Created app config with {} routes",
        app_config.routes.len()
    );

    // Step 2: Prepare mock data for the simulation
    // This simulates the backend data that would be available
    let simulation_data = SimulationData {
        users: vec![serde_json::json!({
            "username": "testuser",
            "password": "password123"
        })],
        api_responses: BTreeMap::new(),
        database_state: BTreeMap::new(),
    };

    println!(
        "✓ Prepared simulation data with {} users",
        simulation_data.users.len()
    );

    // Step 3: Create the simulator with multiple renderers
    // This will test the app with both HTML and Vanilla JS renderers
    let simulator = HyperChadSimulator::new()
        .with_app_config(app_config)
        .with_renderer(RendererType::Html)
        .with_renderer(RendererType::VanillaJs)
        .with_mock_data(simulation_data);

    println!("✓ Created simulator with 2 renderers (HTML, Vanilla JS)\n");

    // Step 4: Create a test plan that simulates user interaction
    // This test plan will:
    // 1. Navigate to the login page
    // 2. Fill in the login form
    // 3. Click the login button
    // 4. Wait for redirect to dashboard
    let test_plan = TestPlan::new()
        .navigate_to("/login")
        .fill_form(
            FormData::new()
                .text("username", "testuser")
                .text("password", "password123"),
        )
        .click("#login-button")
        .wait_for_url("/dashboard");

    println!("✓ Created test plan with {} steps", test_plan.steps.len());
    println!("  - Navigate to /login");
    println!("  - Fill login form");
    println!("  - Click login button");
    println!("  - Wait for dashboard\n");

    // Step 5: Run the simulation
    println!("Running simulation...\n");
    match simulator.run_test_plan(test_plan) {
        Ok(result) => {
            println!("✓ Simulation completed successfully!");
            println!("  Steps executed: {}", result.steps_executed);
            println!("  Execution time: {:?}", result.execution_time);

            if !result.errors.is_empty() {
                println!("\n⚠ Errors encountered:");
                for error in &result.errors {
                    println!("  - {error}");
                }
            }

            if !result.warnings.is_empty() {
                println!("\n⚠ Warnings:");
                for warning in &result.warnings {
                    println!("  - {warning}");
                }
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("✗ Simulation failed: {e}");
            Err(e.into())
        }
    }
}

#[cfg(not(feature = "test-utils"))]
fn main() {
    println!("❌ Test utils feature not enabled.");
    println!("\nTo run this example with full functionality, use:");
    println!(
        "  cargo run --manifest-path packages/hyperchad/simulator/examples/basic_simulation/Cargo.toml --features test-utils"
    );
    println!("\nWithout the test-utils feature, this example cannot execute test plans.");
}
