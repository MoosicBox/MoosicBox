//! Native execution of the repository's Rust validation suite.
//!
//! The stage order and feature strategies mirror
//! `.github/clippier/run-matrix/rust-validation.yml`: fresh target, sequential
//! clippy/tests/doctests, combined coverage, fresh target, format, machete.
//! Sequential means **one invocation per serialized feature entry**, not a union.
//! Only coverage uses the explicitly combined strategy. No GitHub YAML or package
//! metadata is evaluated as shell code. The coverage shell script is trusted
//! repository code, located from the explicit workspace; its legacy workspace
//! environment variable is supplied explicitly, never inherited.
//!
//! Setup is deliberately fail-closed: this executor does not install software,
//! run arbitrary CI setup, fetch submodules, or promise a disposable runner.
//! `allow_setup` is consent, not evidence that such facilities are implemented.
//! Builds use two newly created, exclusively owned target directories instead of
//! deleting a developer's target directory. Cargo and package build scripts are
//! trusted code, not sandboxed. Callers must serialize execution in a checkout.

use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::matrix_schedule::Case;

const LOG_LIMIT: usize = 1024 * 1024;
const SUITE: &str = ".github/clippier/run-matrix/rust-validation.yml";
const COVERAGE_HELPER: &str = ".github/clippier/run-matrix/coverage.sh";
const COVERAGE_CLASSIFIER: &str = ".github/clippier/run-matrix/coverage_evidence.py";
// Changes to the shared suite require reviewing the native mapping and parity tests.
const SUITE_SNAPSHOT: &str =
    include_str!("../../../.github/clippier/run-matrix/rust-validation.yml");

/// Explicit native execution policy. `system` does not invoke rustup (Nix-safe).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProfile {
    /// One of `system`, `stable`, or `nightly`; named toolchains must exist already.
    pub toolchain: String,
    /// Collect instrumented tests and require strict coverage evidence.
    pub coverage: bool,
    /// Existing Clippier binary used for the real machete check.
    pub clippier_executable: String,
    /// Consent for supported setup only; never permits destructive disk cleanup.
    pub allow_setup: bool,
}

/// A prerequisite, including unchanged setup metadata when execution is blocked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRequirement {
    /// Tool, policy, or metadata field name.
    pub name: String,
    /// Human-readable reason or exact serialized metadata.
    pub detail: String,
    /// Execution must refuse this requirement; it cannot be silently skipped.
    pub blocking: bool,
}

/// An ordered command or explicit executor operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCommand {
    /// Stable log/evidence identity.
    pub stage: String,
    /// `command`, `fresh-target`, `doctest`, or `coverage`.
    pub operation: String,
    /// Executable, without shell expansion; empty for executor operations.
    pub program: String,
    /// Literal argv (coverage output is supplied by the executor).
    pub args: Vec<String>,
    /// Literal package environment, parsed from Clippier's JSON serialization.
    pub env: BTreeMap<String, String>,
    /// Workspace-relative working directory.
    pub directory: String,
    /// Exclusively owned target generation (1 or 2).
    pub target: u8,
}

/// Serializable, reviewable plan retaining the exact official case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePlan {
    /// Unmodified case, including metadata and feature entry boundaries.
    pub case: Case,
    /// Unmodified execution policy.
    pub profile: NativeProfile,
    /// Ordered suite operations; execution revalidates these against `plan`.
    pub commands: Vec<NativeCommand>,
    /// Explicit tools and unsupported/unapproved setup.
    pub requirements: Vec<NativeRequirement>,
}

/// Terminal evidence for one operation. Skips are successful only for no-lib docs
/// and explicitly disabled coverage; blocked operations have a nonzero status.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStageOutcome {
    /// Matching plan command identity (or `preflight`).
    pub stage: String,
    /// Zero on success; 1 for executor errors, 126 for refused execution.
    pub exit_code: i32,
    /// Explicit success, failure, skip, or blocking reason.
    pub detail: String,
    /// Output-relative stdout log.
    pub stdout: String,
    /// Output-relative stderr log.
    pub stderr: String,
    /// SHA-256 of the retained, bounded stdout log.
    pub stdout_sha256: String,
    /// SHA-256 of the retained, bounded stderr log.
    pub stderr_sha256: String,
    /// True if either stream exceeded the retained log limit.
    pub truncated: bool,
}

/// Auditable terminal result, compatible with official case reconciliation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeReceipt {
    /// The exact official case, never reconstructed from a feature union.
    pub case: Case,
    /// Zero only when every validation and required evidence check succeeded.
    pub exit_code: i32,
    /// Outcomes in execution order, including all failures.
    pub stages: Vec<NativeStageOutcome>,
    /// Required output-relative files, including logs, plan, and receipt itself.
    pub required_evidence_paths: Vec<String>,
    /// SHA-256 of the exact persisted plan bytes.
    pub plan_sha256: String,
}

fn text<'a>(case: &'a Case, key: &str) -> Result<&'a str, String> {
    case.package
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}

fn relative(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.contains(['\\', ':', '\0', '\n', '\r'])
        || Path::new(value)
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("Not a workspace-relative path: {value:?}"));
    }
    Ok(())
}

fn strings(value: Option<&Value>, name: &str) -> Result<Vec<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(s)) => Ok(s
            .split([',', '\n'])
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{name} entries must be strings"))
            })
            .collect(),
        _ => Err(format!("{name} must be an array or serialized string")),
    }
}

fn feature(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value.bytes().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'/' | b'?' | b'.' | b',')
        })
        || value.split(',').any(str::is_empty)
    {
        return Err(format!("Unsupported feature entry: {value:?}"));
    }
    Ok(())
}

// Clippier serializes env as newline-separated NAME=<JSON value>, not shell.
fn environment(case: &Case) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    let Some(value) = case.package.get("env").filter(|v| !v.is_null()) else {
        return Ok(result);
    };
    let serialized = value.as_str().ok_or("env must be a serialized string")?;
    for line in serialized.lines().filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once('=').ok_or("env needs NAME=<JSON value>")?;
        if name.is_empty()
            || !name
                .bytes()
                .enumerate()
                .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
            || name.starts_with("CARGO_")
            || name.starts_with("RUSTUP_")
            || name.starts_with("LD_")
            || name.starts_with("DYLD_")
            || matches!(
                name,
                "PATH"
                    | "HOME"
                    | "RUSTC"
                    | "RUSTDOC"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
                    | "LLVM_PROFILE_FILE"
                    | "GITHUB_WORKSPACE"
                    | "BASH_ENV"
                    | "ENV"
                    | "SHELLOPTS"
                    | "RUNNER_TEMP"
            )
        {
            return Err(format!(
                "Unsupported executor-control environment variable: {name}"
            ));
        }
        let value: Value = serde_json::from_str(value)
            .map_err(|e| format!("Invalid JSON env value for {name}: {e}"))?;
        let literal = match value {
            Value::String(s) => s,
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            _ => return Err(format!("Unsupported env value for {name}")),
        };
        if literal.contains('\0') || result.insert(name.to_owned(), literal).is_some() {
            return Err(format!(
                "Duplicate or NUL-containing env assignment: {name}"
            ));
        }
    }
    Ok(result)
}

