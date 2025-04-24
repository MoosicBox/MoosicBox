use moosicbox_simulator_harness::turmoil::Sim;
use moosicbox_simulator_utils::SIMULATOR_CANCELLATION_TOKEN;
use serde_json::Value;

use crate::{
    host::moosicbox_server::{HOST, PORT},
    http::{headers_contains_in_order, http_request, parse_http_response},
    try_connect,
};

pub fn start(sim: &mut Sim<'_>) {
    let addr = format!("{HOST}:{PORT}");

    sim.client("McHealthChecker", async move {
        SIMULATOR_CANCELLATION_TOKEN
            .run_until_cancelled(async move {
                loop {
                    static TIMEOUT: u64 = 1000;

                    log::info!("checking health");

                    tokio::select! {
                        resp = assert_health(&addr) => {
                            resp?;
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        () = tokio::time::sleep(std::time::Duration::from_secs(TIMEOUT)) => {
                            return Err(Box::new(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                format!("Failed to get healthy response within {TIMEOUT} seconds")
                            )) as Box<dyn std::error::Error>);
                        }
                    }
                }
            })
            .await
            .transpose()
            .map(|x| x.unwrap_or(()))
    });
}

async fn assert_health(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = loop {
        log::debug!("[Client] Connecting to server...");
        let mut stream = match try_connect(addr, 1).await {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("[Client] Failed to connect to server: {e:?}");
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };
        log::debug!("[Client] Connected!");

        let resp = match http_request("GET", &mut stream, "/health").await {
            Ok(resp) => resp,
            Err(e) => {
                log::error!("failed to make http_request: {e:?}");
                continue;
            }
        };
        log::debug!("Received response={resp}");

        match parse_http_response(&resp) {
            Ok(resp) => break resp,
            Err(e) => {
                log::debug!("Received error response={e}");
            }
        }
    };

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
