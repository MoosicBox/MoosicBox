use super::*;
use std::{
    io::{Read as _, Write as _},
    net::TcpListener,
    thread,
};

fn server(responses: Vec<String>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/fixture", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 16384, "oversized fixture request");
            }
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    (url, task)
}
fn reply(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[switchy_async::test]
async fn actual_429_headers_trigger_transport_retry_then_cache_success() {
    let (url, task) = server(vec![
        reply("429 Too Many Requests", "Retry-After: 0\r\n", ""),
        reply("200 OK", "", "{}"),
    ]);
    let client = RegistryClient::for_test(1);
    let start = Instant::now();
    assert!(client.exists(&url, false).await.unwrap());
    assert!(start.elapsed() >= Duration::from_secs(5));
    task.join().unwrap();
    // Listener is now gone: a second call must hit the run-local cache.
    assert!(client.exists(&url, false).await.unwrap());
}

#[switchy_async::test]
async fn missing_versions_are_not_cached_and_new_versions_become_visible() {
    let (url, task) = server(vec![
        reply("404 Not Found", "", ""),
        reply("200 OK", "", "{}"),
    ]);
    let client = RegistryClient::for_test(0);
    assert!(!client.exists(&url, false).await.unwrap());
    assert!(client.exists(&url, false).await.unwrap());
    task.join().unwrap();
    assert!(client.exists(&url, false).await.unwrap());
}

#[switchy_async::test]
async fn category_missing_results_are_cached_and_permanent_errors_not_retried() {
    let (url, task) = server(vec![reply("404 Not Found", "", "")]);
    let client = RegistryClient::for_test(3);
    assert!(!client.exists(&url, true).await.unwrap());
    task.join().unwrap();
    assert!(!client.exists(&url, true).await.unwrap());
    let (url, task) = server(vec![reply("403 Forbidden", "", "")]);
    assert!(
        client
            .exists(&url, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("403")
    );
    task.join().unwrap();
}

#[switchy_async::test]
async fn exhausted_rate_limit_budget_is_explicit_and_preserves_shared_cooldown() {
    let (url, task) = server(vec![reply("429 Too Many Requests", "", "")]);
    let client = RegistryClient::for_test(0);
    let error = client.exists(&url, false).await.unwrap_err();
    assert!(error.to_string().contains("exhausted 0 registry retries"));
    assert_eq!(client.state.lock().await.cooldown, Duration::from_secs(60));
    assert!(client.state.lock().await.exists.is_empty());
    task.join().unwrap();
    // Exhaustion closes the run's request gate: later validation must not
    // produce another burst of failing requests or start a new retry budget.
    assert!(
        client
            .get(&url)
            .await
            .unwrap_err()
            .to_string()
            .contains("exhausted")
    );
}

#[test]
fn retry_timing_handles_real_headers_and_body_without_cargo_status_text() {
    let now = Utc::now();
    let later = now + chrono::Duration::seconds(90);
    let date = later.to_rfc2822();
    assert!(retry_timing(None, Some(&date), now).wait >= Duration::from_secs(94));
    assert_eq!(
        retry_timing(None, Some("12"), now).wait,
        Duration::from_secs(17)
    );
    assert_eq!(
        retry_timing(None, Some("invalid"), now).wait,
        Duration::from_secs(60)
    );
    let body = format!(
        "{{\"errors\":[{{\"detail\":\"Please try again after {date} and see https://crates.io/docs/rate-limits\"}}]}}"
    );
    assert!(retry_timing(parse_retry_after(&body), None, now).wait >= Duration::from_secs(94));
}

#[switchy_async::test]
async fn sparse_lookup_observes_new_versions_and_does_not_cache_absence() {
    let (url, task) = server(vec![
        reply("200 OK", "", "{\"vers\":\"0.4.0\"}\n"),
        reply(
            "200 OK",
            "",
            "{\"vers\":\"0.4.0\"}\n{\"vers\":\"0.5.0\",\"yanked\":true}\n",
        ),
    ]);
    let client = RegistryClient::for_test(0);
    assert!(!client.version_at(&url, "0.5.0").await.unwrap());
    assert!(client.version_at(&url, "0.5.0").await.unwrap());
    task.join().unwrap();
    assert!(client.version_at(&url, "0.5.0").await.unwrap());
}

#[switchy_async::test]
async fn malformed_index_and_error_statuses_never_mean_unpublished() {
    for (status, body) in [("200 OK", "not JSON"), ("500 Internal Server Error", "")] {
        let (url, task) = server(vec![reply(status, "", body)]);
        let client = RegistryClient::for_test(0);
        assert!(client.version_at(&url, "0.5.0").await.is_err());
        assert!(client.state.lock().await.exists.is_empty());
        task.join().unwrap();
    }
}
