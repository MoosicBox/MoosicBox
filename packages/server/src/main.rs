//! `MoosicBox` server binary entry point.
//!
//! This is the main executable that starts the `MoosicBox` server with all features enabled
//! based on compile-time feature flags. It configures logging, telemetry, and runtime settings
//! before launching the server.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_config::AppType;
use moosicbox_logging::free_log_client::DynLayer;
use switchy_env::{var_or, var_parse_opt, var_parse_or};

#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let addr = var_or("BIND_ADDR", "0.0.0.0");
    let service_port = if args.len() > 1 {
        args[1].parse::<u16>().expect("Invalid port argument")
    } else {
        var_parse_or("PORT", 8000usize)
            .try_into()
            .expect("Invalid PORT environment variable")
    };
    let actix_workers = var_parse_opt::<usize>("ACTIX_WORKERS")
        .map_err(|e| std::io::Error::other(format!("Invalid ACTIX_WORKERS: {e:?}")))?;

    actix_web::rt::System::with_tokio_rt(|| {
        let threads = var_parse_or("MAX_THREADS", 64usize);
        log::debug!("Running with {threads} max blocking threads");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(threads)
            .build()
            .unwrap()
    })
    .block_on(async move {
        let mut layers = vec![];

        if matches!(
            switchy_env::var("TOKIO_CONSOLE").as_deref(),
            Ok("1" | "true")
        ) {
            layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
        }

        #[cfg(feature = "telemetry")]
        layers.push(
            switchy_telemetry::init_tracer(env!("CARGO_PKG_NAME"))
                .map_err(std::io::Error::other)?,
        );

        moosicbox_logging::init(Some("moosicbox_server.log"), Some(layers))
            .expect("Failed to initialize FreeLog");

        #[cfg(feature = "telemetry")]
        let request_metrics = std::sync::Arc::new(switchy_telemetry::get_http_metrics_handler());

        moosicbox_server::run(
            AppType::Server,
            &addr,
            service_port,
            actix_workers,
            None,
            #[cfg(feature = "player")]
            true,
            #[cfg(feature = "upnp")]
            true,
            #[cfg(feature = "telemetry")]
            request_metrics,
            |_| {},
        )
        .await?;

        Ok(())
    })
}
