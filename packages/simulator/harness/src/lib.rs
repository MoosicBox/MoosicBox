#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    any::Any,
    panic::AssertUnwindSafe,
    time::{Duration, SystemTime},
};

use formatting::TimeFormat as _;
use moosicbox_simulator_utils::{
    SEED, SIMULATOR_CANCELLATION_TOKEN, STEP, cancel_simulation, duration,
};
use turmoil::Sim;

pub use getrandom;
#[cfg(feature = "database")]
pub use moosicbox_database_connection as database_connection;
#[cfg(feature = "http")]
pub use moosicbox_http as http;
#[cfg(feature = "mdns")]
pub use moosicbox_mdns as mdns;
#[cfg(feature = "random")]
pub use moosicbox_random as random;
pub use moosicbox_simulator_utils as utils;
#[cfg(feature = "tcp")]
pub use moosicbox_tcp as tcp;
#[cfg(feature = "telemetry")]
pub use moosicbox_telemetry as telemetry;
#[cfg(feature = "time")]
pub use moosicbox_time as time;
#[cfg(feature = "upnp")]
pub use moosicbox_upnp as upnp;
pub use rand;
pub use turmoil;

mod formatting;
pub mod plan;

fn run_info() -> String {
    #[cfg(feature = "time")]
    let extra = {
        use moosicbox_time::simulator::{EPOCH_OFFSET, STEP_MULTIPLIER};

        format!(
            "\n\
            epoch_offset={epoch_offset}\n\
            step_multiplier={step_multiplier}",
            epoch_offset = *EPOCH_OFFSET,
            step_multiplier = *STEP_MULTIPLIER,
        )
    };
    #[cfg(not(feature = "time"))]
    let extra = String::new();

    format!("seed={seed}{extra}", seed = *SEED)
}

#[allow(clippy::cast_precision_loss)]
fn run_info_end(successful: bool, real_time_millis: u128, sim_time_millis: u128) -> String {
    format!(
        "\
        {run_info}\n\
        successful={successful}\n\
        real_time_elapsed={real_time}\n\
        simulated_time_elapsed={simulated_time} ({simulated_time_x:.2}x)",
        run_info = run_info(),
        real_time = real_time_millis.into_formatted(),
        simulated_time = sim_time_millis.into_formatted(),
        simulated_time_x = sim_time_millis as f64 / real_time_millis as f64,
    )
}

/// # Panics
///
/// * If system time went backwards
///
/// # Errors
///
/// * The contents of this function are wrapped in a `catch_unwind` call, so if
///   any panic happens, it will be wrapped into an error on the outer `Result`
/// * If the `Sim` `step` returns an error, we return that in an Ok(Err(e))
pub fn run_simulation(
    bootstrap: &impl SimBootstrap,
) -> Result<Result<(), Box<dyn std::error::Error>>, Box<dyn Any + Send>> {
    ctrlc::set_handler(cancel_simulation).expect("Error setting Ctrl-C handler");

    let duration_secs = duration();

    STEP.store(1, std::sync::atomic::Ordering::SeqCst);

    log::info!("Server simulator starting\n{}", run_info());

    bootstrap.init();

    let builder = bootstrap.build_sim(sim_builder());
    #[cfg(feature = "random")]
    let mut sim = builder.build_with_rng(Box::new(moosicbox_random::RNG.clone()));
    #[cfg(not(feature = "random"))]
    let mut sim = builder.build();

    let start = SystemTime::now();

    bootstrap.on_start(&mut sim);

    let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let print_step = |sim: &Sim<'_>, step| {
            #[allow(clippy::cast_precision_loss)]
            if duration_secs < u64::MAX {
                log::info!(
                    "step {step} ({}) ({:.1}%)",
                    sim.elapsed().as_millis().into_formatted(),
                    SystemTime::now().duration_since(start).unwrap().as_millis() as f64
                        / (duration_secs as f64 * 1000.0)
                        * 100.0,
                );
            } else {
                log::info!(
                    "step {step} ({})",
                    sim.elapsed().as_millis().into_formatted()
                );
            }
        };

        while !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
            let step = STEP.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            if duration_secs < u64::MAX
                && SystemTime::now().duration_since(start).unwrap().as_secs() >= duration_secs
            {
                print_step(&sim, step);
                break;
            }

            if step % 1000 == 0 {
                print_step(&sim, step);
            }

            bootstrap.on_step(&mut sim);

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
        }

        if !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
            cancel_simulation();
        }

        Ok(())
    }));

    bootstrap.on_end(&mut sim);

    let end = SystemTime::now();
    let real_time_millis = end.duration_since(start).unwrap().as_millis();
    let sim_time_millis = sim.elapsed().as_millis();

    log::info!(
        "Server simulator finished\n{}",
        run_info_end(
            resp.as_ref().is_ok_and(Result::is_ok),
            real_time_millis,
            sim_time_millis,
        )
    );

    resp
}
fn sim_builder() -> turmoil::Builder {
    let mut builder = turmoil::Builder::new();

    builder.simulation_duration(Duration::MAX);

    #[cfg(feature = "time")]
    builder.tick_duration(Duration::from_millis(
        *moosicbox_time::simulator::STEP_MULTIPLIER,
    ));

    builder
}

pub trait SimBootstrap {
    #[must_use]
    fn build_sim(&self, builder: turmoil::Builder) -> turmoil::Builder {
        builder
    }

    fn init(&self) {}

    fn on_start(&self, #[allow(unused)] sim: &mut Sim<'_>) {}

    fn on_step(&self, #[allow(unused)] sim: &mut Sim<'_>) {}

    fn on_end(&self, #[allow(unused)] sim: &mut Sim<'_>) {}
}
