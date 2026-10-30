#![cfg(all(feature = "runtime", target_os = "linux"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Native execution of the preparatory Clippier Rust suite on a dependency-free
//! fixture, not evidence of a full `MoosicBox` build or full action parity.
//! Requires existing Cargo, rustc and Clippy on PATH (explicit `system` recipe).
//! The test never installs tools or fetches dependencies; Cargo is forced offline.

use std::{
    collections::BTreeMap,
    io::{Read, Seek},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use gpipe::clippier::{CASE_JSON_ENV, JOB_JSON_ENV, MatrixPlan, Receipt, SEED_ENV};
use reqwest::blocking::Client;
use serde_json::{Value, json};

const ADMIN_ENV: &str = "GPIPE_CLIPPIER_E2E_ADMIN";
const WORKER_ENV: &str = "GPIPE_CLIPPIER_E2E_WORKER";
const CASE_ID: &str = "case-000000";

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run(command: &mut Command, timeout: Duration) -> String {
    // Files avoid pipe deadlocks while the parent waits on process completion.
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let mut process = Process(
        command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone().unwrap())
            .stderr(stderr.try_clone().unwrap())
            .spawn()
            .expect("cannot start required fixture command (git, Cargo/Clippy or gpipe)"),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "fixture subprocess timed out");
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
    assert!(
        status.success(),
        "fixture command failed ({status}): {error}\n{output}"
    );
    output
}

fn git(directory: &Path, args: &[&str]) -> String {
    run(
        Command::new("git")
            .arg("--no-pager")
            .args(args)
            .current_dir(directory)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
        Duration::from_secs(15),
    )
    .trim()
    .to_owned()
}

fn cli(args: &[&str], envs: &BTreeMap<&str, &str>) -> Value {
    serde_json::from_str(&run(
        Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args(args)
            .envs(envs),
        Duration::from_secs(15),
    ))
    .unwrap()
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn approved_environment(directory: &Path) -> BTreeMap<String, String> {
    let host_home =
        std::env::var_os("HOME").expect("HOME is required to locate existing Rust tools");
    let rustup_home = std::env::var_os("RUSTUP_HOME")
        .map_or_else(|| PathBuf::from(host_home).join(".rustup"), PathBuf::from);
    // Isolate Cargo configuration/cache and HOME from the developer's credentials.
    // Rustup alone points to the existing toolchain store. Executor inherits PATH.
    let home = directory.join("command-home");
    let cargo_home = directory.join("cargo-home");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&cargo_home).unwrap();
    BTreeMap::from([
        ("HOME".into(), home.to_str().unwrap().into()),
        ("CARGO_HOME".into(), cargo_home.to_str().unwrap().into()),
        ("RUSTUP_HOME".into(), rustup_home.to_str().unwrap().into()),
        ("CARGO_NET_OFFLINE".into(), "true".into()),
        ("RUSTUP_AUTO_INSTALL".into(), "0".into()),
        ("CARGO_TERM_COLOR".into(), "never".into()),
        ("CARGO_BUILD_JOBS".into(), "1".into()),
    ])
}

