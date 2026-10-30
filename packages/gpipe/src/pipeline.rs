//! Reusable, source-bound pipeline planning and a single-host native deployment.
//!
//! Planning uses a clean local clone at the exact requested commit, including the
//! definition and Clippier configuration. Running uses the normal coordinator,
//! worker and authenticated operator APIs; it is not a second task executor.
//! `system` means pre-provisioned tools (including on Nix hosts), never setup.
//!
//! CLI lifecycle (paths and exact commit identities are caller supplied):
//! ```text
//! gpipe pipeline plan .gpipe/moosicbox.json --workspace /repo --revision SHA --base-revision BASE --clippier /tools/clippier --output /artifacts/plan
//! gpipe pipeline run /artifacts/plan/prepared.json --workspace /repo --state /private/fresh-state --output /artifacts/run
//! gpipe pipeline submit /artifacts/plan/prepared.json --url https://ci.example --token-env CI_TOKEN --key unique-key
//! gpipe pipeline verify /artifacts/plan/prepared.json --url https://ci.example --token-env CI_TOKEN --run-id RUN --output /artifacts/verify
//! ```
//! Tasks remain independent; a single registered worker processes all compatible
//! cases sequentially, including remaining cases after a failed case. Inspection
//! and attempt records survive failed verification. Existing CLI inspect/cancel
//! commands operate on remote runs normally. This does not provision remote hosts.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    moosicbox::{AnalysisRequest, NativeProfile, PreparedRun, RunProfile},
    operator::OperatorClient,
    protocol::{RepositoryPolicy, Request, Response, ServerConfig, WorkerConfig, WorkerPolicy},
    transfer::ArtifactSpec,
};

const LIMIT: u64 = 64 * 1024 * 1024;
const ADMIN_ENV: &str = "GPIPE_PIPELINE_ADMIN_TOKEN";
const WORKER_ENV: &str = "GPIPE_PIPELINE_WORKER_TOKEN";

/// Strict, versioned pipeline input. Packages are always explicitly selected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineDefinition {
    /// Supported schema version: 1.
    pub version: u32,
    /// Integration policy key; currently `moosicbox`.
    pub repository: String,
    /// Official Clippier feature selection.
    pub selection: Selection,
    /// Pre-provisioned native validation policy.
    pub validation: Validation,
    /// Single-worker admission capacity and whole-run deadline.
    pub execution: Execution,
}

/// Explicit official feature-matrix selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    /// Nonempty package names; never an implicit full-workspace run.
    pub packages: Vec<String>,
    /// Clippier OS selection, also retained as the worker placement label.
    pub os: String,
    /// Deterministic feature randomization seed.
    pub seed: u64,
}

/// Native tool policy; no installation or prerequisite setup is supported here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    /// Must be `system`.
    pub toolchain: String,
    /// Require authoritative coverage evidence.
    pub coverage: bool,
    /// Must be false; unsupported setup is a planning error.
    pub allow_setup: bool,
}

/// Worker admission ceilings, not OS resource isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    /// Available CPU units; tasks retain the adapter's CPU-1 recipe.
    pub cpu: u32,
    /// Available MiB; tasks retain the adapter's memory-0 recipe.
    pub memory_mb: u64,
    /// Whole lifecycle deadline, including coordinator startup.
    pub timeout_secs: u64,
}

impl PipelineDefinition {
    /// Validate the currently supported integration and explicit selection.
    ///
    /// # Errors
    /// * Unsupported schema, repository, setup/toolchain, OS or empty selection.
    /// * Duplicate/unrepresentable package names or zero resource/deadline values.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.repository != "moosicbox" {
            return Err("expected pipeline version 1 and repository moosicbox".into());
        }
        if self.validation.toolchain != "system" || self.validation.allow_setup {
            return Err(
                "native pipeline requires pre-provisioned system tools and allow_setup=false"
                    .into(),
            );
        }
        if !matches!(self.selection.os.as_str(), "ubuntu" | "macos" | "windows") {
            return Err("selection.os must be ubuntu, macos or windows".into());
        }
        let packages = &self.selection.packages;
        if packages.is_empty()
            || packages.iter().collect::<BTreeSet<_>>().len() != packages.len()
            || packages.iter().any(|name| {
                name.is_empty()
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
            })
        {
            return Err("selection requires unique explicit package names".into());
        }
        if self.execution.cpu == 0
            || self.execution.memory_mb == 0
            || self.execution.timeout_secs == 0
        {
            return Err("execution capacities and timeout must be positive".into());
        }
        Ok(())
    }
}

