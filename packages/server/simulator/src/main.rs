#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_config::AppType;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};

fn main() -> std::io::Result<()> {
    unsafe {
        moosicbox_simulator_harness::init();
    }

    let addr = default_env("BIND_ADDR", "0.0.0.0");
    let service_port = default_env_usize("PORT", 8000)
        .unwrap_or(8000)
        .try_into()
        .expect("Invalid PORT environment variable");
    let actix_workers = option_env_usize("ACTIX_WORKERS")
        .map_err(|e| std::io::Error::other(format!("Invalid ACTIX_WORKERS: {e:?}")))?;

    actix_web::rt::System::with_tokio_rt(|| {
        let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
        log::debug!("Running with {threads} max blocking threads");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(threads)
            .build()
            .unwrap()
    })
    .block_on(async move {
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
        .await
    })?;

    log::info!("Server simulator finished");

    Ok(())
}
