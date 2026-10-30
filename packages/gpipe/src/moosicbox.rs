//! Actual MoosicBox CI adapter using a separately provisioned, trusted Clippier CLI.
//!
//! `prepare` executes only `ci-plan`, never the source tree's tools or CI commands.
//! Workers must have the same runner binary at the explicit profile path. Retain
//! the entire prepared run, not just its task graph. `verify` needs authenticated
//! artifact downloads added to coordinator inspection as `ci_evidence`; a report
//! saying that a shell exited successfully is deliberately insufficient.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    clippier::{ExecutionRecipe, MatrixPlan},
    plan::{CommandStep, Plan},
    transfer::{ArtifactOutput, ArtifactSpec},
};

const RESULT: &str = ".gpipe-ci/result";
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Inputs accepted by the shared Clippier native CI runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeProfile {
    /// Explicit Rust toolchain, not an inferred GitHub runner default.
    pub toolchain: String,
    /// Require real coverage output in addition to test execution.
    pub coverage: bool,
    /// Absolute, administrator-provisioned runner path on each selected worker.
    pub clippier_executable: String,
    /// Whether the native runner may perform case-defined prerequisite setup.
    pub allow_setup: bool,
}

/// Preparation provenance, flattened alongside the four native profile fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunProfile {
    /// Native runner profile; serialized unchanged to `ci-plan`.
    #[serde(flatten)]
    pub native: NativeProfile,
    /// Exact immutable base used for change analysis; never `HEAD~1` or a branch.
    pub base_revision: String,
    /// Explicit selection scope, for example `workspace`, `packages`, or `changes`.
    pub scope: String,
    /// Original package selection. Empty means no explicit package filter.
    pub packages: Vec<String>,
}

/// Immutable provenance needed to reproduce and reconcile one actual CI run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunManifest {
    /// Manifest schema version.
    pub version: u32,
    /// Worker repository policy key.
    pub repository: String,
    /// Exact tested source commit.
    pub revision: String,
    /// Exact analysis base commit.
    pub base_revision: String,
    /// Deterministic feature generation seed.
    pub seed: u64,
    /// Explicit analysis scope.
    pub scope: String,
    /// Original package filter, not deduplicated or inferred from cases.
    pub packages: Vec<String>,
    /// Original official jobs JSON, including all extension metadata.
    pub jobs: Value,
    /// Native runner profile.
    pub profile: NativeProfile,
    /// SHA-256 of the canonical serialized native profile.
    pub profile_digest: String,
    /// SHA-256 of resolved operations and the compiled suite/helper snapshots.
    pub suite_digest: String,
    /// Exact trusted suite/helper content digests, keyed by evidence file name.
    pub suite_files: BTreeMap<String, String>,
    /// SHA-256 of the trusted Clippier executable used to resolve those plans.
    pub tool_digest: String,
    /// Exact resolved native plan JSON for every upstream case.
    pub native_plans: BTreeMap<String, Value>,
}

/// Serializable run submission and its retained verification contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedRun {
    /// Retain with the submitted coordinator run ID.
    pub manifest: RunManifest,
    /// Fully resolved worker plan.
    pub plan: Plan,
}

/// Native `features` request using the actual workflow's supported CLI flags.
///
/// This produces feature analysis, not the GitHub action's jq/shard transforms.
/// Pass the official transformed output to `plan-matrix` before `prepare`.
/// Docker analysis and GitHub workflow-history selection remain caller concerns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisRequest {
    /// Exact source commit used by the analysis.
    pub revision: String,
    /// Exact comparison base (the caller resolves workflow history first).
    pub base_revision: String,
    /// Reproducible generation seed.
    pub seed: u64,
    /// Explicit package filter.
    pub packages: Vec<String>,
    /// Paths changed in the exact base/source comparison.
    pub changed_files: Vec<String>,
}

