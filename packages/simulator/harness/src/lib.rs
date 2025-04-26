#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, LazyLock, Mutex, atomic::AtomicBool},
    time::{Duration, SystemTime},
};

use formatting::TimeFormat as _;
use moosicbox_simulator_utils::{
    cancel_simulation, duration, reset_simulator_cancellation_token, reset_step,
    simulator_cancellation_token, step_next,
};
use turmoil::Sim;

pub use moosicbox_simulator_utils as utils;
pub use turmoil;

#[cfg(feature = "database")]
pub use moosicbox_database_connection as database_connection;
#[cfg(feature = "fs")]
pub use moosicbox_fs as fs;
#[cfg(feature = "http")]
pub use moosicbox_http as http;
#[cfg(feature = "mdns")]
pub use moosicbox_mdns as mdns;
#[cfg(feature = "random")]
pub use moosicbox_random as random;
#[cfg(feature = "tcp")]
pub use moosicbox_tcp as tcp;
#[cfg(feature = "telemetry")]
pub use moosicbox_telemetry as telemetry;
#[cfg(feature = "time")]
pub use moosicbox_time as time;
#[cfg(feature = "upnp")]
pub use moosicbox_upnp as upnp;

mod formatting;
pub mod plan;

static RUNS: LazyLock<u64> = LazyLock::new(|| {
    let runs = moosicbox_random::RNG.gen_range(1..1000u64);
    let runs = if duration() < u64::MAX && !moosicbox_random::simulator::contains_fixed_seed() {
        runs
    } else {
        1
    };

    std::env::var("SIMULATOR_RUNS")
        .ok()
        .map_or(runs, |x| x.parse::<u64>().unwrap())
});

fn run_info(run_index: u64, props: &[(String, String)]) -> String {
    #[cfg(feature = "time")]
    let extra = {
        use moosicbox_time::simulator::{epoch_offset, step_multiplier};

        format!(
            "\n\
            epoch_offset={epoch_offset}\n\
            step_multiplier={step_multiplier}",
            epoch_offset = epoch_offset(),
            step_multiplier = step_multiplier(),
        )
    };
    #[cfg(not(feature = "time"))]
    let extra = String::new();

    let mut props_str = String::new();
    for (k, v) in props {
        use std::fmt::Write as _;

        write!(props_str, "\n{k}={v}").unwrap();
    }

    let runs = *RUNS;
    let runs = if runs > 1 {
        format!("{run_index}/{runs}")
    } else {
        runs.to_string()
    };

    format!(
        "\
        seed={seed}\n\
        runs={runs}\
        {extra}{props_str}",
        seed = moosicbox_random::simulator::seed(),
    )
}

fn get_cargoified_args() -> Vec<String> {
    let mut args = std::env::args().collect::<Vec<_>>();

    let Some(cmd) = args.first() else {
        return args;
    };

    let mut components = cmd.split('/');

    if matches!(components.next(), Some("target")) {
        let Some(profile) = components.next() else {
            return args;
        };
        let profile = profile.to_string();

        let Some(binary_name) = components.next() else {
            return args;
        };
        let binary_name = binary_name.to_string();

        args.remove(0);
        args.insert(0, binary_name);
        args.insert(0, "-p".to_string());

        if profile == "release" {
            args.insert(0, "--release".to_string());
        } else if profile != "debug" {
            args.insert(0, profile);
            args.insert(0, "--profile".to_string());
        }

        args.insert(0, "run".to_string());
        args.insert(0, "cargo".to_string());
    }

    args
}

fn get_run_command(skip_env: &[&str], seed: u64) -> String {
    let args = get_cargoified_args();
    let quoted_args = args
        .iter()
        .map(|x| shell_words::quote(x.as_str()))
        .collect::<Vec<_>>();
    let cmd = quoted_args.join(" ");

    let mut env_vars = String::new();

    for (name, value) in std::env::vars() {
        use std::fmt::Write as _;

        if !name.starts_with("SIMULATOR_") && name != "RUST_LOG" {
            continue;
        }
        if skip_env.iter().any(|x| *x == name) {
            continue;
        }

        write!(env_vars, "{name}={} ", shell_words::quote(value.as_str())).unwrap();
    }

    format!("SIMULATOR_SEED={seed} {env_vars}{cmd}")
}

#[allow(clippy::cast_precision_loss)]
fn run_info_end(
    run_index: u64,
    props: &[(String, String)],
    successful: bool,
    real_time_millis: u128,
    sim_time_millis: u128,
) -> String {
    let run_from_seed = if *RUNS == 1 && moosicbox_random::simulator::contains_fixed_seed() {
        String::new()
    } else {
        let cmd = get_run_command(
            &["SIMULATOR_SEED", "SIMULATOR_RUNS", "SIMULATOR_DURATION"],
            moosicbox_random::simulator::seed(),
        );
        format!("\n\nTo run again with this seed: `{cmd}`")
    };
    let run_from_start = if !moosicbox_random::simulator::contains_fixed_seed() && *RUNS > 1 {
        let cmd = get_run_command(
            &["SIMULATOR_SEED"],
            moosicbox_random::simulator::initial_seed(),
        );
        format!("\nTo run entire simulation again from the first run: `{cmd}`")
    } else {
        String::new()
    };
    format!(
        "\
        {run_info}\n\
        successful={successful}\n\
        real_time_elapsed={real_time}\n\
        simulated_time_elapsed={simulated_time} ({simulated_time_x:.2}x)\
        {run_from_seed}{run_from_start}",
        run_info = run_info(run_index, props),
        real_time = real_time_millis.into_formatted(),
        simulated_time = sim_time_millis.into_formatted(),
        simulated_time_x = sim_time_millis as f64 / real_time_millis as f64,
    )
}

