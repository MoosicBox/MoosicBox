//! Test simulator for `HyperChad` applications across multiple renderer implementations.
//!
//! This crate provides simulation infrastructure to test `HyperChad` applications with different
//! rendering backends (HTML, Vanilla JavaScript, Egui, FLTK) in an automated environment. It
//! enables testing application behavior, routing, and UI interactions without manual intervention.
//!
//! # Features
//!
//! * Simulate multiple renderer types in a single test run
//! * Mock application configuration, routes, and static assets
//! * Provide simulated data (users, API responses, database state)
//! * Integration with `simvar` for distributed simulation scenarios
//! * Optional test utilities via the `test-utils` feature
//!
//! # Example
//!
//! ```rust
//! use hyperchad_simulator::{HyperChadSimulator, RendererType, AppConfig};
//!
//! # fn example() {
//! let simulator = HyperChadSimulator::new()
//!     .with_app_config(AppConfig {
//!         name: "my-app".to_string(),
//!         routes: vec!["/".to_string(), "/about".to_string()],
//!         ..Default::default()
//!     })
//!     .with_renderer(RendererType::Html)
//!     .with_renderer(RendererType::VanillaJs);
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, sync::Arc};

#[cfg(feature = "test-utils")]
use hyperchad_test_utils::{TestPlan, TestResult};
#[cfg(feature = "test-utils")]
use simvar::{Sim, SimBootstrap};
use switchy_web_server_simulator::SimulationWebServer;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Test utilities for `HyperChad` applications.
///
/// This module provides testing infrastructure including test plans and test results.
/// Only available when the `test-utils` feature is enabled.
#[cfg(feature = "test-utils")]
pub use hyperchad_test_utils as test_utils;

/// Web server simulator for testing.
///
/// This module provides a simulated web server environment for testing `HyperChad` applications
/// without requiring actual network operations.
pub use switchy_web_server_simulator as web_server;

/// Errors that can occur during `HyperChad` simulation operations.
#[derive(Debug, Error)]
pub enum SimulatorError {
    /// Simulation failed to execute properly.
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),
    /// The specified renderer type is not supported.
    #[error("Renderer not supported: {0:?}")]
    UnsupportedRenderer(RendererType),
    /// Test plan execution failed.
    #[error("Test plan execution failed: {0}")]
    TestPlanFailed(String),
    /// Web server error occurred.
    #[error("Web server error: {0}")]
    WebServer(#[from] switchy_web_server_simulator::Error),
    /// Simvar simulation error occurred.
    #[error("Simvar error: {0}")]
    Simvar(#[from] simvar::Error),
}

/// Supported renderer types for `HyperChad` applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererType {
    /// HTML renderer.
    Html,
    /// Vanilla JavaScript renderer.
    VanillaJs,
    /// Egui GUI renderer.
    Egui,
    /// FLTK GUI renderer.
    Fltk,
}

impl std::fmt::Display for RendererType {
    /// Formats the renderer type as a human-readable string identifier.
    ///
    /// Returns lowercase hyphenated strings suitable for use in CLI arguments,
    /// configuration files, and log messages.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Html => write!(f, "html"),
            Self::VanillaJs => write!(f, "vanilla-js"),
            Self::Egui => write!(f, "egui"),
            Self::Fltk => write!(f, "fltk"),
        }
    }
}

/// Configuration for a `HyperChad` application being simulated.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Application name.
    pub name: String,
    /// Available routes in the application.
    pub routes: Vec<String>,
    /// Static assets mapped from path to content.
    pub static_assets: BTreeMap<String, String>,
    /// Environment variables for the application.
    pub environment: BTreeMap<String, String>,
}

impl Default for AppConfig {
    /// Creates a default application configuration for testing.
    ///
    /// Returns a configuration with the name "test-app", a single root route ("/"),
    /// and no static assets or environment variables.
    fn default() -> Self {
        Self {
            name: "test-app".to_string(),
            routes: vec!["/".to_string()],
            static_assets: BTreeMap::new(),
            environment: BTreeMap::new(),
        }
    }
}

/// Mock data for simulation environment.
#[derive(Debug, Clone)]
pub struct SimulationData {
    /// Simulated user data.
    pub users: Vec<serde_json::Value>,
    /// Mock API responses mapped from endpoint to response data.
    pub api_responses: BTreeMap<String, serde_json::Value>,
    /// Simulated database state mapped from key to value.
    pub database_state: BTreeMap<String, serde_json::Value>,
}