impl AnalysisRequest {
    /// Build a native command without shell evaluation or invented action flags.
    ///
    /// Requires Clippier's git-diff and transform features. The CLI itself reports
    /// missing features or prerequisites; this adapter never installs them.
    ///
    /// # Errors
    /// * Either revision is not an exact SHA or a list entry cannot be represented
    ///   by the native CLI's comma-delimited list syntax.
    pub fn command(&self, clippier: &Path, workspace: &Path) -> Result<Command, String> {
        exact_sha(&self.revision)?;
        exact_sha(&self.base_revision)?;
        let mut command = Command::new(clippier);
        command.current_dir(workspace).args([
            "features",
            ".",
            "--chunked",
            "15",
            "--spread",
            "--randomize",
            "--seed",
            &self.seed.to_string(),
            "--skip-features",
            "fail-on-warnings",
            "--include-reasoning",
            "--transform-scripts",
            ".clippier/transforms/asio-filter.lua",
            "--git-base",
            &self.base_revision,
            "--git-head",
            &self.revision,
            "--output",
            "json",
        ]);
        for (flag, values) in [
            ("--packages", &self.packages),
            ("--changed-files", &self.changed_files),
        ] {
            if values
                .iter()
                .any(|value| value.is_empty() || value.contains([',', '\0']))
            {
                return Err(format!("{flag} contains an unrepresentable list entry"));
            }
            if !values.is_empty() {
                command.args([flag, &values.join(",")]);
            }
        }
        Ok(command)
    }
}

