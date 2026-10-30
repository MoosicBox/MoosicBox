#![cfg(all(feature = "runtime", unix))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
//! Dynamic generation through the real CLI, loopback HTTP, SQLite and native worker.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const ADMIN_ENV: &str = "GPIPE_DYNAMIC_E2E_ADMIN";
const WORKER_ENV: &str = "GPIPE_DYNAMIC_E2E_WORKER";
// Synthetic local fixture credentials; no provider tokens are read or used.
const ADMIN_TOKEN: &str = "dynamic-admin-test-token-000000000000000000";
const WORKER_TOKEN: &str = "dynamic-worker-test-token-00000000000000000";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CAPTURE: u64 = 256 * 1024;

/// A private process group makes panic/timeout cleanup cover descendant shells.
struct ProcessGuard(Child);

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // SAFETY: every guarded child was started in a fresh group whose ID is
        // its positive PID. Never signal the caller's process group.
        if let Ok(pid) = i32::try_from(self.0.id()) {
            unsafe { libc::kill(-pid, libc::SIGKILL) };
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn capture(file: &mut File) -> Vec<u8> {
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    file.take(MAX_CAPTURE + 1).read_to_end(&mut bytes).unwrap();
    assert!(
        bytes.len() <= usize::try_from(MAX_CAPTURE).unwrap(),
        "fixture output exceeds bound"
    );
    bytes
}

fn bounded_output(command: &mut Command) -> Output {
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let child = command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap())
        .spawn()
        .unwrap();
    let mut child = ProcessGuard(child);
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < PROCESS_TIMEOUT,
            "fixture subprocess timed out"
        );
        thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: capture(&mut stdout),
        stderr: capture(&mut stderr),
    }
}

fn checked_output(command: &mut Command) -> Output {
    let output = bounded_output(command);
    assert!(
        output.status.success(),
        "fixture process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = checked_output(
        Command::new("git")
            .arg("--no-pager")
            .args(args)
            .current_dir(directory)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn cli(args: &[&str]) -> Output {
    checked_output(
        Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args(args)
            .env(ADMIN_ENV, ADMIN_TOKEN)
            .env(WORKER_ENV, WORKER_TOKEN),
    )
}

fn cli_json(args: &[&str]) -> Value {
    serde_json::from_slice(&cli(args).stdout).unwrap()
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn task(needs: &[&str], run: &str, generator: bool) -> Value {
    let env = if generator {
        BTreeMap::from([("GPIPE_TASKS_OUTPUT", "gpipe.tasks.json")])
    } else {
        BTreeMap::new()
    };
    json!({
        "needs": needs,
        "steps": [{"shell":"sh", "run":run, "env":env, "timeout_secs":5}],
        "labels":{"os":"linux"}, "cpu":1, "memory_mb":128, "max_attempts":1
    })
}

fn start_server(config: &Path, address: SocketAddr) -> ProcessGuard {
    // Use a file instead of an undrained pipe so diagnostics cannot deadlock startup.
    let mut error = tempfile::tempfile().unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_gpipe"))
        .args(["serve", "--config", config.to_str().unwrap()])
        .env(ADMIN_ENV, ADMIN_TOKEN)
        .env(WORKER_ENV, WORKER_TOKEN)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(error.try_clone().unwrap())
        .spawn()
        .unwrap();
    let mut server = ProcessGuard(child);
    let started = Instant::now();
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            panic!(
                "server exited {status}: {}",
                String::from_utf8_lossy(&capture(&mut error))
            );
        }
        if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
            return server;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "coordinator startup timeout"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

struct Fixture {
    // Stop the server before deleting its database/temp directory, including panic paths.
    _server: ProcessGuard,
    temp: tempfile::TempDir,
    source: PathBuf,
    worker_config: PathBuf,
    url: String,
    revision: String,
}

impl Fixture {
    fn new(generator_command: &str, approved_output: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "--initial-branch=main"]);
        // Only tracked, locally approved bytes can be copied by the generator.
        std::fs::write(source.join("approved.tasks.json"), approved_output).unwrap();
        std::fs::write(source.join("input.txt"), "immutable dynamic fixture\n").unwrap();
        git(&source, &["add", "approved.tasks.json", "input.txt"]);
        git(
            &source,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "dynamic fixture",
            ],
        );
        let revision = git(&source, &["rev-parse", "HEAD"]);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{address}");
        let server_config = temp.path().join("server.json");
        write_json(
            &server_config,
            &json!({
                "listen":address.to_string(), "database":temp.path().join("state.db"),
                "admin_token_env":ADMIN_ENV,
                "repositories":{"fixture":{"revisions":[revision]}},
                "workers":{"native":{"token_env":WORKER_ENV,"repositories":["fixture"],
                    "labels":{"os":"linux"},"cpu":1,"memory_mb":128}}
            }),
        );
        let worker_config = temp.path().join("worker.json");
        write_json(
            &worker_config,
            &json!({
                "coordinator":url,"token_env":WORKER_ENV,"directory":temp.path().join("worker"),
                "repositories":{"fixture":source},"poll_secs":1
            }),
        );
        write_json(
            &temp.path().join("plan.json"),
            &json!({
                "repository":"fixture", "revision":revision,
                "tasks":{"generator":task(&[], generator_command, true)}
            }),
        );
        let server = start_server(&server_config, address);
        Self {
            _server: server,
            temp,
            source,
            worker_config,
            url,
            revision,
        }
    }

    fn submit(&self) -> String {
        let submitted = cli_json(&[
            "submit",
            "--url",
            &self.url,
            "--token-env",
            ADMIN_ENV,
            "--key",
            "dynamic-fixture",
            "--dynamic",
            self.temp.path().join("plan.json").to_str().unwrap(),
        ]);
        submitted["run_id"].as_str().unwrap().to_owned()
    }

    fn work_once(&self) {
        cli(&[
            "worker",
            "--config",
            self.worker_config.to_str().unwrap(),
            "--once",
        ]);
    }

    fn inspect(&self, run_id: &str) -> Value {
        let response = cli_json(&[
            "inspect",
            "--url",
            &self.url,
            "--token-env",
            ADMIN_ENV,
            run_id,
        ]);
        assert_eq!(
            response["type"], "DATA",
            "unexpected inspection response: {response}"
        );
        response["value"].clone()
    }

    fn assert_source_unchanged(&self) {
        assert_eq!(git(&self.source, &["status", "--porcelain"]), "");
        assert_eq!(git(&self.source, &["rev-parse", "HEAD"]), self.revision);
        assert!(!self.source.join("gpipe.tasks.json").exists());
    }
}

