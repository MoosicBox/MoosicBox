//! Lossless import of Clippier `plan-matrix` / `schedule-matrix` jobs JSON.
//!
//! Clippier's native CLI exposes `plan-matrix INPUT.json` and
//! `verify-matrix JOBS.json RECEIPTS.json`. `run-matrix` belongs to the GitHub
//! composite action, not the native CLI. This module therefore lowers each exact
//! case to a task using an explicit caller-supplied executable recipe. It neither
//! invents a runner command nor reimplements the action's template language.
//!
//! A preparatory native Rust suite is available through
//! [`MatrixPlan::rust_validation_recipes`]; its documented subset is not full CI parity.
//! Custom recipes must implement the selected command suite, toolchain, setup, feature
//! iteration, checkout isolation, and finalization from the action. In particular,
//! `features` is an array of feature *combinations*, not a feature union. The action
//! templates include conditionals and `{{vars.*}}`; pass fully rendered native
//! commands here, or invoke a caller-owned runner that implements those semantics.
//! No package metadata is interpolated into shell source by this adapter.
//!
//! Each case is lowered independently (even when its upstream job was stacked).
//! A job grouping is retained as metadata, not a promise of runner affinity. Fresh
//! checkouts/processes and `freshRunner` isolation require executor provisioning;
//! scheduler labels alone cannot guarantee those properties. Receipts are trusted
//! outcome evidence, not cryptographic attestations. Collect them for this exact
//! repository/revision/run and retain the imported manifest with the plan.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plan::{CommandStep, MAX_ID_BYTES, MAX_PLAN_TEXT_BYTES, MAX_TASKS, Plan, Task};

/// Environment variable containing the complete original case JSON.
pub const CASE_JSON_ENV: &str = "GPIPE_CLIPPIER_CASE_JSON";
/// Environment variable containing the complete original upstream job JSON.
pub const JOB_JSON_ENV: &str = "GPIPE_CLIPPIER_JOB_JSON";
/// Environment variable containing the explicitly supplied generation seed.
///
/// The value is a decimal `u64`, or `null` when the seed was not recorded. Import
/// never regenerates cases or assumes a missing seed was zero.
pub const SEED_ENV: &str = "GPIPE_CLIPPIER_SEED";

/// An unchanged Clippier case; structurally compatible with `matrix_schedule::Case`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Exact upstream identity, including distinct IDs for duplicate configurations.
    pub id: String,
    /// Complete execution metadata, including features, OS, and unknown extensions.
    pub package: BTreeMap<String, Value>,
}

/// An upstream job with flattened shared setup and ordered cases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// Cases in the original execution order.
    pub cases: Vec<Case>,
    /// Original setup: the complete package metadata minus `features`.
    #[serde(flatten)]
    pub setup: BTreeMap<String, Value>,
}

/// Terminal outcome after all commands and finalization for one complete case.
///
/// Wire-compatible with Clippier's native `verify-matrix` receipt format. Do not
/// produce a successful receipt for a skipped case or a zero-command execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    /// Complete unchanged case, not merely its ID or selected features.
    pub case: Case,
    /// Final exit status; only zero counts as success.
    pub exit_code: i32,
}

/// Explicit executable recipe for one case, supplied by the integration layer.
///
/// Steps are already executable scripts, not unexpanded Clippier templates. They
/// may use the adapter's reserved environment variables to read metadata as data.
/// No default commands or tool installation are inferred. The caller owns shell
/// quoting, setup, OS support, command-suite completeness, and receipt production.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRecipe {
    /// Ordered setup, execution, and finalization commands.
    pub steps: Vec<CommandStep>,
    /// Additional worker requirements. `os` must match the original matrix OS.
    pub labels: BTreeMap<String, String>,
    /// Positive CPU request.
    pub cpu: u32,
    /// Memory request in MiB; zero means no minimum.
    pub memory_mb: u64,
    /// Positive attempt limit, including the initial execution.
    pub max_attempts: u32,
}

/// Validated, retained Clippier case manifest and explicit generation provenance.
///
/// Private fields prevent mutation after import. Unknown package/setup fields are
/// retained exactly as JSON values. Generation seed is external provenance:
/// official jobs JSON does not guarantee a top-level seed field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixPlan {
    jobs: Vec<Job>,
    seed: Option<u64>,
}

impl MatrixPlan {
    /// Import the native CLI's JSON array of jobs, not a GitHub matrix wrapper.
    ///
    /// `seed` must be the generation seed recorded by the caller, if known. Any
    /// seed fields inside package metadata are also preserved without rewriting.
    ///
    /// # Errors
    ///
    /// * Input exceeds [`MAX_PLAN_TEXT_BYTES`] or is not jobs-array JSON.
    /// * Jobs/cases are empty or exceed [`MAX_TASKS`], IDs are invalid or repeated,
    ///   required package metadata is malformed, or setup differs from a case.
    /// * A stacked job contains setup requiring fresh-runner isolation under
    ///   Clippier's scheduling policy.
    pub fn from_jobs_json(json: &str, seed: Option<u64>) -> Result<Self, String> {
        if json.len() > MAX_PLAN_TEXT_BYTES {
            return Err("Clippier jobs JSON exceeds input byte limit".into());
        }
        let jobs: Vec<Job> = serde_json::from_str(json).map_err(|error| error.to_string())?;
        validate_jobs(&jobs)?;
        Ok(Self { jobs, seed })
    }

    /// Original upstream jobs in their original order.
    #[must_use]
    pub const fn jobs(&self) -> &[Job] {
        self.jobs.as_slice()
    }

