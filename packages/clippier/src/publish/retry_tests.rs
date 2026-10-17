//! No network, uploads or real sleeps: test the production retry loop directly.

use super::*;

#[cfg(unix)]
#[test]
fn pre_upload_transport_failure_retries_but_uncertain_upload_does_not() {
    let package = PublishPackageInfo::new("fixture", "0.1.0", true, BTreeSet::new());
    let network = "error: failed to update registry `crates-io`\ndownload of config.json failed\n[7] Could not connect to server";
    for recover in [true, false] {
        let mut calls = 0;
        let mut waits = Vec::new();
        let result = publish_with_retry(
            &package,
            &PublishConfig::default(),
            || {
                calls += 1;
                Ok(response(recover && calls == 3, network))
            },
            |delay| waits.push(delay.as_secs()),
        );
        assert_eq!(result.is_ok(), recover);
        assert_eq!(calls, if recover { 3 } else { 4 });
        assert_eq!(waits, if recover { vec![1, 2] } else { vec![1, 2, 4] });
    }
    let mut calls = 0;
    assert!(
        publish_with_retry(
            &package,
            &PublishConfig::default(),
            || {
                calls += 1;
                Ok(response(
                    false,
                    "failed to publish: [7] Could not connect to server",
                ))
            },
            |_| panic!("uncertain upload must not be blindly replayed")
        )
        .is_err()
    );
    assert_eq!(calls, 1);
}

#[switchy_async::test]
async fn production_skips_existing_version_before_validating_its_manifest() {
    let package = PublishPackageInfo::new("fixture", "0.1.0", true, BTreeSet::new());
    let packages = BTreeMap::from([("fixture".into(), package)]);
    let client = RegistryClient::for_test(0);
    client.cache_test_version("fixture", "0.1.0", true).await;
    let mut reports = Vec::new();
    publish_pending_queue(
        Path::new("/nonexistent"),
        &packages,
        &["fixture".into()],
        &PublishConfig::default(),
        &client,
        &mut reports,
    )
    .await
    .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].status, PublishPackageStatus::AlreadyPublished);
}