static END_SIM: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

pub fn end_sim() {
    END_SIM.store(true, std::sync::atomic::Ordering::SeqCst);

    if !simulator_cancellation_token().is_cancelled() {
        cancel_simulation();
    }
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
#[allow(clippy::too_many_lines)]
pub fn run_simulation(bootstrap: &impl SimBootstrap) -> Result<(), Box<dyn std::error::Error>> {
    ctrlc::set_handler(end_sim).expect("Error setting Ctrl-C handler");

    let panic = Arc::new(Mutex::new(None));
    std::panic::set_hook(Box::new({
        let prev_hook = std::panic::take_hook();
        let panic = panic.clone();
        move |x| {
            *panic.lock().unwrap() = Some(x.to_string());
            end_sim();
            prev_hook(x);
        }
    }));

    let runs = *RUNS;

    for run_index in 1..=runs {
        moosicbox_random::simulator::reset_rng();
        #[cfg(feature = "fs")]
        moosicbox_fs::simulator::reset_fs();
        #[cfg(feature = "time")]
        moosicbox_time::simulator::reset_epoch_offset();
        #[cfg(feature = "time")]
        moosicbox_time::simulator::reset_step_multiplier();
        reset_simulator_cancellation_token();
        reset_step();

        let duration_secs = duration();

        bootstrap.init();

        let builder = bootstrap.build_sim(sim_builder());
        #[cfg(feature = "random")]
        let mut sim = builder.build_with_rng(Box::new(moosicbox_random::RNG.clone()));
        #[cfg(not(feature = "random"))]
        let mut sim = builder.build();

        let props = bootstrap.props();

        println!(
            "\n\
            =========================== START ============================\n\
            Server simulator starting\n{}\n\
            ==============================================================\n",
            run_info(run_index, &props)
        );

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

            while !simulator_cancellation_token().is_cancelled() {
                let step = step_next();

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

            if !simulator_cancellation_token().is_cancelled() {
                cancel_simulation();
            }

            Ok(())
        }));

        bootstrap.on_end(&mut sim);

        let end = SystemTime::now();
        let real_time_millis = end.duration_since(start).unwrap().as_millis();
        let sim_time_millis = sim.elapsed().as_millis();

        println!(
            "\n\
            =========================== FINISH ===========================\n\
            Server simulator finished\n{}\n\
            ==============================================================",
            run_info_end(
                run_index,
                &props,
                resp.as_ref().is_ok_and(Result::is_ok),
                real_time_millis,
                sim_time_millis,
            )
        );

        if let Some(panic) = &*panic.lock().unwrap() {
            eprintln!("{panic}");
        }

        resp.unwrap()?;

        if END_SIM.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        moosicbox_random::simulator::reset_seed();
    }

    Ok(())
}

fn sim_builder() -> turmoil::Builder {
    let mut builder = turmoil::Builder::new();

    builder
        .fail_rate(0.0)
        .repair_rate(1.0)
        .simulation_duration(Duration::MAX);

    #[cfg(feature = "time")]
    builder.tick_duration(Duration::from_millis(
        moosicbox_time::simulator::step_multiplier(),
    ));

    builder
}

pub trait SimBootstrap {
    #[must_use]
    fn props(&self) -> Vec<(String, String)> {
        vec![]
    }

    #[must_use]
    fn build_sim(&self, builder: turmoil::Builder) -> turmoil::Builder {
        builder
    }

    fn init(&self) {}

    fn on_start(&self, #[allow(unused)] sim: &mut Sim<'_>) {}

    fn on_step(&self, #[allow(unused)] sim: &mut Sim<'_>) {}

    fn on_end(&self, #[allow(unused)] sim: &mut Sim<'_>) {}
}

pub trait CancellableSim {
    fn client_until_cancelled(
        &mut self,
        name: &str,
        action: impl Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    );
}

impl CancellableSim for Sim<'_> {
    fn client_until_cancelled(
        &mut self,
        name: &str,
        action: impl Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
    ) {
        client_until_cancelled(self, name, action);
    }
}

pub fn client_until_cancelled(
    sim: &mut Sim<'_>,
    name: &str,
    action: impl Future<Output = Result<(), Box<dyn std::error::Error>>> + 'static,
) {
    sim.client(name, async move {
        simulator_cancellation_token()
            .run_until_cancelled(action)
            .await
            .transpose()?;

        Ok(())
    });
}
