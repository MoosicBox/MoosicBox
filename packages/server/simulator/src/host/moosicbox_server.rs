use std::{
    io::{Read as _, Write as _},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use actix_web::dev::ServerHandle;
use moosicbox_config::AppType;
use moosicbox_env_utils::default_env;
use moosicbox_simulator_harness::{
    rand::Rng as _,
    turmoil::{self, Sim, net::TcpStream},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::RNG;

pub const HOST: &str = "moosicbox_server";
pub const PORT: u16 = 1234;
pub static CANCELLATION_TOKEN: LazyLock<Arc<Mutex<Option<CancellationToken>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));
pub static HANDLE: LazyLock<Arc<Mutex<Option<ServerHandle>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

/// # Panics
///
/// * If `RNG` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>, service_port: Option<u16>) {
    let service_port = service_port.unwrap_or_else(|| {
        openport::pick_unused_port(3000..=u16::MAX).expect("No open ports within acceptable range")
    });
    let host = default_env("BIND_ADDR", "0.0.0.0");
    let actix_workers = Some(RNG.lock().unwrap().gen_range(1..=64_usize));
    #[cfg(feature = "telemetry")]
    let metrics_handler = std::sync::Arc::new(
        moosicbox_telemetry::get_http_metrics_handler().expect("Failed to init telemetry"),
    );
    let addr = format!("{host}:{service_port}");

    sim.host(HOST, move || {
        let token = CancellationToken::new();
        let mut binding = CANCELLATION_TOKEN.lock().unwrap();
        if let Some(existing) = binding.replace(token.clone()) {
            existing.cancel();
        }
        drop(binding);

        #[cfg(feature = "telemetry")]
        let metrics_handler = metrics_handler.clone();
        let host = host.clone();
        let addr = addr.clone();
        async move {
            log::info!("starting 'moosicbox' server");
            let actual_tcp_listener = bind_std_tcp_addr(&addr).await?;

            let join_handle = moosicbox_server::run(
                AppType::Server,
                &host,
                service_port,
                actix_workers,
                Some(actual_tcp_listener),
                #[cfg(feature = "player")]
                true,
                #[cfg(feature = "upnp")]
                true,
                #[cfg(feature = "telemetry")]
                metrics_handler,
                move |handle| {
                    *HANDLE.lock().unwrap() = Some(handle);
                    if token.is_cancelled() {
                        moosicbox_assert::die!("already cancelled");
                    }
                    log::info!("moosicbox server started");
                    start_tcp_listen(&addr, token)
                },
            )
            .await?;

            log::info!("moosicbox server closed");

            if let Err(e) = join_handle.await? {
                log::error!("moosicbox_server error: {e:?}");
            }

            log::info!("moosicbox server read loop closed");

            Ok(())
        }
    });
}

async fn bind_std_tcp_addr(addr: &str) -> Result<std::net::TcpListener, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        match std::net::TcpListener::bind(addr) {
            Ok(x) => break x,
            Err(e) => {
                const MAX_ATTEMPTS: usize = 100;

                count += 1;

                log::debug!("failed to bind tcp: {e:?} (attempt {count}/{MAX_ATTEMPTS})");

                if !matches!(e.kind(), std::io::ErrorKind::AddrInUse) || count >= MAX_ATTEMPTS {
                    return Err(e);
                }

                tokio::time::sleep(Duration::from_millis(20000)).await;
            }
        }
    })
}

async fn connect_std_tcp_addr(addr: &str) -> Result<std::net::TcpStream, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        match moosicbox_task::spawn_blocking("connect_std_tcp_addr", {
            let addr = addr.to_string();
            move || std::net::TcpStream::connect(addr)
        })
        .await
        .unwrap()
        {
            Ok(x) => break x,
            Err(e) => {
                const MAX_ATTEMPTS: usize = 1;

                count += 1;

                log::debug!("failed to bind tcp: {e:?} (attempt {count}/{MAX_ATTEMPTS})");

                if !matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset
                ) || count >= MAX_ATTEMPTS
                {
                    return Err(e);
                }

                tokio::time::sleep(Duration::from_millis(20000)).await;
            }
        }
    })
}

fn start_tcp_listen(
    addr: &str,
    token: CancellationToken,
) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
    let addr = addr.to_string();
    moosicbox_task::spawn("simulation TCP listener", async move {
        log::debug!("simulation TCP listener: starting TcpListener...");

        let listener = turmoil::net::TcpListener::bind(format!("0.0.0.0:{PORT}"))
            .await
            .inspect_err(|e| {
                log::error!("simulation TCP listener: failed to bind TcpListener: {e:?}");
            })
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

        log::debug!("simulation TCP listener: bound TcpListener");

        loop {
            tokio::select! {
                resp = listener.accept() => {
                    match resp {
                        Ok((stream, _addr)) => {
                            handle_moosicbox_connection(stream, &addr).await?;
                        }
                        Err(e) => {
                            log::error!("Failed to accept TCP connection: {e:?}");
                            return Err(Box::new(e) as Box<dyn std::error::Error + Send>);
                        }
                    }
                }
                () = token.cancelled() => {
                    log::debug!("finished tcp_listen");
                    break;
                }
            }
        }

        Ok(())
    })
}

async fn handle_moosicbox_connection(
    stream: TcpStream,
    addr: &str,
) -> Result<(), Box<dyn std::error::Error + Send>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    log::debug!("handle_moosicbox_connection: Received connection!");

    let mut actual_stream = connect_std_tcp_addr(addr)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
    log::debug!("handle_moosicbox_connection: accepted socket connection");

    let (mut read, mut write) = stream.into_split();

    let mut request_bytes = vec![];
    log::trace!("handle_moosicbox_connection: reading from stream");

    let mut buf = [0_u8; 1024];
    let count = read
        .read(&mut buf)
        .await
        .expect("Failed to read from socket");

    log::trace!("handle_moosicbox_connection: read {count} bytes");

    if count == 0 {
        log::trace!("handle_moosicbox_connection: read closed");
    }

    request_bytes.extend_from_slice(&buf[0..count]);

    actual_stream
        .write_all(&request_bytes)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

    log::trace!(
        "handle_moosicbox_connection: wrote {} actual bytes",
        request_bytes.len()
    );

    log::trace!("handle_moosicbox_connection: flushing actual stream...");
    actual_stream
        .flush()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
    log::trace!("handle_moosicbox_connection: flushed actual stream");

    let response_bytes = moosicbox_task::spawn_blocking("read actual stream", move || {
        let mut response_bytes = vec![];

        loop {
            log::trace!("handle_moosicbox_connection: reading from actual stream");
            let mut buf = [0_u8; 1024];
            let count = actual_stream
                .read(&mut buf)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

            log::trace!("handle_moosicbox_connection: read {count} actual bytes");

            if count == 0 {
                log::trace!("handle_moosicbox_connection: actual read closed");
                break;
            }

            response_bytes.extend_from_slice(&buf[0..count]);
        }

        Ok::<_, Box<dyn std::error::Error + Send>>(response_bytes)
    })
    .await
    .unwrap()?;

    write
        .write_all(&response_bytes)
        .await
        .expect("Failed to write to socket");
    log::trace!(
        "handle_moosicbox_connection: responding {} bytes",
        response_bytes.len()
    );

    log::trace!("handle_moosicbox_connection: flushing stream...");
    write
        .flush()
        .await
        .expect("Failed to flush data from socket");
    log::trace!("handle_moosicbox_connection: flushed stream");

    drop(write);

    Ok(())
}
