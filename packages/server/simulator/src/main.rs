#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    sync::{LazyLock, atomic::AtomicU32},
    time::Duration,
};

use moosicbox_server_simulator::{RNG, SIMULATOR_CANCELLATION_TOKEN, client, handle_actions, host};
use moosicbox_simulator_harness::turmoil::{self};
use moosicbox_simulator_utils::SEED;

static STEP: LazyLock<AtomicU32> = LazyLock::new(|| AtomicU32::new(1));

pub trait TimeFormat {
    fn into_formatted(self) -> String;
}

impl TimeFormat for u32 {
    fn into_formatted(self) -> String {
        u128::from(self).into_formatted()
    }
}

impl TimeFormat for u64 {
    fn into_formatted(self) -> String {
        u128::from(self).into_formatted()
    }
}

impl TimeFormat for u128 {
    fn into_formatted(self) -> String {
        #[must_use]
        const fn plural(num: u128) -> &'static str {
            if num == 1 { "" } else { "s" }
        }

        let years = self / 365 / 24 / 60 / 60 / 1000;
        let days = self / 24 / 60 / 60 / 1000 % 365;
        let hours = self / 60 / 60 / 1000 % 24;
        let minutes = self / 60 / 1000 % 60;
        let seconds = self / 1000 % 60;
        let ms = self % 1000;

        if years > 0 {
            format!(
                "{years} year{}, {days} day{}, {hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(years),
                plural(days),
                plural(hours),
                plural(minutes),
            )
        } else if days > 0 {
            format!(
                "{days} day{}, {hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(days),
                plural(hours),
                plural(minutes),
            )
        } else if hours > 0 {
            format!(
                "{hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(hours),
                plural(minutes),
            )
        } else if minutes > 0 {
            format!("{minutes} minute{}, {seconds}s, {ms}ms", plural(minutes))
        } else if seconds > 0 {
            format!("{seconds}s, {ms}ms")
        } else {
            format!("{ms}ms")
        }
    }
}

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

    let start = std::time::SystemTime::now();

    let resp = std::panic::catch_unwind(|| run_simulation(duration_secs));
    let step = STEP.load(std::sync::atomic::Ordering::SeqCst);

    let end = std::time::SystemTime::now();
    let real_time_millis = end.duration_since(start).unwrap().as_millis();

    log::info!(
        "Server simulator finished\n\
        seed={seed}\n\
        successful={successful}\n\
        real_time_elapsed={real_time}\n\
        simulated_time_elapsed={simulated_time}",
        successful = resp.as_ref().is_ok_and(Result::is_ok),
        real_time = real_time_millis.into_formatted(),
        simulated_time = step.into_formatted(),
    );

    resp.unwrap()
}

fn run_simulation(duration_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut sim = turmoil::Builder::new()
        .simulation_duration(Duration::from_secs(duration_secs))
        .build_with_rng(Box::new(RNG.lock().unwrap().clone()));

    let service_port = std::env::var("PORT")
        .ok()
        .map(|x| x.parse::<u16>().expect("Invalid PORT env var"))
        .map(TryInto::try_into)
        .transpose()?;

    host::moosicbox_server::start(&mut sim, service_port);

    client::health_checker::start(&mut sim);
    client::fault_injector::start(&mut sim);
    client::healer::start(&mut sim);

    while !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
        let step = STEP.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

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
    }

    if !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
        SIMULATOR_CANCELLATION_TOKEN.cancel();
    }

    Ok(())
}
