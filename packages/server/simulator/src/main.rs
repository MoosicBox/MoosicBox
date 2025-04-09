#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod http;

use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use http::{headers_contains_in_order, http_request, parse_http_response};
use moosicbox_config::AppType;
use moosicbox_env_utils::{default_env, default_env_usize};
use moosicbox_simulator_harness::{
    getrandom,
    rand::{Rng, SeedableRng, rngs::SmallRng},
    turmoil::{self, Sim, net::TcpStream},
};
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const SERVER_ADDR: &str = "moosicbox:1234";
static SIMULATOR_CANCELLATION_TOKEN: LazyLock<CancellationToken> =
    LazyLock::new(CancellationToken::new);
static SEED: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_SEED")
        .ok()
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or_else(|| getrandom::u64().unwrap())
});
static RNG: LazyLock<Arc<Mutex<SmallRng>>> =
    LazyLock::new(|| Arc::new(Mutex::new(SmallRng::seed_from_u64(*SEED))));
static ACTIONS: LazyLock<Arc<Mutex<VecDeque<Action>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::new())));

enum Action {
    Crash,
    Bounce,
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

    let resp = run_simulation(duration_secs);

    log::info!("Server simulator finished (seed={seed})");

    resp
}

fn run_simulation(duration_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut sim = turmoil::Builder::new()
        .simulation_duration(Duration::from_secs(duration_secs))
        .build_with_rng(Box::new(RNG.lock().unwrap().clone()));

    start_moosicbox_server(&mut sim);
    start_health_checker(&mut sim);
    start_fault_injector(&mut sim);
    start_healer(&mut sim);

    while !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
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

fn start_moosicbox_server(sim: &mut Sim<'_>) {
    let service_port = default_env_usize("PORT", 8000)
        .unwrap_or(8000)
        .try_into()
        .expect("Invalid PORT environment variable");

    sim.host("moosicbox", move || async move {
        let host = default_env("BIND_ADDR", "0.0.0.0");
        let actix_workers = Some(RNG.lock().unwrap().gen_range(1..=64_usize));
        #[cfg(feature = "telemetry")]
        let metrics_handler = std::sync::Arc::new(
            moosicbox_telemetry::get_http_metrics_handler().map_err(std::io::Error::other)?,
        );

        log::info!("starting 'moosicbox' server");
        let addr = format!("{host}:{service_port}");
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
            move |_handle| start_tcp_listen(&addr),
        )
        .await?;

        log::info!("moosicbox server closed");

        join_handle.await?.unwrap();

        log::info!("moosicbox server read loop closed");

        Ok(())
    });
}

async fn bind_std_tcp_addr(addr: &str) -> Result<std::net::TcpListener, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        match std::net::TcpListener::bind(addr) {
            Ok(x) => break x,
            Err(e) => {
                const MAX_ATTEMPTS: usize = 10;

                count += 1;

                log::debug!("failed to bind tcp: {e:?} (attempt {count}/{MAX_ATTEMPTS})");

                if !matches!(e.kind(), std::io::ErrorKind::AddrInUse) || count >= MAX_ATTEMPTS {
                    return Err(e);
                }

                tokio::time::sleep(Duration::from_millis(5000)).await;
            }
        }
    })
}

fn start_tcp_listen(addr: &str) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
    let addr = addr.to_string();
    moosicbox_task::spawn("simulation TCP listener", async move {
        log::debug!("simulation TCP listener: starting TcpListener...");

        let listener = turmoil::net::TcpListener::bind("0.0.0.0:1234")
            .await
            .inspect_err(|e| {
                log::error!("simulation TCP listener: failed to bind TcpListener: {e:?}");
            })
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

        log::debug!("simulation TCP listener: bound TcpListener");

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    handle_moosicbox_connection(stream, &addr).await.unwrap();
                }
                Err(e) => {
                    log::error!("Failed to accept TCP connection: {e:?}");
                    return Err(Box::new(e) as Box<dyn std::error::Error + Send>);
                }
            }
        }
    })
}

async fn handle_moosicbox_connection(
    stream: TcpStream,
    addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    log::debug!("handle_moosicbox_connection: Received connection!");

    let mut actual_stream =
        std::net::TcpStream::connect(addr).expect("Failed to connect to actual TcpStream");
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
        .expect("Failed to propagate data to socket");

    log::trace!(
        "handle_moosicbox_connection: wrote {} actual bytes",
        request_bytes.len()
    );

    log::trace!("handle_moosicbox_connection: flushing actual stream...");
    actual_stream
        .flush()
        .expect("Failed to flush data from actual socket");
    log::trace!("handle_moosicbox_connection: flushed actual stream");

    let response_bytes = moosicbox_task::spawn_blocking("read actual stream", move || {
        let mut response_bytes = vec![];

        loop {
            log::trace!("handle_moosicbox_connection: reading from actual stream");
            let mut buf = [0_u8; 1024];
            let count = actual_stream
                .read(&mut buf)
                .expect("Failed to read from actual_socket");

            log::trace!("handle_moosicbox_connection: read {count} actual bytes");

            if count == 0 {
                log::trace!("handle_moosicbox_connection: actual read closed");
                break;
            }

            response_bytes.extend_from_slice(&buf[0..count]);
        }

        response_bytes
    })
    .await
    .unwrap();

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

    Ok(())
}

