#![cfg(all(feature = "runtime", unix))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Real signed webhook, coordinator, revision policy, and native worker acceptance.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::{Read, Seek},
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use reqwest::blocking::Client;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ADMIN_ENV: &str = "GPIPE_WEBHOOK_E2E_ADMIN";
const WORKER_ENV: &str = "GPIPE_WEBHOOK_E2E_WORKER";
const SECRET_ENV: &str = "GPIPE_WEBHOOK_E2E_SECRET";
const SECRET: &str = "webhook-fixture-secret-not-a-production-credential";

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// Files instead of pipes prevent output-buffer deadlocks; every subprocess has a
// bounded wait and is killed/reaped even when an assertion unwinds.
fn run(command: &mut Command) -> String {
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let mut process = Process(
        command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone().unwrap())
            .stderr(stderr.try_clone().unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "subprocess exceeded 30 seconds");
        thread::sleep(Duration::from_millis(25));
    };
    stdout.rewind().unwrap();
    stderr.rewind().unwrap();
    let mut output = String::new();
    let mut error = String::new();
    stdout
        .take(1024 * 1024)
        .read_to_string(&mut output)
        .unwrap();
    stderr.take(1024 * 1024).read_to_string(&mut error).unwrap();
    assert!(status.success(), "subprocess failed ({status}): {error}");
    output
}

fn git(directory: &Path, args: &[&str]) -> String {
    run(Command::new("git")
        .arg("--no-pager")
        .args(args)
        .current_dir(directory)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null"))
    .trim()
    .to_owned()
}

fn cli(args: &[&str], envs: &BTreeMap<&str, &str>) -> Value {
    serde_json::from_str(&run(Command::new(env!("CARGO_BIN_EXE_gpipe"))
        .args(args)
        .envs(envs)))
    .unwrap()
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

// Independent RFC 2104 HMAC helper: sign exactly the bytes sent over HTTP.
fn signature(body: &[u8]) -> String {
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    assert!(SECRET.len() <= inner_pad.len());
    for (index, byte) in SECRET.bytes().enumerate() {
        inner_pad[index] ^= byte;
        outer_pad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(body);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    let mut signature = String::from("sha256=");
    for byte in outer.finalize() {
        write!(&mut signature, "{byte:02x}").unwrap();
    }
    signature
}

fn push_body(revision: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "repository": {"full_name": "fixture/webhook"},
        "ref": "refs/heads/main",
        "after": revision,
        "deleted": false,
        // Must not replace the trusted administrator-configured command.
        "workflow": {"steps": [{"run": "exit 97"}]}
    }))
    .unwrap()
}

fn post_hook(
    client: &Client,
    url: &str,
    delivery: &str,
    body: &[u8],
    mac: &str,
) -> reqwest::blocking::Response {
    client
        .post(format!("{url}/hooks/github"))
        .header("content-type", "application/json")
        .header("x-github-delivery", delivery)
        .header("x-github-event", "push")
        .header("x-hub-signature-256", mac)
        .body(body.to_vec())
        .send()
        .unwrap()
}

fn inspect(url: &str, run_id: &str, envs: &BTreeMap<&str, &str>) -> Value {
    let response = cli(
        &["inspect", "--url", url, "--token-env", ADMIN_ENV, run_id],
        envs,
    );
    assert_eq!(response["type"], "DATA");
    response["value"].clone()
}

fn assert_succeeded(state: &Value, revision: &str, input: &str) {
    assert_eq!(state["status"], "SUCCEEDED", "{state}");
    assert_eq!(state["plan"]["revision"], revision);
    assert_eq!(state["tasks"]["verify"], "SUCCEEDED");
    let attempts = state["attempts"].as_object().unwrap();
    assert_eq!(attempts.len(), 1);
    let attempt = attempts.values().next().unwrap();
    assert_eq!(attempt["report"]["success"], true);
    let log = attempt["report"]["log"].as_str().unwrap();
    assert!(
        log.contains(&format!("executed-revision={revision}")),
        "{log}"
    );
    assert!(log.contains(input), "{log}");
}