    /// Recorded generation seed, if the caller supplied one.
    #[must_use = "the recorded seed identifies generation provenance"]
    pub const fn seed(&self) -> Option<u64> {
        self.seed
    }

    /// Map exact upstream case IDs to coordinator task IDs.
    ///
    /// Lowering uses the unchanged case ID as its task ID. Retain this mapping
    /// alongside inspect/results output; task success still requires scoped case
    /// receipts before [`Self::verify_receipts`] can establish coverage.
    #[must_use]
    pub fn case_task_mapping(&self) -> BTreeMap<String, String> {
        self.jobs
            .iter()
            .flat_map(|job| &job.cases)
            .map(|case| (case.id.clone(), case.id.clone()))
            .collect()
    }

    /// Generate a native, preparatory Rust validation suite for every case.
    ///
    /// For every feature combination this runs real `cargo +stable` (or
    /// `+nightly`) `check --all-targets`, `clippy --all-targets`, and
    /// `test --no-fail-fast`, each with `--no-default-features` and features
    /// `fail-on-warnings,requiredFeatures,combination`, matching the feature and
    /// toolchain policy in the action templates. Explicit `toolchain: "system"`
    /// with `nightly: false` instead invokes plain Cargo from PATH, without rustup
    /// or a selector argument. This is an unpinned host toolchain: callers must
    /// record its version externally; no stable/pinned equivalence is asserted.
    /// Empty feature arrays run once.
    /// `cargo test` includes its normal doctests; custom test commands are rejected.
    /// All combinations/commands are attempted, with any failure failing the task.
    ///
    /// Ubuntu/macOS runner labels use POSIX `sh`; Windows labels use `pwsh`.
    /// Package paths and feature lists are environment data, never shell source.
    /// Paths must be repository-relative without parent traversal. Workers must
    /// start in an isolated checkout root with Cargo, the selected toolchain,
    /// and Clippy already installed (rustup is needed for stable/nightly selectors,
    /// not system mode). No tool installation is attempted.
    ///
    /// This is NOT full `rust-validation.yml` parity: no coverage, target clearing,
    /// format, machete, custom environment/setup/dependencies, or action template
    /// rendering. It does not emit receipts; the execution integration must record
    /// a scoped terminal receipt only after the complete task succeeds. Defaults
    /// are one CPU, no minimum memory, one attempt, and a one-hour task step timeout.
    ///
    /// # Errors
    ///
    /// * OS, toolchain, test command, metadata key/type, or custom setup is not in
    ///   the supported subset; unknown fields are rejected, not silently ignored.
    /// * Paths/features are invalid, there are more than 64 feature combinations,
    ///   or the generated recipe fails resolved plan validation/size limits.
    pub fn rust_validation_recipes(&self) -> Result<BTreeMap<String, ExecutionRecipe>, String> {
        let recipes = self
            .jobs
            .iter()
            .flat_map(|job| &job.cases)
            .map(|case| native_rust_recipe(case).map(|recipe| (case.id.clone(), recipe)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        // Reuse lowering to check cumulative injected metadata, not only each script.
        self.lower("recipe-validation".into(), "0".repeat(40), recipes.clone())?;
        Ok(recipes)
    }

    /// Lower every case to exactly one task with an explicit executable recipe.
    ///
    /// Recipe keys must equal the complete case-ID set. Every step receives the
    /// unchanged case/job JSON and seed as environment data. The exact upstream
    /// `os` value becomes the `os` label (e.g. `ubuntu-latest`, not guessed `linux`).
    /// Workers must advertise that value or integration must resolve runner aliases
    /// before scheduling. Tasks are independent; upstream stacking is not affinity.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use gpipe::clippier::{ExecutionRecipe, MatrixPlan};
    /// use gpipe::plan::CommandStep;
    /// let jobs = r#"[{"name":"demo","path":".","os":"ubuntu-latest",
    ///   "cases":[{"id":"case-000000","package":{"name":"demo","path":".",
    ///   "os":"ubuntu-latest","features":["std"]}}]}]"#;
    /// let matrix = MatrixPlan::from_jobs_json(jobs, Some(42))?;
    /// // Explicit suite chosen by the caller, not inferred by the importer.
    /// let recipe = ExecutionRecipe {
    ///     steps: vec![CommandStep {
    ///         shell: "sh".into(),
    ///         run: "cargo check --no-default-features --features std".into(),
    ///         env: BTreeMap::new(), timeout_secs: 300,
    ///     }],
    ///     labels: BTreeMap::new(), cpu: 1, memory_mb: 0, max_attempts: 1,
    /// };
    /// let lowered = matrix.lower("repo".into(), "a".repeat(40),
    ///     BTreeMap::from([("case-000000".into(), recipe)]))?;
    /// assert_eq!(lowered.plan().tasks.len(), 1);
    /// # Ok::<(), String>(())
    /// ```
    ///
    /// # Errors
    ///
    /// * Recipes are missing or unexpected, conflict with the upstream OS, or set
    ///   adapter-reserved environment variables.
    /// * JSON serialization or resolved [`Plan::validate`] fails, including size,
    ///   source revision, command, timeout, CPU, and attempt-limit checks.
    pub fn lower(
        &self,
        repository: String,
        revision: String,
        mut recipes: BTreeMap<String, ExecutionRecipe>,
    ) -> Result<LoweredPlan, String> {
        let mut tasks = BTreeMap::new();
        let mut metadata_bytes = 0_usize;
        let seed = self
            .seed
            .map_or_else(|| "null".into(), |seed| seed.to_string());
        for job in &self.jobs {
            let job_json = serde_json::to_string(job).map_err(|error| error.to_string())?;
            for case in &job.cases {
                let mut recipe = recipes
                    .remove(&case.id)
                    .ok_or_else(|| format!("missing executable recipe for {}", case.id))?;
                let os = case.package["os"].as_str().ok_or("invalid case OS")?;
                if recipe.labels.get("os").is_some_and(|value| value != os) {
                    return Err(format!("recipe OS conflicts with case {}", case.id));
                }
                recipe.labels.insert("os".into(), os.into());
                let case_json = serde_json::to_string(case).map_err(|error| error.to_string())?;
                // Bound duplicated job payloads before cloning them into every step.
                let payload_bytes = job_json.len() + case_json.len() + seed.len();
                metadata_bytes = payload_bytes
                    .checked_mul(recipe.steps.len())
                    .and_then(|bytes| metadata_bytes.checked_add(bytes))
                    .filter(|bytes| *bytes <= MAX_PLAN_TEXT_BYTES)
                    .ok_or("lowered Clippier metadata exceeds plan byte limit")?;
                for step in &mut recipe.steps {
                    for (key, value) in [
                        (CASE_JSON_ENV, &case_json),
                        (JOB_JSON_ENV, &job_json),
                        (SEED_ENV, &seed),
                    ] {
                        if step.env.contains_key(key) {
                            return Err(format!("recipe {} sets reserved variable {key}", case.id));
                        }
                        step.env.insert(key.into(), value.clone());
                    }
                }
                tasks.insert(
                    case.id.clone(),
                    Task {
                        when: crate::plan::TaskWhen::default(),
                        needs: Vec::new(),
                        steps: recipe.steps,
                        labels: recipe.labels,
                        cpu: recipe.cpu,
                        memory_mb: recipe.memory_mb,
                        max_attempts: recipe.max_attempts,
                    },
                );
            }
        }
        if !recipes.is_empty() {
            return Err("unexpected executable recipes not present in Clippier plan".into());
        }
        let plan = Plan {
            repository,
            revision,
            tasks,
        };
        plan.validate()?;
        Ok(LoweredPlan {
            plan,
            matrix: self.clone(),
        })
    }

    /// Require exactly one successful receipt for every unchanged planned case.
    ///
    /// Structural JSON equality ignores object-key ordering but preserves array
    /// order, feature combinations, unknown fields, and duplicate configurations
    /// with distinct IDs. Receipt arrival order is irrelevant. This deliberately
    /// rejects duplicate attempts: the caller must select one terminal receipt per
    /// case, retaining attempt history separately. Native Clippier receipts do not
    /// include repository/revision or external generation seed; authenticate and
    /// scope their collection outside this function.
    ///
    /// # Errors
    ///
    /// * Any expected receipt is missing, or any receipt is duplicated, unexpected,
    ///   changed in any case metadata, or has a nonzero exit status.
    pub fn verify_receipts(&self, receipts: &[Receipt]) -> Result<(), String> {
        let expected: BTreeMap<&str, &Case> = self
            .jobs
            .iter()
            .flat_map(|job| &job.cases)
            .map(|case| (case.id.as_str(), case))
            .collect();
        let mut seen = BTreeSet::new();
        for receipt in receipts {
            let id = receipt.case.id.as_str();
            if !seen.insert(id) {
                return Err(format!("duplicate case receipt {id}"));
            }
            if expected.get(id).copied() != Some(&receipt.case) {
                return Err(format!("unexpected or changed case receipt {id}"));
            }
            if receipt.exit_code != 0 {
                return Err(format!("case {id} failed ({})", receipt.exit_code));
            }
        }
        if seen.len() != expected.len() {
            return Err(format!(
                "incomplete case coverage: {} of {} receipts",
                seen.len(),
                expected.len()
            ));
        }
        Ok(())
    }

    /// Parse a bounded official receipts JSON array and verify exact coverage.
    ///
    /// # Errors
    ///
    /// * Input exceeds [`MAX_PLAN_TEXT_BYTES`] or does not match the receipt schema.
    /// * Coverage fails as described by [`Self::verify_receipts`].
    pub fn verify_receipts_json(&self, json: &str) -> Result<(), String> {
        if json.len() > MAX_PLAN_TEXT_BYTES {
            return Err("Clippier receipts JSON exceeds input byte limit".into());
        }
        let receipts: Vec<Receipt> =
            serde_json::from_str(json).map_err(|error| error.to_string())?;
        self.verify_receipts(&receipts)
    }
}

/// Executable case plan paired with the immutable manifest needed for coverage.
///
/// Accessors are read-only so plan tasks and their retained manifest cannot drift.
/// Serialize [`Self::plan`] for coordinator submission and retain [`Self::matrix`]
/// with the execution run. A task's zero exit code alone is not case evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredPlan {
    plan: Plan,
    matrix: MatrixPlan,
}

impl LoweredPlan {
    /// Resolved executable plan, already validated during lowering.
    #[must_use]
    pub const fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Original manifest for exact outcome reconciliation.
    #[must_use]
    pub const fn matrix(&self) -> &MatrixPlan {
        &self.matrix
    }
}

fn native_rust_toolchain(case: &Case) -> Result<&'static str, String> {
    let package = &case.package;
    for key in package.keys() {
        if !matches!(
            key.as_str(),
            "name"
                | "path"
                | "os"
                | "features"
                | "requiredFeatures"
                | "nightly"
                | "toolchain"
                | "test-command"
                | "env"
                | "cargo"
                | "ciSteps"
                | "dependencies"
                | "freshRunner"
                | "gitSubmodules"
                | "seed"
        ) {
            return Err(format!(
                "case {}: unsupported native Rust metadata {key}",
                case.id
            ));
        }
    }
    for field in ["env", "cargo", "ciSteps", "dependencies"] {
        if package
            .get(field)
            .is_some_and(|value| value.as_str() != Some(""))
        {
            return Err(format!("case {}: unsupported native Rust {field}", case.id));
        }
    }
    for field in ["freshRunner", "gitSubmodules"] {
        if package
            .get(field)
            .is_some_and(|value| value.as_bool() != Some(false))
        {
            return Err(format!("case {}: unsupported native Rust {field}", case.id));
        }
    }
    if package
        .get("seed")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err("native Rust seed must be an unsigned integer".into());
    }
    if package
        .get("test-command")
        .is_some_and(|value| value.as_str() != Some("test"))
    {
        return Err("native Rust suite only supports the test command 'test'".into());
    }
    let nightly = match package.get("nightly") {
        None | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        _ => return Err("native Rust nightly must be boolean".into()),
    };
    let toolchain = if nightly { "nightly" } else { "stable" };
    if package.get("toolchain").and_then(Value::as_str) == Some("system") && !nightly {
        return Ok("system");
    }
    if package
        .get("toolchain")
        .is_some_and(|value| value.as_str() != Some(toolchain))
    {
        return Err("native Rust toolchain must agree with nightly (stable/nightly, or system when nightly is false)".into());
    }
    Ok(toolchain)
}