/// Resolve every official case with trusted Clippier and lower it to a real task.
///
/// `profile` is a serialized [`RunProfile`]: four native fields plus mandatory
/// `base_revision`, `scope`, and `packages` provenance. It is not a runner recipe.
/// No Clippier Rust dependency is used; the native plan is a CLI JSON contract.
///
/// # Errors
/// * Source/base provenance, profile, jobs, worker path, or resolved plan is invalid.
/// * Trusted Clippier cannot be read/executed or `ci-plan` rejects a case.
/// * Filesystem preparation, JSON serialization, or gPipe plan validation fails.
pub fn prepare(
    jobs: &Value,
    repository: &str,
    revision: &str,
    seed: u64,
    clippier: &Path,
    profile: &Value,
) -> Result<PreparedRun, String> {
    exact_sha(revision)?;
    let profile: RunProfile =
        serde_json::from_value(profile.clone()).map_err(|error| error.to_string())?;
    exact_sha(&profile.base_revision)?;
    if profile.scope.trim().is_empty() || profile.native.toolchain.trim().is_empty() {
        return Err("explicit analysis scope and native toolchain are required".into());
    }
    if !absolute_executable(&profile.native.clippier_executable) {
        return Err("native profile requires an absolute worker clippier_executable path".into());
    }
    let clippier = std::fs::canonicalize(clippier)
        .map_err(|error| format!("trusted Clippier unavailable: {error}"))?;
    let tool_digest = hash(
        &std::fs::read(&clippier)
            .map_err(|error| format!("cannot digest trusted Clippier: {error}"))?,
    );
    let matrix = MatrixPlan::from_jobs_json(&encode(jobs)?, Some(seed))?;
    let temporary = TemporaryDirectory::create()?;
    let profile_path = temporary.0.join("profile.json");
    std::fs::write(&profile_path, encode(&profile.native)?).map_err(|error| error.to_string())?;
    let mut native_plans = BTreeMap::new();
    let mut recipes = BTreeMap::new();
    for job in matrix.jobs() {
        for case in &job.cases {
            let case_path = temporary.0.join("case.json");
            std::fs::write(&case_path, encode(case)?).map_err(|error| error.to_string())?;
            let output = Command::new(&clippier)
                .args(["ci-plan", "--case"])
                .arg(&case_path)
                .arg("--profile")
                .arg(&profile_path)
                .output()
                .map_err(|error| format!("ci-plan unavailable for {}: {error}", case.id))?;
            if !output.status.success() {
                return Err(format!(
                    "ci-plan rejected {}: {}",
                    case.id,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            let native: Value = serde_json::from_slice(&output.stdout)
                .map_err(|error| format!("invalid ci-plan JSON for {}: {error}", case.id))?;
            if !native.is_object()
                || native["case"]
                    != serde_json::to_value(case).map_err(|error| error.to_string())?
                || native["profile"]
                    != serde_json::to_value(&profile.native).map_err(|error| error.to_string())?
            {
                return Err(format!(
                    "ci-plan did not return the exact case/profile for {}",
                    case.id
                ));
            }
            let requirements = native["requirements"]
                .as_array()
                .ok_or("ci-plan lacks explicit prerequisites")?;
            let blockers: Vec<_> = requirements
                .iter()
                .filter(|requirement| requirement["blocking"] == true)
                .collect();
            if !blockers.is_empty() {
                return Err(format!(
                    "CI prerequisites unavailable for {}: {}",
                    case.id,
                    encode(&blockers)?
                ));
            }
            let os = case
                .package
                .get("os")
                .and_then(Value::as_str)
                .ok_or("case OS missing")?;
            recipes.insert(
                case.id.clone(),
                recipe(&native, &profile.native, &tool_digest, os)?,
            );
            native_plans.insert(case.id.clone(), native);
        }
    }
    let lowered = matrix.lower(repository.into(), revision.into(), recipes)?;
    Ok(PreparedRun {
        manifest: RunManifest {
            version: 1,
            repository: repository.into(),
            revision: revision.into(),
            base_revision: profile.base_revision,
            seed,
            scope: profile.scope,
            packages: profile.packages,
            jobs: jobs.clone(),
            profile_digest: digest_json(&profile.native)?,
            suite_digest: digest_json(&(&native_plans, suite_files()))?,
            suite_files: suite_files(),
            profile: profile.native,
            tool_digest,
            native_plans,
        },
        plan: lowered.plan().clone(),
    })
}

fn recipe(
    native: &Value,
    profile: &NativeProfile,
    digest: &str,
    os: &str,
) -> Result<ExecutionRecipe, String> {
    let windows = os.starts_with("windows");
    if !windows && !os.starts_with("macos") && !os.starts_with("ubuntu") && os != "linux" {
        return Err(format!(
            "unsupported native worker OS capability: {os}; no Linux fallback"
        ));
    }
    let spec = artifact_spec(native, false)?;
    spec.validate()?;
    let always = artifact_spec(native, true)?;
    always.validate()?;
    let mut env = BTreeMap::from([
        ("GPIPE_NATIVE_PLAN_JSON".into(), encode(native)?),
        ("GPIPE_ARTIFACTS".into(), encode(&spec)?),
        ("GPIPE_ARTIFACTS_ALWAYS".into(), encode(&always)?),
    ]);
    env.insert("GPIPE_NATIVE_TOOL_SHA256".into(), digest.into());
    let executable = quote(&profile.clippier_executable, windows)?;
    let mut run = if windows {
        let terminal = if profile.coverage {
            "if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\nif (Test-Path -LiteralPath '.gpipe-ci/result/coverage.lcov') { Copy-Item -LiteralPath '.gpipe-ci/result/coverage.lcov' -Destination '.gpipe-ci/result/coverage.evidence' } elseif (Test-Path -LiteralPath '.gpipe-ci/result/coverage.lcov.status.json') { Copy-Item -LiteralPath '.gpipe-ci/result/coverage.lcov.status.json' -Destination '.gpipe-ci/result/coverage.evidence' } else { throw 'Missing coverage evidence' }"
        } else {
            "exit $LASTEXITCODE"
        };
        format!(
            "$ErrorActionPreference = 'Stop'\nif ((Get-FileHash -Algorithm SHA256 -LiteralPath {executable}).Hash.ToLowerInvariant() -ne $env:GPIPE_NATIVE_TOOL_SHA256) {{ throw 'Clippier binary digest mismatch' }}\n[System.IO.Directory]::CreateDirectory('.gpipe-ci') | Out-Null\n[System.IO.File]::WriteAllText('.gpipe-ci/plan.json', $env:GPIPE_NATIVE_PLAN_JSON, [System.Text.UTF8Encoding]::new($false))\n& {executable} 'ci-run' '--plan' '.gpipe-ci/plan.json' '--workspace' '.' '--output' '{RESULT}'\n{terminal}"
        )
    } else {
        let checksum = if os.starts_with("macos") {
            "shasum -a 256"
        } else {
            "sha256sum"
        };
        let invocation = if profile.coverage { "" } else { "exec " };
        format!(
            "set -eu\nactual=$({checksum} {executable})\nactual=${{actual%% *}}\n[ \"$actual\" = \"$GPIPE_NATIVE_TOOL_SHA256\" ] || {{ printf '%s\\n' 'Clippier binary digest mismatch' >&2; exit 1; }}\nmkdir -p .gpipe-ci\nprintf '%s' \"$GPIPE_NATIVE_PLAN_JSON\" > .gpipe-ci/plan.json\n{invocation}{executable} 'ci-run' '--plan' '.gpipe-ci/plan.json' '--workspace' '.' '--output' '{RESULT}'"
        )
    };
    if profile.coverage && !windows {
        run.push_str("\nif [ -f .gpipe-ci/result/coverage.lcov ]; then cp .gpipe-ci/result/coverage.lcov .gpipe-ci/result/coverage.evidence; elif [ -f .gpipe-ci/result/coverage.lcov.status.json ]; then cp .gpipe-ci/result/coverage.lcov.status.json .gpipe-ci/result/coverage.evidence; else printf '%s\\n' 'Missing coverage evidence' >&2; exit 1; fi");
    }
    Ok(ExecutionRecipe {
        steps: vec![CommandStep {
            shell: if windows { "pwsh" } else { "sh" }.into(),
            run,
            env,
            timeout_secs: 7_200,
        }],
        labels: BTreeMap::new(),
        cpu: 1,
        memory_mb: 0,
        max_attempts: 1,
    })
}

fn artifact_spec(native: &Value, failure: bool) -> Result<ArtifactSpec, String> {
    let mut paths = vec![
        "receipt.json".to_owned(),
        "plan.json".to_owned(),
        "ci.log".to_owned(),
    ];
    if !failure && native["profile"]["coverage"] == true {
        paths.push("lcov.info".into());
    }
    if !failure {
        paths.push("rust-validation.yml".into());
    }
    {
        for command in native["commands"]
            .as_array()
            .ok_or("native plan lacks commands")?
        {
            let stage = command["stage"]
                .as_str()
                .ok_or("native command lacks stage identity")?;
            if stage.is_empty()
                || !stage
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err("native command has unsafe stage file identity".into());
            }
            paths.extend([format!("{stage}.stdout.log"), format!("{stage}.stderr.log")]);
        }
        if !failure && native["profile"]["coverage"] == true {
            paths.extend([
                "coverage.sh".into(),
                "coverage_evidence.py".into(),
                "coverage.evidence".into(),
            ]);
        }
    }
    Ok(ArtifactSpec {
        inputs: Vec::new(),
        outputs: paths
            .into_iter()
            .map(|path| ArtifactOutput {
                name: path.clone(),
                path: format!("{RESULT}/{path}"),
            })
            .collect(),
    })
}

fn quote(value: &str, windows: bool) -> Result<String, String> {
    if value.contains(['\0', '\n', '\r']) {
        return Err("executable path contains unsupported control characters".into());
    }
    Ok(format!(
        "'{}'",
        if windows {
            value.replace('\'', "''")
        } else {
            value.replace('\'', "'\"'\"'")
        }
    ))
}

fn absolute_executable(value: &str) -> bool {
    Path::new(value).is_absolute()
        || (value.as_bytes().get(1) == Some(&b':')
            && value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
            && matches!(value.as_bytes().get(2), Some(b'\\' | b'/')))
}

fn exact_sha(value: &str) -> Result<(), String> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("exact full source and base commit SHAs are required".into())
    }
}