#[test]
#[allow(clippy::too_many_lines)]
fn signed_push_is_idempotent_and_execution_requires_revision_approval() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    std::fs::write(source.join("input.txt"), "first immutable input\n").unwrap();
    git(&source, &["add", "input.txt"]);
    let commit = [
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "fixture",
    ];
    git(&source, &commit);
    let approved_revision = git(&source, &["rev-parse", "HEAD"]);
    std::fs::write(source.join("input.txt"), "second gated input\n").unwrap();
    git(&source, &["add", "input.txt"]);
    git(&source, &commit);
    let gated_revision = git(&source, &["rev-parse", "HEAD"]);
    assert_ne!(approved_revision, gated_revision);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://{address}");
    let envs = BTreeMap::from([
        (ADMIN_ENV, "webhook-admin-test-token-000000000000000000000"),
        (WORKER_ENV, "webhook-worker-test-token-00000000000000000000"),
        (SECRET_ENV, SECRET),
    ]);
    let server_path = temp.path().join("server.json");
    write_json(
        &server_path,
        &json!({
            "listen": address.to_string(), "database": temp.path().join("state.db"),
            "admin_token_env": ADMIN_ENV,
            "repositories": {"fixture": {"revisions": [approved_revision]}},
            "workers": {"native": {"token_env": WORKER_ENV, "repositories": ["fixture"],
                "labels": {}, "cpu": 2, "memory_mb": 4096}}
        }),
    );
    let triggers_path = temp.path().join("triggers.json");
    write_json(
        &triggers_path,
        &json!({
            "secret_env": SECRET_ENV,
            "repositories": {"fixture": {
                "provider_repository": "fixture/webhook", "dynamic": false,
                "plan": {"repository": "fixture", "revision": gated_revision, "tasks": {
                    "verify": {"needs": [], "steps": [{"shell": "sh",
                        "run": "printf 'executed-revision=%s\\n' \"$(git rev-parse HEAD)\"; cat input.txt",
                        "env": {}, "timeout_secs": 10}],
                        "labels": {}, "cpu": 1, "memory_mb": 0, "max_attempts": 1}
                }}
            }}
        }),
    );
    let worker_path = temp.path().join("worker.json");
    write_json(
        &worker_path,
        &json!({
            "coordinator": url, "token_env": WORKER_ENV,
            "directory": temp.path().join("worker"), "repositories": {"fixture": source},
            "poll_secs": 1
        }),
    );
    let mut server = Process(
        Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args([
                "serve",
                "--config",
                server_path.to_str().unwrap(),
                "--triggers",
                triggers_path.to_str().unwrap(),
            ])
            .envs(&envs)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "server exited during startup"
        );
        if client
            .post(format!("{url}/v1/rpc"))
            .bearer_auth(envs[ADMIN_ENV])
            .json(&json!({"version": 1, "request": {"type": "LIST"}}))
            .send()
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "server startup exceeded 10 seconds"
        );
        thread::sleep(Duration::from_millis(25));
    }
    let run_worker = || {
        run(Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args([
                "worker",
                "--config",
                worker_path.to_str().unwrap(),
                "--once",
            ])
            .envs(&envs));
    };

    let body = push_body(&approved_revision);
    let mac = signature(&body);
    let submit: Value = post_hook(&client, &url, "approved-delivery", &body, &mac)
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(submit["type"], "SUBMITTED");
    let run_id = submit["run_id"].as_str().unwrap();
    let replay: Value = post_hook(&client, &url, "approved-delivery", &body, &mac)
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(submit, replay);
    run_worker();
    assert_succeeded(
        &inspect(&url, run_id, &envs),
        &approved_revision,
        "first immutable input",
    );

    let body = push_body(&gated_revision);
    let bad_mac = format!("sha256={}", "0".repeat(64));
    let rejected = post_hook(&client, &url, "gated-delivery", &body, &bad_mac);
    assert!(
        rejected.status().is_client_error(),
        "invalid MAC was not rejected"
    );
    let listed = cli(&["list", "--url", &url, "--token-env", ADMIN_ENV], &envs);
    assert_eq!(
        listed["value"].as_array().unwrap().len(),
        1,
        "invalid MAC created a run"
    );

    let gated: Value = post_hook(&client, &url, "gated-delivery", &body, &signature(&body))
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(gated["type"], "SUBMITTED");
    let gated_id = gated["run_id"].as_str().unwrap();
    assert_ne!(gated_id, run_id);
    // Repeated real worker claims must leave the unapproved revision untouched.
    for _ in 0..2 {
        run_worker();
        let state = inspect(&url, gated_id, &envs);
        assert_eq!(state["plan"]["revision"], gated_revision);
        assert_eq!(state["status"], "RUNNING");
        assert_eq!(state["tasks"]["verify"], "PENDING");
        assert!(state["attempts"].as_object().unwrap().is_empty());
    }
    let approval = cli(
        &[
            "approve",
            "--url",
            &url,
            "--token-env",
            ADMIN_ENV,
            "fixture",
            &gated_revision,
        ],
        &envs,
    );
    assert_eq!(approval["type"], "OK");
    run_worker();
    assert_succeeded(
        &inspect(&url, gated_id, &envs),
        &gated_revision,
        "second gated input",
    );
    let replay: Value = post_hook(
        &client,
        &url,
        "approved-delivery",
        &push_body(&approved_revision),
        &mac,
    )
    .error_for_status()
    .unwrap()
    .json()
    .unwrap();
    assert_eq!(replay, submit);
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), gated_revision);
    assert_eq!(git(&source, &["status", "--porcelain"]), "");
}