fn native_windows_shell(os: &str) -> Result<bool, String> {
    if os.starts_with("windows-") {
        Ok(true)
    } else if os.starts_with("ubuntu-") || os.starts_with("macos-") {
        Ok(false)
    } else {
        Err(format!("unsupported native Rust OS {os}"))
    }
}

fn native_rust_recipe(case: &Case) -> Result<ExecutionRecipe, String> {
    let toolchain = native_rust_toolchain(case)?;
    let package = &case.package;
    let path = package["path"]
        .as_str()
        .ok_or("invalid native Rust package path")?;
    if path.starts_with('/')
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| part == "..")
    {
        return Err("native Rust path must be repository-relative without parent traversal".into());
    }
    let os = package["os"].as_str().ok_or("invalid native Rust OS")?;
    let windows = native_windows_shell(os)?;
    let required = match package.get("requiredFeatures") {
        None => "",
        Some(Value::String(value)) => value,
        _ => return Err("native Rust requiredFeatures must be a string".into()),
    };
    check_feature_list(required)?;
    let features = package["features"]
        .as_array()
        .ok_or("invalid native Rust features")?;
    if features.len() > 64 {
        return Err("native Rust supports at most 64 feature combinations per case".into());
    }
    let combinations: Vec<&str> = if features.is_empty() {
        vec![""]
    } else {
        features
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or("invalid native Rust feature combination")
            })
            .collect::<Result<_, _>>()?
    };
    let mut env = BTreeMap::from([("GPIPE_RUST_PATH".into(), path.into())]);
    if toolchain != "system" {
        env.insert("GPIPE_RUST_TOOLCHAIN".into(), format!("+{toolchain}"));
    }
    // Choose only literal source fragments: system Cargo receives no selector
    // argument at all, rather than an empty argument or a fabricated +system.
    let cargo = match (windows, toolchain == "system") {
        (true, true) => "& cargo",
        (false, true) => "cargo",
        (true, false) => "& cargo $env:GPIPE_RUST_TOOLCHAIN",
        (false, false) => "cargo \"$GPIPE_RUST_TOOLCHAIN\"",
    };
    let mut script = if windows {
        "$ErrorActionPreference = 'Stop'\n$PSNativeCommandUseErrorActionPreference = $false\nSet-Location -LiteralPath (Join-Path (Get-Location) $env:GPIPE_RUST_PATH)\n$status = 0\n".to_owned()
    } else {
        "set -u\ncd -- \"./$GPIPE_RUST_PATH\" || exit 1\nstatus=0\n".to_owned()
    };
    for (index, combination) in combinations.iter().enumerate() {
        check_feature_list(combination)?;
        let mut all_features = "fail-on-warnings".to_owned();
        for features in [required, *combination] {
            if !features.is_empty() {
                all_features.push(',');
                all_features.push_str(features);
            }
        }
        let variable = format!("GPIPE_RUST_FEATURES_{index}");
        env.insert(variable.clone(), all_features);
        for (command, suffix) in [
            ("check --all-targets", ""),
            ("clippy --all-targets", ""),
            ("test", " --no-fail-fast"),
        ] {
            // Only the bounded numeric index enters source; all package data stays in env.
            if windows {
                writeln!(script, "{cargo} {command} --no-default-features --features $env:{variable}{suffix}\nif ($LASTEXITCODE -ne 0) {{ $status = 1 }}")
                    .map_err(|error| error.to_string())?;
            } else {
                writeln!(script, "{cargo} {command} --no-default-features --features \"${variable}\"{suffix} || status=1")
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    script.push_str(if windows {
        "exit $status\n"
    } else {
        "exit \"$status\"\n"
    });
    Ok(ExecutionRecipe {
        steps: vec![CommandStep {
            shell: if windows { "pwsh" } else { "sh" }.into(),
            run: script,
            env,
            timeout_secs: 3_600,
        }],
        labels: BTreeMap::from([("os".into(), os.into())]),
        cpu: 1,
        memory_mb: 0,
        max_attempts: 1,
    })
}

fn check_feature_list(features: &str) -> Result<(), String> {
    if !features.is_empty()
        && features.split(',').any(|feature| {
            feature.is_empty()
                || !feature.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'?' | b':')
                })
        })
    {
        return Err("native Rust features must be comma-separated Cargo feature names".into());
    }
    Ok(())
}