impl Default for SimulationData {
    /// Creates an empty simulation data set.
    ///
    /// Returns a `SimulationData` instance with no users, API responses, or database state.
    fn default() -> Self {
        Self {
            users: Vec::new(),
            api_responses: BTreeMap::new(),
            database_state: BTreeMap::new(),
        }
    }
}

/// Simulator for testing `HyperChad` applications across different renderer implementations.
#[derive(Debug)]
pub struct HyperChadSimulator {
    app_config: AppConfig,
    enabled_renderers: Vec<RendererType>,
    mock_data: SimulationData,
    web_server: Option<Arc<SimulationWebServer>>,
}

impl HyperChadSimulator {
    /// Creates a new simulator with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            app_config: AppConfig::default(),
            enabled_renderers: vec![],
            mock_data: SimulationData::default(),
            web_server: None,
        }
    }

    /// Sets the application configuration for the simulation.
    #[must_use]
    pub fn with_app_config(mut self, config: AppConfig) -> Self {
        self.app_config = config;
        self
    }

    /// Adds a renderer to be tested in the simulation.
    #[must_use]
    pub fn with_renderer(mut self, renderer: RendererType) -> Self {
        if !self.enabled_renderers.contains(&renderer) {
            self.enabled_renderers.push(renderer);
        }
        self
    }

    /// Adds multiple renderers to be tested in the simulation.
    #[must_use]
    pub fn with_renderers(mut self, renderers: Vec<RendererType>) -> Self {
        for renderer in renderers {
            self = self.with_renderer(renderer);
        }
        self
    }

    /// Sets the mock data for the simulation environment.
    #[must_use]
    pub fn with_mock_data(mut self, data: SimulationData) -> Self {
        self.mock_data = data;
        self
    }

    /// Sets the web server for the simulation.
    #[must_use]
    pub fn with_web_server(mut self, server: Arc<SimulationWebServer>) -> Self {
        self.web_server = Some(server);
        self
    }

    /// Runs a test plan within the simulation environment.
    ///
    /// # Errors
    ///
    /// * `SimulatorError::SimulationFailed` - If the simulation fails to start or returns no results
    /// * `SimulatorError::Simvar` - If the underlying simvar simulation fails
    #[cfg(feature = "test-utils")]
    pub fn run_test_plan(&self, plan: TestPlan) -> Result<TestResult, SimulatorError> {
        log::info!(
            "Starting HyperChad simulation with {} renderers",
            self.enabled_renderers.len()
        );

        // Create simulation bootstrap
        let bootstrap = HyperChadSimulationBootstrap {
            simulator: self.clone(),
            test_plan: plan,
        };

        // Run simulation
        let results = simvar::run_simulation(bootstrap)
            .map_err(|e| SimulatorError::SimulationFailed(e.to_string()))?;

        // Process results
        if results.is_empty() {
            return Err(SimulatorError::SimulationFailed(
                "No simulation results".to_string(),
            ));
        }

        // For now, return the first result
        // TODO: Aggregate results from multiple renderers
        Ok(TestResult::success())
    }

    /// Starts the simulation server.
    ///
    /// # Errors
    ///
    /// * `SimulatorError::WebServer` - If the web server fails to start
    pub async fn start_simulation_server(&self) -> Result<(), SimulatorError> {
        if let Some(server) = &self.web_server {
            server.start().await?;
        }
        Ok(())
    }

    /// Simulate a specific renderer
    #[cfg(feature = "test-utils")]
    fn simulate_renderer(renderer: RendererType, plan: &TestPlan) -> TestResult {
        log::info!("Simulating renderer: {renderer}");

        match renderer {
            RendererType::Html => Self::simulate_html_renderer(plan),
            RendererType::VanillaJs => Self::simulate_vanilla_js_renderer(plan),
            RendererType::Egui => Self::simulate_egui_renderer(plan),
            RendererType::Fltk => Self::simulate_fltk_renderer(plan),
        }
    }

    #[cfg(feature = "test-utils")]
    fn simulate_html_renderer(_plan: &TestPlan) -> TestResult {
        // TODO: Implement HTML renderer simulation
        // This would involve:
        // - Setting up a headless browser environment
        // - Loading the HyperChad HTML output
        // - Executing the test plan steps
        log::info!("HTML renderer simulation - placeholder implementation");
        TestResult::success()
    }

    #[cfg(feature = "test-utils")]
    fn simulate_vanilla_js_renderer(_plan: &TestPlan) -> TestResult {
        // TODO: Implement Vanilla JS renderer simulation
        // This would involve:
        // - Setting up a JavaScript runtime environment
        // - Loading the HyperChad Vanilla JS output
        // - Executing the test plan steps
        log::info!("Vanilla JS renderer simulation - placeholder implementation");
        TestResult::success()
    }

    #[cfg(feature = "test-utils")]
    fn simulate_egui_renderer(_plan: &TestPlan) -> TestResult {
        // TODO: Implement egui renderer simulation
        // This would involve:
        // - Setting up an egui context
        // - Running the HyperChad egui application
        // - Executing the test plan steps
        log::info!("egui renderer simulation - placeholder implementation");
        TestResult::success()
    }

    #[cfg(feature = "test-utils")]
    fn simulate_fltk_renderer(_plan: &TestPlan) -> TestResult {
        // TODO: Implement FLTK renderer simulation
        // This would involve:
        // - Setting up an FLTK application context
        // - Running the HyperChad FLTK application
        // - Executing the test plan steps
        log::info!("FLTK renderer simulation - placeholder implementation");
        TestResult::success()
    }
}

