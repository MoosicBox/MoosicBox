#[cfg(feature = "test-utils")]
use hyperchad_test_utils::{FormData, TestPlan};

#[cfg(feature = "test-utils")]
#[switchy_async::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hyperchad_simulator::{AppConfig, HyperChadSimulator, RendererType, SimulationData};

    // Initialize logging
    env_logger::init();

    println!("HyperChad Simulator Example");

    // Create app configuration
    let app_config = AppConfig {
        name: "example-app".to_string(),
        routes: vec![
            "/".to_string(),
            "/login".to_string(),
            "/dashboard".to_string(),
        ],
        static_assets: std::collections::BTreeMap::new(),
        environment: std::collections::BTreeMap::new(),
    };

    // Create simulation data
    let simulation_data = SimulationData {
        users: vec![serde_json::json!({"username": "testuser", "password": "password123"})],
        api_responses: std::collections::BTreeMap::new(),
        database_state: std::collections::BTreeMap::new(),
    };

    // Create simulator
    let simulator = HyperChadSimulator::new()
        .with_app_config(app_config)
        .with_renderer(RendererType::Html)
        .with_renderer(RendererType::VanillaJs)
        .with_mock_data(simulation_data);

    println!("Created simulator with {} renderers", 2);

    // Create a test plan
    let test_plan = TestPlan::new()
        .navigate_to("/login")
        .fill_form(
            FormData::new()
                .text("username", "testuser")
                .text("password", "password123"),
        )
        .click("#login-button")
        .wait_for_url("/dashboard");

    println!("Created test plan with {} steps", test_plan.steps.len());

    // Run the simulation
    match simulator.run_test_plan(test_plan) {
        Ok(result) => {
            println!("Simulation completed successfully!");
            println!("Steps executed: {}", result.steps_executed);
            println!("Steps executed: {}", result.steps_executed);
            println!("Execution time: {:?}", result.execution_time);

            if !result.errors.is_empty() {
                println!("Errors: {:?}", result.errors);
            }
            if !result.warnings.is_empty() {
                println!("Warnings: {:?}", result.warnings);
            }
        }
        Err(e) => {
            eprintln!("Simulation failed: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

#[cfg(not(feature = "test-utils"))]
fn main() {
    println!("Test utils feature not enabled. Enable with --features test-utils");
}
