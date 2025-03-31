#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_config::AppType;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_simulator_harness::sim_buider;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        moosicbox_simulator_harness::init();
    }

    moosicbox_logging::init(None, None)?;

    let mut sim = sim_buider().build();

    sim.host("moosicbox", || async {
        let addr = default_env("BIND_ADDR", "0.0.0.0");
        let service_port = default_env_usize("PORT", 8000)
            .unwrap_or(8000)
            .try_into()
            .expect("Invalid PORT environment variable");
        let actix_workers = option_env_usize("ACTIX_WORKERS")
            .map_err(|e| std::io::Error::other(format!("Invalid ACTIX_WORKERS: {e:?}")))?;
        #[cfg(feature = "telemetry")]
        let otel =
            std::sync::Arc::new(moosicbox_telemetry::Otel::new().map_err(std::io::Error::other)?);

        moosicbox_server::run(
            AppType::Server,
            &addr,
            service_port,
            actix_workers,
            #[cfg(feature = "player")]
            true,
            #[cfg(feature = "upnp")]
            true,
            #[cfg(feature = "telemetry")]
            otel,
            || {},
        )
        .await?;

        Ok(())
    });

    sim.client("client", async { Ok(()) });

    let result = sim.run();

    log::info!("Server simulator finished");

    result
}
