#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    io::{Read, Write},
};

use moosicbox_config::AppType;
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_simulator_harness::{
    getrandom,
    rand::{SeedableRng, rngs::SmallRng},
    turmoil::{self, net::TcpStream},
};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt},
    task::JoinHandle,
};

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        moosicbox_simulator_harness::init();
    }

    let seed = std::env::var("SIMULATOR_SEED")
        .ok()
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or_else(|| getrandom::u64().unwrap());
    let rng = SmallRng::seed_from_u64(seed);

    println!("Starting simulation with seed={seed}");

    moosicbox_logging::init(None, None)?;

    let mut sim = turmoil::Builder::new().build_with_rng(Box::new(rng));

    let service_port = default_env_usize("PORT", 8000)
        .unwrap_or(8000)
        .try_into()
        .expect("Invalid PORT environment variable");

    sim.host("moosicbox", move || async move {
        let addr = default_env("BIND_ADDR", "0.0.0.0");
        let actix_workers = option_env_usize("ACTIX_WORKERS")
            .map_err(|e| std::io::Error::other(format!("Invalid ACTIX_WORKERS: {e:?}")))?;
        #[cfg(feature = "telemetry")]
        let otel =
            std::sync::Arc::new(moosicbox_telemetry::Otel::new().map_err(std::io::Error::other)?);

        let actual_tcp_listener = std::net::TcpListener::bind(format!("{addr}:{service_port}"))?;

        let handle: JoinHandle<Result<(), _>> = moosicbox_server::run(
            AppType::Server,
            &addr.clone(),
            service_port,
            actix_workers,
            Some(actual_tcp_listener),
            #[cfg(feature = "player")]
            true,
            #[cfg(feature = "upnp")]
            true,
            #[cfg(feature = "telemetry")]
            otel,
            move || {
                moosicbox_task::spawn("simulation TCP listener", async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    log::debug!("simulation TCP listener: starting TcpListener...");

                    let listener = turmoil::net::TcpListener::bind("0.0.0.0:1234")
                        .await
                        .inspect_err(|e| {
                            log::error!(
                                "simulation TCP listener: failed to bind TcpListener: {e:?}"
                            );
                        })
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

                    log::debug!("simulation TCP listener: bound TcpListener");

                    loop {
                        match listener.accept().await {
                            Ok((stream, _addr)) => {
                                log::debug!("[Server] Received connection!");

                                let mut actual_stream =
                                    std::net::TcpStream::connect(format!("{addr}:{service_port}"))
                                        .expect("Failed to connect to actual TcpStream");
                                log::debug!("simulation TCP listener: accepted socket connection");

                                let (mut read, mut write) = stream.into_split();

                                let mut request_bytes = vec![];
                                // loop {
                                log::trace!("simulation TCP listener: reading from stream");

                                let mut buf = [0_u8; 1024];
                                let count = read
                                    .read(&mut buf)
                                    .await
                                    .expect("Failed to read from socket");

                                log::trace!("simulation TCP listener: read {count} bytes");

                                if count == 0 {
                                    log::trace!("simulation TCP listener: read closed");
                                    // break;
                                }

                                request_bytes.extend_from_slice(&buf[0..count]);
                                // }

                                actual_stream
                                    .write_all(&request_bytes)
                                    .expect("Failed to propagate data to socket");

                                log::trace!(
                                    "simulation TCP listener: wrote {} actual bytes",
                                    request_bytes.len()
                                );

                                log::trace!("simulation TCP listener: flushing actual stream...");
                                actual_stream
                                    .flush()
                                    .expect("Failed to flush data from actual socket");
                                log::trace!("simulation TCP listener: flushed actual stream");

                                let mut response_bytes = vec![];
                                loop {
                                    log::trace!(
                                        "simulation TCP writer: reading from actual stream"
                                    );
                                    let (buf, count) =
                                        moosicbox_task::spawn_blocking("read actual stream", {
                                            let mut actual_stream =
                                                actual_stream.try_clone().unwrap();
                                            move || {
                                                let mut buf = [0_u8; 1024];
                                                let read = actual_stream
                                                    .read(&mut buf)
                                                    .expect("Failed to read from actual_socket");
                                                (buf, read)
                                            }
                                        })
                                        .await
                                        .unwrap();

                                    log::trace!("simulation TCP writer: read {count} actual bytes");

                                    if count == 0 {
                                        log::trace!("simulation TCP writer: actual read closed");
                                        break;
                                    }

                                    response_bytes.extend_from_slice(&buf[0..count]);
                                }

                                write
                                    .write_all(&response_bytes)
                                    .await
                                    .expect("Failed to write to socket");
                                log::trace!(
                                    "simulation TCP writer: responding {} bytes",
                                    response_bytes.len()
                                );

                                log::trace!("simulation TCP writer: flushing stream...");
                                write
                                    .flush()
                                    .await
                                    .expect("Failed to flush data from socket");
                                log::trace!("simulation TCP writer: flushed stream");
                            }
                            Err(e) => {
                                log::error!("Failed to accept TCP connection: {e:?}");
                                return Err(Box::new(e) as Box<dyn std::error::Error + Send>);
                            }
                        }
                    }
                })
            },
        )
        .await?;

        handle.await?.unwrap();

        Ok(())
    });

    sim.client("client", async move {
        let addr = "moosicbox:1234";

        for _ in 0..100 {
            assert_health(addr).await?;
        }

        Ok(())
    });

    let result = sim.run();

    log::info!("Server simulator finished (seed={seed})");

    result
}