#[test]
fn dynamic_generator_publishes_child_and_seals_but_waits_for_child_execution() {
    let child = task(
        &["generator"],
        "test \"$(cat input.txt)\" = 'immutable dynamic fixture'; printf 'dynamic-child-executed\\n'",
        false,
    );
    let generated = json!({"tasks":{"child":child},"seal":true});
    let fixture = Fixture::new(
        "cp approved.tasks.json \"$GPIPE_TASKS_OUTPUT\"",
        &generated.to_string(),
    );
    let run_id = fixture.submit();
    let initial = fixture.inspect(&run_id);
    assert_eq!(initial["status"], "RUNNING");
    assert_eq!(initial["dynamic"], true);
    assert_eq!(initial["sealed"], false);
    assert_eq!(initial["tasks"].as_object().unwrap().len(), 1);
    fixture.work_once();
    let expanded = fixture.inspect(&run_id);
    assert_eq!(expanded["status"], "RUNNING");
    assert_eq!(expanded["sealed"], true);
    assert_eq!(expanded["tasks"]["generator"], "SUCCEEDED");
    assert_eq!(expanded["tasks"]["child"], "PENDING");
    assert_eq!(expanded["plan"]["tasks"]["child"], child);
    assert_eq!(expanded["attempts"].as_object().unwrap().len(), 1);
    let generator = expanded["attempts"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(generator["task_id"], "generator");
    assert_eq!(generator["status"], "COMPLETED");
    assert_eq!(generator["report"]["success"], true);
    // This retained payload distinguishes CompleteGenerated from ordinary completion.
    assert_eq!(generator["generated"], generated);
    fixture.work_once();
    let final_view = fixture.inspect(&run_id);
    assert_eq!(final_view["status"], "SUCCEEDED");
    assert_eq!(final_view["tasks"]["generator"], "SUCCEEDED");
    assert_eq!(final_view["tasks"]["child"], "SUCCEEDED");
    assert_eq!(final_view["attempts"].as_object().unwrap().len(), 2);
    let child_attempt = final_view["attempts"]
        .as_object()
        .unwrap()
        .values()
        .find(|attempt| attempt["task_id"] == "child")
        .unwrap();
    assert_eq!(child_attempt["report"]["success"], true);
    assert!(
        child_attempt["report"]["log"]
            .as_str()
            .unwrap()
            .contains("dynamic-child-executed")
    );
    fixture.assert_source_unchanged();
}

fn assert_invalid_generation(command: &str, approved_output: &str) {
    let fixture = Fixture::new(command, approved_output);
    let run_id = fixture.submit();
    fixture.work_once();
    let view = fixture.inspect(&run_id);
    assert_eq!(view["status"], "FAILED");
    assert_eq!(view["dynamic"], true);
    assert_eq!(view["sealed"], false);
    assert_eq!(view["tasks"], json!({"generator":"FAILED"}));
    assert_eq!(view["plan"]["tasks"].as_object().unwrap().len(), 1);
    assert_eq!(view["attempts"].as_object().unwrap().len(), 1);
    let attempt = view["attempts"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(attempt["task_id"], "generator");
    assert_eq!(attempt["status"], "COMPLETED");
    assert_eq!(attempt["report"]["success"], false);
    assert!(attempt.get("generated").is_none());
    // A second worker poll must not discover an accidentally published child/retry.
    fixture.work_once();
    assert_eq!(fixture.inspect(&run_id), view);
    fixture.assert_source_unchanged();
}

#[test]
fn missing_generator_output_fails_without_publishing_graph() {
    assert_invalid_generation(
        "printf 'generator ran without output\\n'",
        "{\"tasks\":{},\"seal\":true}",
    );
}

#[test]
fn malformed_generator_output_fails_without_publishing_graph() {
    assert_invalid_generation(
        "cp approved.tasks.json \"$GPIPE_TASKS_OUTPUT\"",
        "{not-json",
    );
}