/// Prepared definition, exact source tree and executable bindings, and native plan.
/// Digests detect accidental changes; this is not a signed trust attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedPipeline {
    /// Envelope version.
    pub version: u32,
    /// Definition parsed from the source commit, not the dirty worktree.
    pub definition: PipelineDefinition,
    /// Repository-relative committed definition path.
    pub definition_path: String,
    /// Definition origin: always `git-revision`.
    pub definition_source: String,
    /// Digest of exact definition bytes (origin is disclosed, never inferred).
    pub definition_sha256: String,
    /// Git tree identity at the exact source commit, binding all tracked config.
    pub source_tree: String,
    /// SHA-256 of the gpipe executable that prepared this run.
    pub executable_sha256: String,
    /// Unmodified `MoosicBox` adapter contract, used by the authoritative verifier.
    pub prepared: PreparedRun,
    /// Digest of all other envelope fields.
    pub binding_sha256: String,
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| {
            [
                char::from(TABLE[usize::from(b >> 4)]),
                char::from(TABLE[usize::from(b & 15)]),
            ]
        })
        .collect()
}
fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn bytes(path: &Path) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    File::open(path)
        .map_err(error)?
        .take(LIMIT + 1)
        .read_to_end(&mut result)
        .map_err(error)?;
    if result.len() as u64 > LIMIT {
        return Err("pipeline input exceeds 64 MiB".into());
    }
    Ok(result)
}
fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&bytes(path)?).map_err(error)
}
fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::write(path, serde_json::to_vec_pretty(value).map_err(error)?).map_err(error)
}
fn artifact_digest(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(error)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 65536].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer).map_err(error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex(&hash.finalize()))
}
fn git(workspace: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(error)?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.len() as u64 > LIMIT {
        return Err("git output exceeds input budget".into());
    }
    Ok(output.stdout)
}
fn text(value: Vec<u8>) -> Result<String, String> {
    String::from_utf8(value)
        .map(|s| s.trim().to_owned())
        .map_err(error)
}
fn root(workspace: &Path) -> Result<PathBuf, String> {
    let workspace = fs::canonicalize(workspace).map_err(error)?;
    if fs::canonicalize(text(git(&workspace, &["rev-parse", "--show-toplevel"])?)?)
        .map_err(error)?
        != workspace
    {
        return Err("--workspace must be the repository root".into());
    }
    Ok(workspace)
}
fn exact(workspace: &Path, revision: &str) -> Result<(), String> {
    if !matches!(revision.len(), 40 | 64)
        || !revision.bytes().all(|b| b.is_ascii_hexdigit())
        || text(git(
            workspace,
            &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
        )?)? != revision
    {
        return Err("revision must be an exact existing lowercase commit identity".into());
    }
    Ok(())
}
impl PreparedPipeline {
    fn binding(&self) -> Result<String, String> {
        let mut value = serde_json::to_value(self).map_err(error)?;
        value
            .as_object_mut()
            .ok_or("invalid prepared envelope")?
            .remove("binding_sha256");
        Ok(digest(&serde_json::to_vec(&value).map_err(error)?))
    }
    fn validate(&self) -> Result<(), String> {
        self.definition.validate()?;
        self.prepared.plan.validate()?;
        if self.definition_source != "git-revision" {
            return Err("unknown definition provenance".into());
        }
        let manifest = &self.prepared.manifest;
        let matrix = crate::clippier::MatrixPlan::from_jobs_json(
            &manifest.jobs.to_string(),
            Some(manifest.seed),
        )?;
        let selected: BTreeSet<_> = self
            .definition
            .selection
            .packages
            .iter()
            .map(String::as_str)
            .collect();
        let mut actual = BTreeSet::new();
        for job in matrix.jobs() {
            for case in &job.cases {
                let name = case.package["name"]
                    .as_str()
                    .ok_or("official case lacks package name")?;
                if !selected.contains(name) {
                    return Err("official analysis returned an unselected package".into());
                }
                actual.insert(name);
            }
        }
        if actual != selected {
            return Err("official analysis omitted an explicitly selected package".into());
        }
        if self.version != 1
            || self.binding_sha256 != self.binding()?
            || manifest.repository != self.definition.repository
            || manifest.packages != self.definition.selection.packages
            || manifest.seed != self.definition.selection.seed
            || manifest.profile.toolchain != self.definition.validation.toolchain
            || manifest.profile.coverage != self.definition.validation.coverage
            || manifest.profile.allow_setup
            || self.prepared.plan.repository != manifest.repository
            || self.prepared.plan.revision != manifest.revision
            || self.prepared.plan.tasks.values().any(|task| {
                task.labels.get("os").map(String::as_str)
                    != Some(self.definition.selection.os.as_str())
                    || task.cpu != 1
                    || task.memory_mb != 0
            })
        {
            return Err("prepared pipeline binding or selection mismatch".into());
        }
        Ok(())
    }
}

/// Load and validate a bounded prepared envelope.
///
/// # Errors
/// * Input cannot be read, parsed, or does not match its retained binding/policy.
pub fn load(path: &Path) -> Result<PreparedPipeline, String> {
    let prepared: PreparedPipeline = read(path)?;
    prepared.validate()?;
    Ok(prepared)
}