fn encode(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn hash(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

fn suite_files() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "rust-validation.yml".into(),
            hash(include_bytes!(
                "../../../.github/clippier/run-matrix/rust-validation.yml"
            )),
        ),
        (
            "coverage.sh".into(),
            hash(include_bytes!(
                "../../../.github/clippier/run-matrix/coverage.sh"
            )),
        ),
        (
            "coverage_evidence.py".into(),
            hash(include_bytes!(
                "../../../.github/clippier/run-matrix/coverage_evidence.py"
            )),
        ),
    ])
}

fn digest_json(value: &impl Serialize) -> Result<String, String> {
    Ok(hash(encode(value)?.as_bytes()))
}

/// Authenticated, downloaded artifact bytes for a completed coordinator attempt.
///
/// The parent must retrieve these via gPipe's authorized artifact data plane, not
/// accept them from untrusted source output. Byte arrays preserve exact digests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseEvidence {
    /// Coordinator run whose successful attempt authorized the downloads.
    pub run_id: String,
    /// Exact source revision, copied from the authenticated run inspection.
    pub revision: String,
    /// Exact upstream case / coordinator task ID.
    pub case_id: String,
    /// Exact completed producer attempt, never a latest-attempt guess.
    pub attempt_id: String,
    /// Named artifact bytes: exact output-relative native file names. The task
    /// publishes either LCOV/status at the fixed `coverage.evidence` name.
    pub artifacts: BTreeMap<String, Vec<u8>>,
}