async fn assert_health(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("[Client] Connecting to server...");
    let mut stream = turmoil::net::TcpStream::connect(addr).await?;
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

fn headers_contains_in_order(
    expected: &[(String, String)],
    actual: &BTreeMap<String, String>,
) -> bool {
    let mut iter = actual.iter();
    for expected in expected {
        loop {
            if let Some(actual) = iter.next() {
                if &expected.0 == actual.0 && &expected.1 == actual.1 {
                    break;
                }
            } else {
                return false;
            }
        }
    }

    true
}

async fn http_request(method: &str, stream: &mut TcpStream, path: &str) -> turmoil::Result<String> {
    let host = "127.0.0.1";

    let request = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\
         \r\n"
    );

    let bytes = request.as_bytes();
    log::trace!(
        "http_request: method={method} path={path} sending {} bytes",
        bytes.len()
    );
    stream.write_all(bytes).await?;
    stream.write_all(&[]).await?;
    stream.flush().await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    Ok(response)
}

struct HttpResponse {
    status_code: u16,
    headers: BTreeMap<String, String>,
    body: String,
}

fn parse_http_response(raw_response: &str) -> Result<HttpResponse, &'static str> {
    // Split the response into headers and body sections
    let parts: Vec<&str> = raw_response.split("\r\n\r\n").collect();
    if parts.len() < 2 {
        return Err("Invalid HTTP response format");
    }

    let headers_section = parts[0];
    let body = parts[1..].join("\r\n\r\n"); // Join in case there are \r\n\r\n sequences in the body

    // Parse the headers section
    let header_lines: Vec<&str> = headers_section.split("\r\n").collect();
    if header_lines.is_empty() {
        return Err("No headers found");
    }

    // Parse the status line
    let status_line = header_lines[0];
    let status_parts: Vec<&str> = status_line.split_whitespace().collect();
    if status_parts.len() < 3 {
        return Err("Invalid status line");
    }

    // Extract the status code
    let Ok(status_code) = status_parts[1].parse::<u16>() else {
        return Err("Invalid status code");
    };

    // Parse the headers
    let mut headers = BTreeMap::new();
    for line in header_lines.into_iter().skip(1) {
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let value = line[colon_pos + 1..].trim();
            headers.insert(key.to_string(), value.to_string());
        }
    }

    // If Content-Length is specified, we might want to truncate the body accordingly
    if let Some(content_length_str) = headers.iter().find_map(|(key, value)| {
        if key == "Content-Length" {
            Some(value)
        } else {
            None
        }
    }) {
        if let Ok(content_length) = content_length_str.parse::<usize>() {
            // Ensure we don't read beyond the specified content length
            // This is a simplification; actual HTTP might have complex encoding
            if body.len() >= content_length {
                let truncated_body = &body[..content_length];
                return Ok(HttpResponse {
                    status_code,
                    headers,
                    body: truncated_body.to_string(),
                });
            }
        }
    }

    Ok(HttpResponse {
        status_code,
        headers,
        body,
    })
}
