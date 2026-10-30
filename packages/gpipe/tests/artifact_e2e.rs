#![cfg(all(feature = "runtime", unix))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
//! Cross-worker artifact acceptance through the real CLI and loopback data plane.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const ADMIN_ENV: &str = "GPIPE_ARTIFACT_E2E_ADMIN";
const PRODUCER_ENV: &str = "GPIPE_ARTIFACT_E2E_PRODUCER";
const CONSUMER_ENV: &str = "GPIPE_ARTIFACT_E2E_CONSUMER";
const ADMIN_TOKEN: &str = "artifact-test-admin-000000000000000000000000";
const PRODUCER_TOKEN: &str = "artifact-test-producer-00000000000000000000";
const CONSUMER_TOKEN: &str = "artifact-test-consumer-00000000000000000000";
const TIMEOUT: Duration = Duration::from_secs(30);
const CONTENTS: &str = "artifact bytes from isolated producer\n";

struct ProcessGuard(Child);

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // SAFETY: every guarded child starts in a new process group with its PID
        // as group ID; never signal the caller's group.
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
    file.take(256 * 1024 + 1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() <= 256 * 1024, "fixture output too large");
    bytes
}

struct RunningCommand {
    process: ProcessGuard,
    stdout: File,
    stderr: File,
}

impl RunningCommand {
    fn spawn(command: &mut Command) -> Self {
        let stdout = tempfile::tempfile().unwrap();
        let stderr = tempfile::tempfile().unwrap();
        let child = command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(stdout.try_clone().unwrap())
            .stderr(stderr.try_clone().unwrap())
            .spawn()
            .unwrap();
        Self {
            process: ProcessGuard(child),
            stdout,
            stderr,
        }
    }

    fn finish(mut self) -> Output {
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.process.0.try_wait().unwrap() {
                break status;
            }
            assert!(started.elapsed() < TIMEOUT, "fixture process timeout");
            thread::sleep(Duration::from_millis(20));
        };
        let output = Output {
            status,
            stdout: capture(&mut self.stdout),
            stderr: capture(&mut self.stderr),
        };
        assert!(
            output.status.success(),
            "fixture process failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = RunningCommand::spawn(
        Command::new("git")
            .arg("--no-pager")
            .args(args)
            .current_dir(directory)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
    )
    .finish();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gpipe"));
    command
        .env(ADMIN_ENV, ADMIN_TOKEN)
        .env(PRODUCER_ENV, PRODUCER_TOKEN)
        .env(CONSUMER_ENV, CONSUMER_TOKEN);
    command
}