// Deliberately limited Cargo metadata grammar: no shell, feature overrides,
// manifest relocation, --config, alternate target directories, or command tails.
fn cargo_arguments(case: &Case) -> Result<Vec<String>, String> {
    let Some(value) = case.package.get("cargo").filter(|v| !v.is_null()) else {
        return Ok(Vec::new());
    };
    let input = value.as_str().ok_or("cargo must be a string")?;
    let tokens: Vec<_> = input.split_whitespace().collect();
    let mut output = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        match token {
            "--release" | "--locked" | "--offline" | "--frozen" | "--lib" => {
                output.push(token.to_owned());
            }
            "--bin" | "--example" | "--target" | "--profile" | "--jobs" | "-j" => {
                index += 1;
                let value = tokens.get(index).ok_or("Missing Cargo option value")?;
                if value.starts_with('-')
                    || value.is_empty()
                    || !value
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
                {
                    return Err(format!("Unsupported Cargo option value: {value:?}"));
                }
                output.extend([token.to_owned(), (*value).to_owned()]);
            }
            _ => return Err(format!("Unsupported serialized Cargo argument: {token:?}")),
        }
        index += 1;
    }
    Ok(output)
}

fn enabled(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
        && value.as_str() != Some("")
        && value.as_array().is_none_or(|a| !a.is_empty())
}

fn cargo_prefix(profile: &NativeProfile) -> Vec<String> {
    if profile.toolchain == "system" {
        Vec::new()
    } else {
        vec![format!("+{}", profile.toolchain)]
    }
}

/// Plan the exact Rust validation suite without executing or installing anything.
///
/// # Errors
///
/// * Invalid profile, package path, feature entries, serialized env or Cargo args.
/// * Metadata whose type cannot be interpreted safely.
///
/// Unsupported setup is returned as blocking requirements, not erased. Callers
/// can display these before `execute`, which also refuses tampered plans.
#[allow(clippy::too_many_lines)]
pub fn plan(case: &Case, profile: &NativeProfile) -> Result<NativePlan, String> {
    if !matches!(profile.toolchain.as_str(), "system" | "stable" | "nightly") {
        return Err("toolchain must be system, stable, or nightly".into());
    }
    if profile.clippier_executable.is_empty() || profile.clippier_executable.contains('\0') {
        return Err("clippier_executable must identify an existing executable".into());
    }
    let path = text(case, "path")?;
    relative(path)?;
    let name = text(case, "name")?;
    if name.is_empty() || name.starts_with('-') {
        return Err("Invalid package name".into());
    }
    let os = text(case, "os")?;
    let env = environment(case)?;
    let extra = cargo_arguments(case)?;
    let entries = strings(case.package.get("features"), "features")?;
    let required = strings(case.package.get("requiredFeatures"), "requiredFeatures")?;
    for entry in entries.iter().chain(&required) {
        feature(entry)?;
    }
    let mut requirements = Vec::new();
    for tool in ["cargo", "rustc", "rustfmt", "clippy", "clippier"] {
        requirements.push(NativeRequirement {
            name: tool.into(),
            detail: "Must already be available; no automatic installation".into(),
            blocking: false,
        });
    }
    if profile.coverage {
        for tool in ["cargo-llvm-cov", "llvm-tools", "bash", "python3"] {
            requirements.push(NativeRequirement {
                name: tool.into(),
                detail: "Required for strict coverage helper and zero-test classifier".into(),
                blocking: false,
            });
        }
    }
    for (key, value) in &case.package {
        if matches!(key.as_str(), "toolchains" | "ciToolchains") {
            // Workspace-level toolchain propagation also includes tools for
            // non-Rust validation. Classify these individually; never discard
            // the whole field because one entry happens to be irrelevant.
            for tool in strings(Some(value), key)? {
                let outside_suite = tool == "taplo";
                requirements.push(NativeRequirement {
                    name: format!("{key}:{tool}"),
                    detail: if outside_suite {
                        "Satisfied by scope: taplo validates TOML, and the shared Rust suite does not invoke it; no installation or setup required".into()
                    } else if tool == "free_disk_space" {
                        "Destructive free_disk_space setup is never permitted on a native desktop".into()
                    } else {
                        format!("Native toolchain setup unsupported (allow_setup={}): {tool}", profile.allow_setup)
                    },
                    blocking: !outside_suite,
                });
            }
            continue;
        }
        if matches!(
            key.as_str(),
            "dependencies"
                | "ciSteps"
                | "freshRunner"
                | "gitSubmodules"
                | "toolchains"
                | "ciToolchains"
                | "setup"
        ) && enabled(value)
        {
            requirements.push(NativeRequirement {
                name: key.clone(),
                detail: format!(
                    "Native setup unsupported (allow_setup={}): {value}; free_disk_space is never permitted",
                    profile.allow_setup
                ),
                blocking: true,
            });
        } else if !matches!(
            key.as_str(),
            "name"
                | "path"
                | "os"
                | "features"
                | "requiredFeatures"
                | "nightly"
                | "env"
                | "cargo"
                | "dependencies"
                | "ciSteps"
                | "freshRunner"
                | "gitSubmodules"
                | "toolchains"
                | "ciToolchains"
                | "setup"
        ) {
            requirements.push(NativeRequirement {
                name: key.clone(),
                detail: format!("Unrecognized execution metadata: {value}"),
                blocking: true,
            });
        }
    }
    let nightly = match case.package.get("nightly") {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        _ => return Err("nightly must be boolean".into()),
    };
    if nightly && profile.toolchain == "stable" {
        requirements.push(NativeRequirement {
            name: "nightly".into(),
            detail: "Case requires nightly; stable profile cannot satisfy it".into(),
            blocking: true,
        });
    }
    let host_matches = match os {
        "ubuntu" | "ubuntu-latest" | "linux" => cfg!(target_os = "linux"),
        "macos" | "macos-latest" => cfg!(target_os = "macos"),
        "windows" | "windows-latest" => cfg!(target_os = "windows"),
        _ => false,
    };
    if !host_matches {
        requirements.push(NativeRequirement {
            name: "os".into(),
            detail: format!("Case OS {os} does not match this native host"),
            blocking: true,
        });
    }
    let mut commands = Vec::new();
    let base = NativeCommand {
        stage: String::new(),
        operation: "command".into(),
        program: "cargo".into(),
        args: Vec::new(),
        env,
        directory: path.into(),
        target: 1,
    };
    commands.push(NativeCommand {
        stage: "clear-target-1".into(),
        operation: "fresh-target".into(),
        ..base.clone()
    });
    if nightly && profile.toolchain == "system" {
        commands.push(NativeCommand {
            stage: "nightly-toolchain".into(),
            operation: "nightly-check".into(),
            program: "rustc".into(),
            args: vec!["--version".into()],
            ..base.clone()
        });
    }
    let combinations = if entries.is_empty() {
        vec![String::new()]
    } else {
        entries.clone()
    };
    for stage in ["clippy", "tests", "doctests"] {
        for (index, combination) in combinations.iter().enumerate() {
            let mut features = vec!["fail-on-warnings".to_owned()];
            features.extend(required.clone());
            if !combination.is_empty() {
                features.push(combination.clone());
            }
            let mut args = cargo_prefix(profile);
            match stage {
                "clippy" => args.extend(["clippy".into(), "--all-targets".into()]),
                "tests" if profile.coverage => {
                    args.extend(["llvm-cov".into(), "test".into(), "--no-report".into()]);
                }
                "tests" => args.push("test".into()),
                _ => args.extend(["test".into(), "--doc".into()]),
            }
            args.extend([
                "--no-default-features".into(),
                "--features".into(),
                features.join(","),
            ]);
            args.extend(extra.clone());
            if stage != "clippy" {
                args.push("--no-fail-fast".into());
            }
            commands.push(NativeCommand {
                stage: format!("{stage}-{}", index + 1),
                operation: if stage == "doctests" {
                    "doctest"
                } else {
                    "command"
                }
                .into(),
                args,
                ..base.clone()
            });
        }
    }
    let mut combined = vec!["fail-on-warnings".to_owned()];
    combined.extend(required);
    combined.extend(entries);
    let mut args = cargo_prefix(profile);
    args.extend([
        "--no-default-features".into(),
        "--features".into(),
        combined.join(","),
    ]);
    args.extend(extra);
    commands.push(NativeCommand {
        stage: "coverage".into(),
        operation: if profile.coverage {
            "coverage"
        } else {
            "coverage-disabled"
        }
        .into(),
        program: "bash".into(),
        args,
        ..base.clone()
    });
    commands.push(NativeCommand {
        stage: "clear-target-2".into(),
        operation: "fresh-target".into(),
        target: 2,
        ..base.clone()
    });
    let mut args = cargo_prefix(profile);
    args.push("fmt".into());
    if matches!(os, "windows" | "windows-latest") {
        args.extend(["--package".into(), name.into()]);
    } else {
        args.push("--all".into());
    }
    args.extend(["--".into(), "--check".into()]);
    commands.push(NativeCommand {
        stage: "format".into(),
        args,
        target: 2,
        env: BTreeMap::new(),
        ..base.clone()
    });
    commands.push(NativeCommand {
        stage: "machete".into(),
        program: profile.clippier_executable.clone(),
        args: vec![
            "machete".into(),
            "check".into(),
            "--allow-incomplete".into(),
            "--manifest-path".into(),
            format!("{path}/Cargo.toml"),
        ],
        directory: ".".into(),
        target: 2,
        env: BTreeMap::new(),
        ..base
    });
    Ok(NativePlan {
        case: case.clone(),
        profile: profile.clone(),
        commands,
        requirements,
    })
}

fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    hex
}

fn json_file(path: &Path, value: &impl Serialize) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, &bytes).map_err(|e| format!("Writing {}: {e}", path.display()))?;
    Ok(bytes)
}

struct Captured {
    code: i32,
    out: Vec<u8>,
    err: Vec<u8>,
    truncated: bool,
}

fn drain(mut stream: impl Read) -> Result<(Vec<u8>, bool), String> {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let n = stream.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let keep = n.min(LOG_LIMIT - retained.len());
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep != n;
    }
    Ok((retained, truncated))
}

// Drain both pipes concurrently even after the retained limit, avoiding deadlocks
// and unbounded Command::output allocations. No shell is involved.
fn capture(command: &mut Command) -> Result<Captured, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Starting {}: {e}", command.get_program().display()))?;
    let stdout = child.stdout.take().ok_or("Missing stdout pipe")?;
    let stderr = child.stderr.take().ok_or("Missing stderr pipe")?;
    let out = thread::spawn(move || drain(stdout));
    let err = thread::spawn(move || drain(stderr));
    let status = child.wait().map_err(|e| e.to_string());
    let (out, truncated_out) = out.join().map_err(|_| "stdout reader panicked")??;
    let (err, truncated_err) = err.join().map_err(|_| "stderr reader panicked")??;
    Ok(Captured {
        code: status?.code().unwrap_or(1),
        out,
        err,
        truncated: truncated_out || truncated_err,
    })
}

fn contained(workspace: &Path, path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Resolving {}: {e}", path.display()))?;
    if !canonical.starts_with(workspace) {
        return Err(format!("Path escapes workspace: {}", path.display()));
    }
    Ok(canonical)
}

fn command_for(spec: &NativeCommand, workspace: &Path, target: &Path) -> Result<Command, String> {
    let cwd = contained(workspace, &workspace.join(&spec.directory))?;
    let mut command = Command::new(&spec.program);
    command
        .current_dir(cwd)
        .args(&spec.args)
        .envs(&spec.env)
        .env("CARGO_TARGET_DIR", target)
        .env("CARGO_BUILD_TARGET_DIR", target)
        .env("CARGO_LLVM_COV_TARGET_DIR", target.join("llvm-cov"))
        .env("RUSTUP_AUTO_INSTALL", "0");
    // Do not reuse caller-owned instrumentation outputs or compiler wrappers.
    for key in [
        "LLVM_PROFILE_FILE",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        if !spec.env.contains_key(key) {
            command.env_remove(key);
        }
    }
    Ok(command)
}