fn require_installed_clippy(environment: &BTreeMap<String, String>) {
    // This is a runtime prerequisite check, not a tool installation or fallback.
    // Missing system Cargo/Clippy fails this test, never silently skips execution.
    run(
        Command::new("cargo-clippy")
            .arg("--version")
            .envs(environment),
        Duration::from_secs(15),
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn clippier_rust_case_executes_offline_through_native_coordinator_and_worker() {
    let temp = tempfile::tempdir().unwrap();
    let environment = approved_environment(temp.path());
    require_installed_clippy(&environment);
    let source = temp.path().join("source");
    std::fs::create_dir_all(source.join("src")).unwrap();
    std::fs::write(
        source.join("Cargo.toml"),
        concat!(
            "[package]\nname = \"clippier_e2e_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            "[features]\ndefault = []\nfail-on-warnings = []\nfixture = []\n",
        ),
    )
    .unwrap();
    std::fs::write(source.join("src/lib.rs"), concat!(
        "#![cfg_attr(feature = \"fail-on-warnings\", deny(warnings))]\n",
        "#[cfg(not(feature = \"fixture\"))]\ncompile_error!(\"expected exact Clippier feature case\");\n",
        "#[must_use]\npub const fn fixture_value() -> u32 { 42 }\n",
        "#[cfg(test)]\nmod tests {\n",
        "    #[test]\n    fn exact_case_runs() { assert_eq!(super::fixture_value(), 42); }\n}\n",
    )).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    git(&source, &["add", "Cargo.toml", "src/lib.rs"]);
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
            "dependency-free Clippier fixture",
        ],
    );
    let revision = git(&source, &["rev-parse", "HEAD"]);

    let package = json!({"name": "clippier_e2e_fixture", "path": ".", "os": "ubuntu-latest",
        "nightly": false, "toolchain": "system", "requiredFeatures": "", "features": ["fixture"]});
    let case = json!({"id": CASE_ID, "package": package});
    let mut job = package;
    job.as_object_mut().unwrap().remove("features");
    job["cases"] = json!([case]);
    let jobs = json!([job]);
    let matrix = MatrixPlan::from_jobs_json(&jobs.to_string(), Some(42)).unwrap();
    let mapping = matrix.case_task_mapping();
    assert_eq!(mapping, BTreeMap::from([(CASE_ID.into(), CASE_ID.into())]));
    let mut recipes = matrix.rust_validation_recipes().unwrap();
    assert_eq!(recipes.len(), 1);
    let recipe = recipes.get_mut(CASE_ID).unwrap();
    assert_eq!(recipe.steps.len(), 1);
    let step = &mut recipe.steps[0];
    assert!(!step.env.contains_key("GPIPE_RUST_TOOLCHAIN"));
    assert_eq!(
        step.env["GPIPE_RUST_FEATURES_0"],
        "fail-on-warnings,fixture"
    );
    assert!(step.run.contains("check --all-targets"));
    assert!(step.run.contains("clippy --all-targets"));
    assert!(step.run.contains("test --no-default-features"));
    // Bound the unchanged generated suite; do not replace it with a success stub.
    step.timeout_secs = 90;
    let lowered = matrix
        .lower("fixture".into(), revision.clone(), recipes)
        .unwrap();
    let plan = lowered.plan();
    assert_eq!(plan.revision, revision);
    assert_eq!(plan.tasks.len(), 1);
    let task = &plan.tasks[&mapping[CASE_ID]];
    assert_eq!(
        serde_json::from_str::<Value>(&task.steps[0].env[CASE_JSON_ENV]).unwrap(),
        case
    );
    assert_eq!(
        serde_json::from_str::<Value>(&task.steps[0].env[JOB_JSON_ENV]).unwrap(),
        jobs[0]
    );
    assert_eq!(task.steps[0].env[SEED_ENV], "42");

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://{address}");
    let envs = BTreeMap::from([
        (ADMIN_ENV, "clippier-admin-fixture-token-0000000000000000"),
        (WORKER_ENV, "clippier-worker-fixture-token-000000000000000"),
    ]);
    let server_path = temp.path().join("server.json");
    write_json(
        &server_path,
        &json!({"listen": address.to_string(),
        "database": temp.path().join("state.db"), "admin_token_env": ADMIN_ENV,
        "repositories": {"fixture": {"revisions": [revision]}},
        "workers": {"native": {"token_env": WORKER_ENV, "repositories": ["fixture"],
            "labels": {"os": "ubuntu-latest"}, "cpu": 1, "memory_mb": 4096}}}),
    );
    let worker_path = temp.path().join("worker.json");
    write_json(
        &worker_path,
        &json!({"coordinator": url, "token_env": WORKER_ENV,
        "directory": temp.path().join("worker"), "repositories": {"fixture": source},
        "environment": environment, "poll_secs": 1}),
    );
    let plan_path = temp.path().join("plan.json");
    write_json(&plan_path, &serde_json::to_value(plan).unwrap());
    let mut server = Process(
        Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args(["serve", "--config", server_path.to_str().unwrap()])
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
            "coordinator exited during startup"
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
    let submitted = cli(
        &[
            "submit",
            "--url",
            &url,
            "--token-env",
            ADMIN_ENV,
            "--key",
            "clippier-one-case",
            plan_path.to_str().unwrap(),
        ],
        &envs,
    );
    assert_eq!(submitted["type"], "SUBMITTED");
    let run_id = submitted["run_id"].as_str().unwrap();
    run(
        Command::new(env!("CARGO_BIN_EXE_gpipe"))
            .args([
                "worker",
                "--config",
                worker_path.to_str().unwrap(),
                "--once",
            ])
            .envs(&envs),
        Duration::from_secs(120),
    );
    let inspected = cli(
        &["inspect", "--url", &url, "--token-env", ADMIN_ENV, run_id],
        &envs,
    );
    assert_eq!(inspected["type"], "DATA");
    let state = &inspected["value"];
    assert_eq!(state["status"], "SUCCEEDED", "{state}");
    assert_eq!(state["plan"], serde_json::to_value(plan).unwrap());
    assert_eq!(state["tasks"][&mapping[CASE_ID]], "SUCCEEDED");
    let attempts = state["attempts"].as_object().unwrap();
    assert_eq!(attempts.len(), 1);
    let attempt = attempts.values().next().unwrap();
    assert_eq!(attempt["task_id"], mapping[CASE_ID]);
    assert_eq!(attempt["report"]["success"], true);
    let log = attempt["report"]["log"].as_str().unwrap();
    assert!(
        log.len() <= 16 * 1024,
        "worker report exceeded bounded evidence budget"
    );
    assert!(
        log.contains("test result: ok."),
        "Cargo test evidence missing: {log}"
    );
    // Produce a scoped receipt only after this exact native task has succeeded.
    let receipt = Receipt {
        case: serde_json::from_value(case).unwrap(),
        exit_code: 0,
    };
    matrix.verify_receipts(&[receipt]).unwrap();
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), revision);
    assert_eq!(git(&source, &["status", "--porcelain"]), "");
}