impl Default for HyperChadSimulator {
    /// Creates a new simulator with default configuration.
    ///
    /// Equivalent to calling [`HyperChadSimulator::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for HyperChadSimulator {
    /// Creates a clone of the simulator.
    ///
    /// All fields are cloned, including the shared web server reference.
    fn clone(&self) -> Self {
        Self {
            app_config: self.app_config.clone(),
            enabled_renderers: self.enabled_renderers.clone(),
            mock_data: self.mock_data.clone(),
            web_server: self.web_server.clone(),
        }
    }
}

#[cfg(feature = "test-utils")]
#[derive(Debug)]
struct HyperChadSimulationBootstrap {
    simulator: HyperChadSimulator,
    test_plan: TestPlan,
}

#[cfg(feature = "test-utils")]
impl SimBootstrap for HyperChadSimulationBootstrap {
    /// Initializes the simulation environment at startup.
    ///
    /// This method sets up the simulation by:
    /// * Starting a simulated web server host to serve the application
    /// * Creating client nodes for each enabled renderer type
    /// * Preparing the test plan for execution
    fn on_start(&self, sim: &mut impl Sim) {
        log::info!("Starting HyperChad simulation bootstrap");

        // Start simulated web server
        let simulator = self.simulator.clone();
        sim.host("hyperchad-server", move || {
            let simulator = simulator.clone();
            async move {
                if let Err(e) = simulator.start_simulation_server().await {
                    log::error!("Failed to start simulation server: {e}");
                    return Err(Box::new(e) as Box<dyn std::error::Error + Send>);
                }
                Ok(())
            }
        });

        // Start simulated clients for each renderer
        for renderer in &self.simulator.enabled_renderers {
            let renderer = *renderer;
            let test_plan = self.test_plan.clone();

            sim.client(format!("{renderer}-client"), async move {
                let result = HyperChadSimulator::simulate_renderer(renderer, &test_plan);
                log::info!("Renderer {renderer} simulation completed: {result:?}");
                Ok(())
            });
        }
    }

    /// Called on each simulation step.
    ///
    /// Currently a no-op, but can be extended to perform per-step actions
    /// such as updating simulation state or injecting events.
    fn on_step(&self, _sim: &mut impl Sim) {
        // Per-step actions can be added here if needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_simulator_creation() {
        let simulator = HyperChadSimulator::new()
            .with_renderer(RendererType::VanillaJs)
            .with_renderer(RendererType::Html);

        assert_eq!(simulator.enabled_renderers.len(), 2);
        assert!(
            simulator
                .enabled_renderers
                .contains(&RendererType::VanillaJs)
        );
        assert!(simulator.enabled_renderers.contains(&RendererType::Html));
    }

    #[test_log::test]
    fn test_renderer_type_display() {
        assert_eq!(RendererType::Html.to_string(), "html");
        assert_eq!(RendererType::VanillaJs.to_string(), "vanilla-js");
        assert_eq!(RendererType::Egui.to_string(), "egui");
        assert_eq!(RendererType::Fltk.to_string(), "fltk");
    }