fn cli_json(args: &[&str]) -> Value {
    let output = RunningCommand::spawn(command().args(args)).finish();
    serde_json::from_slice(&output.stdout).unwrap()
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn start_server(config: &Path, address: SocketAddr) -> RunningCommand {
    let mut server =
        RunningCommand::spawn(command().args(["serve", "--config", config.to_str().unwrap()]));
    let started = Instant::now();
    loop {
        if let Some(status) = server.process.0.try_wait().unwrap() {
            panic!(
                "server exited {status}: {}",
                String::from_utf8_lossy(&capture(&mut server.stderr))
            );
        }
        if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
            return server;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "server startup timeout"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn task(role: &str, needs: &[&str], run: &str, artifacts: &Value) -> Value {
    json!({"needs":needs,"labels":{"role":role},"cpu":1,"memory_mb":128,"max_attempts":1,
        "steps":[{"shell":"sh","run":run,"timeout_secs":15,
            "env":{"GPIPE_ARTIFACTS":artifacts.to_string()}}]})
}

struct Fixture {
    // Server must stop before its temp directory is removed, even on assertion panic.
    _server: RunningCommand,
    temp: tempfile::TempDir,
    source: PathBuf,
    url: String,
    run_id: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "--initial-branch=main"]);
        std::fs::write(source.join("input.txt"), "approved immutable source\n").unwrap();
        git(&source, &["add", "input.txt"]);
        git(
            &source,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "artifact fixture",
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
                "listen":address.to_string(),"database":temp.path().join("state.db"),"admin_token_env":ADMIN_ENV,
                "repositories":{"fixture":{"revisions":[revision]}},
                "workers":{
                    "producer-worker":{"token_env":PRODUCER_ENV,"repositories":["fixture"],"labels":{"role":"producer"},"cpu":1,"memory_mb":128},
                    "consumer-worker":{"token_env":CONSUMER_ENV,"repositories":["fixture"],"labels":{"role":"consumer"},"cpu":1,"memory_mb":128}
                }
            }),
        );
        for (role, token) in [("producer", PRODUCER_ENV), ("consumer", CONSUMER_ENV)] {
            write_json(
                &temp.path().join(format!("{role}.json")),
                &json!({
                    "coordinator":url,"token_env":token,"directory":temp.path().join(role),
                    "repositories":{"fixture":source},"poll_secs":1
                }),
            );
        }
        let producer = task(
            "producer",
            &[],
            "set -eu; test ! -e incoming; mkdir -p output; printf 'artifact bytes from isolated producer\\n' > output/payload.txt",
            &json!({"inputs":[],"outputs":[{"name":"payload","path":"output/payload.txt"}]}),
        );
        let consumer = task(
            "consumer",
            &["producer"],
            "set -eu; test ! -e output/payload.txt; test \"$(cat incoming/payload.txt)\" = 'artifact bytes from isolated producer'; printf ready > consumer-ready; i=0; while test ! -f consumer-release; do i=$((i+1)); test \"$i\" -lt 100 || exit 1; sleep 0.1; done; printf 'consumer-verified-artifact\\n'",
            &json!({"inputs":[{"task":"producer","name":"payload","path":"incoming/payload.txt"}],"outputs":[]}),
        );
        let plan_path = temp.path().join("plan.json");
        write_json(
            &plan_path,
            &json!({"repository":"fixture","revision":revision,
            "tasks":{"producer":producer,"consumer":consumer}}),
        );
        let server = start_server(&server_config, address);
        let submitted = cli_json(&[
            "submit",
            "--url",
            &url,
            "--token-env",
            ADMIN_ENV,
            "--key",
            "cross-worker-artifact",
            plan_path.to_str().unwrap(),
        ]);
        let run_id = submitted["run_id"].as_str().unwrap().to_owned();
        Self {
            _server: server,
            temp,
            source,
            url,
            run_id,
        }
    }

    fn worker(&self, role: &str) -> RunningCommand {
        RunningCommand::spawn(
            command().args([
                "worker",
                "--config",
                self.temp
                    .path()
                    .join(format!("{role}.json"))
                    .to_str()
                    .unwrap(),
                "--once",
            ]),
        )
    }

    fn inspect(&self) -> Value {
        let response = cli_json(&[
            "inspect",
            "--url",
            &self.url,
            "--token-env",
            ADMIN_ENV,
            &self.run_id,
        ]);
        assert_eq!(response["type"], "DATA");
        response["value"].clone()
    }

    fn wait_for_consumer(&self, worker: &mut RunningCommand) -> (String, PathBuf) {
        let started = Instant::now();
        loop {
            if let Some(status) = worker.process.0.try_wait().unwrap() {
                let view = self.inspect();
                panic!(
                    "consumer exited {status} before artifact readiness; inspect={view}; stderr={}",
                    String::from_utf8_lossy(&capture(&mut worker.stderr))
                );
            }
            let view = self.inspect();
            if let Some((id, _)) =
                view["attempts"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .find(|(_, attempt)| {
                        attempt["task_id"] == "consumer" && attempt["status"] == "ACTIVE"
                    })
            {
                let workspace = self.temp.path().join("consumer/attempts").join(id);
                if workspace.join("consumer-ready").is_file() {
                    return (id.clone(), workspace);
                }
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "consumer readiness timeout"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }
}

#[test]
fn isolated_workers_transfer_only_direct_dependency_artifacts() {
    let fixture = Fixture::new();
    assert!(!fixture.source.join("output").exists());
    assert!(!fixture.source.join("incoming").exists());
    fixture.worker("producer").finish();
    let after_producer = fixture.inspect();
    assert_eq!(after_producer["status"], "RUNNING");
    assert_eq!(after_producer["tasks"]["producer"], "SUCCEEDED");
    assert_eq!(after_producer["tasks"]["consumer"], "PENDING");
    let producer_attempt = after_producer["attempts"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, attempt)| attempt["task_id"] == "producer")
        .unwrap();
    assert_eq!(producer_attempt.1["worker"], "producer-worker");
    let producer_workspace = fixture
        .temp
        .path()
        .join("producer/attempts")
        .join(producer_attempt.0);
    assert_eq!(
        std::fs::read_to_string(producer_workspace.join("output/payload.txt")).unwrap(),
        CONTENTS
    );
    let mut consumer = fixture.worker("consumer");
    let (attempt_id, consumer_workspace) = fixture.wait_for_consumer(&mut consumer);
    assert_ne!(producer_workspace, consumer_workspace);
    assert_eq!(
        std::fs::read_to_string(consumer_workspace.join("incoming/payload.txt")).unwrap(),
        CONTENTS
    );
    assert!(!consumer_workspace.join("output/payload.txt").exists());
    let instance: Value = serde_json::from_reader(
        std::fs::File::open(fixture.temp.path().join("consumer/worker-instance.json")).unwrap(),
    )
    .unwrap();
    assert_forbidden_transfers(
        &fixture.url,
        &attempt_id,
        instance["instance_id"].as_str().unwrap(),
    );
    std::fs::write(consumer_workspace.join("consumer-release"), "release").unwrap();
    consumer.finish();
    let view = fixture.inspect();
    assert_eq!(view["status"], "SUCCEEDED");
    assert_eq!(
        view["tasks"],
        json!({"producer":"SUCCEEDED","consumer":"SUCCEEDED"})
    );
    assert_eq!(view["attempts"].as_object().unwrap().len(), 2);
    let consumer_attempt = &view["attempts"][&attempt_id];
    assert_eq!(consumer_attempt["worker"], "consumer-worker");
    assert_eq!(consumer_attempt["report"]["success"], true);
    assert!(
        consumer_attempt["report"]["log"]
            .as_str()
            .unwrap()
            .contains("consumer-verified-artifact")
    );
    assert_eq!(git(&fixture.source, &["status", "--porcelain"]), "");
    assert!(!fixture.source.join("output").exists());
    assert!(!fixture.source.join("incoming").exists());
}

fn assert_forbidden_transfers(url: &str, attempt_id: &str, instance: &str) {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let authorized = gpipe::transfer::TransferRequest {
        attempt_id: attempt_id.to_owned(),
        producer_task: Some("producer".into()),
        name: "payload".into(),
        offset: 0,
        data: Vec::new(),
        finish: false,
    };
    let response = client
        .post(format!("{url}/v1/artifacts"))
        .bearer_auth(CONSUMER_TOKEN)
        .header(gpipe::transfer::WORKER_INSTANCE_HEADER, instance)
        .json(&authorized)
        .send()
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut bytes = Vec::new();
    response
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 256 * 1024);
    let response: gpipe::transfer::TransferResponse = serde_json::from_slice(&bytes).unwrap();
    let gpipe::transfer::TransferResponse::Chunk { data, done, sha256 } = response else {
        panic!("authorized dependency read did not return an artifact chunk");
    };
    assert_eq!(data, CONTENTS.as_bytes());
    assert!(done);
    assert!(sha256.is_some());
    for producer_task in ["nonexistent", "consumer"] {
        let response = client
            .post(format!("{url}/v1/artifacts"))
            .bearer_auth(CONSUMER_TOKEN)
            .json(
                &json!({"attempt_id":attempt_id,"producer_task":producer_task,
                "name":"payload","offset":0,"data":[],"finish":false}),
            )
            .send()
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::FORBIDDEN,
            "active consumer must not read nonexistent or nondependency task {producer_task}"
        );
    }
}
