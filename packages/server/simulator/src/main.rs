#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{process::ExitCode, sync::LazyLock};

use moosicbox_server_simulator::{client, handle_actions, host};
use simvar::{Sim, SimBootstrap, run_simulation};

/// Optional port number from environment variable for the `MoosicBox` server.
///
/// # Panics
///
/// * If `PORT` environment variable is set but cannot be parsed as a `u16`
static PORT: LazyLock<Option<u16>> =
    LazyLock::new(|| switchy_env::var_parse_opt("PORT").expect("Invalid PORT env var"));

/// Main simulator bootstrap for running the `MoosicBox` server simulation.
///
/// Implements `SimBootstrap` to coordinate the server and client components.
pub struct Simulator;

impl SimBootstrap for Simulator {
    fn on_start(&self, sim: &mut impl Sim) {
        host::moosicbox_server::start(sim, *PORT);

        client::health_checker::start(sim);
        client::fault_injector::start(sim);
    }

    fn on_step(&self, sim: &mut impl Sim) {
        handle_actions(sim);
    }
}

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let results = run_simulation(Simulator)?;

    if results.iter().any(|x| !x.is_success()) {
        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}