struct Snapshot(PathBuf);
impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `MoosicBox`'s explicit Clippier integration (not a generic inferred CI recipe).
pub struct MoosicBoxClippierAdapter;
impl MoosicBoxClippierAdapter {
    /// Plan exact source/config using official feature flags and `plan-matrix`.
    ///
    /// The definition must be tracked inside `workspace` at the selected exact
    /// revision. Its committed bytes are used, never dirty or untracked input.
    /// All source/config inputs likewise come from that exact revision. The
    /// output directory may contain existing build logs.
    ///
    /// # Errors
    /// * Missing exact commits/definition, invalid policy or nonabsolute Clippier.
    /// * Snapshot, official analysis, native prerequisites or lowering fails.
    /// * Existing pipeline artifacts or any output/digest operation fails.
    #[allow(clippy::too_many_lines)] // One source snapshot and its ordered preparation lifecycle.
    pub fn plan(
        definition: &Path,
        workspace: &Path,
        revision: &str,
        base_revision: &str,
        clippier: &Path,
        output: &Path,
    ) -> Result<PreparedPipeline, String> {
        let workspace = root(workspace)?;
        exact(&workspace, revision)?;
        exact(&workspace, base_revision)?;
        if !clippier.is_absolute() {
            return Err("--clippier must be absolute".into());
        }
        let clippier = fs::canonicalize(clippier).map_err(error)?;
        let definition = fs::canonicalize(definition).map_err(error)?;
        let definition_locator = definition
            .strip_prefix(&workspace)
            .map_err(|_| "definition must be tracked inside workspace at the exact revision")?
            .to_str()
            .ok_or("non-UTF8 definition path")?;
        let source_definition = git(
            &workspace,
            &["show", &format!("{revision}:{definition_locator}")],
        )?;
        let definition_source = "git-revision";
        let definition_value: PipelineDefinition =
            serde_json::from_slice(&source_definition).map_err(error)?;
        definition_value.validate()?;
        fs::create_dir_all(output).map_err(error)?;
        for name in [
            "prepared.json",
            "plan.json",
            "provenance.json",
            "definition.raw.json",
            "analysis.raw.json",
            "features.json",
            "jobs.json",
        ] {
            if output.join(name).exists() {
                return Err(format!("pipeline output already exists: {name}"));
            }
        }
        fs::write(output.join("definition.raw.json"), &source_definition).map_err(error)?;
        let temporary =
            std::env::temp_dir().join(format!("gpipe-plan-{}-{}", std::process::id(), nonce()?));
        fs::create_dir(&temporary).map_err(error)?;
        let snapshot = Snapshot(temporary);
        let source = snapshot.0.join("source");
        let clone = Command::new("git")
            .current_dir(&workspace)
            .args(["clone", "--quiet", "--shared", "--no-checkout", "--"])
            .arg(&workspace)
            .arg(&source)
            .output()
            .map_err(error)?;
        if !clone.status.success() {
            return Err("cannot create local exact-source analysis clone".into());
        }
        git(&source, &["checkout", "--quiet", "--detach", revision])?;
        let request = AnalysisRequest {
            revision: revision.into(),
            base_revision: base_revision.into(),
            seed: definition_value.selection.seed,
            packages: definition_value.selection.packages.clone(),
            changed_files: Vec::new(),
        };
        let mut analysis = request.command(&clippier, &source)?;
        analysis.args(["--os", &definition_value.selection.os]);
        if !source
            .join(".clippier/transforms/asio-filter.lua")
            .is_file()
        {
            let arguments: Vec<_> = analysis.get_args().map(std::ffi::OsStr::to_owned).collect();
            let mut filtered = Command::new(&clippier);
            filtered.current_dir(&source);
            let mut index = 0;
            while index < arguments.len() {
                if arguments[index] == "--transform-scripts" {
                    index += 2;
                } else {
                    filtered.arg(&arguments[index]);
                    index += 1;
                }
            }
            analysis = filtered;
        }
        // Keep the official transform whenever it exists in the selected source;
        // older source revisions without that transform use official analysis as-is.
        let raw_features = checked_output(&mut analysis, "Clippier features")?;
        fs::write(output.join("analysis.raw.json"), &raw_features).map_err(error)?;
        let mut values: Vec<Value> = serde_json::from_slice(&raw_features).map_err(error)?;
        normalize_package_paths(&mut values, &source)?;
        let features = serde_json::to_vec(&values).map_err(error)?;
        let features_path = snapshot.0.join("features.json");
        fs::write(&features_path, &features).map_err(error)?;
        let jobs_bytes = checked_output(
            Command::new(&clippier)
                .current_dir(&source)
                .arg("plan-matrix")
                .arg(&features_path),
            "Clippier plan-matrix",
        )?;
        let jobs: Value = serde_json::from_slice(&jobs_bytes).map_err(error)?;
        let profile = RunProfile {
            native: NativeProfile {
                toolchain: definition_value.validation.toolchain.clone(),
                coverage: definition_value.validation.coverage,
                clippier_executable: clippier.to_str().ok_or("non-UTF8 Clippier path")?.into(),
                allow_setup: false,
            },
            base_revision: base_revision.into(),
            scope: "packages".into(),
            packages: definition_value.selection.packages.clone(),
        };
        let prepared = crate::moosicbox::prepare(
            &jobs,
            &definition_value.repository,
            revision,
            definition_value.selection.seed,
            &clippier,
            &serde_json::to_value(profile).map_err(error)?,
        )?;
        let mut result = PreparedPipeline {
            version: 1,
            definition: definition_value,
            definition_path: definition_locator.into(),
            definition_source: definition_source.into(),
            definition_sha256: digest(&source_definition),
            source_tree: text(git(
                &workspace,
                &["rev-parse", &format!("{revision}^{{tree}}")],
            )?)?,
            executable_sha256: artifact_digest(&std::env::current_exe().map_err(error)?)?,
            prepared,
            binding_sha256: String::new(),
        };
        result.binding_sha256 = result.binding()?;
        result.validate()?;
        fs::write(output.join("features.json"), features).map_err(error)?;
        fs::write(output.join("jobs.json"), jobs_bytes).map_err(error)?;
        write(&output.join("plan.json"), &result.prepared.plan)?;
        write(
            &output.join("provenance.json"),
            &json!({"source_config": "exact git commit; dirty source/config ignored", "definition_source": result.definition_source, "revision": revision, "base_revision": base_revision, "definition_worktree_differs": bytes(&definition)? != source_definition, "source_tree": result.source_tree, "definition_sha256": result.definition_sha256, "executable_sha256": result.executable_sha256, "clippier_sha256": result.prepared.manifest.tool_digest, "task_resources": {"cpu": 1, "memory_mb": 0}, "resource_policy": "adapter reconstruction fixes task CPU=1/memory=0; definition resources configure worker admission capacity, not OS limits"}),
        )?;
        write(&output.join("prepared.json"), &result)?;
        Ok(result)
    }
}