    #[test_log::test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        assert_eq!(config.name, "test-app");
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.routes[0], "/");
        assert!(config.static_assets.is_empty());
        assert!(config.environment.is_empty());
    }

    #[test_log::test]
    fn test_simulation_data_default() {
        let data = SimulationData::default();
        assert!(data.users.is_empty());
        assert!(data.api_responses.is_empty());
        assert!(data.database_state.is_empty());
    }

    #[test_log::test]
    fn test_simulator_default() {
        let simulator = HyperChadSimulator::default();
        assert_eq!(simulator.app_config.name, "test-app");
        assert!(simulator.enabled_renderers.is_empty());
        assert!(simulator.mock_data.users.is_empty());
        assert!(simulator.web_server.is_none());
    }

    #[test_log::test]
    fn test_with_app_config() {
        let config = AppConfig {
            name: "my-app".to_string(),
            routes: vec!["/".to_string(), "/about".to_string()],
            static_assets: BTreeMap::new(),
            environment: BTreeMap::new(),
        };

        let simulator = HyperChadSimulator::new().with_app_config(config);

        assert_eq!(simulator.app_config.name, "my-app");
        assert_eq!(simulator.app_config.routes.len(), 2);
    }

    #[test_log::test]
    fn test_with_mock_data() {
        let mut api_responses = BTreeMap::new();
        api_responses.insert("/api/test".to_string(), serde_json::json!({"status": "ok"}));

        let data = SimulationData {
            users: vec![serde_json::json!({"id": 1, "name": "test"})],
            api_responses,
            database_state: BTreeMap::new(),
        };

        let simulator = HyperChadSimulator::new().with_mock_data(data);

        assert_eq!(simulator.mock_data.users.len(), 1);
        assert_eq!(simulator.mock_data.api_responses.len(), 1);
    }

    #[test_log::test]
    fn test_renderer_deduplication() {
        let simulator = HyperChadSimulator::new()
            .with_renderer(RendererType::Html)
            .with_renderer(RendererType::Html)
            .with_renderer(RendererType::VanillaJs)
            .with_renderer(RendererType::Html);

        assert_eq!(simulator.enabled_renderers.len(), 2);
        assert!(simulator.enabled_renderers.contains(&RendererType::Html));
        assert!(
            simulator
                .enabled_renderers
                .contains(&RendererType::VanillaJs)
        );
    }

    #[test_log::test]
    fn test_with_renderers_batch() {
        let renderers = vec![
            RendererType::Html,
            RendererType::VanillaJs,
            RendererType::Egui,
        ];

        let simulator = HyperChadSimulator::new().with_renderers(renderers);

        assert_eq!(simulator.enabled_renderers.len(), 3);
        assert!(simulator.enabled_renderers.contains(&RendererType::Html));
        assert!(
            simulator
                .enabled_renderers
                .contains(&RendererType::VanillaJs)
        );
        assert!(simulator.enabled_renderers.contains(&RendererType::Egui));
    }

    #[test_log::test]
    fn test_with_renderers_deduplication() {
        let renderers = vec![
            RendererType::Html,
            RendererType::Html,
            RendererType::VanillaJs,
        ];

        let simulator = HyperChadSimulator::new().with_renderers(renderers);

        assert_eq!(simulator.enabled_renderers.len(), 2);
    }

    #[test_log::test]
    fn test_simulator_clone() {
        let mut environment = BTreeMap::new();
        environment.insert("ENV".to_string(), "test".to_string());

        let config = AppConfig {
            name: "clone-test".to_string(),
            routes: vec!["/".to_string(), "/test".to_string()],
            static_assets: BTreeMap::new(),
            environment,
        };

        let mut api_responses = BTreeMap::new();
        api_responses.insert(
            "/api/data".to_string(),
            serde_json::json!({"data": "value"}),
        );

        let data = SimulationData {
            users: vec![serde_json::json!({"id": 42})],
            api_responses,
            database_state: BTreeMap::new(),
        };

        let simulator = HyperChadSimulator::new()
            .with_app_config(config)
            .with_renderer(RendererType::Html)
            .with_renderer(RendererType::VanillaJs)
            .with_mock_data(data);

        let cloned = simulator.clone();

        // Verify all fields are cloned correctly
        assert_eq!(cloned.app_config.name, simulator.app_config.name);
        assert_eq!(cloned.app_config.routes, simulator.app_config.routes);
        assert_eq!(
            cloned.app_config.environment,
            simulator.app_config.environment
        );
        assert_eq!(cloned.enabled_renderers, simulator.enabled_renderers);
        assert_eq!(
            cloned.mock_data.users.len(),
            simulator.mock_data.users.len()
        );
        assert_eq!(
            cloned.mock_data.api_responses.len(),
            simulator.mock_data.api_responses.len()
        );
    }

    #[test_log::test]
    fn test_renderer_type_equality() {
        assert_eq!(RendererType::Html, RendererType::Html);
        assert_eq!(RendererType::VanillaJs, RendererType::VanillaJs);
        assert_eq!(RendererType::Egui, RendererType::Egui);
        assert_eq!(RendererType::Fltk, RendererType::Fltk);

        assert_ne!(RendererType::Html, RendererType::VanillaJs);
        assert_ne!(RendererType::Egui, RendererType::Fltk);
    }

    #[test_log::test]
    fn test_simulator_error_display() {
        let error = SimulatorError::SimulationFailed("test failure".to_string());
        assert_eq!(error.to_string(), "Simulation failed: test failure");

        let error = SimulatorError::UnsupportedRenderer(RendererType::Html);
        assert_eq!(error.to_string(), "Renderer not supported: Html");

        let error = SimulatorError::TestPlanFailed("plan error".to_string());
        assert_eq!(error.to_string(), "Test plan execution failed: plan error");
    }

    #[test_log::test]
    fn test_renderer_type_serialization() {
        let html = RendererType::Html;
        let serialized = serde_json::to_string(&html).unwrap();
        let deserialized: RendererType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(html, deserialized);

        let vanilla_js = RendererType::VanillaJs;
        let serialized = serde_json::to_string(&vanilla_js).unwrap();
        let deserialized: RendererType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(vanilla_js, deserialized);
    }

    #[test_log::test]
    fn test_app_config_with_static_assets() {
        let mut static_assets = BTreeMap::new();
        static_assets.insert("/style.css".to_string(), "body { margin: 0; }".to_string());
        static_assets.insert("/script.js".to_string(), "console.log('test');".to_string());

        let config = AppConfig {
            name: "asset-test".to_string(),
            routes: vec!["/".to_string()],
            static_assets,
            environment: BTreeMap::new(),
        };

        let simulator = HyperChadSimulator::new().with_app_config(config);

        assert_eq!(simulator.app_config.static_assets.len(), 2);
        assert!(
            simulator
                .app_config
                .static_assets
                .contains_key("/style.css")
        );
        assert!(
            simulator
                .app_config
                .static_assets
                .contains_key("/script.js")
        );
    }

    #[test_log::test]
    fn test_simulation_data_with_database_state() {
        let mut database_state = BTreeMap::new();
        database_state.insert("users:1".to_string(), serde_json::json!({"name": "Alice"}));
        database_state.insert("users:2".to_string(), serde_json::json!({"name": "Bob"}));

        let data = SimulationData {
            users: Vec::new(),
            api_responses: BTreeMap::new(),
            database_state,
        };

        let simulator = HyperChadSimulator::new().with_mock_data(data);

        assert_eq!(simulator.mock_data.database_state.len(), 2);
        assert!(simulator.mock_data.database_state.contains_key("users:1"));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_start_simulation_server_without_web_server() {
        let simulator = HyperChadSimulator::new();

        // When no web server is configured, start_simulation_server should succeed
        // without doing anything (no-op path)
        let result = simulator.start_simulation_server().await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_start_simulation_server_with_web_server() {
        let web_server = Arc::new(SimulationWebServer::new());
        let simulator = HyperChadSimulator::new().with_web_server(Arc::clone(&web_server));

        // Before starting, server should not be running
        assert!(!web_server.is_running().await);

        // Start the simulation server
        let result = simulator.start_simulation_server().await;
        assert!(result.is_ok());

        // After starting, the web server should be running
        assert!(web_server.is_running().await);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_clone_shares_web_server() {
        let web_server = Arc::new(SimulationWebServer::new());
        let simulator = HyperChadSimulator::new().with_web_server(Arc::clone(&web_server));
        let cloned = simulator.clone();

        // Start simulation on the cloned simulator
        cloned.start_simulation_server().await.unwrap();

        // The original simulator's web server reference should also see it as running
        // since they share the same Arc
        assert!(web_server.is_running().await);
    }
}
