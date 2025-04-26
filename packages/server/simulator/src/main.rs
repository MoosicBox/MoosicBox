#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::LazyLock;

use moosicbox_server_simulator::{client, handle_actions, host};
use moosicbox_simulator_harness::{SimBootstrap, run_simulation, turmoil::Sim};

static PORT: LazyLock<Option<u16>> = LazyLock::new(|| {
    std::env::var("PORT")
        .ok()
        .map(|x| x.parse::<u16>().expect("Invalid PORT env var"))
        .map(TryInto::try_into)
        .transpose()
        .unwrap()
});

pub struct Simulator;

impl SimBootstrap for Simulator {
    fn on_start(&self, sim: &mut Sim<'_>) {
        host::moosicbox_server::start(sim, *PORT);

        client::health_checker::start(sim);
        client::fault_injector::start(sim);
    }

    fn on_step(&self, sim: &mut Sim<'_>) {
        handle_actions(sim);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None, None)?;

    run_simulation(&Simulator)
}