fn validate_jobs(jobs: &[Job]) -> Result<(), String> {
    if jobs.is_empty() || jobs.len() > MAX_TASKS {
        return Err("Clippier plan must contain a bounded nonempty job array".into());
    }
    let mut ids = BTreeSet::new();
    for job in jobs {
        if job.cases.is_empty() {
            return Err("Clippier job must contain cases".into());
        }
        if job.cases.len() > 1 && requires_isolation(&job.setup) {
            return Err("stacked Clippier job requires fresh-runner isolation".into());
        }
        for case in &job.cases {
            let digits = case.id.strip_prefix("case-").unwrap_or_default();
            if digits.is_empty()
                || !digits.bytes().all(|byte| byte.is_ascii_digit())
                || case.id.len() > MAX_ID_BYTES
                || !ids.insert(case.id.as_str())
            {
                return Err("invalid or duplicate Clippier case ID".into());
            }
            if ids.len() > MAX_TASKS {
                return Err("Clippier case count exceeds task limit".into());
            }
            for field in ["name", "path", "os"] {
                if case
                    .package
                    .get(field)
                    .and_then(Value::as_str)
                    .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(format!("case {} has missing or invalid {field}", case.id));
                }
            }
            if !case
                .package
                .get("features")
                .and_then(Value::as_array)
                .is_some_and(|features| features.iter().all(Value::is_string))
            {
                return Err(format!("case {} features must be a string array", case.id));
            }
            if case.package.contains_key("cases") || case.package.contains_key("caseId") {
                return Err("package contains scheduler-owned fields".into());
            }
            let mut setup = case.package.clone();
            setup.remove("features");
            if setup != job.setup {
                return Err(format!("case {} differs from its job setup", case.id));
            }
        }
    }
    Ok(())
}