// Only the official feature-array package path field is rewritten. Canonical
// containment rejects symlink/parent escapes without touching extension metadata.
fn normalize_package_paths(packages: &mut [Value], workspace: &Path) -> Result<(), String> {
    let root = fs::canonicalize(workspace).map_err(error)?;
    for package in packages {
        let original = package["path"]
            .as_str()
            .ok_or("Clippier case lacks package path")?;
        let canonical = fs::canonicalize(root.join(original)).map_err(error)?;
        if !canonical.is_dir() {
            return Err("Clippier package path is not a directory".into());
        }
        let relative = canonical
            .strip_prefix(&root)
            .map_err(|_| "Clippier path escapes exact source snapshot")?;
        package["path"] = if relative.as_os_str().is_empty() {
            Value::String(".".into())
        } else {
            Value::String(
                relative
                    .components()
                    .map(|component| {
                        component
                            .as_os_str()
                            .to_str()
                            .ok_or("non-UTF8 package path")
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join("/"),
            )
        };
    }
    Ok(())
}

fn checked_output(command: &mut Command, label: &str) -> Result<Vec<u8>, String> {
    let output = command.output().map_err(error)?;
    if !output.status.success() {
        return Err(format!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.len() as u64 > LIMIT {
        return Err(format!("{label} output exceeds 64 MiB"));
    }
    Ok(output.stdout)
}

/// Submit the exact prepared plan with an explicit idempotency key.
///
/// # Errors
/// * Binding, authentication, transport, admission, or response contract fails.
pub fn submit(
    prepared: &PreparedPipeline,
    client: &OperatorClient,
    key: &str,
) -> Result<String, String> {
    prepared.validate()?;
    match client.call(Request::Submit {
        key: key.into(),
        plan: prepared.prepared.plan.clone(),
        dynamic: false,
    })? {
        Response::Submitted { run_id } => Ok(run_id),
        _ => Err("unexpected pipeline submission response".into()),
    }
}

fn safe_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
    {
        return Err("unsafe evidence identity".into());
    }
    Ok(())
}

fn save_evidence(
    prepared: &PreparedPipeline,
    client: &OperatorClient,
    run_id: &str,
    output: &Path,
) -> Result<(), String> {
    let inspection = client.inspect(run_id)?;
    write(&output.join("inspection.json"), &inspection)?;
    let mut total = 0_u64;
    let mut unavailable = Vec::new();
    for (attempt_id, attempt) in inspection["attempts"]
        .as_object()
        .ok_or("inspection lacks attempts")?
    {
        safe_name(attempt_id)?;
        let task_id = attempt["task_id"].as_str().ok_or("attempt lacks task_id")?;
        let task = prepared
            .prepared
            .plan
            .tasks
            .get(task_id)
            .ok_or("unexpected task in inspection")?;
        let directory = output.join("attempts").join(attempt_id);
        fs::create_dir_all(&directory).map_err(error)?;
        write(&directory.join("attempt.json"), attempt)?;
        let mut names = BTreeSet::new();
        for key in ["GPIPE_ARTIFACTS", "GPIPE_ARTIFACTS_ALWAYS"] {
            for step in &task.steps {
                if let Some(spec) = step.env.get(key) {
                    let spec: ArtifactSpec = serde_json::from_str(spec).map_err(error)?;
                    names.extend(spec.outputs.into_iter().map(|item| item.name));
                }
            }
        }
        for name in names {
            safe_name(&name)?;
            match client.artifact(run_id, task_id, attempt_id, &name) {
                Ok(bytes) => {
                    total = total
                        .checked_add(bytes.len() as u64)
                        .ok_or("evidence budget overflow")?;
                    if total > LIMIT {
                        return Err("pipeline evidence exceeds 64 MiB".into());
                    }
                    fs::write(directory.join(&name), bytes).map_err(error)?;
                }
                Err(_) => unavailable.push(
                    json!({"attempt_id": attempt_id, "artifact": name, "status": "unavailable"}),
                ),
            }
        }
    }
    write(&output.join("unavailable-artifacts.json"), &unavailable)
}

/// Persist inspection and attempt evidence even when authoritative verification fails.
/// A failed verification writes `verified.json` with `verified:false` and returns Err.
///
/// # Errors
/// * Evidence cannot be retained or the normal CI verifier rejects any stage/case.
pub fn verify(
    prepared: &PreparedPipeline,
    client: &OperatorClient,
    run_id: &str,
    output: &Path,
) -> Result<(), String> {
    prepared.validate()?;
    fs::create_dir_all(output).map_err(error)?;
    if output.join("verified.json").exists() {
        return Err("verified.json already exists; use a fresh verification output".into());
    }
    let evidence = save_evidence(prepared, client, run_id, output);
    let result = evidence.and_then(|()| client.verify_ci(&prepared.prepared, run_id));
    match result {
        Ok(verified) => write(
            &output.join("verified.json"),
            &json!({"verified": true, "result": verified}),
        ),
        Err(message) => {
            write(
                &output.join("verified.json"),
                &json!({"verified": false, "run_id": run_id, "error": message}),
            )?;
            Err(message)
        }
    }
}

fn nonce() -> Result<String, String> {
    let mut random = [0; 32];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(error)?;
    Ok(digest(&random))
}

// Only these toolchain variables enter commands. Never forward arbitrary ambient
// environment, provider credentials, coordinator tokens or configuration files.
fn tool_environment() -> BTreeMap<String, String> {
    [
        "PATH",
        "HOME",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "TMPDIR",
        "NIX_SSL_CERT_FILE",
        "SSL_CERT_FILE",
        "NIX_CC",
        "NIX_CFLAGS_COMPILE",
        "NIX_LDFLAGS",
        "PKG_CONFIG_PATH",
        "LIBRARY_PATH",
        "LD_LIBRARY_PATH",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
    .collect()
}

struct Children(Vec<Child>, bool);
impl Drop for Children {
    fn drop(&mut self) {
        // The supervisor is a Linux child subreaper. Orphaned executor groups
        // remain its descendants even if a worker crashes before cancellation.
        if self.1 {
            kill_descendants(std::process::id());
        } else {
            for child in &self.0 {
                kill_descendants(child.id());
            }
        }
        for child in self.0.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn kill_descendants(root: u32) {
    #[cfg(unix)]
    {
        let Ok(output) = Command::new("ps")
            .args(["-axo", "pid=,ppid=,pgid="])
            .output()
        else {
            return;
        };
        let processes: Vec<(i32, i32, i32)> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                Some((
                    fields.next()?.parse().ok()?,
                    fields.next()?.parse().ok()?,
                    fields.next()?.parse().ok()?,
                ))
            })
            .collect();
        let Ok(root) = i32::try_from(root) else {
            return;
        };
        let mut owned = BTreeSet::from([root]);
        loop {
            let previous = owned.len();
            for &(pid, parent, _) in &processes {
                if owned.contains(&parent) {
                    owned.insert(pid);
                }
            }
            if previous == owned.len() {
                break;
            }
        }
        for &(pid, _, group) in processes.iter().rev() {
            if pid != root && owned.contains(&pid) {
                // SAFETY: target IDs came from this supervisor's descendant
                // tree. Kill independent executor groups as well as processes.
                unsafe {
                    if group == pid {
                        libc::kill(-group, libc::SIGKILL);
                    }
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
    #[cfg(not(unix))]
    let _ = root;
}
fn spawn(
    executable: &Path,
    args: &[&str],
    environment: &BTreeMap<String, String>,
    log: &Path,
) -> Result<Child, String> {
    let stdout = File::create(log).map_err(error)?;
    let stderr = stdout.try_clone().map_err(error)?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map_err(error)
}

/// Start a same-host deployment in a fresh private state directory.
///
/// Uses a short-lived supervisor subprocess so generated auth tokens can be
/// passed through child environment without mutating a multithreaded process's
/// environment. Tokens are never written into state, output or task environment.
///
/// # Errors
/// * Unsupported host, changed artifacts/source, nonfresh state/output, or failed run.
#[allow(clippy::too_many_lines)] // Validate all bindings before launching the supervisor.
pub fn run(
    prepared_path: &Path,
    workspace: &Path,
    state: &Path,
    output: &Path,
) -> Result<(), String> {
    let prepared = load(prepared_path)?;
    if !cfg!(unix) {
        return Err(
            "local native deployment currently requires Unix; use submit for remote workers".into(),
        );
    }
    let workspace = root(workspace)?;
    exact(&workspace, &prepared.prepared.manifest.revision)?;
    let os = prepared.definition.selection.os.as_str();
    if !((cfg!(target_os = "linux") && os == "ubuntu")
        || (cfg!(target_os = "macos") && os == "macos"))
    {
        return Err("selected OS is incompatible with this host (Linux, including Nix, supports ubuntu system profile)".into());
    }
    let executable = std::env::current_exe().map_err(error)?;
    if artifact_digest(&executable)? != prepared.executable_sha256
        || artifact_digest(Path::new(
            &prepared.prepared.manifest.profile.clippier_executable,
        ))? != prepared.prepared.manifest.tool_digest
        || text(git(
            &workspace,
            &[
                "rev-parse",
                &format!("{}^{{tree}}", prepared.prepared.manifest.revision),
            ],
        )?)? != prepared.source_tree
        || (prepared.definition_source == "git-revision"
            && digest(&git(
                &workspace,
                &[
                    "show",
                    &format!(
                        "{}:{}",
                        prepared.prepared.manifest.revision, prepared.definition_path
                    ),
                ],
            )?) != prepared.definition_sha256)
    {
        return Err("prepared source or executable artifact binding changed; re-plan".into());
    }
    fs::create_dir(state)
        .map_err(|_| "--state must be a fresh directory with an existing parent".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(state, fs::Permissions::from_mode(0o700)).map_err(error)?;
    }
    fs::create_dir_all(output).map_err(error)?;
    if output.join("verified.json").exists() || output.join("inspection.json").exists() {
        return Err("run output already contains lifecycle evidence".into());
    }
    let mut environment = tool_environment();
    environment.insert(ADMIN_ENV.into(), nonce()?);
    environment.insert(WORKER_ENV.into(), nonce()?);
    install_cancellation_handler()?;
    let mut command = Command::new(executable);
    command
        .arg("pipeline")
        .arg("__host")
        .arg(fs::canonicalize(prepared_path).map_err(error)?)
        .arg("--workspace")
        .arg(workspace)
        .arg("--state")
        .arg(fs::canonicalize(state).map_err(error)?)
        .arg("--output")
        .arg(fs::canonicalize(output).map_err(error)?)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null());
    let mut supervisor = command.spawn().map_err(error)?;
    let mut cancellation_sent = false;
    let status = loop {
        if let Some(status) = supervisor.try_wait().map_err(error)? {
            break status;
        }
        if !cancellation_sent && INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed) {
            #[cfg(unix)]
            {
                let id = i32::try_from(supervisor.id()).map_err(error)?;
                // SAFETY: sends cancellation only to our retained child.
                unsafe {
                    libc::kill(id, libc::SIGTERM);
                }
            }
            cancellation_sent = true;
        }
        thread::sleep(Duration::from_millis(100));
    };
    if status.success() {
        Ok(())
    } else {
        Err("native pipeline failed; see output lifecycle evidence and private state logs".into())
    }
}

/// Internal supervisor entry point; callers should use [`run`].
///
/// # Errors
/// * Missing private token environment, config/startup, deadline, worker or CI failure.
#[doc(hidden)]
pub fn host(
    prepared_path: &Path,
    workspace: &Path,
    state: &Path,
    output: &Path,
) -> Result<(), String> {
    let prepared = load(prepared_path)?;
    let result = host_inner(&prepared, workspace, state, output);
    if let Err(message) = &result {
        if !output.join("verified.json").exists() {
            write(
                &output.join("verified.json"),
                &json!({"verified": false, "error": message}),
            )?;
        }
        if !output.join("inspection.json").exists() {
            write(
                &output.join("inspection.json"),
                &json!({"available": false, "error": "coordinator inspection unavailable"}),
            )?;
        }
    }
    result
}

static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(unix)]
extern "C" fn interrupted(_: libc::c_int) {
    INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
}
fn install_cancellation_handler() -> Result<(), String> {
    #[cfg(unix)]
    {
        // SAFETY: the handler only writes an atomic flag; no allocation or I/O.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = interrupted as *const () as usize;
            libc::sigemptyset(&raw mut action.sa_mask);
            if libc::sigaction(libc::SIGINT, &raw const action, std::ptr::null_mut()) != 0
                || libc::sigaction(libc::SIGTERM, &raw const action, std::ptr::null_mut()) != 0
            {
                return Err("cannot install lifecycle cancellation handlers".into());
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Keep child ownership, cancellation and evidence in one scope.
fn host_inner(
    prepared: &PreparedPipeline,
    workspace: &Path,
    state: &Path,
    output: &Path,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: changes child reparenting only in this dedicated supervisor.
        if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0 {
            return Err("cannot establish lifecycle child subreaper".into());
        }
    }
    install_cancellation_handler()?;
    let started = Instant::now();
    let deadline = Duration::from_secs(prepared.definition.execution.timeout_secs);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(error)?;
    let address = listener.local_addr().map_err(error)?;
    drop(listener); // Coordinator owns binding; startup failure is detected below.
    let url = format!("http://{address}");
    let client = OperatorClient::new(&url, ADMIN_ENV)?;
    let repository = prepared.definition.repository.clone();
    let labels = [("os".into(), prepared.definition.selection.os.clone())].into();
    let server = ServerConfig {
        listen: address.to_string(),
        database: state
            .join("coordinator.sqlite")
            .to_string_lossy()
            .into_owned(),
        admin_token_env: ADMIN_ENV.into(),
        repositories: [(
            repository.clone(),
            RepositoryPolicy {
                revisions: [prepared.prepared.manifest.revision.clone()].into(),
                generation: None,
            },
        )]
        .into(),
        workers: [(
            "local".into(),
            WorkerPolicy {
                token_env: WORKER_ENV.into(),
                repositories: [repository.clone()].into(),
                labels,
                cpu: prepared.definition.execution.cpu,
                memory_mb: prepared.definition.execution.memory_mb,
            },
        )]
        .into(),
    };
    let worker = WorkerConfig {
        coordinator: url.clone(),
        token_env: WORKER_ENV.into(),
        directory: state.join("worker").to_string_lossy().into_owned(),
        repositories: [(repository, workspace.to_string_lossy().into_owned())].into(),
        environment: tool_environment(),
        poll_secs: 1,
    };
    let server_path = state.join("server.json");
    let worker_path = state.join("worker.json");
    write(&server_path, &server)?;
    write(&worker_path, &worker)?;
    let executable = std::env::current_exe().map_err(error)?;
    let mut environment = tool_environment();
    for name in [ADMIN_ENV, WORKER_ENV] {
        environment.insert(
            name.into(),
            std::env::var(name).map_err(|_| "missing private lifecycle token")?,
        );
    }
    let mut children = Children(Vec::new(), true);
    children.0.push(spawn(
        &executable,
        &[
            "serve",
            "--config",
            server_path.to_str().ok_or("non-UTF8 state")?,
        ],
        &environment,
        &state.join("coordinator.log"),
    )?);
    loop {
        if children.0[0].try_wait().map_err(error)?.is_some() {
            return Err(
                "coordinator exited during startup; see private state coordinator.log".into(),
            );
        }
        if client.call(Request::List).is_ok() {
            break;
        }
        if started.elapsed() >= deadline.min(Duration::from_secs(30)) {
            return Err("coordinator startup deadline exceeded".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let run_id = submit(
        prepared,
        &client,
        &format!("pipeline-{}", prepared.binding_sha256),
    )?;
    write(
        &output.join("run.json"),
        &json!({"run_id": run_id, "url": url, "topology": "loopback coordinator and one sequential native worker; not a multi-host deployment"}),
    )?;
    // Worker never receives the administrator token; neither token enters tasks.
    environment.remove(ADMIN_ENV);
    let execution: Result<(), String> = (|| {
        children.0.push(spawn(
            &executable,
            &[
                "worker",
                "--config",
                worker_path.to_str().ok_or("non-UTF8 state")?,
            ],
            &environment,
            &state.join("worker.log"),
        )?);
        loop {
            let inspection = client.inspect(&run_id)?;
            write(&output.join("inspection.json"), &inspection)?;
            if matches!(
                inspection["status"].as_str(),
                Some("SUCCEEDED" | "FAILED" | "CANCELLED")
            ) {
                return Ok(());
            }
            if children.0[1].try_wait().map_err(error)?.is_some() {
                return Err(
                    "worker exited before run settlement; see private state worker.log".into(),
                );
            }
            if children.0[0].try_wait().map_err(error)?.is_some() {
                return Err("coordinator exited before run settlement".into());
            }
            if INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("pipeline interrupted; cancelling active work".into());
            }
            if started.elapsed() >= deadline {
                return Err("pipeline lifecycle timeout".into());
            }
            thread::sleep(Duration::from_millis(200));
        }
    })();
    if execution.is_err() {
        let _ = client.call(Request::Cancel {
            run_id: run_id.clone(),
        });
        // Leave the worker alive to observe cancellation and kill its complete
        // executor process group. Never kill a busy worker before that cleanup.
        let cleanup = Instant::now();
        while cleanup.elapsed() < Duration::from_secs(45) {
            match client.inspect(&run_id) {
                Ok(inspection)
                    if inspection["attempts"].as_object().is_some_and(|attempts| {
                        attempts
                            .values()
                            .all(|attempt| attempt["status"] != "ACTIVE")
                    }) =>
                {
                    break;
                }
                _ => thread::sleep(Duration::from_millis(200)),
            }
        }
    }
    let verified = verify(prepared, &client, &run_id, output);
    // Children drop in reverse order, after cancellation/evidence collection.
    execution.and(verified)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> Value {
        json!({"version": 1, "repository": "moosicbox", "selection": {"packages": ["moosicbox_assert"], "os": "ubuntu", "seed": 42}, "validation": {"toolchain": "system", "coverage": true, "allow_setup": false}, "execution": {"cpu": 1, "memory_mb": 4096, "timeout_secs": 1200}})
    }

    #[test]
    fn package_paths_are_normalized_before_scheduling_without_rewriting_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("workspace");
        let package = root.join("packages/assert");
        fs::create_dir_all(&package).unwrap();
        let absolute = fs::canonicalize(&package).unwrap();
        let mut values = vec![
            json!({"path": absolute, "name": "example", "metadata": {"path": absolute}, "features": ["default"]}),
            json!({"path": "./packages/assert"}),
            json!({"path": "."}),
        ];
        normalize_package_paths(&mut values, &root).unwrap();
        assert_eq!(values[0]["path"], "packages/assert");
        assert_eq!(values[1]["path"], "packages/assert");
        assert_eq!(values[2]["path"], ".");
        assert_eq!(values[0]["metadata"]["path"], json!(absolute));
        assert_eq!(values[0]["features"], json!(["default"]));
        assert!(normalize_package_paths(&mut [json!({"path": ".."})], &root).is_err());
        assert!(normalize_package_paths(&mut [json!({"path": directory.path()})], &root).is_err());
        assert!(normalize_package_paths(&mut [json!({"path": "missing"})], &root).is_err());
        assert!(normalize_package_paths(&mut [json!({"name": "missing-path"})], &root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn package_symlink_escape_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("workspace");
        let outside = directory.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        assert!(normalize_package_paths(&mut [json!({"path": "escape"})], &root).is_err());
    }

    #[test]
    fn sha256_hex_matches_known_vector_for_current_sha2_api() {
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("artifact");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(artifact_digest(&file).unwrap(), digest(b"abc"));
    }

    #[test]
    fn strict_definition_at_every_level() {
        for section in [
            None,
            Some("selection"),
            Some("validation"),
            Some("execution"),
        ] {
            let mut input = definition();
            let object = match section {
                Some(key) => &mut input[key],
                None => &mut input,
            };
            object["typo"] = true.into();
            assert!(serde_json::from_value::<PipelineDefinition>(input).is_err());
        }
    }

    #[test]
    fn unsupported_policy_fails_closed() {
        for (section, key, value) in [
            ("validation", "allow_setup", json!(true)),
            ("validation", "toolchain", json!("stable")),
            ("selection", "packages", json!([])),
            ("selection", "packages", json!(["x", "x"])),
            ("execution", "timeout_secs", json!(0)),
            ("selection", "os", json!("anything")),
        ] {
            let mut input = definition();
            input[section][key] = value;
            assert!(
                serde_json::from_value::<PipelineDefinition>(input)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        serde_json::from_value::<PipelineDefinition>(definition())
            .unwrap()
            .validate()
            .unwrap();
    }

    #[test]
    fn evidence_names_cannot_escape_output() {
        for value in ["", ".", "..", "../secret", "a/b", "a\\b", "/tmp/x"] {
            assert!(safe_name(value).is_err());
        }
        assert!(safe_name("stage-test.receipt.json").is_ok());
    }

    #[test]
    #[allow(clippy::too_many_lines)] // End-to-end durable lifecycle transitions in one scenario.
    fn coordinator_runs_every_independent_case_after_failure_and_fences_cancelled_attempt() {
        use crate::{
            coordinator::Coordinator,
            plan::{CommandStep, Plan, Task, TaskWhen},
            protocol::{Envelope, Identity, PROTOCOL_VERSION, Report},
        };
        let directory = tempfile::tempdir().unwrap();
        let revision = "a".repeat(40);
        let config = ServerConfig {
            listen: "127.0.0.1:0".into(),
            database: "unused".into(),
            admin_token_env: ADMIN_ENV.into(),
            repositories: [(
                "moosicbox".into(),
                RepositoryPolicy {
                    revisions: [revision.clone()].into(),
                    generation: None,
                },
            )]
            .into(),
            workers: [(
                "local".into(),
                WorkerPolicy {
                    token_env: WORKER_ENV.into(),
                    repositories: ["moosicbox".into()].into(),
                    labels: BTreeMap::new(),
                    cpu: 1,
                    memory_mb: 4096,
                },
            )]
            .into(),
        };
        let mut coordinator =
            Coordinator::open(&directory.path().join("state.sqlite"), config).unwrap();
        let task = Task {
            needs: vec![],
            when: TaskWhen::Success,
            steps: vec![CommandStep {
                shell: "sh".into(),
                run: "true".into(),
                env: BTreeMap::new(),
                timeout_secs: 1,
            }],
            labels: BTreeMap::new(),
            cpu: 1,
            memory_mb: 0,
            max_attempts: 1,
        };
        let plan = Plan {
            repository: "moosicbox".into(),
            revision,
            tasks: [("first".into(), task.clone()), ("second".into(), task)].into(),
        };
        let admin = Identity::Admin;
        let worker = Identity::Worker("local".into());
        let mut call = |identity: &Identity, request| {
            coordinator.handle(
                identity,
                Envelope {
                    version: PROTOCOL_VERSION,
                    request,
                },
                1,
            )
        };
        let Response::Submitted { run_id } = call(
            &admin,
            Request::Submit {
                key: "test".into(),
                plan,
                dynamic: false,
            },
        ) else {
            panic!("submission failed")
        };
        let Response::Assignment {
            assignment: Some(first),
        } = call(&worker, Request::Claim)
        else {
            panic!("first case missing")
        };
        assert!(matches!(
            call(
                &worker,
                Request::Complete {
                    attempt_id: first.attempt_id.clone(),
                    report: Report {
                        success: false,
                        log: "stage failed".into()
                    }
                }
            ),
            Response::Ok
        ));
        let Response::Assignment {
            assignment: Some(second),
        } = call(&worker, Request::Claim)
        else {
            panic!("failure incorrectly skipped remaining case")
        };
        assert_ne!(first.task_id, second.task_id);
        assert!(matches!(
            call(
                &admin,
                Request::Cancel {
                    run_id: run_id.clone()
                }
            ),
            Response::Ok
        ));
        let response = call(
            &worker,
            Request::Complete {
                attempt_id: second.attempt_id,
                report: Report {
                    success: true,
                    log: "late success".into(),
                },
            },
        );
        assert!(!matches!(response, Response::Ok));
        let Response::Data { value } = call(&admin, Request::Inspect { run_id }) else {
            panic!("inspection missing")
        };
        assert_eq!(value["status"], "CANCELLED");
        assert_eq!(value["attempts"].as_object().unwrap().len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn child_guard_reaps_failed_lifecycle() {
        let mut guard = Children(Vec::new(), false);
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 60"]);
        let child = command.spawn().unwrap();
        let id = child.id();
        guard.0.push(child);
        drop(guard);
        #[cfg(unix)]
        {
            let id = i32::try_from(id).unwrap();
            // SAFETY: signal 0 probes existence; it does not send a signal.
            assert_eq!(unsafe { libc::kill(id, 0) }, -1);
        }
    }
}