fn no_library(
    spec: &NativeCommand,
    workspace: &Path,
    target: &Path,
    profile: &NativeProfile,
    run: &mut impl FnMut(&mut Command) -> Result<Captured, String>,
) -> Result<Captured, String> {
    let mut metadata = spec.clone();
    metadata.args = cargo_prefix(profile);
    metadata.args.extend([
        "metadata".into(),
        "--format-version=1".into(),
        "--no-deps".into(),
    ]);
    let captured = run(&mut command_for(&metadata, workspace, target)?)?;
    if captured.code != 0 || captured.truncated {
        return Err(format!(
            "Doctest metadata failed or exceeded log bound: {}",
            String::from_utf8_lossy(&captured.err)
        ));
    }
    let value: Value = serde_json::from_slice(&captured.out).map_err(|e| e.to_string())?;
    let manifest = contained(
        workspace,
        &workspace.join(&spec.directory).join("Cargo.toml"),
    )?;
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .ok_or("Missing metadata packages")?;
    let package = packages
        .iter()
        .find(|p| {
            p.get("manifest_path")
                .and_then(Value::as_str)
                .is_some_and(|p| Path::new(p) == manifest)
        })
        .ok_or("Exact package manifest missing from metadata")?;
    let targets = package
        .get("targets")
        .and_then(Value::as_array)
        .ok_or("Missing metadata targets")?;
    let library = targets.iter().any(|t| {
        t.get("kind")
            .and_then(Value::as_array)
            .is_some_and(|kinds| {
                kinds
                    .iter()
                    .any(|kind| kind.as_str().is_some_and(|k| k.contains("lib")))
            })
    });
    if library {
        run(&mut command_for(spec, workspace, target)?)
    } else {
        Ok(Captured {
            code: 0,
            out: b"Skipping doctests: exact package has no library target\n".to_vec(),
            err: Vec::new(),
            truncated: false,
        })
    }
}

fn stage_result(
    output: &Path,
    stage: &str,
    result: Result<Captured, String>,
) -> Result<NativeStageOutcome, String> {
    let (captured, detail) = match result {
        Ok(mut c) => {
            c.truncated |= c.out.len() > LOG_LIMIT || c.err.len() > LOG_LIMIT;
            c.out.truncate(LOG_LIMIT);
            c.err.truncate(LOG_LIMIT);
            let detail = if c.code == 0 {
                "completed"
            } else {
                "validation failed"
            }
            .to_owned();
            (c, detail)
        }
        Err(error) => (
            Captured {
                code: 1,
                out: Vec::new(),
                err: error.as_bytes().iter().copied().take(LOG_LIMIT).collect(),
                truncated: error.len() > LOG_LIMIT,
            },
            error.chars().take(4096).collect(),
        ),
    };
    let stdout = format!("{stage}.stdout.log");
    let stderr = format!("{stage}.stderr.log");
    fs::write(output.join(&stdout), &captured.out).map_err(|e| e.to_string())?;
    fs::write(output.join(&stderr), &captured.err).map_err(|e| e.to_string())?;
    Ok(NativeStageOutcome {
        stage: stage.into(),
        exit_code: captured.code,
        detail,
        stdout,
        stderr,
        stdout_sha256: digest(&captured.out),
        stderr_sha256: digest(&captured.err),
        truncated: captured.truncated,
    })
}

/// Execute a reviewed native plan, continuing every validation after failures.
///
/// Use a **new** output directory for each execution. It owns logs, receipt,
/// coverage evidence, and isolated target generations. Nothing is recursively
/// deleted. Nonzero validation results are returned as `Ok(receipt)`, not lost in
/// an error; blocked setup produces a failed receipt and blocked stage logs.
///
/// # Errors
///
/// * Output already exists, cannot be created, or evidence cannot be persisted.
/// * Plan serialization fails. Preflight/validation failures otherwise produce
///   a persisted receipt, including missing workspace, tools, or helper files.
#[allow(clippy::too_many_lines)]
pub fn execute(
    plan: &NativePlan,
    workspace: &Path,
    output: &Path,
) -> Result<NativeReceipt, String> {
    execute_with(plan, workspace, output, capture)
}

