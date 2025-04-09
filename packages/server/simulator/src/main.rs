#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::time::Duration;

use moosicbox_env_utils::default_env_usize;
use moosicbox_server_simulator::{
    RNG, SEED, SIMULATOR_CANCELLATION_TOKEN, client, handle_actions, host,
};
use moosicbox_simulator_harness::turmoil::{self};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        moosicbox_simulator_harness::init();
    }

    ctrlc::set_handler(move || SIMULATOR_CANCELLATION_TOKEN.cancel())
        .expect("Error setting Ctrl-C handler");

    let duration_secs = std::env::var("SIMULATOR_DURATION")
        .ok()
        .map_or(u64::MAX, |x| x.parse::<u64>().unwrap());

    let seed = *SEED;

    println!("Starting simulation with seed={seed}");

    moosicbox_logging::init(None, None)?;

    let resp = run_simulation(duration_secs);

    log::info!("Server simulator finished (seed={seed})");

    resp
}

fn run_simulation(duration_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut sim = turmoil::Builder::new()
        .simulation_duration(Duration::from_secs(duration_secs))
        .build_with_rng(Box::new(RNG.lock().unwrap().clone()));

    let service_port = default_env_usize("PORT", 8000)
        .unwrap_or(8000)
        .try_into()
        .expect("Invalid PORT environment variable");

    host::moosicbox_server::start(&mut sim, service_port);

    client::health_checker::start(&mut sim);
    client::fault_injector::start(&mut sim);
    client::healer::start(&mut sim);

    let mut step = 1;

    while !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
        if step % 1000 == 0 {
            #[allow(clippy::cast_precision_loss)]
            if duration_secs < u64::MAX {
                log::info!(
                    "step {step} ({:.1}%)",
                    (f64::from(step) / duration_secs as f64 / 10.0)
                );
            } else {
                log::info!("step {step}");
            }
        }

        handle_actions(&mut sim);

        match sim.step() {
            Ok(..) => {}
            Err(e) => {
                let message = e.to_string();
                if message.starts_with("Ran for duration: ")
                    && message.ends_with(" without completing")
                {
                    break;
                }
                return Err(e);
            }
        }

        step += 1;
    }

    if !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
        SIMULATOR_CANCELLATION_TOKEN.cancel();
    }

    Ok(())
}
