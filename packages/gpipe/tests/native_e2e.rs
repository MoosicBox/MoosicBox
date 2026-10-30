#![cfg(feature = "runtime")]
//! Product-path acceptance: real CLI, HTTP, SQLite and native source checkout.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("--no-pager")
        .args(args)
        .current_dir(directory)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn cli(args: &[&str], envs: &BTreeMap<&str, &str>) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_gpipe"))
        .args(args)
        .envs(envs)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
#[cfg(unix)]
fn webhook_independent_native_run_survives_coordinator_restart() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    std::fs::write(source.join("input.txt"), "immutable input\n").unwrap();
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
            "fixture",
        ],
    );
    let revision = git(&source, &["rev-parse", "HEAD"]);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://{address}");
    let admin = "admin-test-token-000000000000000000000000";
    let worker = "worker-test-token-00000000000000000000000";
    let envs = BTreeMap::from([("GPIPE_E2E_ADMIN", admin), ("GPIPE_E2E_WORKER", worker)]);
    let config = json!({"listen":address.to_string(),"database":temp.path().join("state.db"),"admin_token_env":"GPIPE_E2E_ADMIN",
        "repositories":{"fixture":{"revisions":[revision]}},
        "workers":{"native":{"token_env":"GPIPE_E2E_WORKER","repositories":["fixture"],"labels":{"os":"linux"},"cpu":2,"memory_mb":4096}}});
    let server_path = temp.path().join("server.json");
    std::fs::write(&server_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let start = || {
        let child = Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args(["serve", "--config", server_path.to_str().unwrap()])
            .envs(&envs)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut server = Server(child);
        let started = Instant::now();
        loop {
            if std::net::TcpStream::connect(address).is_ok() {
                break;
            }
            if let Some(status) = server.0.try_wait().unwrap() {
                let mut error = String::new();
                server
                    .0
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut error)
                    .unwrap();
                panic!("server exited {status}: {error}");
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "server startup timeout"
            );
            thread::sleep(Duration::from_millis(25));
        }
        server
    };
    let server = start();
    let plan = json!({"repository":"fixture","revision":revision,"tasks":{
        "build":{"needs":[],"steps":[{"shell":"sh","run":"test \"$(cat input.txt)\" = 'immutable input'; printf built","env":{},"timeout_secs":10}],"labels":{"os":"linux"},"cpu":1,"memory_mb":128,"max_attempts":1},
        "test":{"needs":["build"],"steps":[{"shell":"sh","run":"printf tested","env":{},"timeout_secs":10}],"labels":{"os":"linux"},"cpu":1,"memory_mb":128,"max_attempts":1}}});
    let plan_path = temp.path().join("plan.json");
    std::fs::write(&plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let submit = cli(
        &[
            "submit",
            "--url",
            &url,
            "--token-env",
            "GPIPE_E2E_ADMIN",
            "--key",
            "native-e2e",
            plan_path.to_str().unwrap(),
        ],
        &envs,
    );
    let run_id = submit["run_id"].as_str().unwrap();
    assert_eq!(
        submit,
        cli(
            &[
                "submit",
                "--url",
                &url,
                "--token-env",
                "GPIPE_E2E_ADMIN",
                "--key",
                "native-e2e",
                plan_path.to_str().unwrap()
            ],
            &envs
        )
    );
    let worker_path = temp.path().join("worker.json");
    std::fs::write(&worker_path,serde_json::to_vec(&json!({"coordinator":url,"token_env":"GPIPE_E2E_WORKER","directory":temp.path().join("worker"),"repositories":{"fixture":source},"poll_secs":1})).unwrap()).unwrap();
    let run_worker = || {
        let output = Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args([
                "worker",
                "--config",
                worker_path.to_str().unwrap(),
                "--once",
            ])
            .envs(&envs)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "worker failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run_worker();
    drop(server);
    let _restarted = start();
    run_worker();
    let inspect = cli(
        &[
            "inspect",
            "--url",
            &url,
            "--token-env",
            "GPIPE_E2E_ADMIN",
            run_id,
        ],
        &envs,
    );
    let text = serde_json::to_string(&inspect).unwrap();
    assert!(text.contains("SUCCEEDED"), "unexpected final state: {text}");
    assert!(!text.contains("FAILED"), "unexpected failure: {text}");
    assert_eq!(git(&source, &["status", "--porcelain"]), "");
}