/// Final evidence bound to the exact run, manifest, source, cases and attempts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedRun {
    /// Coordinator run identity.
    pub run_id: String,
    /// Exact tested commit.
    pub revision: String,
    /// SHA-256 of the entire retained manifest, including unchanged upstream jobs.
    pub manifest_digest: String,
    /// Verified successful case evidence, keyed by exact upstream case ID.
    pub cases: BTreeMap<String, VerifiedCase>,
}

/// Digests of real artifacts retained for a successful case attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedCase {
    /// Completed coordinator attempt that produced all evidence.
    pub attempt_id: String,
    /// Exact resolved native plan digest.
    pub plan_digest: String,
    /// Digests of exact downloaded artifact bytes.
    pub artifacts: BTreeMap<String, String>,
    /// Native runner receipt, retained without stripping stage metadata.
    pub receipt: Value,
}

/// Verify exact task completion and actual native runner stage/artifact evidence.
///
/// `inspection` is the coordinator's inspection object augmented by the parent
/// with `run_id` and `ci_evidence: [CaseEvidence, ...]` after authenticated artifact
/// retrieval. This function does not perform network I/O or trust log text as a
/// replacement for the receipt. Missing artifacts/prerequisites are errors.
///
/// # Errors
/// * The prepared manifest/plan has drifted or the inspected run is not its exact
///   sealed, successful plan with one completed successful producer per case.
/// * Evidence is absent, duplicated, unexpected, from another attempt/revision/run,
///   or the native receipt lacks exact successful stages and required real output.
pub fn verify(prepared: &PreparedRun, inspection: &Value) -> Result<VerifiedRun, String> {
    validate_prepared(prepared)?;
    let run_id = inspection["run_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("inspection needs authenticated run_id")?;
    if inspection["plan"]
        != serde_json::to_value(&prepared.plan).map_err(|error| error.to_string())?
        || inspection["status"] != "SUCCEEDED"
        || inspection["sealed"] != true
    {
        return Err("inspection is not the exact sealed successful prepared run".into());
    }
    let tasks = inspection["tasks"]
        .as_object()
        .ok_or("inspection lacks task states")?;
    let expected: BTreeSet<_> = prepared.plan.tasks.keys().map(String::as_str).collect();
    if tasks.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected
        || tasks.values().any(|state| state != "SUCCEEDED")
    {
        return Err("inspection does not have exact successful case coverage".into());
    }
    let attempts = inspection["attempts"]
        .as_object()
        .ok_or("inspection lacks attempts")?;
    let evidence: Vec<CaseEvidence> = serde_json::from_value(inspection["ci_evidence"].clone())
        .map_err(|_| "missing authenticated ci_evidence artifact downloads".to_owned())?;
    let mut cases = BTreeMap::new();
    for item in evidence {
        if item.run_id != run_id
            || item.revision != prepared.manifest.revision
            || !expected.contains(item.case_id.as_str())
        {
            return Err("CI evidence run/revision/case mismatch".into());
        }
        let attempt = attempts
            .get(&item.attempt_id)
            .ok_or("CI evidence attempt missing from coordinator")?;
        if attempt["task_id"] != item.case_id
            || attempt["status"] != "COMPLETED"
            || attempt["report"]["success"] != true
        {
            return Err("CI evidence is not from a completed successful case attempt".into());
        }
        let producers = attempts
            .values()
            .filter(|candidate| {
                candidate["task_id"] == item.case_id
                    && candidate["status"] == "COMPLETED"
                    && candidate["report"]["success"] == true
            })
            .count();
        if producers != 1 {
            return Err("CI evidence has ambiguous successful producer attempts".into());
        }
        let native = &prepared.manifest.native_plans[&item.case_id];
        let receipt_bytes = item
            .artifacts
            .get("receipt.json")
            .ok_or("missing native receipt artifact")?;
        let receipt: Value = serde_json::from_slice(receipt_bytes)
            .map_err(|error| format!("invalid native receipt: {error}"))?;
        verify_native_receipt(
            native,
            &receipt,
            &item.artifacts,
            prepared.manifest.profile.coverage,
        )?;
        verify_suite_files(&prepared.manifest, &item.artifacts)?;
        let verified = VerifiedCase {
            attempt_id: item.attempt_id,
            plan_digest: digest_json(native)?,
            artifacts: item
                .artifacts
                .iter()
                .map(|(name, bytes)| (name.clone(), hash(bytes)))
                .collect(),
            receipt,
        };
        if cases.insert(item.case_id, verified).is_some() {
            return Err("duplicate CI case evidence".into());
        }
    }
    if cases.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected {
        return Err(
            "missing required case evidence; zero-exit reports do not establish CI coverage".into(),
        );
    }
    Ok(VerifiedRun {
        run_id: run_id.into(),
        revision: prepared.manifest.revision.clone(),
        manifest_digest: digest_json(&prepared.manifest)?,
        cases,
    })
}

fn verify_suite_files(
    manifest: &RunManifest,
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    for (name, digest) in &manifest.suite_files {
        if name == "rust-validation.yml" || manifest.profile.coverage {
            let bytes = artifacts
                .get(name)
                .ok_or("missing trusted suite/helper snapshot")?;
            if hash(bytes) != *digest {
                return Err(format!("executed suite/helper digest mismatch: {name}"));
            }
        }
    }
    Ok(())
}

fn validate_prepared(prepared: &PreparedRun) -> Result<(), String> {
    let manifest = &prepared.manifest;
    exact_sha(&manifest.revision)?;
    exact_sha(&manifest.base_revision)?;
    if manifest.version != 1
        || manifest.scope.trim().is_empty()
        || !absolute_executable(&manifest.profile.clippier_executable)
        || manifest.profile_digest != digest_json(&manifest.profile)?
        || manifest.suite_digest != digest_json(&(&manifest.native_plans, &manifest.suite_files))?
        || manifest.suite_files != suite_files()
        || manifest.tool_digest.len() != 64
        || !manifest
            .tool_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("invalid or changed CI manifest provenance".into());
    }
    let matrix = MatrixPlan::from_jobs_json(&encode(&manifest.jobs)?, Some(manifest.seed))?;
    let mut recipes = BTreeMap::new();
    for job in matrix.jobs() {
        for case in &job.cases {
            let native = manifest
                .native_plans
                .get(&case.id)
                .ok_or("manifest lacks native case plan")?;
            if native["case"] != serde_json::to_value(case).map_err(|error| error.to_string())?
                || native["profile"]
                    != serde_json::to_value(&manifest.profile).map_err(|error| error.to_string())?
            {
                return Err("native plan has changed case/profile metadata".into());
            }
            let os = case.package["os"].as_str().ok_or("case lacks OS")?;
            recipes.insert(
                case.id.clone(),
                recipe(native, &manifest.profile, &manifest.tool_digest, os)?,
            );
        }
    }
    if recipes.len() != manifest.native_plans.len() {
        return Err("manifest contains unexpected native plans".into());
    }
    let lowered = matrix.lower(
        manifest.repository.clone(),
        manifest.revision.clone(),
        recipes,
    )?;
    if lowered.plan() != &prepared.plan {
        return Err("prepared execution plan differs from manifest".into());
    }
    Ok(())
}