#[switchy_async::test]
async fn production_stops_at_current_invalid_package_without_looking_up_later_queue() {
    let package = PublishPackageInfo::new("fixture", "0.1.0", true, BTreeSet::new());
    let packages = BTreeMap::from([("fixture".into(), package)]);
    let client = RegistryClient::for_test(0);
    client.cache_test_version("fixture", "0.1.0", false).await;
    let mut reports = Vec::new();
    let error = publish_pending_queue(
        Path::new("/nonexistent"),
        &packages,
        &["fixture".into(), "later-unavailable".into()],
        &PublishConfig::default(),
        &client,
        &mut reports,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(!error.contains("later-unavailable"), "{error}");
    assert_eq!(reports.len(), 0);
}

const BARE_429: &str = "error: failed to publish\nCaused by:\n  failed to get a 200 OK response, got 429\n  headers:\n        date: Fri, 16 Oct 2026 19:09:13 GMT\n        content-length: 0\n  body:\n";

#[test]
fn both_cargo_rate_limit_formats_honor_timing() {
    let now = DateTime::parse_from_rfc3339("2026-10-16T19:08:30Z")
        .unwrap()
        .with_timezone(&Utc);
    let body = "status 429 Too Many Requests: Please try again after Fri, 16 Oct 2026 19:09:06 GMT and see https://crates.io/docs/rate-limits";
    assert_eq!(
        parse_rate_limit_at(body, now).unwrap().wait,
        Duration::from_secs(41)
    );
    // Date is the response time, NOT a retry deadline.
    assert_eq!(
        parse_rate_limit_at(BARE_429, now).unwrap().wait,
        Duration::from_secs(60)
    );
    for (header, expected) in [
        ("Retry-After: 120", 125),
        ("retry-after: Fri, 16 Oct 2026 19:09:06 GMT", 41),
        ("Retry-After: invalid", 60),
        ("Retry-After: 0", 5),
    ] {
        assert_eq!(
            parse_rate_limit_at(&format!("{BARE_429}\n {header}"), now)
                .unwrap()
                .wait,
            Duration::from_secs(expected)
        );
    }
    assert_eq!(
        parse_rate_limit_at(&format!("{body}\nRetry-After: 120"), now)
            .unwrap()
            .wait,
        Duration::from_secs(125)
    );
    let later = now + chrono::Duration::minutes(10);
    assert_eq!(
        parse_rate_limit_at(body, later).unwrap().wait,
        Duration::from_secs(5)
    );
}

#[test]
fn unrelated_numbers_and_errors_are_not_rate_limits() {
    for text in [
        "crate-429 failed",
        "https://example.invalid/429",
        "status 4290",
        "failed to get a 200 OK response, got 4290",
        "status 403 Forbidden",
        "status 500",
        "warning: spurious network error: [16] HTTP2 framing layer",
    ] {
        assert!(parse_rate_limit(text).is_none(), "{text}");
    }
    for text in ["status 429", "HTTP/2 429", "HTTP/1.1 429 Too Many Requests"] {
        assert!(parse_rate_limit(text).is_some(), "{text}");
    }
}

#[cfg(unix)]
fn response(success: bool, stderr: &str) -> CommandOutput {
    use std::os::unix::process::ExitStatusExt as _;
    CommandOutput {
        status: std::process::ExitStatus::from_raw(if success { 0 } else { 101 << 8 }),
        stderr: stderr.into(),
    }
}

#[cfg(unix)]
#[test]
fn dated_429_then_header_only_429_then_success_retries_twice() {
    let package = PublishPackageInfo::new("fixture", "0.1.0", true, BTreeSet::new());
    let config = PublishConfig::default();
    let mut responses = [
        response(false, "status 429 Too Many Requests: Please try again after Fri, 16 Oct 2020 19:09:06 GMT and see https://crates.io/docs/rate-limits"),
        response(false, BARE_429),
        response(true, ""),
    ].into_iter();
    let mut waits = Vec::new();
    let result = publish_with_retry(
        &package,
        &config,
        || Ok(responses.next().expect("unexpected extra upload attempt")),
        |wait| waits.push(wait),
    )
    .unwrap();
    assert!(matches!(result, PublishAttempt::Published));
    assert_eq!(waits, [Duration::from_secs(5), Duration::from_secs(60)]);
    assert!(responses.next().is_none());
}

#[cfg(unix)]
#[test]
fn retry_budget_and_permanent_errors_stop_without_extra_attempts() {
    let package = PublishPackageInfo::new("fixture", "0.1.0", true, BTreeSet::new());
    for budget in [0, 1, 3] {
        let config = PublishConfig {
            rate_limit_retries: budget,
            ..PublishConfig::default()
        };
        let mut attempts = 0;
        let mut waits = Vec::new();
        let result = publish_with_retry(
            &package,
            &config,
            || {
                attempts += 1;
                Ok(response(false, BARE_429))
            },
            |wait| waits.push(wait),
        );
        assert!(result.is_err());
        assert_eq!(attempts, budget + 1);
        assert_eq!(waits.len(), usize::from(budget));
    }
    for (stderr, already_published) in [
        ("status 403 Forbidden", false),
        ("status 500 Internal Server Error", false),
        ("version already exists", true),
    ] {
        let mut attempts = 0;
        let result = publish_with_retry(
            &package,
            &PublishConfig::default(),
            || {
                attempts += 1;
                Ok(response(false, stderr))
            },
            |_| panic!("must not sleep"),
        );
        assert_eq!(attempts, 1);
        if already_published {
            assert!(matches!(result.unwrap(), PublishAttempt::AlreadyPublished));
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn real_publishes_require_preflight_opt_in_but_dry_runs_always_validate() {
    for preflight in [None, Some(false), Some(true)] {
        for dry_run in [false, true] {
            let config = PublishConfig {
                dry_run,
                preflight,
                ..PublishConfig::default()
            };
            assert_eq!(
                should_run_preflight(&config),
                dry_run || preflight == Some(true)
            );
        }
    }
}