fn start_health_checker(sim: &mut Sim<'_>) {
    sim.client("McHealthChecker", async move {
        loop {
            log::info!("checking health");
            assert_health(SERVER_ADDR).await?;

            tokio::select! {
                () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                    break;
                }
                () = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            }
        }

        Ok(())
    });
}

fn start_fault_injector(sim: &mut Sim<'_>) {
    sim.client("McFaultInjector", {
        let actions = ACTIONS.clone();
        async move {
            loop {
                tokio::select! {
                    () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                        break;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(RNG.lock().unwrap().gen_range(0..60))) => {}
                }

                actions.lock().unwrap().push_back(Action::Crash);
            }

            Ok(())
        }
    });
}

fn start_healer(sim: &mut Sim<'_>) {
    sim.client("McHealer", {
        let actions = ACTIONS.clone();
        async move {
            loop {
                tokio::select! {
                    () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                        break;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(RNG.lock().unwrap().gen_range(0..60))) => {}
                }

                actions.lock().unwrap().push_back(Action::Bounce);
            }

            Ok(())
        }
    });
}

fn handle_actions(_sim: &mut Sim<'_>) {
    // static BOUNCED: LazyLock<Arc<Mutex<bool>>> = LazyLock::new(|| Arc::new(Mutex::new(false)));

    let mut actions = ACTIONS.lock().unwrap();
    for action in actions.drain(..) {
        match action {
            Action::Crash => {
                log::info!("crashing 'moosicbox'");
                // sim.crash("moosicbox");
            }
            Action::Bounce => {
                log::info!("bouncing 'moosicbox'");
                // let mut bounced = BOUNCED.lock().unwrap();
                // if !*bounced {
                //     *bounced = true;
                //     sim.bounce("moosicbox");
                // }
                // drop(bounced);
            }
        }
    }
    drop(actions);
}

async fn try_connect(addr: &str) -> Result<TcpStream, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        match turmoil::net::TcpStream::connect(addr).await {
            Ok(x) => break x,
            Err(e) => {
                const MAX_ATTEMPTS: usize = 10;

                count += 1;

                log::debug!("failed to bind tcp: {e:?} (attempt {count}/{MAX_ATTEMPTS})");

                if !matches!(e.kind(), std::io::ErrorKind::ConnectionRefused)
                    || count >= MAX_ATTEMPTS
                {
                    return Err(e);
                }

                tokio::time::sleep(Duration::from_millis(5000)).await;
            }
        }
    })
}

async fn assert_health(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("[Client] Connecting to server...");
    let mut stream = try_connect(addr).await?;
    log::debug!("[Client] Connected!");

    let resp = http_request("GET", &mut stream, "/health").await?;
    log::debug!("Received response={resp}");
    let response = parse_http_response(&resp).unwrap();
    moosicbox_assert::assert_or_panic!(
        response.status_code == 200,
        "expected successful 200 response, get {}",
        response.status_code
    );
    moosicbox_assert::assert_or_panic!(
        headers_contains_in_order(
            &[
                (
                    "access-control-allow-credentials".to_string(),
                    "true".to_string()
                ),
                ("connection".to_string(), "close".to_string()),
                ("content-length".to_string(), "66".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
                (
                    "vary".to_string(),
                    "Origin, Access-Control-Request-Method, Access-Control-Request-Headers"
                        .to_string()
                ),
            ],
            &response.headers
        ),
        "unexpected headers in response: {:?}",
        response.headers
    );
    let json: Value = serde_json::from_str(&response.body).unwrap();
    moosicbox_assert::assert_or_panic!(json.is_object(), "expected json object response");
    moosicbox_assert::assert_or_panic!(
        json.get("healthy").and_then(Value::as_bool) == Some(true),
        "expected healthy response"
    );
    moosicbox_assert::assert_or_panic!(json.get("hash").is_some(), "expected git hash in response");
    moosicbox_assert::assert_or_panic!(
        json.get("hash").unwrap().as_str().unwrap().len() == 40,
        "expected git hash to be 40 chars"
    );

    Ok(())
}
