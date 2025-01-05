#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use moosicbox_config::AppType;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_logging::free_log_client::DynLayer;

#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let addr = default_env("BIND_ADDR", "0.0.0.0");
    let service_port = if args.len() > 1 {
        args[1].parse::<u16>().expect("Invalid port argument")
    } else {
        default_env_usize("PORT", 8000)
            .unwrap_or(8000)
            .try_into()
            .expect("Invalid PORT environment variable")
    };
    let actix_workers = option_env_usize("ACTIX_WORKERS").map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Invalid ACTIX_WORKERS: {e:?}"),
        )
    })?;

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
        let mut layers = vec![];

        if std::env::var("TOKIO_CONSOLE") == Ok("1".to_string()) {
            layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
        }

        #[cfg(feature = "telemetry")]
        layers.push(
            moosicbox_telemetry::init_tracer(env!("CARGO_PKG_NAME"))
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
        );

        moosicbox_logging::init(Some("moosicbox_server.log"), Some(layers))
            .expect("Failed to initialize FreeLog");

        #[cfg(feature = "telemetry")]
        let otel = std::sync::Arc::new(
            moosicbox_telemetry::Otel::new()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
        );

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
    })
}
