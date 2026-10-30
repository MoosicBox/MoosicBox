#![cfg(all(feature = "runtime", unix))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Real CLI cancellation acceptance: heartbeat revocation terminates the native
//! task process tree; unauthenticated calls and cancelled pending work fail closed.

use std::{
    collections::BTreeMap,
    io::{Read, Seek},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use reqwest::{StatusCode, blocking::Client};
use serde_json::{Value, json};

const ADMIN_ENV: &str = "GPIPE_CANCEL_E2E_ADMIN";
const WORKER_ENV: &str = "GPIPE_CANCEL_E2E_WORKER";

struct Process {
    child: Child,
    // The executor creates a dedicated task process group. The fixture publishes
    // its leader PID before readiness, letting panic cleanup cover that group too.
    task_pid: Option<PathBuf>,
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(path) = &self.task_pid
            && let Ok(text) = std::fs::read_to_string(path)
            && let Ok(pid) = text.trim().parse::<i32>()
            && pid > 1
        {
            // SAFETY: kill takes scalar arguments; this PID comes only
            // from our trusted temporary fixture's process-group leader.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait(process: &mut Process, deadline: Instant) -> ExitStatus {
    loop {
        if let Some(status) = process.child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "fixture subprocess timed out");
        thread::sleep(Duration::from_millis(25));
    }
}

fn run(command: &mut Command) -> String {
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let mut process = Process {
        child: command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone().unwrap())
            .stderr(stderr.try_clone().unwrap())
            .spawn()
            .unwrap(),
        task_pid: None,
    };
    let status = wait(&mut process, Instant::now() + Duration::from_secs(15));
    stdout.rewind().unwrap();
    stderr.rewind().unwrap();
    let mut output = String::new();
    let mut error = String::new();
    stdout
        .take(1024 * 1024)
        .read_to_string(&mut output)
        .unwrap();
    stderr.take(1024 * 1024).read_to_string(&mut error).unwrap();
    assert!(
        status.success(),
        "fixture command failed ({status}): {error}"
    );
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

fn inspect(url: &str, id: &str, envs: &BTreeMap<&str, &str>) -> Value {
    let response = cli(
        &["inspect", "--url", url, "--token-env", ADMIN_ENV, id],
        envs,
    );
    assert_eq!(response["type"], "DATA");
    response["value"].clone()
}

fn cancel(url: &str, id: &str, envs: &BTreeMap<&str, &str>) {
    assert_eq!(
        cli(
            &["cancel", "--url", url, "--token-env", ADMIN_ENV, id],
            envs
        )["type"],
        "OK"
    );
}

fn task(script: &str, environment: &Value) -> Value {
    json!({"needs": [], "steps": [{"shell": "sh", "run": script,
        "env": environment, "timeout_secs": 30}], "labels": {},
        "cpu": 1, "memory_mb": 0, "max_attempts": 1})
}

#[test]
#[allow(clippy::too_many_lines)]
fn admin_cancel_stops_running_descendants_and_prevents_pending_execution() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    std::fs::write(source.join("input.txt"), "cancellation fixture\n").unwrap();
    git(&source, &["add", "input.txt"]);
    git(
        &source,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture",
        ],
    );
    let revision = git(&source, &["rev-parse", "HEAD"]);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://{address}");
    let envs = BTreeMap::from([
        (ADMIN_ENV, "cancel-admin-test-token-000000000000000000000"),
        (WORKER_ENV, "cancel-worker-test-token-00000000000000000000"),
    ]);
    let server_path = temp.path().join("server.json");
    write_json(
        &server_path,
        &json!({"listen": address.to_string(),
        "database": temp.path().join("state.db"), "admin_token_env": ADMIN_ENV,
        "repositories": {"fixture": {"revisions": [revision]}},
        "workers": {"cancel-worker": {"token_env": WORKER_ENV, "repositories": ["fixture"],
            "labels": {}, "cpu": 1, "memory_mb": 4096}}}),
    );
    let worker_path = temp.path().join("worker.json");
    write_json(
        &worker_path,
        &json!({"coordinator": url, "token_env": WORKER_ENV,
        "directory": temp.path().join("worker"), "repositories": {"fixture": source},
        "poll_secs": 1}),
    );
    let mut server = Process {
        child: Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args(["serve", "--config", server_path.to_str().unwrap()])
            .envs(&envs)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
        task_pid: None,
    };
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            server.child.try_wait().unwrap().is_none(),
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
        assert!(Instant::now() < deadline, "coordinator startup timed out");
        thread::sleep(Duration::from_millis(25));
    }
    let ready = temp.path().join("ready");
    let late = temp.path().join("late");
    let pending = temp.path().join("pending");
    let dependent = temp.path().join("dependent");
    let task_pid = temp.path().join("task.pid");
    let environment = json!({"READY": ready, "LATE": late, "PENDING": pending,
        "DEPENDENT": dependent, "TASK_PID": task_pid});
    let mut after = task("printf forbidden > \"$DEPENDENT\"", &environment);
    after["needs"] = json!(["long"]);
    let plan_path = temp.path().join("running-plan.json");
    write_json(
        &plan_path,
        &json!({"repository": "fixture", "revision": revision, "tasks": {
            "long": task(concat!(
                "printf '%s\\n' \"$$\" > \"$TASK_PID\"\n",
                "(sleep 8; printf forbidden > \"$LATE\") &\n",
                "child=$!\nprintf ready > \"$READY\"\nwait \"$child\"\n",
            ), &environment), "after": after
        }}),
    );
    let submitted = cli(
        &[
            "submit",
            "--url",
            &url,
            "--token-env",
            ADMIN_ENV,
            "--key",
            "running-cancel",
            plan_path.to_str().unwrap(),
        ],
        &envs,
    );
    assert_eq!(submitted["type"], "SUBMITTED");
    let run_id = submitted["run_id"].as_str().unwrap();
    let mut worker = Process {
        child: Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args([
                "worker",
                "--config",
                worker_path.to_str().unwrap(),
                "--once",
            ])
            .envs(&envs)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
        task_pid: Some(task_pid),
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            worker.child.try_wait().unwrap().is_none(),
            "worker exited before readiness"
        );
        assert!(Instant::now() < deadline, "task readiness timed out");
        thread::sleep(Duration::from_millis(25));
    }
    let ready_at = Instant::now();
    let state = inspect(&url, run_id, &envs);
    assert_eq!(state["tasks"]["long"], "RUNNING");
    assert_eq!(state["tasks"]["after"], "PENDING");
    // A wrong bearer must not be able to cancel the admitted running task.
    let unauthorized = client
        .post(format!("{url}/v1/rpc"))
        .bearer_auth("wrong-token-000000000000000000000000000000")
        .json(&json!({"version": 1, "request": {"type": "CANCEL", "run_id": run_id}}))
        .send()
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(inspect(&url, run_id, &envs)["status"], "RUNNING");
    let cancelled_at = Instant::now();
    cancel(&url, run_id, &envs);
    // Current heartbeat is once/second; five seconds includes HTTP and teardown.
    let status = wait(&mut worker, cancelled_at + Duration::from_secs(5));
    assert!(status.success(), "cancel acknowledgement failed: {status}");
    assert!(cancelled_at.elapsed() < Duration::from_secs(5));
    let state = inspect(&url, run_id, &envs);
    assert_eq!(state["status"], "CANCELLED", "{state}");
    assert_eq!(state["tasks"]["long"], "CANCELLED");
    assert_eq!(state["tasks"]["after"], "CANCELLED");
    let attempts = state["attempts"].as_object().unwrap();
    assert_eq!(attempts.len(), 1);
    for attempt in attempts.values() {
        assert_eq!(attempt["task_id"], "long");
        assert_ne!(attempt["report"]["success"], true);
        assert_ne!(attempt["status"], "ACTIVE");
    }

    // A separately submitted run cancelled before claim must get no attempt.
    let pending_path = temp.path().join("pending-plan.json");
    write_json(
        &pending_path,
        &json!({"repository": "fixture", "revision": revision,
        "tasks": {"pending": task("printf forbidden > \"$PENDING\"", &environment)}}),
    );
    let submitted = cli(
        &[
            "submit",
            "--url",
            &url,
            "--token-env",
            ADMIN_ENV,
            "--key",
            "pending-cancel",
            pending_path.to_str().unwrap(),
        ],
        &envs,
    );
    assert_eq!(submitted["type"], "SUBMITTED");
    let pending_id = submitted["run_id"].as_str().unwrap();
    cancel(&url, pending_id, &envs);
    run(Command::new(env!("CARGO_BIN_EXE_gpipe"))
        .args([
            "worker",
            "--config",
            worker_path.to_str().unwrap(),
            "--once",
        ])
        .envs(&envs));
    let state = inspect(&url, pending_id, &envs);
    assert_eq!(state["status"], "CANCELLED");
    assert_eq!(state["tasks"]["pending"], "CANCELLED");
    assert!(state["attempts"].as_object().unwrap().is_empty());
    // Wait beyond the background child's scheduled write, without using the guard
    // to kill it: a leaked descendant must make this assertion fail naturally.
    while ready_at.elapsed() < Duration::from_secs(9) {
        assert!(!late.exists(), "cancelled descendant wrote the late marker");
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        !late.exists(),
        "cancelled descendant survived tree termination"
    );
    assert!(!pending.exists(), "cancelled pending task executed");
    assert!(!dependent.exists(), "cancelled dependent task executed");
    assert_eq!(git(&source, &["status", "--porcelain"]), "");
}