#[allow(clippy::too_many_lines)]
fn execute_with(
    plan: &NativePlan,
    workspace: &Path,
    output: &Path,
    mut run: impl FnMut(&mut Command) -> Result<Captured, String>,
) -> Result<NativeReceipt, String> {
    fs::create_dir(output)
        .map_err(|e| format!("Output must be a new directory ({}): {e}", output.display()))?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let bytes = json_file(&output.join("plan.json"), plan)?;
    let mut receipt = NativeReceipt {
        case: plan.case.clone(),
        exit_code: 0,
        stages: Vec::new(),
        required_evidence_paths: vec!["plan.json".into(), "receipt.json".into()],
        plan_sha256: digest(&bytes),
    };
    let preflight = (|| {
        let expected = self::plan(&plan.case, &plan.profile)?;
        if &expected != plan {
            return Err("Plan differs from canonical suite; refusing modified commands".into());
        }
        let blockers: Vec<_> = plan
            .requirements
            .iter()
            .filter(|r| r.blocking)
            .map(|r| format!("{}: {}", r.name, r.detail))
            .collect();
        if !blockers.is_empty() {
            return Err(blockers.join("\n"));
        }
        let workspace = workspace
            .canonicalize()
            .map_err(|e| format!("Workspace: {e}"))?;
        contained(
            &workspace,
            &workspace.join(text(&plan.case, "path")?).join("Cargo.toml"),
        )?;
        // Pin the suite/helper inputs in the evidence bundle for auditability.
        for (source, destination) in [
            (SUITE, "rust-validation.yml"),
            (COVERAGE_HELPER, "coverage.sh"),
            (COVERAGE_CLASSIFIER, "coverage_evidence.py"),
        ] {
            if source != SUITE && !plan.profile.coverage {
                continue;
            }
            let path = contained(&workspace, &workspace.join(source))?;
            let data = fs::read(&path).map_err(|e| e.to_string())?;
            if source == SUITE && data != SUITE_SNAPSHOT.as_bytes() {
                return Err("Workspace Rust suite differs from the compiled native suite; rebuild after reviewing parity".into());
            }
            fs::write(output.join(destination), data).map_err(|e| e.to_string())?;
        }
        Ok(workspace)
    })();
    let (workspace, blocked) = match preflight {
        Ok(workspace) => (Some(workspace), None),
        Err(error) => (None, Some(error)),
    };
    if let Some(error) = &blocked {
        let mut outcome = stage_result(&output, "preflight", Err(error.clone()))?;
        outcome.exit_code = 126;
        receipt.stages.push(outcome);
    }
    let mut gate_failure = blocked.clone();
    let mut target_failures = BTreeMap::new();
    for spec in &plan.commands {
        // Untrusted deserialized stage names cannot become filesystem paths.
        if !spec
            .stage
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || spec.stage.is_empty()
        {
            continue; // preflight already failed; do not use attacker-chosen filenames
        }
        let result = match (&blocked, &workspace) {
            (Some(error), _) => Err(format!("Blocked before execution: {error}")),
            (None, Some(workspace)) => {
                let target = output.join(format!("target-{}", spec.target));
                (|| {
                    if spec.operation == "fresh-target" {
                        fs::create_dir(&target)
                            .map_err(|e| format!("Creating owned target: {e}"))?;
                        return Ok(Captured { code: 0, out: b"Using a fresh, exclusively owned target directory; no developer targets deleted\n".to_vec(), err: Vec::new(), truncated: false });
                    }
                    // An unsatisfied system-nightly check blocks compilation, not
                    // unrelated formatting/machete evidence collection.
                    if spec.target == 1
                        && spec.operation != "nightly-check"
                        && let Some(error) = &gate_failure
                    {
                        return Err(error.clone());
                    }
                    if let Some(error) = target_failures.get(&spec.target) {
                        return Err(format!("Owned target unavailable: {error}"));
                    }
                    match spec.operation.as_str() {
                        "coverage-disabled" => Ok(Captured {
                            code: 0,
                            out: b"Coverage explicitly disabled by NativeProfile\n".to_vec(),
                            err: Vec::new(),
                            truncated: false,
                        }),
                        "coverage" => {
                            // Explicit trusted script/output paths. No metadata
                            // enters shell source.
                            let mut command = {
                                let mut c = Command::new("bash");
                                c.current_dir(contained(
                                    workspace,
                                    &workspace.join(&spec.directory),
                                )?)
                                .arg(output.join("coverage.sh"))
                                .arg(output.join("coverage.lcov"))
                                .args(&spec.args)
                                .envs(&spec.env)
                                .env("GITHUB_WORKSPACE", workspace)
                                .env("RUNNER_TEMP", &output)
                                .env("CARGO_TARGET_DIR", &target)
                                .env("CARGO_BUILD_TARGET_DIR", &target)
                                .env("CARGO_LLVM_COV_TARGET_DIR", target.join("llvm-cov"))
                                .env("RUSTUP_AUTO_INSTALL", "0")
                                .env_remove("BASH_ENV")
                                .env_remove("ENV")
                                .env_remove("LLVM_PROFILE_FILE")
                                .env_remove("CARGO_ENCODED_RUSTFLAGS")
                                .env_remove("RUSTC_WRAPPER")
                                .env_remove("RUSTC_WORKSPACE_WRAPPER");
                                c
                            };
                            let mut captured = run(&mut command)?;
                            if captured.code == 0 {
                                let lcov = output.join("coverage.lcov");
                                let status = output.join("coverage.lcov.status.json");
                                let valid = if lcov.is_file() {
                                    fs::metadata(&lcov).map_err(|e| e.to_string())?.len() > 0
                                        && !status.exists()
                                } else if status.is_file() {
                                    let value: Value = serde_json::from_slice(
                                        &fs::read(status).map_err(|e| e.to_string())?,
                                    )
                                    .map_err(|e| e.to_string())?;
                                    value.get("status").and_then(Value::as_str)
                                        == Some("NO_INSTRUMENTABLE_COVERAGE")
                                        && value.get("test_count").and_then(Value::as_u64)
                                            == Some(0)
                                        && value.get("evidence").and_then(Value::as_str)
                                            == Some(
                                                "successful exact-configuration test enumeration",
                                            )
                                } else {
                                    false
                                };
                                if !valid {
                                    captured.code = 1;
                                    captured.err =
                                        b"Missing or contradictory strict coverage evidence\n"
                                            .to_vec();
                                }
                            }
                            Ok(captured)
                        }
                        "doctest" => no_library(spec, workspace, &target, &plan.profile, &mut run),
                        "nightly-check" => {
                            let mut captured = run(&mut command_for(spec, workspace, &target)?)?;
                            if captured.code == 0
                                && !String::from_utf8_lossy(&captured.out).contains("nightly")
                            {
                                captured.code = 1;
                                captured
                                    .err
                                    .extend_from_slice(b"System rustc is not nightly\n");
                            }
                            if captured.code != 0 {
                                gate_failure = Some("Required system nightly unavailable".into());
                            }
                            Ok(captured)
                        }
                        "command" => run(&mut command_for(spec, workspace, &target)?),
                        _ => Err("Unknown executor operation".into()),
                    }
                })()
            }
            (None, None) => Err("Missing canonical workspace".into()),
        };
        let mut outcome = stage_result(&output, &spec.stage, result)?;
        if spec.operation == "fresh-target" && outcome.exit_code != 0 {
            target_failures.insert(spec.target, outcome.detail.clone());
        }
        if spec.operation == "nightly-check" && outcome.exit_code != 0 {
            gate_failure = Some("Required system nightly unavailable".into());
        }
        if blocked.is_some() {
            outcome.exit_code = 126;
        }
        receipt.stages.push(outcome);
    }
    for stage in &receipt.stages {
        receipt
            .required_evidence_paths
            .extend([stage.stdout.clone(), stage.stderr.clone()]);
        if stage.exit_code != 0 && receipt.exit_code == 0 {
            receipt.exit_code = stage.exit_code;
        }
    }
    for file in [
        "rust-validation.yml",
        "coverage.sh",
        "coverage_evidence.py",
        "coverage.lcov",
        "coverage.lcov.status.json",
    ] {
        if output.join(file).is_file() {
            receipt.required_evidence_paths.push(file.into());
        }
    }
    // Adapter contract: concatenate bounded retained stage streams in suite
    // order, including refused/preflight stages. Stream the aggregate to disk
    // rather than allocating a second copy of the complete run in memory.
    let mut ci_log = fs::File::create(output.join("ci.log")).map_err(|e| e.to_string())?;
    for stage in &receipt.stages {
        writeln!(ci_log, "=== {} (exit {}) ===", stage.stage, stage.exit_code)
            .map_err(|e| e.to_string())?;
        for (label, path) in [("stdout", &stage.stdout), ("stderr", &stage.stderr)] {
            writeln!(ci_log, "--- {label} ---").map_err(|e| e.to_string())?;
            let mut source = fs::File::open(output.join(path)).map_err(|e| e.to_string())?;
            std::io::copy(&mut source, &mut ci_log).map_err(|e| e.to_string())?;
            writeln!(ci_log).map_err(|e| e.to_string())?;
        }
    }
    ci_log.flush().map_err(|e| e.to_string())?;
    receipt.required_evidence_paths.push("ci.log".into());
    // Retain helper-native evidence and expose the adapter's exact names.
    // Zero-test evidence remains a status artifact, never a fabricated LCOV.
    for (source, destination) in [
        ("coverage.lcov", "lcov.info"),
        ("coverage.lcov.status.json", "lcov.info.status.json"),
    ] {
        if output.join(source).is_file() {
            fs::copy(output.join(source), output.join(destination)).map_err(|e| e.to_string())?;
            receipt.required_evidence_paths.push(destination.into());
        }
    }
    json_file(&output.join("receipt.json"), &receipt)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> Case {
        Case { id: "case-000001".into(), package: serde_json::from_value(serde_json::json!({
            "name": "moosicbox_assert", "path": "packages/assert", "os": std::env::consts::OS,
            "features": ["default", "alpha,beta"], "requiredFeatures": ["required"], "nightly": false
        })).unwrap() }
    }

    fn profile() -> NativeProfile {
        NativeProfile {
            toolchain: "system".into(),
            coverage: true,
            clippier_executable: "clippier".into(),
            allow_setup: false,
        }
    }

    #[test]
    fn preserves_exact_case_and_entry_boundaries() {
        let case = case();
        let plan = plan(&case, &profile()).unwrap();
        assert_eq!(plan.case, case);
        let encoded = serde_json::to_vec(&plan).unwrap();
        let decoded: NativePlan = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, plan);
        for prefix in ["clippy", "tests", "doctests"] {
            let stages: Vec<_> = plan
                .commands
                .iter()
                .filter(|s| s.stage.starts_with(&format!("{prefix}-")))
                .collect();
            assert_eq!(stages.len(), 2);
            assert!(
                stages[0]
                    .args
                    .contains(&"fail-on-warnings,required,default".into())
            );
            assert!(
                stages[1]
                    .args
                    .contains(&"fail-on-warnings,required,alpha,beta".into())
            );
        }
        let coverage = plan
            .commands
            .iter()
            .find(|s| s.stage == "coverage")
            .unwrap();
        assert!(
            coverage
                .args
                .contains(&"fail-on-warnings,required,default,alpha,beta".into())
        );
    }

    #[test]
    fn empty_features_still_runs_every_check() {
        let mut case = case();
        case.package
            .insert("features".into(), serde_json::json!([]));
        let mut profile = profile();
        profile.coverage = false;
        let plan = plan(&case, &profile).unwrap();
        assert_eq!(plan.commands.len(), 8);
        let test = plan.commands.iter().find(|s| s.stage == "tests-1").unwrap();
        assert_eq!(test.args[0], "test");
        assert!(test.args.contains(&"--no-fail-fast".into()));
        assert!(test.args.contains(&"fail-on-warnings,required".into()));
        assert!(
            plan.commands
                .iter()
                .any(|s| s.operation == "coverage-disabled")
        );
    }

    #[test]
    fn setup_is_never_silently_dropped_or_destructive() {
        for key in [
            "dependencies",
            "ciSteps",
            "freshRunner",
            "gitSubmodules",
            "toolchains",
            "ciToolchains",
            "setup",
            "unknownSetup",
        ] {
            let mut case = case();
            case.package.insert(
                key.into(),
                if matches!(key, "freshRunner" | "gitSubmodules") {
                    Value::Bool(true)
                } else {
                    Value::String("free_disk_space".into())
                },
            );
            let mut profile = profile();
            profile.allow_setup = true;
            let plan = plan(&case, &profile).unwrap();
            assert!(plan.requirements.iter().any(|r| (r.name == key
                || r.name == format!("{key}:free_disk_space"))
                && r.blocking));
            assert!(!plan.commands.iter().any(|s| s.program == "rm"));
        }
    }

    #[test]
    fn official_assert_taplo_is_explicitly_outside_rust_suite() {
        let official = Case {
            id: "case-000000".into(),
            package: serde_json::from_value(serde_json::json!({
                "os": "ubuntu", "path": "packages/assert", "name": "moosicbox_assert",
                "features": ["default"], "requiredFeatures": null,
                "nightly": false, "toolchains": "taplo"
            }))
            .unwrap(),
        };
        let plan = plan(&official, &profile()).unwrap();
        assert_eq!(plan.case, official);
        let taplo = plan
            .requirements
            .iter()
            .find(|r| r.name == "toolchains:taplo")
            .unwrap();
        assert!(!taplo.blocking);
        assert!(taplo.detail.contains("Satisfied by scope"));
        assert!(
            plan.commands
                .iter()
                .all(|c| !c.args.iter().any(|a| a.starts_with('+')))
        );
        let test = plan.commands.iter().find(|c| c.stage == "tests-1").unwrap();
        assert_eq!(&test.args[..3], ["llvm-cov", "test", "--no-report"]);
        assert!(test.args.contains(&"fail-on-warnings,default".into()));
        if cfg!(target_os = "linux") {
            assert!(plan.requirements.iter().all(|r| !r.blocking));
        }
        for tools in [
            serde_json::json!("taplo\nfree_disk_space"),
            serde_json::json!(["taplo", "node"]),
        ] {
            let mut case = official.clone();
            case.package.insert("toolchains".into(), tools);
            let mixed = super::plan(&case, &profile()).unwrap();
            assert!(mixed.requirements.iter().any(|r| r.blocking));
            assert!(
                mixed
                    .requirements
                    .iter()
                    .any(|r| r.name == "toolchains:taplo" && !r.blocking)
            );
        }
    }

    #[test]
    fn rejects_shell_and_cargo_control_injection() {
        for input in [
            "--bin okay;touch /tmp/bad",
            "--config x=y",
            "--features evil",
            "--target-dir /tmp/x",
            "$(echo evil)",
            "--manifest-path other/Cargo.toml",
            "--bin $(bad)",
            "-- --ignored",
        ] {
            let mut case = case();
            case.package
                .insert("cargo".into(), Value::String(input.into()));
            assert!(plan(&case, &profile()).is_err(), "{input}");
        }
        let mut case = case();
        case.package.insert(
            "cargo".into(),
            Value::String("--bin image_helper --locked".into()),
        );
        assert!(plan(&case, &profile()).is_ok());
    }

    #[test]
    fn env_is_json_not_shell() {
        let mut case = case();
        case.package.insert(
            "env".into(),
            Value::String("TOKEN=\"$(not-executed); spaces\"\nPORT=123".into()),
        );
        let plan = plan(&case, &profile()).unwrap();
        assert_eq!(plan.commands[1].env["TOKEN"], "$(not-executed); spaces");
        for assignment in [
            "X=$(bad)",
            "PATH=\"/bad\"",
            "CARGO_TARGET_DIR=\"/tmp\"",
            "BASH_ENV=\"bad\"",
            "X=\"a\"\nX=\"b\"",
        ] {
            case.package
                .insert("env".into(), Value::String(assignment.into()));
            assert!(super::plan(&case, &profile()).is_err());
        }
    }

    #[test]
    fn paths_are_confined() {
        for path in ["../other", "/tmp/pkg", "C:\\pkg", "a/../../b", "a\\b"] {
            let mut case = case();
            case.package
                .insert("path".into(), Value::String(path.into()));
            assert!(plan(&case, &profile()).is_err());
        }
    }

    #[test]
    fn platform_format_and_explicit_toolchains() {
        let mut case = case();
        case.package
            .insert("os".into(), Value::String("windows-latest".into()));
        let mut profile = profile();
        profile.toolchain = "nightly".into();
        let windows = plan(&case, &profile).unwrap();
        let format = windows
            .commands
            .iter()
            .find(|c| c.stage == "format")
            .unwrap();
        assert_eq!(
            format.args,
            [
                "+nightly",
                "fmt",
                "--package",
                "moosicbox_assert",
                "--",
                "--check"
            ]
        );
        case.package
            .insert("os".into(), Value::String("ubuntu-latest".into()));
        let linux = plan(&case, &profile).unwrap();
        let format = linux.commands.iter().find(|c| c.stage == "format").unwrap();
        assert_eq!(format.args, ["+nightly", "fmt", "--all", "--", "--check"]);
        profile.toolchain = "system".into();
        case.package.insert("nightly".into(), Value::Bool(true));
        let system = plan(&case, &profile).unwrap();
        assert!(
            system
                .commands
                .iter()
                .any(|c| c.operation == "nightly-check")
        );
        assert!(
            system
                .commands
                .iter()
                .all(|c| !c.args.iter().any(|a| a.starts_with('+')))
        );
        profile.toolchain = "stable".into();
        assert!(
            plan(&case, &profile)
                .unwrap()
                .requirements
                .iter()
                .any(|r| r.name == "nightly" && r.blocking)
        );
    }

    #[test]
    fn logs_are_bounded_and_hashed() {
        let data = vec![b'x'; LOG_LIMIT + 100];
        let (retained, truncated) = drain(&data[..]).unwrap();
        assert_eq!(retained.len(), LOG_LIMIT);
        assert!(truncated);
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn execute_continues_after_failures_and_persists_evidence() {
        let temp = TestWorkspace::new();
        let mut case = case();
        case.package
            .insert("features".into(), serde_json::json!([]));
        case.package
            .insert("path".into(), Value::String("pkg".into()));
        let plan = plan(&case, &profile()).unwrap();
        let output = temp.0.join("run");
        let mut invocations = Vec::new();
        let receipt = execute_with(&plan, &temp.0, &output, |command| {
            let args: Vec<_> = command
                .get_args()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            invocations.push(args.clone());
            let mut result = success();
            if args.iter().any(|s| s == "metadata") {
                result.out = serde_json::to_vec(&serde_json::json!({"packages": [{
                    "manifest_path": temp.0.join("pkg/Cargo.toml"),
                    "targets": [{"kind": ["lib"]}]
                }]}))
                .unwrap();
            } else if args.iter().any(|s| s == "clippy") {
                result.code = 7;
            }
            // Deliberately no coverage file: a zero helper exit must not turn
            // missing evidence into a successful coverage stage.
            Ok(result)
        })
        .unwrap();
        assert_eq!(receipt.case, case);
        assert_eq!(receipt.exit_code, 7);
        assert!(invocations.iter().any(|a| a.iter().any(|s| s == "fmt")));
        assert!(invocations.iter().any(|a| a.iter().any(|s| s == "machete")));
        assert!(
            receipt
                .stages
                .iter()
                .any(|s| s.stage == "coverage" && s.exit_code != 0)
        );
        assert_eq!(receipt.stages.len(), plan.commands.len());
        for evidence in &receipt.required_evidence_paths {
            assert!(output.join(evidence).is_file(), "{evidence}");
        }
        let persisted: NativeReceipt =
            serde_json::from_slice(&fs::read(output.join("receipt.json")).unwrap()).unwrap();
        assert_eq!(persisted.exit_code, 7);
        assert_eq!(
            persisted.plan_sha256,
            digest(&fs::read(output.join("plan.json")).unwrap())
        );
        assert!(output.join("target-1").is_dir());
        assert!(output.join("target-2").is_dir());
        assert_eq!(
            fs::read(temp.0.join("target/desktop-sentinel")).unwrap(),
            b"keep"
        );
        assert!(
            execute_with(&plan, &temp.0, &output, |_| panic!("must not reuse output")).is_err()
        );
    }

    #[test]
    fn blocked_and_tampered_plans_execute_nothing() {
        let temp = TestWorkspace::new();
        let mut case = case();
        case.package.insert(
            "dependencies".into(),
            Value::String("sudo apt install something".into()),
        );
        let plan = plan(&case, &profile()).unwrap();
        let receipt = execute_with(&plan, &temp.0, &temp.0.join("blocked"), |_| {
            panic!("setup must block all commands")
        })
        .unwrap();
        assert_eq!(receipt.exit_code, 126);
        assert_eq!(receipt.stages.len(), plan.commands.len() + 1);
        assert!(receipt.stages.iter().all(|s| s.exit_code == 126));
        let log = fs::read_to_string(temp.0.join("blocked/ci.log")).unwrap();
        assert!(log.contains("preflight (exit 126)"));
        assert!(log.contains("machete (exit 126)"));
        let mut plan = super::plan(&self::case(), &profile()).unwrap();
        plan.commands[1].program = "malicious-executable".into();
        let receipt = execute_with(&plan, &temp.0, &temp.0.join("tampered"), |_| {
            panic!("tampered plan must not execute")
        })
        .unwrap();
        assert_eq!(receipt.exit_code, 126);
    }

    #[test]
    fn no_lib_docs_skip_only_after_exact_manifest_metadata() {
        let temp = TestWorkspace::new();
        let mut case = case();
        case.package
            .insert("features".into(), serde_json::json!([]));
        case.package
            .insert("path".into(), Value::String("pkg".into()));
        let mut profile = profile();
        profile.coverage = false;
        let plan = plan(&case, &profile).unwrap();
        let output = temp.0.join("no-lib");
        let receipt = execute_with(&plan, &temp.0, &output, |command| {
            let args: Vec<_> = command.get_args().collect();
            assert!(!args.iter().any(|s| *s == "--doc"));
            let mut result = success();
            if args.iter().any(|s| *s == "metadata") {
                result.out = serde_json::to_vec(&serde_json::json!({"packages": [{
                    "manifest_path": temp.0.join("pkg/Cargo.toml"),
                    "targets": [{"kind": ["bin"]}]
                }]}))
                .unwrap();
            }
            Ok(result)
        })
        .unwrap();
        assert_eq!(receipt.exit_code, 0);
        assert!(
            String::from_utf8(fs::read(output.join("doctests-1.stdout.log")).unwrap())
                .unwrap()
                .contains("no library target")
        );
    }

    #[test]
    fn strict_coverage_accepts_only_real_or_verified_zero_test_evidence() {
        for (index, evidence) in ["lcov", "zero", "invalid", "partial"].iter().enumerate() {
            let temp = TestWorkspace::new();
            let mut case = case();
            case.package
                .insert("features".into(), serde_json::json!([]));
            case.package
                .insert("path".into(), Value::String("pkg".into()));
            let plan = plan(&case, &profile()).unwrap();
            let output = temp.0.join(format!("coverage-{index}"));
            let receipt = execute_with(&plan, &temp.0, &output, |command| {
                let args: Vec<_> = command.get_args().map(|s| s.to_string_lossy().into_owned()).collect();
                let mut result = success();
                if args.iter().any(|s| s == "metadata") {
                    result.out = serde_json::to_vec(&serde_json::json!({"packages": [{
                        "manifest_path": temp.0.join("pkg/Cargo.toml"), "targets": [{"kind": ["bin"]}]
                    }]})).unwrap();
                }
                if command.get_program() == "bash" {
                    assert_eq!(command.get_envs().find(|(k, _)| *k == "GITHUB_WORKSPACE").unwrap().1.unwrap(), temp.0.as_os_str());
                    match *evidence {
                        "lcov" => fs::write(output.join("coverage.lcov"), "TN:\nSF:src/lib.rs\nDA:1,1\nend_of_record\n").unwrap(),
                        "zero" => { json_file(&output.join("coverage.lcov.status.json"), &serde_json::json!({
                            "status": "NO_INSTRUMENTABLE_COVERAGE", "test_count": 0,
                            "evidence": "successful exact-configuration test enumeration"
                        })).unwrap(); },
                        "invalid" => { json_file(&output.join("coverage.lcov.status.json"), &serde_json::json!({"status": "NO_INSTRUMENTABLE_COVERAGE", "test_count": 1})).unwrap(); },
                        _ => fs::write(output.join("coverage.lcov"), "").unwrap(),
                    }
                }
                Ok(result)
            }).unwrap();
            assert_eq!(receipt.exit_code == 0, matches!(*evidence, "lcov" | "zero"));
            assert!(output.join("ci.log").is_file());
            if *evidence == "lcov" {
                assert_eq!(
                    fs::read(output.join("lcov.info")).unwrap(),
                    fs::read(output.join("coverage.lcov")).unwrap()
                );
                assert!(
                    receipt
                        .required_evidence_paths
                        .contains(&"lcov.info".into())
                );
            } else if *evidence == "zero" {
                assert!(!output.join("lcov.info").exists());
                assert!(output.join("lcov.info.status.json").is_file());
            }
        }
    }

    // These execution-control tests inject subprocess results, never invoke
    // Cargo or external services, and use an exclusively created test tree.
    struct TestWorkspace(PathBuf);

    impl TestWorkspace {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "clippier-native-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("pkg")).unwrap();
            fs::write(
                root.join("pkg/Cargo.toml"),
                "[package]\nname='native_fixture'\nversion='0.1.0'\n",
            )
            .unwrap();
            fs::create_dir_all(root.join(".github/clippier/run-matrix")).unwrap();
            fs::write(root.join(SUITE), SUITE_SNAPSHOT).unwrap();
            fs::write(
                root.join(COVERAGE_HELPER),
                include_str!("../../../.github/clippier/run-matrix/coverage.sh"),
            )
            .unwrap();
            fs::write(
                root.join(COVERAGE_CLASSIFIER),
                include_str!("../../../.github/clippier/run-matrix/coverage_evidence.py"),
            )
            .unwrap();
            fs::create_dir(root.join("target")).unwrap();
            fs::write(root.join("target/desktop-sentinel"), "keep").unwrap();
            Self(root.canonicalize().unwrap())
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            // Only this test's freshly created and exclusively owned tree.
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn success() -> Captured {
        Captured {
            code: 0,
            out: Vec::new(),
            err: Vec::new(),
            truncated: false,
        }
    }

    #[test]
    fn suite_parity_is_explicit() {
        assert_eq!(
            digest(SUITE_SNAPSHOT.as_bytes()),
            "4e92a374faeca4515a9a14ec17611bda76f5b8aaca4f4089879debef81dfcdcc"
        );
        let yaml: Value = serde_yaml::from_str(SUITE_SNAPSHOT).unwrap();
        let stages = yaml.as_object().unwrap();
        assert_eq!(stages.len(), 8);
        for name in ["clippy", "tests", "doctests"] {
            assert_eq!(stages[name]["strategy"], "sequential");
            assert!(
                stages[name]["commands"]
                    .as_str()
                    .unwrap()
                    .contains("--no-default-features")
            );
        }
        for name in ["coverage", "format", "machete"] {
            assert_eq!(stages[name]["strategy"], "combined");
        }
        assert!(
            stages["machete"]["commands"]
                .as_str()
                .unwrap()
                .contains("machete check --allow-incomplete --manifest-path")
        );
        assert!(
            stages["coverage"]["commands"]
                .as_str()
                .unwrap()
                .contains("coverage.sh")
        );
        let plan = plan(&case(), &profile()).unwrap();
        let names: Vec<_> = plan.commands.iter().map(|s| s.stage.as_str()).collect();
        assert_eq!(
            names,
            [
                "clear-target-1",
                "clippy-1",
                "clippy-2",
                "tests-1",
                "tests-2",
                "doctests-1",
                "doctests-2",
                "coverage",
                "clear-target-2",
                "format",
                "machete"
            ]
        );
        let test = plan.commands.iter().find(|s| s.stage == "tests-1").unwrap();
        assert_eq!(&test.args[..3], ["llvm-cov", "test", "--no-report"]);
        let machete = plan.commands.last().unwrap();
        assert_eq!(
            machete.args,
            [
                "machete",
                "check",
                "--allow-incomplete",
                "--manifest-path",
                "packages/assert/Cargo.toml"
            ]
        );
    }
}
