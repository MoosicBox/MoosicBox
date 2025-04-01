#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_config::AppType;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_simulator_harness::turmoil;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        moosicbox_simulator_harness::init();
    }

    moosicbox_logging::init(None, None)?;

    let mut sim = turmoil::Builder::new().build();

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
            || {
                moosicbox_task::spawn("simulation TCP listener", async {
                    let listener = turmoil::net::TcpListener::bind("moosicbox:1234")
                        .await
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

                    while let Ok((_, _addr)) = listener.accept().await {
                        println!("[Server] Received connection!");
                    }

                    Ok::<_, Box<dyn std::error::Error + Send>>(())
                });
            },
        )
        .await?;

        Ok(())
    });

    sim.client("client", async {
        println!("[Client] Connecting to server...");
        turmoil::net::TcpStream::connect("moosicbox:1234").await?;
        println!("[Client] Connected!");

        Ok(())
    });

    let result = sim.run();

    log::info!("Server simulator finished");

    result
}
