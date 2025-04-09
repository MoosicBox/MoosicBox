use moosicbox_simulator_harness::turmoil::Sim;
use serde_json::Value;

use crate::{
    SERVER_ADDR, SIMULATOR_CANCELLATION_TOKEN,
    http::{headers_contains_in_order, http_request, parse_http_response},
    try_connect,
};

pub fn start(sim: &mut Sim<'_>) {
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