fn verify_native_receipt(
    native: &Value,
    receipt: &Value,
    artifacts: &BTreeMap<String, Vec<u8>>,
    coverage: bool,
) -> Result<(), String> {
    // Kept in one place so the CLI JSON contract remains independent of the
    // Clippier Rust implementation. Fail closed on unknown receipt versions.
    if receipt["exit_code"] != 0 || receipt["case"] != native["case"] {
        return Err("native receipt is not a successful exact official case".into());
    }
    let plan_bytes = artifacts
        .get("plan.json")
        .ok_or("missing persisted native plan")?;
    let persisted: Value =
        serde_json::from_slice(plan_bytes).map_err(|_| "invalid persisted native plan")?;
    if persisted != *native || receipt["plan_sha256"] != hash(plan_bytes) {
        return Err("native receipt plan digest or content mismatch".into());
    }
    let stages = native["commands"]
        .as_array()
        .ok_or("native plan lacks required commands")?;
    let actual = receipt["stages"]
        .as_array()
        .ok_or("native receipt lacks stage evidence")?;
    if stages.is_empty() || stages.len() != actual.len() {
        return Err("native receipt has missing or unexpected stages".into());
    }
    let required = receipt["required_evidence_paths"]
        .as_array()
        .ok_or("native receipt lacks required evidence paths")?;
    let required: BTreeSet<&str> = required
        .iter()
        .map(|path| path.as_str().ok_or("invalid required evidence path"))
        .collect::<Result<_, _>>()?;
    for path in &required {
        let name = if matches!(*path, "coverage.lcov" | "coverage.lcov.status.json") {
            "coverage.evidence"
        } else {
            path
        };
        if !artifacts.contains_key(name) {
            return Err(format!("missing required native evidence: {path}"));
        }
    }
    if !required.contains("receipt.json")
        || !required.contains("plan.json")
        || !required.contains("rust-validation.yml")
    {
        return Err("native receipt omitted mandatory suite/plan evidence".into());
    }
    for (expected, actual) in stages.iter().zip(actual) {
        if actual["stage"] != expected["stage"]
            || actual["exit_code"] != 0
            || actual["detail"].as_str().is_none_or(str::is_empty)
        {
            return Err(
                "native receipt has changed, missing, or unsuccessful stage evidence".into(),
            );
        }
        let stage = expected["stage"].as_str().ok_or("invalid native stage")?;
        for stream in ["stdout", "stderr"] {
            let path = format!("{stage}.{stream}.log");
            if actual[stream] != path || !required.contains(path.as_str()) {
                return Err("native receipt lacks exact per-stage log paths".into());
            }
            let bytes = artifacts.get(&path).ok_or("missing native stage log")?;
            if actual[format!("{stream}_sha256")] != hash(bytes) {
                return Err("native stage log digest mismatch".into());
            }
        }
    }
    if coverage {
        let bytes = artifacts
            .get("coverage.evidence")
            .ok_or("missing required coverage evidence")?;
        if required.contains("coverage.lcov") && !required.contains("coverage.lcov.status.json") {
            let text = std::str::from_utf8(bytes).map_err(|_| "coverage is not UTF-8 LCOV")?;
            if !text.lines().any(|line| line.starts_with("SF:"))
                || !text.lines().any(|line| line.starts_with("DA:"))
                || !text.lines().any(|line| line == "end_of_record")
            {
                return Err("coverage contains no real LCOV source/line records".into());
            }
        } else {
            return Err("coverage requires real LCOV records; zero-test status alone is not actual coverage".into());
        }
    }
    Ok(())
}

