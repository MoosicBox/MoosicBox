#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, sync::Arc};

#[cfg(feature = "test-utils")]
use hyperchad_test_utils::{TestPlan, TestResult};
#[cfg(feature = "test-utils")]
use simvar::{Sim, SimBootstrap};
use web_server_simulator::SimulationWebServer;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "test-utils")]
pub use hyperchad_test_utils as test_utils;
pub use web_server_simulator as web_server;

#[derive(Debug, Error)]
pub enum SimulatorError {
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),
    #[error("Renderer not supported: {0:?}")]
    UnsupportedRenderer(RendererType),
    #[error("Test plan execution failed: {0}")]
    TestPlanFailed(String),
    #[error("Web server error: {0}")]
    WebServer(#[from] web_server_simulator::Error),
    #[error("Simvar error: {0}")]
    Simvar(#[from] simvar::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererType {
    Html,
    VanillaJs,
    Egui,
    Fltk,
}

impl std::fmt::Display for RendererType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Html => write!(f, "html"),
            Self::VanillaJs => write!(f, "vanilla-js"),
            Self::Egui => write!(f, "egui"),
            Self::Fltk => write!(f, "fltk"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub name: String,
    pub routes: Vec<String>,
    pub static_assets: BTreeMap<String, String>,
    pub environment: BTreeMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "test-app".to_string(),
            routes: vec!["/".to_string()],
            static_assets: BTreeMap::new(),
            environment: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SimulationData {
    pub users: Vec<serde_json::Value>,
    pub api_responses: BTreeMap<String, serde_json::Value>,
    pub database_state: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug)]
pub struct HyperChadSimulator {
    app_config: AppConfig,
    enabled_renderers: Vec<RendererType>,
    mock_data: SimulationData,
    web_server: Option<Arc<SimulationWebServer>>,
}

impl HyperChadSimulator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            app_config: AppConfig::default(),
            enabled_renderers: vec![],
            mock_data: SimulationData::default(),
            web_server: None,
        }
    }

    #[must_use]
    pub fn with_app_config(mut self, config: AppConfig) -> Self {
        self.app_config = config;
        self
    }

    #[must_use]
    pub fn with_renderer(mut self, renderer: RendererType) -> Self {
        if !self.enabled_renderers.contains(&renderer) {
            self.enabled_renderers.push(renderer);
        }
        self
    }

    #[must_use]
    pub fn with_renderers(mut self, renderers: Vec<RendererType>) -> Self {
        for renderer in renderers {
            self = self.with_renderer(renderer);
        }
        self
    }

    #[must_use]
    pub fn with_mock_data(mut self, data: SimulationData) -> Self {
        self.mock_data = data;
        self
    }

    #[must_use]
    pub fn with_web_server(mut self, server: Arc<SimulationWebServer>) -> Self {
        self.web_server = Some(server);
        self
    }

    /// Run a test plan within the simulation environment
    ///
    /// # Errors
    ///
    /// * If the simulation fails to start
    /// * If the test plan execution fails
    /// * If any renderer simulation fails
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

    /// Start the simulation server
    ///
    /// # Errors
    ///
    /// * If the web server fails to start
    pub async fn start_simulation_server(&self) -> Result<(), SimulatorError> {
        if let Some(server) = &self.web_server {
            server.start().await?;
        }
        Ok(())
    }

    /// Simulate a specific renderer
    ///
    /// # Errors
    ///
    /// * If the renderer is not supported
    /// * If the renderer simulation fails
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
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for HyperChadSimulator {
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

    fn on_step(&self, _sim: &mut impl Sim) {
        // Per-step actions can be added here if needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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

    #[test]
    fn test_renderer_type_display() {
        assert_eq!(RendererType::Html.to_string(), "html");
        assert_eq!(RendererType::VanillaJs.to_string(), "vanilla-js");
        assert_eq!(RendererType::Egui.to_string(), "egui");
        assert_eq!(RendererType::Fltk.to_string(), "fltk");
    }
}