fn requires_isolation(setup: &BTreeMap<String, Value>) -> bool {
    setup.get("freshRunner").and_then(Value::as_bool) == Some(true)
        || setup.get("gitSubmodules").and_then(Value::as_bool) == Some(true)
        || ["ciSteps", "dependencies"].iter().any(|field| {
            setup
                .get(*field)
                .is_some_and(|value| value.as_str().is_none_or(|text| !text.is_empty()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn jobs_json() -> Value {
        let package = json!({
            "name": "demo", "path": "packages/demo", "os": "windows-latest",
            "features": ["std,serde", "std"], "seed": 99,
            "nightly": true, "extension": {"nested": [1, "value"]}
        });
        let mut setup = package.clone();
        setup.as_object_mut().unwrap().remove("features");
        setup["cases"] = json!([
            {"id": "case-000000", "package": package},
            {"id": "case-000001", "package": package}
        ]);
        json!([setup])
    }

    fn matrix() -> MatrixPlan {
        MatrixPlan::from_jobs_json(&jobs_json().to_string(), Some(42)).unwrap()
    }

    fn recipe() -> ExecutionRecipe {
        ExecutionRecipe {
            steps: vec![CommandStep {
                shell: "pwsh".into(),
                run: "cargo check --no-default-features --features std,serde".into(),
                env: BTreeMap::from([("CUSTOM".into(), "value".into())]),
                timeout_secs: 60,
            }],
            labels: BTreeMap::new(),
            cpu: 1,
            memory_mb: 128,
            max_attempts: 2,
        }
    }

    fn recipes() -> BTreeMap<String, ExecutionRecipe> {
        BTreeMap::from([
            ("case-000000".into(), recipe()),
            ("case-000001".into(), recipe()),
        ])
    }

    fn receipts(matrix: &MatrixPlan) -> Vec<Receipt> {
        matrix
            .jobs()
            .iter()
            .flat_map(|job| &job.cases)
            .map(|case| Receipt {
                case: case.clone(),
                exit_code: 0,
            })
            .collect()
    }

    #[test]
    fn imports_duplicate_configurations_without_losing_identity_or_extensions() {
        let matrix = matrix();
        assert_eq!(matrix.seed(), Some(42));
        assert_eq!(serde_json::to_value(matrix.jobs()).unwrap(), jobs_json());
        assert_ne!(matrix.jobs()[0].cases[0].id, matrix.jobs()[0].cases[1].id);
        assert_eq!(
            matrix.jobs()[0].cases[0].package,
            matrix.jobs()[0].cases[1].package
        );
    }

    #[test]
    fn lowers_exact_cases_and_preserves_scripts_os_and_seed_as_data() {
        let matrix = matrix();
        let lowered = matrix
            .lower("repo".into(), "a".repeat(40), recipes())
            .unwrap();
        assert_eq!(lowered.plan().tasks.len(), 2);
        assert_eq!(lowered.matrix(), &matrix);
        for case in &matrix.jobs()[0].cases {
            let task = &lowered.plan().tasks[&case.id];
            assert_eq!(task.labels["os"], "windows-latest");
            assert_eq!(task.steps[0].run, recipe().steps[0].run);
            assert_eq!(task.steps[0].env["CUSTOM"], "value");
            assert_eq!(task.steps[0].env[SEED_ENV], "42");
            assert_eq!(
                serde_json::from_str::<Case>(&task.steps[0].env[CASE_JSON_ENV]).unwrap(),
                *case
            );
            assert_eq!(
                serde_json::from_str::<Job>(&task.steps[0].env[JOB_JSON_ENV]).unwrap(),
                matrix.jobs()[0]
            );
            assert_eq!(task.needs, Vec::<String>::new());
        }
        assert_eq!(lowered.plan().validate(), Ok(()));
    }

    #[test]
    fn missing_seed_is_not_zero_and_singleton_plan_is_supported() {
        let mut jobs = jobs_json();
        jobs[0]["cases"].as_array_mut().unwrap().pop();
        let matrix = MatrixPlan::from_jobs_json(&jobs.to_string(), None).unwrap();
        assert_eq!(matrix.seed(), None);
        let lowered = matrix
            .lower(
                "repo".into(),
                "F".repeat(64),
                BTreeMap::from([("case-000000".into(), recipe())]),
            )
            .unwrap();
        assert_eq!(
            lowered.plan().tasks["case-000000"].steps[0].env[SEED_ENV],
            "null"
        );
    }

    #[test]
    fn receipt_coverage_accepts_arbitrary_arrival_order_and_official_json() {
        let matrix = matrix();
        let mut receipts = receipts(&matrix);
        receipts.reverse();
        assert_eq!(matrix.verify_receipts(&receipts), Ok(()));
        assert_eq!(
            matrix.verify_receipts_json(&serde_json::to_string(&receipts).unwrap()),
            Ok(())
        );
    }

    #[test]
    fn rejects_missing_duplicate_unexpected_failed_and_changed_receipts() {
        let matrix = matrix();
        let original = receipts(&matrix);
        assert!(matrix.verify_receipts(&[]).is_err());
        assert!(matrix.verify_receipts(&original[..1]).is_err());
        let mut duplicate = original.clone();
        duplicate.push(original[0].clone());
        assert!(matrix.verify_receipts(&duplicate).is_err());
        let mut unexpected = original.clone();
        unexpected[0].case.id = "case-999999".into();
        assert!(matrix.verify_receipts(&unexpected).is_err());
        for code in [1, -1, 127] {
            let mut failed = original.clone();
            failed[0].exit_code = code;
            assert!(matrix.verify_receipts(&failed).is_err());
        }
        for (field, value) in [
            ("features", json!(["std", "std,serde"])),
            ("features", json!(["std,serde"])),
            ("os", json!("linux")),
            ("seed", json!(100)),
            ("extension", json!({"nested": [2, "value"]})),
        ] {
            let mut changed = original.clone();
            changed[0].case.package.insert(field.into(), value);
            assert!(matrix.verify_receipts(&changed).is_err());
        }
    }

    #[test]
    fn rejects_bad_transport_empty_jobs_and_duplicate_case_ids() {
        for input in ["{}", "[]", "null", "[{}]", "[{\"cases\":[]}]", "not json"] {
            assert!(MatrixPlan::from_jobs_json(input, None).is_err());
        }
        let mut jobs = jobs_json();
        jobs[0]["cases"][1]["id"] = json!("case-000000");
        assert!(MatrixPlan::from_jobs_json(&jobs.to_string(), None).is_err());
        for id in ["", "../case", "build", "case-", "case-é"] {
            let mut jobs = jobs_json();
            jobs[0]["cases"][0]["id"] = json!(id);
            assert!(MatrixPlan::from_jobs_json(&jobs.to_string(), None).is_err());
        }
    }

    #[test]
    fn rejects_changed_setup_and_invalid_package_shapes() {
        for (field, value) in [
            ("os", json!("linux")),
            ("path", json!(null)),
            ("name", json!("")),
            ("features", json!([1])),
            ("features", json!("std")),
            ("caseId", json!("owned")),
        ] {
            let mut jobs = jobs_json();
            jobs[0]["cases"][0]["package"][field] = value;
            assert!(MatrixPlan::from_jobs_json(&jobs.to_string(), None).is_err());
        }
        let mut jobs = jobs_json();
        jobs[0]["extra"] = json!(true);
        assert!(MatrixPlan::from_jobs_json(&jobs.to_string(), None).is_err());
    }

    #[test]
    fn rejects_stacked_isolation_but_retains_isolated_singletons() {
        for (field, value) in [
            ("freshRunner", json!(true)),
            ("gitSubmodules", json!(true)),
            ("ciSteps", json!("setup")),
            ("dependencies", json!(["libssl"])),
        ] {
            let mut jobs = jobs_json();
            jobs[0][field] = value.clone();
            for case in jobs[0]["cases"].as_array_mut().unwrap() {
                case["package"][field] = value.clone();
            }
            assert!(MatrixPlan::from_jobs_json(&jobs.to_string(), None).is_err());
            jobs[0]["cases"].as_array_mut().unwrap().pop();
            assert!(MatrixPlan::from_jobs_json(&jobs.to_string(), None).is_ok());
        }
    }

    #[test]
    fn requires_exact_recipe_coverage_and_matching_os() {
        let matrix = matrix();
        let mut missing = recipes();
        missing.remove("case-000001");
        let mut extra = recipes();
        extra.insert("case-999999".into(), recipe());
        let mut os = recipes();
        os.get_mut("case-000000")
            .unwrap()
            .labels
            .insert("os".into(), "linux".into());
        for recipes in [missing, extra, os] {
            assert!(
                matrix
                    .lower("repo".into(), "a".repeat(40), recipes)
                    .is_err()
            );
        }
    }

    #[test]
    fn reserved_environment_cannot_be_overridden() {
        for key in [CASE_JSON_ENV, JOB_JSON_ENV, SEED_ENV] {
            let mut recipes = recipes();
            recipes.get_mut("case-000000").unwrap().steps[0]
                .env
                .insert(key.into(), "forged".into());
            assert!(
                matrix()
                    .lower("repo".into(), "a".repeat(40), recipes)
                    .is_err()
            );
        }
    }

    #[test]
    fn lowering_validates_resources_commands_and_revision() {
        for variant in 0..4 {
            let mut recipes = recipes();
            let recipe = recipes.get_mut("case-000000").unwrap();
            match variant {
                0 => recipe.steps.clear(),
                1 => recipe.cpu = 0,
                2 => recipe.max_attempts = 0,
                _ => recipe.steps[0].timeout_secs = 0,
            }
            assert!(
                matrix()
                    .lower("repo".into(), "a".repeat(40), recipes)
                    .is_err()
            );
        }
        assert!(
            matrix()
                .lower("repo".into(), "main".into(), recipes())
                .is_err()
        );
    }

    #[test]
    fn multiple_jobs_keep_os_distinct_and_duplicate_ids_are_global_errors() {
        let mut jobs = jobs_json();
        let mut second = jobs[0].clone();
        second["os"] = json!("ubuntu-latest");
        for (index, case) in second["cases"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            case["id"] = json!(format!("case-{:06}", index + 2));
            case["package"]["os"] = json!("ubuntu-latest");
        }
        jobs.as_array_mut().unwrap().push(second);
        let matrix = MatrixPlan::from_jobs_json(&jobs.to_string(), Some(42)).unwrap();
        let mut all_recipes = recipes();
        all_recipes.insert("case-000002".into(), recipe());
        all_recipes.insert("case-000003".into(), recipe());
        let lowered = matrix
            .lower("repo".into(), "a".repeat(40), all_recipes)
            .unwrap();
        assert_eq!(
            lowered.plan().tasks["case-000000"].labels["os"],
            "windows-latest"
        );
        assert_eq!(
            lowered.plan().tasks["case-000002"].labels["os"],
            "ubuntu-latest"
        );
        assert_eq!(matrix.verify_receipts(&receipts(&matrix)), Ok(()));
        jobs[1]["cases"][0]["id"] = json!("case-000000");
        assert!(MatrixPlan::from_jobs_json(&jobs.to_string(), None).is_err());
    }

    #[test]
    fn metadata_is_never_interpolated_into_executable_source() {
        let mut jobs = jobs_json();
        let hostile_name = "demo; $(not-a-command) \"quoted\"";
        jobs[0]["name"] = json!(hostile_name);
        for case in jobs[0]["cases"].as_array_mut().unwrap() {
            case["package"]["name"] = json!(hostile_name);
        }
        let matrix = MatrixPlan::from_jobs_json(&jobs.to_string(), None).unwrap();
        let lowered = matrix
            .lower("repo".into(), "a".repeat(40), recipes())
            .unwrap();
        assert_eq!(
            lowered.plan().tasks["case-000000"].steps[0].run,
            recipe().steps[0].run
        );
        let case: Case =
            serde_json::from_str(&lowered.plan().tasks["case-000000"].steps[0].env[CASE_JSON_ENV])
                .unwrap();
        assert_eq!(case.package["name"], json!(hostile_name));
    }

    #[test]
    fn all_steps_receive_metadata_and_matching_os_labels_are_accepted() {
        let mut recipes = recipes();
        let recipe = recipes.get_mut("case-000000").unwrap();
        recipe.labels.insert("os".into(), "windows-latest".into());
        recipe.steps.push(recipe.steps[0].clone());
        let lowered = matrix()
            .lower("repo".into(), "a".repeat(40), recipes)
            .unwrap();
        for step in &lowered.plan().tasks["case-000000"].steps {
            assert!(step.env.contains_key(CASE_JSON_ENV));
            assert!(step.env.contains_key(JOB_JSON_ENV));
            assert_eq!(step.env[SEED_ENV], "42");
        }
    }

    fn native_matrix(os: &str, features: Value, extra: Value) -> MatrixPlan {
        let mut package = json!({"name": "demo", "path": "packages/demo", "os": os,
            "nightly": false, "requiredFeatures": "required"});
        package
            .as_object_mut()
            .unwrap()
            .insert("features".into(), features);
        let Value::Object(extra) = extra else {
            panic!("native matrix fixture extensions must be an object");
        };
        package.as_object_mut().unwrap().extend(extra);
        let mut job = package.clone();
        job.as_object_mut().unwrap().remove("features");
        job["cases"] = json!([{"id":"case-000000", "package":package}]);
        MatrixPlan::from_jobs_json(&json!([job]).to_string(), Some(42)).unwrap()
    }

    #[test]
    fn native_suite_runs_real_commands_for_each_exact_combination() {
        let matrix = native_matrix("ubuntu-latest", json!(["std,serde", "std"]), json!({}));
        let recipes = matrix.rust_validation_recipes().unwrap();
        let step = &recipes["case-000000"].steps[0];
        assert_eq!(step.shell, "sh");
        assert_eq!(step.env["GPIPE_RUST_TOOLCHAIN"], "+stable");
        assert_eq!(step.env["GPIPE_RUST_PATH"], "packages/demo");
        assert_eq!(
            step.env["GPIPE_RUST_FEATURES_0"],
            "fail-on-warnings,required,std,serde"
        );
        assert_eq!(
            step.env["GPIPE_RUST_FEATURES_1"],
            "fail-on-warnings,required,std"
        );
        for command in ["check --all-targets", "clippy --all-targets", "test"] {
            assert_eq!(
                step.run
                    .matches(&format!(" {command} --no-default-features"))
                    .count(),
                2
            );
        }
        assert_eq!(step.run.matches("--no-fail-fast").count(), 2);
        assert!(!step.run.contains("std,serde"));
        assert!(!step.run.contains("packages/demo"));
        assert!(step.run.ends_with("exit \"$status\"\n"));
        assert_eq!(
            matrix.case_task_mapping(),
            BTreeMap::from([("case-000000".into(), "case-000000".into())])
        );
        let lowered = matrix
            .lower("repo".into(), "a".repeat(40), recipes)
            .unwrap();
        assert_eq!(lowered.plan().validate(), Ok(()));
    }

    #[test]
    fn native_suite_supports_nightly_windows_and_empty_features() {
        let matrix = native_matrix(
            "windows-latest",
            json!([]),
            json!({"nightly": true, "toolchain": "nightly"}),
        );
        let recipes = matrix.rust_validation_recipes().unwrap();
        let step = &recipes["case-000000"].steps[0];
        assert_eq!(step.shell, "pwsh");
        assert_eq!(step.env["GPIPE_RUST_TOOLCHAIN"], "+nightly");
        assert_eq!(
            step.env["GPIPE_RUST_FEATURES_0"],
            "fail-on-warnings,required"
        );
        assert_eq!(step.run.matches("& cargo ").count(), 3);
        assert_eq!(step.run.matches("$LASTEXITCODE").count(), 3);
        assert!(step.run.contains("Set-Location -LiteralPath"));
        assert!(step.run.ends_with("exit $status\n"));
        let mac = native_matrix("macos-latest", json!([""]), json!({"requiredFeatures": ""}));
        assert_eq!(
            mac.rust_validation_recipes().unwrap()["case-000000"].steps[0].env["GPIPE_RUST_FEATURES_0"],
            "fail-on-warnings"
        );
    }

    #[test]
    fn native_suite_rejects_unsupported_setup_toolchains_and_test_suites() {
        for extra in [
            json!({"cargo":"--all-features"}),
            json!({"env":"RUSTFLAGS=-foo"}),
            json!({"ciSteps":"setup"}),
            json!({"dependencies":"openssl"}),
            json!({"freshRunner":true}),
            json!({"gitSubmodules":true}),
            json!({"test-command":"nextest run"}),
            json!({"toolchain":"beta"}),
            json!({"toolchain":"nightly"}),
            json!({"nightly":"true"}),
            json!({"requiredFeatures":["std"]}),
            json!({"extension":true}),
            json!({"seed":-1}),
            json!({"cargo":null}),
        ] {
            let matrix = native_matrix("ubuntu-latest", json!(["std"]), extra.clone());
            assert!(
                matrix.rust_validation_recipes().is_err(),
                "accepted {extra}"
            );
        }
        assert!(
            native_matrix("custom-runner", json!([]), json!({}))
                .rust_validation_recipes()
                .is_err()
        );
    }

    #[test]
    fn native_suite_rejects_path_escape_and_feature_injection() {
        for path in [
            "../outside",
            "/tmp/outside",
            "C:/outside",
            "foo/../bar",
            "foo\\bar",
            "foo\nbar",
        ] {
            assert!(
                native_matrix("ubuntu-latest", json!([]), json!({"path":path}))
                    .rust_validation_recipes()
                    .is_err()
            );
        }
        for feature in ["std;exit 0", "$(exit)", "a b", "a,,b", "a\n"] {
            assert!(
                native_matrix("ubuntu-latest", json!([feature]), json!({}))
                    .rust_validation_recipes()
                    .is_err()
            );
        }
        let path = "packages/$(not-a-command) 'quoted'";
        let recipes = native_matrix("ubuntu-latest", json!(["std"]), json!({"path":path}))
            .rust_validation_recipes()
            .unwrap();
        assert_eq!(recipes["case-000000"].steps[0].env["GPIPE_RUST_PATH"], path);
        assert!(
            !recipes["case-000000"].steps[0]
                .run
                .contains("not-a-command")
        );
    }

    #[test]
    fn native_system_toolchain_has_no_selector_argument_on_either_shell() {
        for (os, prefix) in [("ubuntu-latest", "cargo"), ("windows-latest", "& cargo")] {
            let matrix = native_matrix(
                os,
                json!(["std"]),
                json!({"nightly": false, "toolchain": "system"}),
            );
            let recipes = matrix.rust_validation_recipes().unwrap();
            let step = &recipes["case-000000"].steps[0];
            assert!(!step.env.contains_key("GPIPE_RUST_TOOLCHAIN"));
            assert!(!step.run.contains("GPIPE_RUST_TOOLCHAIN"));
            assert!(!step.run.contains("+stable"));
            assert!(!step.run.contains("+system"));
            for command in ["check --all-targets", "clippy --all-targets", "test"] {
                assert!(
                    step.run
                        .contains(&format!("{prefix} {command} --no-default-features"))
                );
            }
            assert!(!step.run.contains("cargo \"\""));
        }
        let conflict = native_matrix(
            "ubuntu-latest",
            json!(["std"]),
            json!({"nightly": true, "toolchain": "system"}),
        );
        assert!(conflict.rust_validation_recipes().is_err());
        for toolchain in ["", "beta", "+stable", "unknown"] {
            let invalid = native_matrix(
                "ubuntu-latest",
                json!(["std"]),
                json!({"toolchain": toolchain}),
            );
            assert!(invalid.rust_validation_recipes().is_err());
        }
    }

    #[test]
    fn native_suite_bounds_feature_combinations() {
        let accepted = native_matrix("ubuntu-latest", json!(vec!["std"; 64]), json!({}));
        assert!(accepted.rust_validation_recipes().is_ok());
        let rejected = native_matrix("ubuntu-latest", json!(vec!["std"; 65]), json!({}));
        assert!(rejected.rust_validation_recipes().is_err());
    }

    #[test]
    fn rejects_oversized_input_and_malformed_receipt_schema() {
        let oversized = " ".repeat(MAX_PLAN_TEXT_BYTES + 1);
        assert!(MatrixPlan::from_jobs_json(&oversized, None).is_err());
        assert!(matrix().verify_receipts_json(&oversized).is_err());
        for input in ["{}", "[]", "[{\"case\":{},\"exit_code\":0}]", "[{}]"] {
            assert!(matrix().verify_receipts_json(input).is_err());
        }
    }
}