struct TemporaryDirectory(PathBuf);
impl TemporaryDirectory {
    fn create() -> Result<Self, String> {
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "gpipe-ci-plan-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(format!("cannot create CI plan directory: {error}")),
            }
        }
        Err("cannot allocate private CI plan directory".into())
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile() -> NativeProfile {
        NativeProfile {
            toolchain: "system".into(),
            coverage: false,
            clippier_executable: "/trusted/cli'pper".into(),
            allow_setup: false,
        }
    }

    #[test]
    fn digest_is_lowercase_sha256_with_sha2_011() {
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn argv_builder_uses_real_features_flags_without_shell_interpolation() {
        let request = AnalysisRequest {
            revision: "a".repeat(40),
            base_revision: "b".repeat(40),
            seed: 42,
            packages: vec!["moosicbox_assert".into()],
            changed_files: vec!["packages/a/$(touch nope).rs".into()],
        };
        let command = request
            .command(Path::new("/trusted/clippier"), Path::new("."))
            .unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|value| value.to_str().unwrap())
            .collect();
        assert_eq!(command.get_program(), "/trusted/clippier");
        assert_eq!(&args[..4], &["features", ".", "--chunked", "15"]);
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--changed-files", "packages/a/$(touch nope).rs"])
        );
        assert!(!args.contains(&"--shard-matrix"));
    }

    #[test]
    fn analysis_rejects_movable_revisions_and_ambiguous_lists() {
        let mut request = AnalysisRequest {
            revision: "HEAD".into(),
            base_revision: "b".repeat(40),
            seed: 1,
            packages: vec![],
            changed_files: vec![],
        };
        assert!(request.command(Path::new("/cli"), Path::new(".")).is_err());
        request.revision = "a".repeat(40);
        request.packages.push("a,b".into());
        assert!(request.command(Path::new("/cli"), Path::new(".")).is_err());
    }

    #[test]
    fn scripts_quote_only_trusted_path_and_keep_case_data_in_environment() {
        let value = json!({"profile": profile(), "commands": [{"stage": "clippy-1"}], "case": {"id": "$(touch forbidden)"}});
        let recipe = recipe(&value, &profile(), &"a".repeat(64), "ubuntu-latest").unwrap();
        assert!(!recipe.steps[0].run.contains("touch forbidden"));
        assert!(recipe.steps[0].run.contains("'/trusted/cli'\"'\"'pper'"));
        assert_eq!(
            serde_json::from_str::<Value>(&recipe.steps[0].env["GPIPE_NATIVE_PLAN_JSON"]).unwrap(),
            value
        );
        let artifacts = ArtifactSpec::from_steps(&recipe.steps).unwrap();
        assert!(
            artifacts
                .outputs
                .iter()
                .any(|output| output.name == "clippy-1.stdout.log")
        );
        assert!(recipe.steps[0].env.contains_key("GPIPE_ARTIFACTS_ALWAYS"));
    }

    #[test]
    fn windows_has_native_quoting_and_explicit_runner() {
        let mut policy = profile();
        policy.clippier_executable = "C:\\trusted\\cli'pper.exe".into();
        let value = json!({"profile": policy, "commands": [{"stage": "tests-1"}]});
        let recipe = recipe(&value, &policy, &"a".repeat(64), "windows-latest").unwrap();
        assert_eq!(recipe.steps[0].shell, "pwsh");
        assert!(
            recipe.steps[0]
                .run
                .contains("& 'C:\\trusted\\cli''pper.exe' 'ci-run'")
        );
        assert!(absolute_executable(&policy.clippier_executable));
        assert!(!absolute_executable("clippier"));
    }

    #[test]
    fn untrusted_stage_paths_and_control_characters_are_rejected() {
        let value = json!({"commands": [{"stage": "../../escaped"}]});
        assert!(recipe(&value, &profile(), &"a".repeat(64), "macos-latest").is_err());
        assert!(quote("/a\nwhoami", false).is_err());
    }

    #[test]
    fn zero_exit_alone_is_never_native_evidence() {
        let native = json!({"case": {"id": "case-1"}, "commands": [{"stage": "tests-1"}]});
        assert!(
            verify_native_receipt(&native, &json!({"exit_code": 0}), &BTreeMap::new(), false)
                .is_err()
        );
        assert!(
            verify_native_receipt(&native, &json!({"success": true}), &BTreeMap::new(), false)
                .is_err()
        );
    }

    #[test]
    fn actual_native_plan_accepts_only_explicit_profile_provenance() {
        let value = serde_json::to_value(profile()).unwrap();
        assert!(serde_json::from_value::<RunProfile>(value).is_err());
        let with_provenance = json!({"toolchain": "system", "coverage": false,
            "clippier_executable": "/trusted/clippier", "allow_setup": false,
            "base_revision": "b".repeat(40), "scope": "packages", "packages": ["moosicbox_assert"]});
        let policy: RunProfile = serde_json::from_value(with_provenance).unwrap();
        assert_eq!(policy.packages, vec!["moosicbox_assert"]);
        assert_eq!(digest_json(&policy.native).unwrap().len(), 64);
    }
}
